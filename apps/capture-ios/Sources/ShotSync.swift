import CryptoKit
import Foundation
import UIKit

// Shot sync: every saved shot of a cave goes to the server (`/api/caves/:cave/captures`, apps/server/CAPTURES.md)
// in resumable chunks, and every shot the server has that this device lacks comes down. A shot is known by its
// shot id (its file name, the header's `shot_id`) and its version by the SHA-256 of the whole file: a
// supplemental pass rewrites the file, so the new SHA is uploaded and replaces the old one on the server.
// Nothing here ever deletes a local shot because the server lacks it; deleting on the device deletes on the
// server (queued while offline).

/// One shot on the server, from `GET /api/caves/:cave/captures`.
struct RemoteShot: Equatable, Sendable {
    let shotId: String
    let sha: String
    let size: Int64
    /// Milliseconds since 1970 (the header's `captured_at_ms`).
    let capturedAt: Int64
    let stationLabel: String

    static func parseList(_ data: Data) -> [RemoteShot] {
        let list = ((try? JSONSerialization.jsonObject(with: data)) as? [String: Any])?["captures"] as? [[String: Any]] ?? []
        return list.compactMap { c in
            guard let id = c["shotId"] as? String, let sha = c["sha"] as? String,
                  let size = (c["size"] as? NSNumber)?.int64Value else { return nil }
            return RemoteShot(shotId: id, sha: sha, size: size, capturedAt: (c["capturedAt"] as? NSNumber)?.int64Value ?? 0,
                              stationLabel: c["stationLabel"] as? String ?? "")
        }
    }
}

/// One shot on this device, hashed.
struct LocalShot: Equatable, Sendable {
    let shotId: String
    let sha: String
    let size: Int64
}

/// What this device knows about one of its shots, kept across launches.
struct ShotRecord: Codable, Equatable, Sendable {
    /// SHA-256 of the file as it is here, and the size and modification time it was hashed at (a changed file is
    /// hashed again; an unchanged one is not).
    var sha: String
    var size: Int64
    var modified: Double
    /// The version the server is known to hold (uploaded or downloaded); nil before the first sync.
    var syncedSha: String? = nil
    /// Came down from the server (taken on another device).
    var fromServer = false
    /// The server refused this version (bad file, wrong cave, too big): not sent again until the file changes.
    var rejectedSha: String? = nil
    /// Bytes of `sha` the server held after the last chunk (the server's `have` is asked again on resume).
    var uploaded: Int64 = 0
}

/// A cave's sync state on this device: Application Support/shot-sync/<cave>.json.
struct CaveSyncState: Codable, Equatable, Sendable {
    var shots: [String: ShotRecord] = [:]
    /// Shots deleted here whose delete has not reached the server yet.
    var pendingDeletes: [String] = []

    static func url(cave: String) -> URL {
        let u = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0].appendingPathComponent("shot-sync")
        try? FileManager.default.createDirectory(at: u, withIntermediateDirectories: true)
        return u.appendingPathComponent("\(cave).json")
    }
    static func load(from url: URL) -> CaveSyncState {
        (try? Data(contentsOf: url)).flatMap { try? JSONDecoder().decode(CaveSyncState.self, from: $0) } ?? CaveSyncState()
    }
    func save(to url: URL) throws {
        try JSONEncoder().encode(self).write(to: url, options: .atomic)
    }
}

/// The pure decisions of shot sync.
enum SyncPlan {
    /// Upload chunks and download ranges (the server takes up to 8 MiB; 4 MiB is kinder to a weak signal).
    static let chunkSize: Int64 = 4 << 20

    /// The next chunk to send when the server holds `have` of `total` bytes; nil when it has them all.
    static func nextChunk(have: Int64, total: Int64, chunk: Int64 = chunkSize) -> Range<Int64>? {
        let from = max(0, have)
        guard from < total else { return nil }
        return from..<min(total, from + chunk)
    }

    /// Every chunk still to send.
    static func chunks(total: Int64, from have: Int64, chunk: Int64 = chunkSize) -> [Range<Int64>] {
        var out: [Range<Int64>] = [], at = have
        while let r = nextChunk(have: at, total: total, chunk: chunk) { out.append(r); at = r.upperBound }
        return out
    }

