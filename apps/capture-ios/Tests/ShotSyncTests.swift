import CryptoKit
import XCTest
@testable import CaveCAD_Capture

final class SyncPlanTests: XCTestCase {
    private let mib: Int64 = 1 << 20

    func testChunksFromZero() {
        XCTAssertEqual(SyncPlan.chunkSize, 4 * mib)
        XCTAssertEqual(SyncPlan.nextChunk(have: 0, total: 10 * mib), 0..<(4 * mib))
        XCTAssertEqual(SyncPlan.chunks(total: 10 * mib, from: 0), [0..<(4 * mib), (4 * mib)..<(8 * mib), (8 * mib)..<(10 * mib)])
    }

    func testChunksResumeFromHave() {
        // the server's `have` need not be on a chunk boundary (a lost answer, another device's chunk size)
        XCTAssertEqual(SyncPlan.chunks(total: 10 * mib, from: 5 * mib), [(5 * mib)..<(9 * mib), (9 * mib)..<(10 * mib)])
        XCTAssertEqual(SyncPlan.nextChunk(have: 10 * mib, total: 10 * mib), nil)
        XCTAssertEqual(SyncPlan.nextChunk(have: 11 * mib, total: 10 * mib), nil)
        XCTAssertEqual(SyncPlan.chunks(total: 3, from: 0), [0..<3])
        XCTAssertEqual(SyncPlan.nextChunk(have: -1, total: 3), 0..<3, "a nonsense have starts over")
    }

    func testBackoff() {
        XCTAssertEqual(SyncPlan.backoff(attempt: 0), 5)
        XCTAssertEqual(SyncPlan.backoff(attempt: 1), 10)
        XCTAssertEqual(SyncPlan.backoff(attempt: 3), 40)
        XCTAssertEqual(SyncPlan.backoff(attempt: 30), 300)
    }

    func testRangeHeader() {
        XCTAssertEqual(SyncPlan.range(have: 0, total: 10 * mib), "bytes=0-4194303")
        XCTAssertEqual(SyncPlan.range(have: 9 * mib, total: 10 * mib), "bytes=9437184-10485759")
        XCTAssertNil(SyncPlan.range(have: 10 * mib, total: 10 * mib))
    }

    private func local(_ id: String, _ sha: String) -> LocalShot { LocalShot(shotId: id, sha: sha, size: 10) }
    private func remote(_ id: String, _ sha: String) -> RemoteShot {
        RemoteShot(shotId: id, sha: sha, size: 10, capturedAt: 0, stationLabel: "")
    }

    func testDecideUploadsWhatTheServerLacks() {
        let d = SyncPlan.decide(local: [local("a", "1"), local("b", "2")], server: [remote("a", "1")], state: CaveSyncState())
        XCTAssertEqual(d.upload.map(\.shotId), ["b"])
        XCTAssertEqual(d.download, [])
        XCTAssertEqual(d.inSync, ["a": "1"])
    }

    func testDecideDownloadsWhatThisDeviceLacks() {
        let d = SyncPlan.decide(local: [local("a", "1")], server: [remote("a", "1"), remote("c", "3")], state: CaveSyncState())
        XCTAssertEqual(d.download.map(\.shotId), ["c"])
        XCTAssertEqual(d.upload, [])
    }

    func testChangedHereUploadsChangedThereDownloads() {
        var state = CaveSyncState()
        // a: synced as "1", then rewritten here (a supplemental pass): the new file replaces the server's
        state.shots["a"] = ShotRecord(sha: "1b", size: 10, modified: 0, syncedSha: "1")
        // b: synced as "2" and unchanged here, but the server has a newer "2b" (a pass on another device)
        state.shots["b"] = ShotRecord(sha: "2", size: 10, modified: 0, syncedSha: "2")
        let d = SyncPlan.decide(local: [local("a", "1b"), local("b", "2")], server: [remote("a", "1"), remote("b", "2b")], state: state)
        XCTAssertEqual(d.upload.map(\.shotId), ["a"])
        XCTAssertEqual(d.download.map(\.shotId), ["b"])
    }

    func testNeverSyncedAndDifferentUploads() {
        let d = SyncPlan.decide(local: [local("a", "x")], server: [remote("a", "y")], state: CaveSyncState())
        XCTAssertEqual(d.upload.map(\.shotId), ["a"], "the phone's own file is never overwritten by guesswork")
        XCTAssertEqual(d.download, [])
    }

