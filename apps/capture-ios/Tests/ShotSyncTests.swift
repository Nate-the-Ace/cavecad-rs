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
    nonisolated(unsafe) private static var _answer: @Sendable (Seen) -> Reply = { _ in (500, [:], Data()) }

    static var seen: [Seen] { lock.withLock { _seen } }
    static func answer(_ f: @escaping @Sendable (Seen) -> Reply) { lock.withLock { _seen = []; _answer = f } }

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
        StubProtocol.answer { _ in FakeCaptures.json(#"{"captures":[{"shotId":"s","sha":"aa","size":3,"capturedAt":5,"stationLabel":"A1"}]}"#) }
        let list = try await api().list(cave: cave)
        XCTAssertEqual(list.map(\.shotId), ["s"])
        let r = StubProtocol.seen[0]
        XCTAssertEqual(r.method, "GET")
        XCTAssertEqual(r.url.path, "/api/caves/\(cave)/captures")
        XCTAssertEqual(r.headers["Authorization"], "Bearer t0k")
        XCTAssertEqual(r.headers["X-Cave-Device"], "dev1")

        StubProtocol.answer { _ in FakeCaptures.json(#"{"error":"not-found"}"#, 404) }
        do { _ = try await api().list(cave: cave); XCTFail("404 must say not available") }
        catch { XCTAssertEqual(error as? SyncError, .unavailable) }
    }

    func testUploadStatus() async throws {
        StubProtocol.answer { _ in FakeCaptures.json(#"{"have":4194304}"#) }
        let have = try await api().uploadHave(cave: cave, sha: sha)
        XCTAssertEqual(have, 4_194_304)
        XCTAssertEqual(StubProtocol.seen[0].url.path, "/api/caves/\(cave)/captures/\(sha)/upload")
        XCTAssertEqual(StubProtocol.seen[0].method, "GET")
    }

    func testPutChunkShapeAndAnswers() async throws {
        let chunk = Data(repeating: 7, count: 1000)
        StubProtocol.answer { _ in FakeCaptures.json(#"{"have":5000}"#) }
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

        StubProtocol.answer { _ in FakeCaptures.json(#"{"error":"offset","have":3000}"#, 409) }
        let conflict = try await api().put(cave: cave, sha: sha, offset: 4000, total: 9000, chunk: chunk)
        XCTAssertEqual(conflict, .conflict(3000))

        StubProtocol.answer { _ in FakeCaptures.json(#"{"capture":{"shotId":"s","sha":"x"}}"#) }
        let done = try await api().put(cave: cave, sha: sha, offset: 8000, total: 9000, chunk: chunk)
        XCTAssertEqual(done, .done)

        StubProtocol.answer { _ in FakeCaptures.json(#"{"error":"bad-sha"}"#, 422) }
        do { _ = try await api().put(cave: cave, sha: sha, offset: 8000, total: 9000, chunk: chunk); XCTFail() }
        catch { XCTAssertEqual(error as? SyncError, .refused(422, "bad-sha")) }
    }

    func testDeleteSendsJsonType() async throws {
        StubProtocol.answer { _ in FakeCaptures.json(#"{"ok":true}"#) }
        try await api().delete(cave: cave, shotId: "shot-1")
        let r = StubProtocol.seen[0]
        XCTAssertEqual(r.method, "DELETE")
        XCTAssertEqual(r.url.path, "/api/caves/\(cave)/captures/shot-1")
        XCTAssertEqual(r.headers["Content-Type"], "application/json")
        // already gone on the server: done as well
        StubProtocol.answer { _ in FakeCaptures.json(#"{"error":"not-found"}"#, 404) }
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

final class SyncPlanEdgeTests: XCTestCase {
    func testChunkToSendNeverCrashesOnAnEmptyFile() {
        XCTAssertNil(SyncPlan.chunkToSend(have: 0, total: 0), "an empty file has nothing to send: refused, not a crash")
        XCTAssertNil(SyncPlan.chunkToSend(have: 5, total: 0))
        XCTAssertEqual(SyncPlan.chunkToSend(have: 0, total: 10), 0..<10)
        XCTAssertEqual(SyncPlan.chunkToSend(have: 10, total: 10), 9..<10, "server has it all but kept nothing: resend the last byte")
    }

    func testDownloadReserve() {
        let gb: Int64 = 1_000_000_000
        XCTAssertEqual(SyncPlan.downloadReserve, 2 * gb)
        XCTAssertTrue(SyncPlan.downloadFits(need: 1 * gb, free: 3 * gb))
        XCTAssertFalse(SyncPlan.downloadFits(need: 1 * gb + 1, free: 3 * gb), "would leave less than 2 GB")
        XCTAssertFalse(SyncPlan.downloadFits(need: 0, free: gb), "already under the reserve: nothing comes down")
        XCTAssertTrue(SyncPlan.downloadFits(need: 5 * gb, free: nil), "unknown free space does not stop a download")
    }
}

/// True the first time only (a hook that fires once, from the URL loading thread).
final class Once: @unchecked Sendable {
    private let lock = NSLock()
    private var fired = false
    func first() -> Bool { lock.withLock { defer { fired = true }; return !fired } }
}

/// A captures server in memory, for whole sync passes through `StubProtocol`.
final class FakeCaptures: @unchecked Sendable {
    private let lock = NSLock()
    private var shots: [String: (sha: String, data: Data)] = [:]
    private var parts: [String: Data] = [:]
    private(set) var deletedIds: [String] = []
    /// Answers before the fake's own (status overrides, hooks); nil falls through.
    var before: (@Sendable (StubProtocol.Seen) -> StubProtocol.Reply?)? {
        get { lock.withLock { _before } }
        set { lock.withLock { _before = newValue } }
    }
    private var _before: (@Sendable (StubProtocol.Seen) -> StubProtocol.Reply?)?

    func add(_ id: String, _ data: Data) { lock.withLock { shots[id] = (ShotHash.hex(data), data) } }
    var deleted: [String] { lock.withLock { deletedIds } }

    static func json(_ s: String, _ status: Int = 200) -> StubProtocol.Reply { (status, ["Content-Type": "application/json"], Data(s.utf8)) }

    func answer(_ seen: StubProtocol.Seen) -> StubProtocol.Reply {
        if let r = before?(seen) { return r }
        let parts = seen.url.path.split(separator: "/").map(String.init) // api caves <cave> captures [x] [upload]
        let q = URLComponents(url: seen.url, resolvingAgainstBaseURL: false)?.queryItems ?? []
        return lock.withLock {
            switch (seen.method, parts.count) {
            case ("GET", 4):
                let list = shots.map { #"{"shotId":"\#($0.key)","sha":"\#($0.value.sha)","size":\#($0.value.data.count),"capturedAt":0,"stationLabel":""}"# }
                return Self.json(#"{"captures":[\#(list.joined(separator: ","))]}"#)
            case ("GET", 6):
                return Self.json(#"{"have":\#(self.parts[parts[4]]?.count ?? 0)}"#)
            case ("PUT", 5):
                let sha = parts[4], offset = Int(q.first { $0.name == "offset" }!.value!)!, total = Int(q.first { $0.name == "total" }!.value!)!
                var have = self.parts[sha] ?? Data()
                guard offset == have.count else { return Self.json(#"{"error":"offset","have":\#(have.count)}"#, 409) }
                have.append(seen.body)
                self.parts[sha] = have
                return have.count == total ? Self.json(#"{"capture":{"sha":"\#(sha)"}}"#) : Self.json(#"{"have":\#(have.count)}"#)
            case ("GET", 5):
                guard let file = shots.values.first(where: { $0.sha == parts[4] })?.data else { return Self.json(#"{"error":"not-found"}"#, 404) }
                guard let r = seen.headers["Range"], let m = r.wholeMatch(of: /bytes=(\d+)-(\d+)/) else { return (200, [:], file) }
                let a = Int(m.1)!, b = min(Int(m.2)!, file.count - 1)
                return (206, [:], file.subdata(in: a..<(b + 1)))
            case ("DELETE", 5):
                shots[parts[4]] = nil
                deletedIds.append(parts[4])
                return Self.json(#"{"ok":true}"#)
            default:
                return Self.json(#"{"error":"bad-request"}"#, 400)
            }
        }
    }
}

/// Whole passes of `ShotSync` against `FakeCaptures`: the orderings a cave trip makes (a delete while a pass runs,
/// the sign-in ending, a view-only member, a full phone, iOS ending background time).
@MainActor
final class ShotSyncPassTests: XCTestCase {
    private var cave = ""
    private var fake = FakeCaptures()
    private var ended = 0

    override func setUp() async throws {
        cave = "test-sync-\(UUID().uuidString.prefix(8))"
        fake = FakeCaptures()
        ended = 0
        let f = fake
        StubProtocol.answer { @Sendable seen in f.answer(seen) }
    }

    override func tearDown() async throws {
        try? FileManager.default.removeItem(at: ShotFiles.folder(cave: cave))
        try? FileManager.default.removeItem(at: CaveSyncState.url(cave: cave))
    }

    private func makeSync(request: (@Sendable (String, [URLQueryItem]) async throws -> URLRequest)? = nil) -> ShotSync {
        let s = ShotSync()
        let api = CaptureAPI(session: StubProtocol.session(), request: request ?? { path, query in
            var c = URLComponents(string: "https://cavecad.test/")!
            c.path = "/" + path
            if !query.isEmpty { c.queryItems = query }
            return URLRequest(url: c.url!)
        })
        s.attach(api: api, signedIn: { true }, signInEnded: { [weak self] in self?.ended += 1 })
        s.freeSpace = { 100_000_000_000 }
        return s
    }

    private func shot(_ text: String, size: Int = 0) -> Data {
        var d = SyncPlan.magic + Data(text.utf8)
        if size > d.count { d.append(Data(repeating: 7, count: size - d.count)) }
        return d
    }

    @discardableResult
    private func writeLocal(_ id: String, _ data: Data) throws -> URL {
        let u = ShotFiles.folder(cave: cave).appendingPathComponent("\(id).cvcap")
        try data.write(to: u)
        return u
    }

    private func waitIdle(_ s: ShotSync, file: StaticString = #filePath, line: UInt = #line) async {
        for _ in 0..<1000 where !s.idle { try? await Task.sleep(nanoseconds: 10_000_000) }
        XCTAssertTrue(s.idle, "the pass never finished", file: file, line: line)
    }

    private func onDisk() -> CaveSyncState { CaveSyncState.load(from: CaveSyncState.url(cave: cave)) }

    /// Runs `body` on the main actor from the stub's thread and waits for it, so it lands mid-request.
    private nonisolated static func onMain(_ body: @escaping @MainActor () -> Void) {
        let done = DispatchSemaphore(value: 0)
        DispatchQueue.main.async { MainActor.assumeIsolated { body() }; done.signal() }
        done.wait()
    }

    func testDeleteDuringAnUploadReachesTheServer() async throws {
        let file = try writeLocal("shotA", shot("a"))
        let s = makeSync()
        let cave = cave
        let once = Once()
        fake.before = { @Sendable seen in
            guard seen.method == "PUT", once.first() else { return nil }
            Self.onMain { ShotFiles.delete(file); s.deleted(cave: cave, shotIds: ["shotA"]) }
            return nil
        }
        s.kick(cave)
        await waitIdle(s)
        XCTAssertEqual(fake.deleted, ["shotA"], "the delete made while the pass ran was not overwritten by the pass's save")
        XCTAssertEqual(onDisk().pendingDeletes, [])
        XCTAssertNil(onDisk().shots["shotA"])
        XCTAssertFalse(FileManager.default.fileExists(atPath: file.path))
    }

    func testDeleteDuringADownloadDiscardsItAndIsNotFetchedBack() async throws {
        let v1 = shot("one"), v2 = shot("two, from the other phone", size: 5 << 20)
        let file = try writeLocal("shotS", v1)
        let mtime = (try FileManager.default.attributesOfItem(atPath: file.path)[.modificationDate] as! Date).timeIntervalSince1970
        var st = CaveSyncState()
        st.shots["shotS"] = ShotRecord(sha: ShotHash.hex(v1), size: Int64(v1.count), modified: mtime, syncedSha: ShotHash.hex(v1))
        try st.save(to: CaveSyncState.url(cave: cave))
        fake.add("shotS", v2) // a newer version: this phone's unchanged copy is replaced by a download
        let s = makeSync()
        let cave = cave
        let once = Once()
        fake.before = { @Sendable seen in
            guard seen.headers["Range"] != nil, once.first() else { return nil }
            Self.onMain { ShotFiles.delete(file); s.deleted(cave: cave, shotIds: ["shotS"]) }
            return nil
        }
        s.kick(cave)
        await waitIdle(s)
        let folder = ShotFiles.folder(cave: cave)
        XCTAssertFalse(FileManager.default.fileExists(atPath: file.path), "the deleted shot was not moved back into place")
        XCTAssertFalse(FileManager.default.fileExists(atPath: folder.appendingPathComponent("shotS.cvcap.part").path), "its part is thrown away")
        XCTAssertEqual(fake.deleted, ["shotS"])
        XCTAssertEqual(onDisk().pendingDeletes, [])
        XCTAssertEqual(StubProtocol.seen.filter { $0.headers["Range"] != nil }.count, 1, "stopped after the range in flight, never fetched back")
    }

    func testUnauthorizedEndsTheSignInButKeepsKeptCaves() async throws {
        try writeLocal("shotA", shot("a"))
        let kept = CaveStore.folder.appendingPathComponent("\(cave).json")
        try JSONEncoder().encode(KeptCave(id: cave, name: "Kept", snapshot: nil, entries: [], last: 0, keptAt: Date())).write(to: kept)
        defer { try? FileManager.default.removeItem(at: kept) }
        fake.before = { @Sendable _ in FakeCaptures.json(#"{"error":"unauthorized"}"#, 401) }
        let s = makeSync()
        s.kick(cave)
        await waitIdle(s)
        XCTAssertEqual(ended, 1, "a 401 ends the sign-in")
        XCTAssertTrue(FileManager.default.fileExists(atPath: kept.path), "the offline copy of the cave stays")
        XCTAssertTrue(CaveStore.keptIdsOnDisk().contains(cave))
    }

    func testViewOnlyUploadIsRefusedAndDownloadsStillRun() async throws {
        let a = shot("mine")
        try writeLocal("shotA", a)
        fake.add("shotB", shot("theirs"))
        fake.before = { @Sendable seen in
            seen.method == "PUT" || seen.url.path.hasSuffix("/upload") ? FakeCaptures.json(#"{"error":"watcher"}"#, 403) : nil
        }
        let s = makeSync()
        s.kick(cave)
        await waitIdle(s)
        XCTAssertEqual(onDisk().shots["shotA"]?.rejectedSha, ShotHash.hex(a), "refused for that shot, not sent again until it changes")
        XCTAssertEqual(s.state(cave: cave, shot: "shotA"), .refused)
        XCTAssertEqual(try Data(contentsOf: ShotFiles.folder(cave: cave).appendingPathComponent("shotB.cvcap")), shot("theirs"),
                       "the others' shots still came down")
        XCTAssertEqual(s.state(cave: cave, shot: "shotB"), .fromServer)
    }

    func testEmptyAndForeignFilesAreLeftOutAndRepairedFromTheServer() async throws {
        try writeLocal("empty", Data())
        try writeLocal("junk", Data("not a shot at all".utf8))
        let good = shot("good")
        try writeLocal("good", good)
        let hashed = ShotSync.hashLocal(cave: cave, state: CaveSyncState())
        XCTAssertEqual(hashed.local.map(\.shotId), ["good"])
        XCTAssertEqual(Set(hashed.skipped), ["empty", "junk"])

        fake.add("empty", shot("the real empty"))
        let s = makeSync()
        s.kick(cave)
        await waitIdle(s)
        XCTAssertEqual(try Data(contentsOf: ShotFiles.folder(cave: cave).appendingPathComponent("empty.cvcap")), shot("the real empty"),
                       "a good copy on the server comes down over an empty file")
        XCTAssertFalse(StubProtocol.seen.contains { $0.method == "PUT" && $0.body.isEmpty }, "nothing empty was sent")
        XCTAssertNil(s.state(cave: cave, shot: "junk"))
    }

    func testNoSignalDuringADownloadIsOfflineNotAWriteFailure() async throws {
        let theirs = shot("theirs")
        fake.add("shotB", theirs)
        let sha = ShotHash.hex(theirs)
        let s = makeSync { path, query in
            if path.hasSuffix(sha) { throw URLError(.notConnectedToInternet) } // the device call failing: no signal
            var c = URLComponents(string: "https://cavecad.test/")!
            c.path = "/" + path
            if !query.isEmpty { c.queryItems = query }
            return URLRequest(url: c.url!)
        }
        try writeLocal("shotA", shot("a"))
        s.kick(cave)
        await waitIdle(s)
        XCTAssertTrue(s.offline, "a URLError on download is the no-signal path (retried with backoff)")
        XCTAssertFalse(FileManager.default.fileExists(atPath: ShotFiles.folder(cave: cave).appendingPathComponent("shotB.cvcap").path))
    }

    func testDownloadKeepsTwoGigabytesFree() async throws {
        fake.add("shotB", shot("theirs", size: 1000))
        let s = makeSync()
        s.freeSpace = { SyncPlan.downloadReserve + 500 }
        s.kick(cave)
        await waitIdle(s)
        XCTAssertFalse(StubProtocol.seen.contains { $0.headers["Range"] != nil }, "skipped before asking for a byte")
        XCTAssertFalse(FileManager.default.fileExists(atPath: ShotFiles.folder(cave: cave).appendingPathComponent("shotB.cvcap").path))
        XCTAssertFalse(s.offline)
    }

    func testBackgroundTimeRunningOutStopsCleanlyAndResumes() async throws {
        let a = shot("big", size: 5 << 20) // two chunks
        try writeLocal("shotA", a)
        let s = makeSync()
        let once = Once()
        fake.before = { @Sendable seen in
            guard seen.method == "PUT", once.first() else { return nil }
            Self.onMain { s.backgroundTimeRanOut() } // iOS ends the time while the first chunk is in flight
            return nil
        }
        s.kick(cave)
        await waitIdle(s)
        XCTAssertTrue(s.paused)
        XCTAssertFalse(s.offline, "not a lost connection: no backoff")
        XCTAssertTrue(s.testStatus().contains("queued=1"), s.testStatus())
        XCTAssertEqual(StubProtocol.seen.filter { $0.method == "PUT" }.count, 1, "no new chunk after the time ran out")
        XCTAssertNil(onDisk().shots["shotA"]?.syncedSha)

        s.kick(cave) // a save in the background: queued, not started
        XCTAssertTrue(s.idle)
        s.resume() // the app is opened again
        await waitIdle(s)
        XCTAssertFalse(s.paused)
        let puts = StubProtocol.seen.filter { $0.method == "PUT" }
        XCTAssertEqual(puts.count, 2, "carried on from what the server held, not from zero")
        XCTAssertEqual(URLComponents(url: puts[1].url, resolvingAgainstBaseURL: false)?.queryItems?.first { $0.name == "offset" }?.value, "4194304")
        XCTAssertEqual(onDisk().shots["shotA"]?.syncedSha, ShotHash.hex(a))
    }
}