    /// The `Range` header for the next download piece when `have` bytes are here; nil when the file is whole.
    static func range(have: Int64, total: Int64, chunk: Int64 = chunkSize) -> String? {
        nextChunk(have: have, total: total, chunk: chunk).map { "bytes=\($0.lowerBound)-\($0.upperBound - 1)" }
    }

    /// Seconds to wait before retry `attempt` (0-based) after no connection: 5, 10, 20, … at most 5 minutes.
    static func backoff(attempt: Int) -> TimeInterval {
        min(300, 5 * pow(2, Double(min(attempt, 10))))
    }

    struct Decision: Equatable {
        var upload: [LocalShot] = []
        var download: [RemoteShot] = []
        /// Shots both sides hold in the same version (shot id → SHA).
        var inSync: [String: String] = [:]
    }

    /// What to send and what to fetch. A shot the server lacks, or holds in another version while this one
    /// changed here since the last sync (or was never synced), is uploaded; a shot this device lacks, or holds
    /// unchanged since the last sync while the server's changed, is downloaded. Shots deleted here (delete not
    /// sent yet) are not fetched back; a version the server refused is not sent again.
    static func decide(local: [LocalShot], server: [RemoteShot], state: CaveSyncState) -> Decision {
        var d = Decision()
        let byId = Dictionary(server.map { ($0.shotId, $0) }, uniquingKeysWith: { a, _ in a })
        let deleted = Set(state.pendingDeletes)
        for l in local {
            let r = byId[l.shotId], rec = state.shots[l.shotId]
            if let r, r.sha == l.sha { d.inSync[l.shotId] = l.sha; continue }
            if let r, rec?.syncedSha == l.sha { d.download.append(r); continue }
            if rec?.rejectedSha == l.sha { continue }
            d.upload.append(l)
        }
        let here = Set(local.map(\.shotId))
        for r in server where !here.contains(r.shotId) && !deleted.contains(r.shotId) { d.download.append(r) }
        return d
    }
}

/// SHA-256 of shot files (lowercase hex, as the server names them).
enum ShotHash {
    static func hex(_ data: Data) -> String { SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined() }

    /// Read in 1 MiB pieces: shots run to hundreds of MB.
    static func file(_ url: URL) throws -> String {
        let h = try FileHandle(forReadingFrom: url)
        defer { try? h.close() }
        var hasher = SHA256()
        while let piece = try h.read(upToCount: 1 << 20), !piece.isEmpty { hasher.update(data: piece) }
        return hasher.finalize().map { String(format: "%02x", $0) }.joined()
    }

    static func verify(_ url: URL, sha: String) -> Bool { (try? file(url)) == sha }
}

/// Whether this device takes shots or only views them (spec: "Devices without LiDAR").
enum DeviceMode {
    static func canCapture(hasLiDAR: Bool) -> Bool { hasLiDAR }
    @MainActor static var canCapture: Bool { canCapture(hasLiDAR: ShotRecorder.hasLiDAR) }
}

/// A shot's sync state, as its row shows it.
enum ShotSyncState: Equatable, Sendable {
    case uploaded, uploading(Double), waiting, fromServer, refused

    func text(serverMissing: Bool) -> String {
        switch self {
        case .uploaded: "uploaded ✓"
        case .uploading(let p): "uploading \(Int((p * 100).rounded(.down)))%"
        case .waiting: serverMissing ? "not uploaded" : "waiting for signal"
        case .fromServer: "from another device"
        case .refused: "the server refused this shot"
        }
    }
}

/// The cave-level line: "5 shots, 2 to upload".
struct CaveSyncSummary: Equatable, Sendable {
    var shots: Int
    var toUpload: Int
    var line: String {
        let n = "\(shots) shot\(shots == 1 ? "" : "s")"
        return toUpload == 0 ? (shots == 0 ? n : "\(n), all uploaded") : "\(n), \(toUpload) to upload"
    }
}

enum SyncError: Error, Equatable {
    /// The captures routes answered 404 on the list: the server keeps no captures (not deployed yet).
    case unavailable
    case notFound
    case unauthorized
    /// Any other refusal: status and the server's `error` code.
    case refused(Int, String)
    /// No answer (no signal, timed out).
    case network
    /// A downloaded file did not match its SHA-256.
    case badDownload
}

/// A chunk's answer.
enum PutAnswer: Equatable, Sendable {
    /// Stored; the server holds this many bytes.
    case have(Int64)
    /// 409: the server holds a different amount; carry on from it.
    case conflict(Int64)
    /// The last chunk arrived and the shot was kept (or the server already had this file).
    case done
}