    func testPendingDeletesAndRejectedAreLeftAlone() {
        var state = CaveSyncState()
        state.pendingDeletes = ["gone"]
        state.shots["bad"] = ShotRecord(sha: "9", size: 10, modified: 0, rejectedSha: "9")
        let d = SyncPlan.decide(local: [local("bad", "9")], server: [remote("gone", "5")], state: state)
        XCTAssertEqual(d.upload, [], "a file the server refused is not sent again until it changes")
        XCTAssertEqual(d.download, [], "a shot deleted here is not fetched back while its delete waits")
        var changed = state
        changed.shots["bad"]?.sha = "9b"
        XCTAssertEqual(SyncPlan.decide(local: [local("bad", "9b")], server: [], state: changed).upload.map(\.shotId), ["bad"])
    }

    func testServerLackingAShotNeverDeletesIt() {
        var state = CaveSyncState()
        state.shots["a"] = ShotRecord(sha: "1", size: 10, modified: 0, syncedSha: "1")
        let d = SyncPlan.decide(local: [local("a", "1")], server: [], state: state)
        XCTAssertEqual(d.upload.map(\.shotId), ["a"])
    }

    func testStatePersistsRoundTrip() throws {
        var state = CaveSyncState()
        state.shots["a"] = ShotRecord(sha: "abc", size: 123, modified: 456.5, syncedSha: "abb", fromServer: true, rejectedSha: nil, uploaded: 4_194_304)
        state.pendingDeletes = ["z"]
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("sync-\(UUID().uuidString).json")
        defer { try? FileManager.default.removeItem(at: url) }
        try state.save(to: url)
        XCTAssertEqual(CaveSyncState.load(from: url), state)
        XCTAssertEqual(CaveSyncState.load(from: url.appendingPathExtension("missing")), CaveSyncState())
    }

    func testParseList() {
        let json = #"{"captures":[{"shotId":"s1","sha":"aa","size":12,"stationId":"","stationLabel":"A9","capturedAt":1700000000000,"device":"iPhone","addedBy":"m","addedAt":1}]}"#
        XCTAssertEqual(RemoteShot.parseList(Data(json.utf8)), [RemoteShot(shotId: "s1", sha: "aa", size: 12, capturedAt: 1_700_000_000_000, stationLabel: "A9")])
        XCTAssertEqual(RemoteShot.parseList(Data("nope".utf8)), [])
    }

    func testShaOfFileAndVerify() throws {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("hash-\(UUID().uuidString).bin")
        defer { try? FileManager.default.removeItem(at: url) }
        // more than one read buffer, so the streaming path is used
        let data = Data((0..<(3 * 1024 * 1024 + 7)).map { UInt8($0 & 0xff) })
        try data.write(to: url)
        let want = SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
        XCTAssertEqual(try ShotHash.file(url), want)
        XCTAssertTrue(ShotHash.verify(url, sha: want))
        XCTAssertFalse(ShotHash.verify(url, sha: String(repeating: "0", count: 64)))
        XCTAssertEqual(ShotHash.hex(Data("abc".utf8)), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
    }

    func testViewerModeDecision() {
        XCTAssertTrue(DeviceMode.canCapture(hasLiDAR: true))
        XCTAssertFalse(DeviceMode.canCapture(hasLiDAR: false))
    }

    func testRowText() {
        XCTAssertEqual(ShotSyncState.uploaded.text(serverMissing: false), "uploaded ✓")
        XCTAssertEqual(ShotSyncState.uploading(0.427).text(serverMissing: false), "uploading 42%")
        XCTAssertEqual(ShotSyncState.waiting.text(serverMissing: false), "waiting for signal")
        XCTAssertEqual(ShotSyncState.waiting.text(serverMissing: true), "not uploaded")
        XCTAssertEqual(ShotSyncState.fromServer.text(serverMissing: false), "from another device")
        XCTAssertEqual(CaveSyncSummary(shots: 5, toUpload: 2).line, "5 shots, 2 to upload")
        XCTAssertEqual(CaveSyncSummary(shots: 1, toUpload: 0).line, "1 shot, all uploaded")
    }
}

/// Answers every request of a session with a canned reply and keeps what was asked (headers, body).
final class StubProtocol: URLProtocol, @unchecked Sendable {
    struct Seen { let method: String; let url: URL; let headers: [String: String]; let body: Data }
    typealias Reply = (status: Int, headers: [String: String], body: Data)
    private static let lock = NSLock()
    nonisolated(unsafe) private static var _seen: [Seen] = []
    nonisolated(unsafe) private static var _answer: (Seen) -> Reply = { _ in (500, [:], Data()) }