/// The captures routes. `request` makes an authorized request (bearer + `X-Cave-Device`) for a path.
struct CaptureAPI: Sendable {
    let session: URLSession
    let request: @Sendable (_ path: String, _ query: [URLQueryItem]) async throws -> URLRequest
    /// A chunk or range over a weak signal: give it longer than an ordinary call.
    static let transferTimeout: TimeInterval = 120

    init(session: URLSession, request: @escaping @Sendable (_ path: String, _ query: [URLQueryItem]) async throws -> URLRequest) {
        self.session = session
        self.request = request
    }

    private func base(_ cave: String) -> String { "api/caves/\(cave)/captures" }

    private func send(_ req: URLRequest) async throws -> (Data, Int) {
        let (data, resp): (Data, URLResponse)
        do { (data, resp) = try await session.data(for: req) } catch { throw SyncError.network }
        let status = (resp as? HTTPURLResponse)?.statusCode ?? 0
        return (data, status)
    }

    private static func code(_ data: Data) -> String {
        ((try? JSONSerialization.jsonObject(with: data)) as? [String: Any])?["error"] as? String ?? ""
    }
    private static func number(_ data: Data, _ key: String) -> Int64? {
        (((try? JSONSerialization.jsonObject(with: data)) as? [String: Any])?[key] as? NSNumber)?.int64Value
    }

    /// Throws for anything but a 2xx.
    private static func check(_ status: Int, _ data: Data) throws {
        switch status {
        case 200..<300: return
        case 401: throw SyncError.unauthorized
        case 404: throw SyncError.notFound
        default: throw SyncError.refused(status, code(data))
        }
    }

    func list(cave: String) async throws -> [RemoteShot] {
        let (data, status) = try await send(try await request(base(cave), []))
        if status == 404 { throw SyncError.unavailable }
        try Self.check(status, data)
        return RemoteShot.parseList(data)
    }

    func uploadHave(cave: String, sha: String) async throws -> Int64 {
        let (data, status) = try await send(try await request("\(base(cave))/\(sha)/upload", []))
        try Self.check(status, data)
        return Self.number(data, "have") ?? 0
    }

    func put(cave: String, sha: String, offset: Int64, total: Int64, chunk: Data) async throws -> PutAnswer {
        var req = try await request("\(base(cave))/\(sha)", [.init(name: "offset", value: String(offset)), .init(name: "total", value: String(total))])
        req.httpMethod = "PUT"
        req.timeoutInterval = Self.transferTimeout
        req.setValue("application/octet-stream", forHTTPHeaderField: "Content-Type")
        req.httpBody = chunk
        let (data, status) = try await send(req)
        if status == 409 { return .conflict(Self.number(data, "have") ?? 0) }
        try Self.check(status, data)
        if (try? JSONSerialization.jsonObject(with: data) as? [String: Any])?["capture"] != nil { return .done }
        guard let have = Self.number(data, "have") else { throw SyncError.refused(status, "bad-answer") }
        return .have(have)
    }

    /// Deletes a shot on the server; one already gone there counts as done.
    func delete(cave: String, shotId: String) async throws {
        var req = try await request("\(base(cave))/\(shotId)", [])
        req.httpMethod = "DELETE"
        req.setValue("application/json", forHTTPHeaderField: "Content-Type")
        let (data, status) = try await send(req)
        if status == 404 { return }
        try Self.check(status, data)
    }

    /// Downloads `shot` into `part` from where `part` ends (4 MiB ranges), checks its SHA-256 and moves it to
    /// `final` (replacing an older version). A part that fails the check is deleted, so the next try starts over.
    func download(cave: String, shot: RemoteShot, part: URL, to final: URL) async throws {
        let fm = FileManager.default
        if !fm.fileExists(atPath: part.path) {
            fm.createFile(atPath: part.path, contents: nil, attributes: [.protectionKey: FileProtectionType.completeUntilFirstUserAuthentication])
        }
        var have = Self.size(part)
        while let range = SyncPlan.range(have: have, total: shot.size) {
            var req = try await request("\(base(cave))/\(shot.sha)", [])
            req.timeoutInterval = Self.transferTimeout
            req.setValue(range, forHTTPHeaderField: "Range")
            let (data, status) = try await send(req)
            if status == 416 { break } // the part already reaches the end
            try Self.check(status, data)
            if status == 206 {
                let h = try FileHandle(forWritingTo: part)
                defer { try? h.close() }
                try h.seek(toOffset: UInt64(have))
                try h.write(contentsOf: data)
                try h.truncate(atOffset: UInt64(have) + UInt64(data.count))
            } else {
                try data.write(to: part) // the whole file (a server that ignored the range)
            }
            let now = Self.size(part)
            guard now > have else { throw SyncError.network }
            have = now
        }
        guard Self.size(part) == shot.size, ShotHash.verify(part, sha: shot.sha) else {
            try? fm.removeItem(at: part)
            throw SyncError.badDownload
        }
        if fm.fileExists(atPath: final.path) { _ = try fm.replaceItemAt(final, withItemAt: part) }
        else { try fm.moveItem(at: part, to: final) }
    }