    static var seen: [Seen] { lock.withLock { _seen } }
    static func answer(_ f: @escaping (Seen) -> Reply) { lock.withLock { _seen = []; _answer = f } }

    static func session() -> URLSession {
        let c = URLSessionConfiguration.ephemeral
        c.protocolClasses = [StubProtocol.self]
        return URLSession(configuration: c)
    }

    override class func canInit(with request: URLRequest) -> Bool { true }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }
    override func startLoading() {
        var body = request.httpBody ?? Data()
        if body.isEmpty, let s = request.httpBodyStream {
            s.open()
            var buf = [UInt8](repeating: 0, count: 65536)
            while s.hasBytesAvailable { let n = s.read(&buf, maxLength: buf.count); if n <= 0 { break }; body.append(buf, count: n) }
            s.close()
        }
        let seen = Seen(method: request.httpMethod ?? "GET", url: request.url!, headers: request.allHTTPHeaderFields ?? [:], body: body)
        let reply = Self.lock.withLock { Self._seen.append(seen); return Self._answer }(seen)
        let resp = HTTPURLResponse(url: request.url!, statusCode: reply.status, httpVersion: "HTTP/1.1", headerFields: reply.headers)!
        client?.urlProtocol(self, didReceive: resp, cacheStoragePolicy: .notAllowed)
        client?.urlProtocol(self, didLoad: reply.body)
        client?.urlProtocolDidFinishLoading(self)
    }
    override func stopLoading() {}
}

final class CaptureAPITests: XCTestCase {
    private let cave = "cave12345"
    private let sha = String(repeating: "ab", count: 32)

    private func api() -> CaptureAPI {
        CaptureAPI(session: StubProtocol.session()) { path, query in
            var c = URLComponents(string: "https://cavecad.test/")!
            c.path = "/" + path
            if !query.isEmpty { c.queryItems = query }
            var r = URLRequest(url: c.url!)
            r.setValue("Bearer t0k", forHTTPHeaderField: "Authorization")
            r.setValue("dev1", forHTTPHeaderField: "X-Cave-Device")
            return r
        }
    }
    private func json(_ s: String, _ status: Int = 200) -> StubProtocol.Reply { (status, ["Content-Type": "application/json"], Data(s.utf8)) }