    static func size(_ url: URL) -> Int64 {
        ((try? FileManager.default.attributesOfItem(atPath: url.path))?[.size] as? NSNumber)?.int64Value ?? 0
    }
}

/// Runs shot sync for the whole app (one per run, so it carries on across screen changes): one cave at a time,
/// one upload at a time, retried with backoff when there is no signal. State per cave is kept on disk
/// (`CaveSyncState`), so an upload resumes after a relaunch from what the server says it holds.
@MainActor
final class ShotSync: ObservableObject {
    /// Per cave: each shot's state (by shot id), the cave line, and a counter bumped when files arrived.
    @Published private(set) var states: [String: [String: ShotSyncState]] = [:]
    @Published private(set) var summaries: [String: CaveSyncSummary] = [:]
    @Published private(set) var revisions: [String: Int] = [:]
    /// The server has no capture storage (404 on the captures routes): shots stay local, no retries.
    @Published private(set) var serverMissing = false
    /// The last try found no connection.
    @Published private(set) var offline = false

    private weak var account: Account?
    private var api: CaptureAPI?
    private var queue: [String] = []
    private var worker: Task<Void, Never>?
    private var retries: [String: Task<Void, Never>] = [:]
    private var attempts: [String: Int] = [:]
    private var observer: NSObjectProtocol?

    /// The cave line under the saved shots.
    func line(cave: String) -> String {
        var s = (summaries[cave] ?? CaveSyncSummary(shots: 0, toUpload: 0)).line
        if serverMissing { s += " · Sync not available on the server yet" } else if offline { s += " · no connection" }
        return s
    }

    func state(cave: String, shot: String) -> ShotSyncState? { states[cave]?[shot] }

    func attach(_ account: Account) {
        guard self.account !== account else { return }
        self.account = account
        api = CaptureAPI(session: URLSession(configuration: .default)) { path, query in
            try await account.authorized(path, query: query)
        }
        observer = NotificationCenter.default.addObserver(forName: ShotFiles.changed, object: nil, queue: .main) { [weak self] n in
            let cave = n.object as? String
            MainActor.assumeIsolated { if let cave { self?.kick(cave) } }
        }
    }