    func testListAndNotDeployed() async throws {
        StubProtocol.answer { _ in self.json(#"{"captures":[{"shotId":"s","sha":"aa","size":3,"capturedAt":5,"stationLabel":"A1"}]}"#) }
        let list = try await api().list(cave: cave)
        XCTAssertEqual(list.map(\.shotId), ["s"])
        let r = StubProtocol.seen[0]
        XCTAssertEqual(r.method, "GET")
        XCTAssertEqual(r.url.path, "/api/caves/\(cave)/captures")
        XCTAssertEqual(r.headers["Authorization"], "Bearer t0k")
        XCTAssertEqual(r.headers["X-Cave-Device"], "dev1")

        StubProtocol.answer { _ in self.json(#"{"error":"not-found"}"#, 404) }
        do { _ = try await api().list(cave: cave); XCTFail("404 must say not available") }
        catch { XCTAssertEqual(error as? SyncError, .unavailable) }
    }

    func testUploadStatus() async throws {
        StubProtocol.answer { _ in self.json(#"{"have":4194304}"#) }
        let have = try await api().uploadHave(cave: cave, sha: sha)
        XCTAssertEqual(have, 4_194_304)
        XCTAssertEqual(StubProtocol.seen[0].url.path, "/api/caves/\(cave)/captures/\(sha)/upload")
        XCTAssertEqual(StubProtocol.seen[0].method, "GET")
    }

    func testPutChunkShapeAndAnswers() async throws {
        let chunk = Data(repeating: 7, count: 1000)
        StubProtocol.answer { _ in self.json(#"{"have":5000}"#) }
        let a = try await api().put(cave: cave, sha: sha, offset: 4000, total: 9000, chunk: chunk)
        XCTAssertEqual(a, .have(5000))
        let r = StubProtocol.seen[0]
        XCTAssertEqual(r.method, "PUT")
        XCTAssertEqual(r.url.path, "/api/caves/\(cave)/captures/\(sha)")
        let q = URLComponents(url: r.url, resolvingAgainstBaseURL: false)!.queryItems!
        XCTAssertEqual(q.first { $0.name == "offset" }?.value, "4000")
        XCTAssertEqual(q.first { $0.name == "total" }?.value, "9000")
        XCTAssertEqual(r.headers["Content-Type"], "application/octet-stream")
        XCTAssertEqual(r.headers["X-Cave-Device"], "dev1")
        XCTAssertEqual(r.body, chunk)

        StubProtocol.answer { _ in self.json(#"{"error":"offset","have":3000}"#, 409) }
        let conflict = try await api().put(cave: cave, sha: sha, offset: 4000, total: 9000, chunk: chunk)
        XCTAssertEqual(conflict, .conflict(3000))

        StubProtocol.answer { _ in self.json(#"{"capture":{"shotId":"s","sha":"x"}}"#) }
        let done = try await api().put(cave: cave, sha: sha, offset: 8000, total: 9000, chunk: chunk)
        XCTAssertEqual(done, .done)

        StubProtocol.answer { _ in self.json(#"{"error":"bad-sha"}"#, 422) }
        do { _ = try await api().put(cave: cave, sha: sha, offset: 8000, total: 9000, chunk: chunk); XCTFail() }
        catch { XCTAssertEqual(error as? SyncError, .refused(422, "bad-sha")) }
    }

    func testDeleteSendsJsonType() async throws {
        StubProtocol.answer { _ in self.json(#"{"ok":true}"#) }
        try await api().delete(cave: cave, shotId: "shot-1")
        let r = StubProtocol.seen[0]
        XCTAssertEqual(r.method, "DELETE")
        XCTAssertEqual(r.url.path, "/api/caves/\(cave)/captures/shot-1")
        XCTAssertEqual(r.headers["Content-Type"], "application/json")
        // already gone on the server: done as well
        StubProtocol.answer { _ in self.json(#"{"error":"not-found"}"#, 404) }
        try await api().delete(cave: cave, shotId: "shot-1")
    }

    func testDownloadResumesWithRangeAndVerifies() async throws {
        let file = Data((0..<(5 * 1024 * 1024)).map { UInt8(($0 * 31) & 0xff) })
        let sha = ShotHash.hex(file)
        StubProtocol.answer { seen in
            guard let r = seen.headers["Range"], let m = r.wholeMatch(of: /bytes=(\d+)-(\d+)/) else { return (200, [:], file) }
            let a = Int(m.1)!, b = min(Int(m.2)!, file.count - 1)
            return (206, ["Content-Range": "bytes \(a)-\(b)/\(file.count)"], file.subdata(in: a..<(b + 1)))
        }
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("dl-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let part = dir.appendingPathComponent("s.cvcap.part"), final = dir.appendingPathComponent("s.cvcap")
        // a part already holding the first 1 MiB (an earlier, interrupted download)
        try file.prefix(1 << 20).write(to: part)
        try await api().download(cave: cave, shot: RemoteShot(shotId: "s", sha: sha, size: Int64(file.count), capturedAt: 0, stationLabel: ""),
                                 part: part, to: final)
        XCTAssertEqual(StubProtocol.seen.map { $0.headers["Range"] }, ["bytes=1048576-5242879"])
        XCTAssertEqual(StubProtocol.seen[0].url.path, "/api/caves/\(cave)/captures/\(sha)")
        XCTAssertEqual(try Data(contentsOf: final), file)
        XCTAssertFalse(FileManager.default.fileExists(atPath: part.path))

        // a wrong file is never moved into place
        try? FileManager.default.removeItem(at: final)
        StubProtocol.answer { _ in (200, [:], Data(repeating: 1, count: file.count)) }
        do {
            try await api().download(cave: cave, shot: RemoteShot(shotId: "s", sha: sha, size: Int64(file.count), capturedAt: 0, stationLabel: ""),
                                     part: part, to: final)
            XCTFail("a bad SHA must throw")
        } catch { XCTAssertEqual(error as? SyncError, .badDownload) }
        XCTAssertFalse(FileManager.default.fileExists(atPath: final.path))
        XCTAssertFalse(FileManager.default.fileExists(atPath: part.path), "a bad part is thrown away, the next try starts over")
    }
}