    /// Syncs every cave with shots on this device (launch, coming back to the app, signing in).
    func syncAll() {
        let root = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0].appendingPathComponent("shots")
        let caves = ((try? FileManager.default.contentsOfDirectory(at: root, includingPropertiesForKeys: nil)) ?? [])
            .filter { $0.hasDirectoryPath }.map(\.lastPathComponent)
        for c in caves { kick(c) }
    }

    /// Syncs one cave as soon as the worker is free.
    func kick(_ cave: String) {
        retries[cave]?.cancel()
        retries[cave] = nil
        if !queue.contains(cave) { queue.append(cave) }
        startIfIdle()
    }

    private func startIfIdle() {
        guard worker == nil, !queue.isEmpty else { return }
        worker = Task { [weak self] in
            await self?.run()
            self?.worker = nil
            self?.startIfIdle()
        }
    }

    /// Shots deleted on this device: deleted on the server too (queued until there is a connection).
    func deleted(cave: String, shotIds: [String]) {
        let url = CaveSyncState.url(cave: cave)
        var st = CaveSyncState.load(from: url)
        for id in shotIds {
            st.shots[id] = nil
            if !st.pendingDeletes.contains(id) { st.pendingDeletes.append(id) }
        }
        try? st.save(to: url)
        Log.write("sync", "delete queued: \(shotIds.count) shot(s)")
        kick(cave)
    }

    private func run() async {
        let bg = UIApplication.shared.beginBackgroundTask(withName: "shot sync")
        defer { if bg != .invalid { UIApplication.shared.endBackgroundTask(bg) } }
        while let cave = queue.first {
            queue.removeFirst()
            let ok = await syncCave(cave)
            if ok { attempts[cave] = 0 } else { retryLater(cave) }
        }
    }

    private func retryLater(_ cave: String) {
        guard !serverMissing, account?.token != nil else { return }
        let n = attempts[cave, default: 0]
        attempts[cave] = n + 1
        let wait = SyncPlan.backoff(attempt: n)
        Log.write("sync", "retry in \(Int(wait)) s (attempt \(n + 1))")
        retries[cave]?.cancel()
        retries[cave] = Task { [weak self] in
            try? await Task.sleep(nanoseconds: UInt64(wait * 1e9))
            guard !Task.isCancelled else { return }
            self?.retries[cave] = nil
            self?.kick(cave)
        }
    }

    private static func short(_ id: String) -> String { String(id.prefix(8)) }

    /// Hashes the cave's shots (only those changed since they were last hashed); drops records of shots gone.
    nonisolated static func hashLocal(cave: String, state: CaveSyncState) -> ([LocalShot], CaveSyncState) {
        var st = state, out: [LocalShot] = []
        let files = ShotFiles.list(cave: cave)
        for u in files {
            let id = ShotFiles.shotId(of: u)
            let attrs = try? FileManager.default.attributesOfItem(atPath: u.path)
            let size = (attrs?[.size] as? NSNumber)?.int64Value ?? 0
            let modified = (attrs?[.modificationDate] as? Date)?.timeIntervalSince1970 ?? 0
            if let r = st.shots[id], r.size == size, r.modified == modified {
                out.append(LocalShot(shotId: id, sha: r.sha, size: size)); continue
            }
            guard let sha = try? ShotHash.file(u) else { continue }
            var r = st.shots[id] ?? ShotRecord(sha: sha, size: size, modified: modified)
            if r.sha != sha { r.uploaded = 0 }
            r.sha = sha; r.size = size; r.modified = modified
            st.shots[id] = r
            out.append(LocalShot(shotId: id, sha: sha, size: size))
        }
        let here = Set(out.map(\.shotId))
        for id in st.shots.keys where !here.contains(id) { st.shots[id] = nil }
        st.pendingDeletes.removeAll { here.contains($0) }
        return (out, st)
    }

    private func publish(cave: String, local: [LocalShot], state st: CaveSyncState, uploading: (String, Double)? = nil) {
        var m: [String: ShotSyncState] = [:]
        var toUpload = 0
        for l in local {
            let r = st.shots[l.shotId]
            if let u = uploading, u.0 == l.shotId { m[l.shotId] = .uploading(u.1); toUpload += 1 }
            else if r?.syncedSha == l.sha { m[l.shotId] = r?.fromServer == true ? .fromServer : .uploaded }
            else if r?.rejectedSha == l.sha { m[l.shotId] = .refused; toUpload += 1 }
            else { m[l.shotId] = .waiting; toUpload += 1 }
        }
        states[cave] = m
        summaries[cave] = CaveSyncSummary(shots: local.count, toUpload: toUpload)
    }

    /// One pass over a cave: deletes, uploads, downloads. False when it stopped for want of a connection.
    private func syncCave(_ cave: String) async -> Bool {
        guard let api, account?.token != nil else { return true }
        let url = CaveSyncState.url(cave: cave)
        var (local, st) = await Task.detached { Self.hashLocal(cave: cave, state: CaveSyncState.load(from: url)) }.value
        publish(cave: cave, local: local, state: st)
        let save = { (s: CaveSyncState) -> Void in _ = try? s.save(to: url) }
        save(st)
        let server: [RemoteShot]
        do {
            server = try await api.list(cave: cave)
        } catch let e as SyncError {
            return stopped(e, cave: cave, what: "list")
        } catch { return stopped(.network, cave: cave, what: "list") }
        if serverMissing { Log.write("sync", "captures available on the server again") }
        serverMissing = false
        offline = false

        // deletes made here
        for id in st.pendingDeletes {
            do {
                try await api.delete(cave: cave, shotId: id)
                st.pendingDeletes.removeAll { $0 == id }
                save(st)
                Log.write("sync", "deleted on the server: \(Self.short(id))")
            } catch let e as SyncError {
                if case .refused(403, _) = e { st.pendingDeletes.removeAll { $0 == id }; save(st); Log.write("sync", "delete refused (403): view-only cave"); continue }
                return stopped(e, cave: cave, what: "delete")
            } catch { return stopped(.network, cave: cave, what: "delete") }
        }

        let d = SyncPlan.decide(local: local, server: server, state: st)
        for (id, sha) in d.inSync { st.shots[id]?.syncedSha = sha }
        save(st)
        publish(cave: cave, local: local, state: st)
        Log.write("sync", "list: \(server.count) on the server, \(local.count) here; \(d.upload.count) to upload, \(d.download.count) to download")

        // uploads, one at a time
        for shot in d.upload {
            guard account?.token != nil else { return true }
            switch await upload(api, cave: cave, shot: shot, state: &st, local: local, save: save) {
            case .done:
                st.shots[shot.shotId]?.syncedSha = shot.sha
                st.shots[shot.shotId]?.rejectedSha = nil
                save(st)
            case .refused(let why):
                st.shots[shot.shotId]?.rejectedSha = shot.sha
                save(st)
                Log.write("sync", "upload refused \(Self.short(shot.shotId)): \(why)")
            case .changed:
                Log.write("sync", "upload \(Self.short(shot.shotId)): file changed while sending; will hash and send again")
                publish(cave: cave, local: local, state: st)
                queue.append(cave)
                return true
            case .stop(let e):
                publish(cave: cave, local: local, state: st)
                return stopped(e, cave: cave, what: "upload")
            }
            publish(cave: cave, local: local, state: st)
        }

        // downloads
        let folder = ShotFiles.folder(cave: cave)
        var arrived = 0
        for r in d.download {
            guard account?.token != nil else { return true }
            let final = folder.appendingPathComponent("\(r.shotId).cvcap"), part = folder.appendingPathComponent("\(r.shotId).cvcap.part")
            let partHave = CaptureAPI.size(part)
            Log.write("sync", "download start \(Self.short(r.shotId)) \(r.size) bytes" + (partHave > 0 ? ", resuming from \(partHave)" : ""))
            do {
                try await api.download(cave: cave, shot: r, part: part, to: final)
            } catch let e as SyncError {
                switch e {
                case .badDownload: Log.write("sync", "download \(Self.short(r.shotId)) failed its SHA-256 check; will start over"); continue
                case .notFound: Log.write("sync", "download \(Self.short(r.shotId)): gone from the server"); continue
                default: if arrived > 0 { bump(cave) }; return stopped(e, cave: cave, what: "download")
                }
            } catch {
                Log.write("sync", "download \(Self.short(r.shotId)) could not be written"); continue
            }
            if r.capturedAt > 0 {
                try? FileManager.default.setAttributes([.creationDate: Date(timeIntervalSince1970: Double(r.capturedAt) / 1000)], ofItemAtPath: final.path)
            }
            let modified = ((try? FileManager.default.attributesOfItem(atPath: final.path))?[.modificationDate] as? Date)?.timeIntervalSince1970 ?? 0
            let was = st.shots[r.shotId]
            st.shots[r.shotId] = ShotRecord(sha: r.sha, size: r.size, modified: modified, syncedSha: r.sha, fromServer: was?.fromServer ?? true)
            for t in ShotFiles.thumbnails(of: final) { try? FileManager.default.removeItem(at: t) }
            save(st)
            if !local.contains(where: { $0.shotId == r.shotId }) { local.append(LocalShot(shotId: r.shotId, sha: r.sha, size: r.size)) }
            else { local = local.map { $0.shotId == r.shotId ? LocalShot(shotId: r.shotId, sha: r.sha, size: r.size) : $0 } }
            arrived += 1
            Log.write("sync", "download done \(Self.short(r.shotId))")
            publish(cave: cave, local: local, state: st)
        }
        if arrived > 0 { bump(cave) }
        return true
    }

    private func bump(_ cave: String) { revisions[cave, default: 0] += 1 }

    /// Logs why a pass stopped; false (retry later) only when there was no connection or the server failed.
    private func stopped(_ e: SyncError, cave: String, what: String) -> Bool {
        switch e {
        case .unavailable, .notFound:
            if !serverMissing { Log.write("sync", "\(what): the server has no capture storage (404); shots stay on this device") }
            serverMissing = true
            return true
        case .unauthorized:
            Log.write("sync", "\(what): 401, signing out")
            account?.signOut()
            return true
        case .refused(403, _):
            Log.write("sync", "\(what): 403 (view-only in this cave); nothing sent")
            return true
        case .refused(let status, let code):
            Log.write("sync", "\(what) failed: \(status) \(code)")
            return false
        case .network, .badDownload:
            offline = true
            Log.write("sync", "\(what): no connection")
            return false
        }
    }

    private enum UploadOutcome { case done, refused(String), changed, stop(SyncError) }

    private func upload(_ api: CaptureAPI, cave: String, shot: LocalShot, state st: inout CaveSyncState, local: [LocalShot],
                        save: (CaveSyncState) -> Void) async -> UploadOutcome {
        let file = ShotFiles.folder(cave: cave).appendingPathComponent("\(shot.shotId).cvcap")
        let id = Self.short(shot.shotId)
        let modified = st.shots[shot.shotId]?.modified
        func map(_ e: Error) -> UploadOutcome {
            guard let e = e as? SyncError else { return .stop(.network) }
            switch e {
            case .refused(413, let c), .refused(422, let c): return .refused("\(c.isEmpty ? "too big" : c)")
            default: Log.write("sync", "upload \(id) failed: \(e)"); return .stop(e)
            }
        }
        var have: Int64
        do { have = try await api.uploadHave(cave: cave, sha: shot.sha) } catch { return map(error) }
        let known = st.shots[shot.shotId]?.uploaded ?? 0
        Log.write("sync", have > 0 ? "upload resume \(id) from \(have) of \(shot.size) (this device had \(known))"
                                   : "upload start \(id) \(shot.size) bytes")
        var conflicts = 0
        while true {
            // a pass rewrote the file meanwhile: what was hashed is not what would be sent
            let now = ((try? FileManager.default.attributesOfItem(atPath: file.path))?[.modificationDate] as? Date)?.timeIntervalSince1970
            if now != modified { return .changed }
            // the server holds every byte but did not keep it (an answer lost mid-check): resend the last chunk
            let r = SyncPlan.nextChunk(have: have, total: shot.size) ?? SyncPlan.nextChunk(have: max(0, shot.size - 1), total: shot.size)!
            let chunk: Data
            do { chunk = try await Task.detached { try Self.read(file, r) }.value } catch { return .changed }
            let answer: PutAnswer
            do { answer = try await api.put(cave: cave, sha: shot.sha, offset: r.lowerBound, total: shot.size, chunk: chunk) }
            catch {
                if case .refused(422, "bad-sha")? = error as? SyncError,
                   await Task.detached(operation: { try? ShotHash.file(file) }).value != shot.sha { return .changed }
                return map(error)
            }
            switch answer {
            case .done:
                Log.write("sync", "upload done \(id)")
                st.shots[shot.shotId]?.uploaded = shot.size
                return .done
            case .have(let n):
                have = n
                st.shots[shot.shotId]?.uploaded = n
                save(st)
                publish(cave: cave, local: local, state: st, uploading: (shot.shotId, Double(n) / Double(max(1, shot.size))))
            case .conflict(let n):
                conflicts += 1
                Log.write("sync", "upload \(id): 409, the server has \(n)")
                if conflicts > 5 { return .stop(.refused(409, "offset")) }
                have = n
            }
        }
    }

    nonisolated static func read(_ url: URL, _ r: Range<Int64>) throws -> Data {
        let h = try FileHandle(forReadingFrom: url)
        defer { try? h.close() }
        try h.seek(toOffset: UInt64(r.lowerBound))
        let d = try h.read(upToCount: Int(r.count)) ?? Data()
        guard d.count == r.count else { throw CocoaError(.fileReadCorruptFile) }
        return d
    }

    /// For `action=status`: server state, queue, and each cave's counts.
    func testStatus() -> String {
        let s = serverMissing ? "server-missing" : offline ? "offline" : "ok"
        let caves = summaries.sorted { $0.key < $1.key }.map { "\(Self.short($0.key)):\($0.value.shots)/\($0.value.toUpload)" }
        return "sync=\(s) running=\(worker != nil ? 1 : 0) queued=\(queue.count) shots/to-upload=[\(caves.joined(separator: " "))]"
    }
}
