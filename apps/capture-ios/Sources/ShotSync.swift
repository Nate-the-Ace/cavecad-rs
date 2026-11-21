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

    enum ListNotFound: Equatable { case serverMissing, caveNotOnServer }

    /// What a 404 on a cave's captures list means. For a cave kept here (downloaded, so the server has it): the
    /// server keeps no captures. For one not kept (only in CaveCAD on this phone, made offline): the server does not
    /// have the cave yet, so its shots wait here and the next pass (opening it, coming back to the app) tries again.
    static func listNotFound(caveKept: Bool) -> ListNotFound { caveKept ? .serverMissing : .caveNotOnServer }

    /// Seconds to wait before retry `attempt` (0-based) after no connection: 5, 10, 20, … at most 5 minutes.
    static func backoff(attempt: Int) -> TimeInterval {
        min(300, 5 * pow(2, Double(min(attempt, 10))))
    }

    /// The chunk to send when the server holds `have` of `total`: the next one, or, when the server holds every
    /// byte but did not keep the file (an answer lost mid-check), the last byte again. Nil for an empty file
    /// (nothing can be sent; the caller refuses it).
    static func chunkToSend(have: Int64, total: Int64) -> Range<Int64>? {
        guard total > 0 else { return nil }
        return nextChunk(have: have, total: total) ?? nextChunk(have: total - 1, total: total)
    }

    /// Free space a download must leave on the phone, so a capture in the cave never runs out of room.
    static let downloadReserve: Int64 = 2_000_000_000

    /// Whether fetching `need` more bytes leaves at least `reserve` free. Unknown free space does not stop it.
    static func downloadFits(need: Int64, free: Int64?, reserve: Int64 = downloadReserve) -> Bool {
        guard let free else { return true }
        return free - max(0, need) >= reserve
    }

    /// A shot file's first bytes: anything else (an empty file, something dropped into the folder through Files)
    /// is not a shot and is neither hashed nor sent.
    static let magic = Data("CVCAP\0".utf8)

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
        while try await !fetchNext(cave: cave, shot: shot, part: part) {}
        guard Self.verified(part, shot: shot) else { throw SyncError.badDownload }
        try Self.install(part, to: final)
    }

    /// Fetches the next range of `shot` onto the end of `part` (made if missing); true when `part` is whole.
    /// The part is only written once a whole range has arrived, so a stopped transfer leaves it resumable.
    func fetchNext(cave: String, shot: RemoteShot, part: URL) async throws -> Bool {
        let fm = FileManager.default
        if !fm.fileExists(atPath: part.path) {
            fm.createFile(atPath: part.path, contents: nil, attributes: [.protectionKey: FileProtectionType.completeUntilFirstUserAuthentication])
        }
        let have = Self.size(part)
        guard let range = SyncPlan.range(have: have, total: shot.size) else { return true }
        var req = try await request("\(base(cave))/\(shot.sha)", [])
        req.timeoutInterval = Self.transferTimeout
        req.setValue(range, forHTTPHeaderField: "Range")
        let (data, status) = try await send(req)
        if status == 416 { return true } // the part already reaches the end
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
        return now >= shot.size
    }

    /// Whether `part` is exactly `shot` (size and SHA-256); a part that is not is deleted, so the next try starts over.
    static func verified(_ part: URL, shot: RemoteShot) -> Bool {
        guard size(part) == shot.size, ShotHash.verify(part, sha: shot.sha) else {
            try? FileManager.default.removeItem(at: part)
            return false
        }
        return true
    }

    /// Moves a verified part into place, replacing an older version of the shot.
    static func install(_ part: URL, to final: URL) throws {
        let fm = FileManager.default
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
///
/// Each cave's state lives in memory here (`caveStates`) and every change goes through `mutate`, which also
/// writes it to disk: a pass and a delete made on the phone while it runs change the same state, so neither
/// overwrites the other's.
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
    /// Caves the server does not have yet (made in CaveCAD on this phone): their shots wait here.
    @Published private(set) var notOnServer: Set<String> = []
    /// iOS ended the background time: the transfer stopped where it was (state saved) and nothing starts again
    /// until the app is opened (`resume`).
    private(set) var paused = false

    private weak var account: Account?
    private var api: CaptureAPI?
    /// Signed in (a token to send).
    private var signedIn: @MainActor () -> Bool = { false }
    /// The server said the sign-in ended (401).
    private var signInEnded: @MainActor () -> Void = {}
    /// Free bytes on the phone's data volume; nil when unknown.
    var freeSpace: () -> Int64? = {
        let home = URL(fileURLWithPath: NSHomeDirectory())
        return (try? home.resourceValues(forKeys: [.volumeAvailableCapacityForImportantUsageKey]))?.volumeAvailableCapacityForImportantUsage
    }
    private var caveStates: [String: CaveSyncState] = [:]
    private var queue: [String] = []
    private var worker: Task<Void, Never>?
    private var retries: [String: Task<Void, Never>] = [:]
    private var attempts: [String: Int] = [:]
    private var observer: NSObjectProtocol?
    /// The background task held while one transfer step runs (a chunk, a range, a hash), and how many steps hold it.
    private var bgTask: UIBackgroundTaskIdentifier = .invalid
    private var bgHolds = 0

    /// The cave line under the saved shots.
    func line(cave: String) -> String {
        var s = (summaries[cave] ?? CaveSyncSummary(shots: 0, toUpload: 0)).line
        if serverMissing { s += " · Sync not available on the server yet" }
        else if notOnServer.contains(cave) { s += " · cave not on the server yet" }
        else if offline { s += " · no connection" }
        return s
    }

    func state(cave: String, shot: String) -> ShotSyncState? { states[cave]?[shot] }

    func attach(_ account: Account) {
        guard self.account !== account else { return }
        self.account = account
        attach(api: CaptureAPI(session: URLSession(configuration: .default)) { path, query in
            try await account.authorized(path, query: query)
        }, signedIn: { [weak account] in account?.token != nil }, signInEnded: { [weak account] in account?.signInEnded() })
        observer = NotificationCenter.default.addObserver(forName: ShotFiles.changed, object: nil, queue: .main) { [weak self] n in
            let cave = n.object as? String
            MainActor.assumeIsolated { if let cave { self?.kick(cave) } }
        }
    }

    /// The server calls and the sign-in it runs under (the account's; tests pass stubs).
    func attach(api: CaptureAPI, signedIn: @escaping @MainActor () -> Bool, signInEnded: @escaping @MainActor () -> Void) {
        self.api = api
        self.signedIn = signedIn
        self.signInEnded = signInEnded
    }

    /// Syncs every cave with shots on this device (launch, coming back to the app, signing in).
    func syncAll() {
        resume()
        let root = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0].appendingPathComponent("shots")
        let caves = ((try? FileManager.default.contentsOfDirectory(at: root, includingPropertiesForKeys: nil)) ?? [])
            .filter { $0.hasDirectoryPath }.map(\.lastPathComponent)
        for c in caves { kick(c) }
    }

    /// The app is in front again: carry on with what the background stop left queued.
    func resume() {
        paused = false
        startIfIdle()
    }

    /// Syncs one cave as soon as the worker is free.
    func kick(_ cave: String) {
        retries[cave]?.cancel()
        retries[cave] = nil
        if !queue.contains(cave) { queue.append(cave) }
        startIfIdle()
    }

    /// No pass running and none waiting to start.
    var idle: Bool { worker == nil }

    private func startIfIdle() {
        guard worker == nil, !paused, !queue.isEmpty else { return }
        worker = Task { [weak self] in
            await self?.run()
            self?.worker = nil
            self?.startIfIdle()
        }
    }

    // MARK: state

    /// The cave's sync state (read from disk the first time).
    private func current(_ cave: String) -> CaveSyncState {
        if let s = caveStates[cave] { return s }
        let s = CaveSyncState.load(from: CaveSyncState.url(cave: cave))
        caveStates[cave] = s
        return s
    }

    /// Changes the cave's state and writes it to disk.
    private func mutate(_ cave: String, _ change: (inout CaveSyncState) -> Void) {
        var s = current(cave)
        change(&s)
        caveStates[cave] = s
        try? s.save(to: CaveSyncState.url(cave: cave))
    }

    private func isDeleted(_ cave: String, _ shotId: String) -> Bool { current(cave).pendingDeletes.contains(shotId) }

    /// Shots deleted on this device: deleted on the server too (queued until there is a connection).
    func deleted(cave: String, shotIds: [String]) {
        mutate(cave) { st in
            for id in shotIds {
                st.shots[id] = nil
                if !st.pendingDeletes.contains(id) { st.pendingDeletes.append(id) }
            }
        }
        Log.write("sync", "delete queued: \(shotIds.count) shot(s)")
        kick(cave)
    }

    // MARK: background time

    /// Runs one transfer step under a background task, so it may finish if the app is sent to the background.
    /// When iOS ends the time, `backgroundTimeRanOut` stops the worker and ends the task at once.
    private func held<T>(_ step: () async throws -> T) async rethrows -> T {
        holdBackground()
        defer { releaseBackground() }
        return try await step()
    }

    private func holdBackground() {
        bgHolds += 1
        guard bgTask == .invalid, !paused else { return }
        bgTask = UIApplication.shared.beginBackgroundTask(withName: "shot sync") { [weak self] in
            MainActor.assumeIsolated { self?.backgroundTimeRanOut() }
        }
    }

    private func releaseBackground() {
        bgHolds = max(0, bgHolds - 1)
        if bgHolds == 0 { endBackgroundTask() }
    }

    private func endBackgroundTask() {
        guard bgTask != .invalid else { return }
        UIApplication.shared.endBackgroundTask(bgTask)
        bgTask = .invalid
    }

    /// iOS's background time is up: stop the transfer in flight (every chunk and range already stored stays
    /// stored, the next pass resumes from it) and give the time back now, before iOS ends the app.
    func backgroundTimeRanOut() {
        Log.write("sync", "background time ran out: stopped, carries on from where it got to when the app is opened")
        paused = true
        worker?.cancel()
        endBackgroundTask()
    }

    // MARK: passes

    private func run() async {
        while !paused, !Task.isCancelled, let cave = queue.first {
            queue.removeFirst()
            let ok = await syncCave(cave)
            if paused || Task.isCancelled {
                if !queue.contains(cave) { queue.insert(cave, at: 0) }
                return
            }
            if ok { attempts[cave] = 0 } else { retryLater(cave) }
        }
    }

    private func retryLater(_ cave: String) {
        guard !serverMissing, signedIn() else { return }
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

    /// Whether the file starts like a shot (`CVCAP\0`); an empty or foreign file does not.
    nonisolated static func looksLikeShot(_ url: URL) -> Bool {
        guard let h = try? FileHandle(forReadingFrom: url) else { return false }
        defer { try? h.close() }
        return (try? h.read(upToCount: SyncPlan.magic.count)) == SyncPlan.magic
    }

    /// Hashes the cave's shots (only those changed since they were last hashed); drops records of shots gone.
    /// Files that are not shots (empty, or not starting `CVCAP\0`) are left out and named in `skipped`: they are
    /// never sent, and a good copy on the server comes down over them.
    nonisolated static func hashLocal(cave: String, state: CaveSyncState) -> (local: [LocalShot], state: CaveSyncState, skipped: [String]) {
        var st = state, out: [LocalShot] = [], skipped: [String] = []
        let files = ShotFiles.list(cave: cave)
        for u in files {
            let id = ShotFiles.shotId(of: u)
            let attrs = try? FileManager.default.attributesOfItem(atPath: u.path)
            let size = (attrs?[.size] as? NSNumber)?.int64Value ?? 0
            let modified = (attrs?[.modificationDate] as? Date)?.timeIntervalSince1970 ?? 0
            guard size > 0, looksLikeShot(u) else { skipped.append(id); continue }
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
        return (out, st, skipped)
    }

    private func publish(cave: String, local: [LocalShot], uploading: (String, Double)? = nil) {
        let st = current(cave)
        var m: [String: ShotSyncState] = [:]
        var toUpload = 0
        let shown = local.filter { !st.pendingDeletes.contains($0.shotId) }
        for l in shown {
            let r = st.shots[l.shotId]
            if let u = uploading, u.0 == l.shotId { m[l.shotId] = .uploading(u.1); toUpload += 1 }
            else if r?.syncedSha == l.sha { m[l.shotId] = r?.fromServer == true ? .fromServer : .uploaded }
            else if r?.rejectedSha == l.sha { m[l.shotId] = .refused; toUpload += 1 }
            else { m[l.shotId] = .waiting; toUpload += 1 }
        }
        states[cave] = m
        summaries[cave] = CaveSyncSummary(shots: shown.count, toUpload: toUpload)
    }

    /// One pass over a cave: deletes, uploads, downloads. False when it stopped for want of a connection.
    private func syncCave(_ cave: String) async -> Bool {
        guard let api, signedIn() else { return true }
        let before = current(cave)
        let hashed = await held { await Task.detached { Self.hashLocal(cave: cave, state: before) }.value }
        var local = hashed.local
        if !hashed.skipped.isEmpty {
            Log.write("sync", "not a shot, left out: \(hashed.skipped.map(Self.short).joined(separator: ", ")) (empty or not a .cvcap)")
        }
        // shots deleted here while hashing: the delete wins over the hash
        let late = current(cave).pendingDeletes.filter { !before.pendingDeletes.contains($0) }
        mutate(cave) { st in
            st = hashed.state
            for id in late {
                st.shots[id] = nil
                if !st.pendingDeletes.contains(id) { st.pendingDeletes.append(id) }
            }
        }
        local.removeAll { late.contains($0.shotId) }
        publish(cave: cave, local: local)
        var server: [RemoteShot]
        do {
            server = try await held { try await api.list(cave: cave) }
        } catch SyncError.unavailable where SyncPlan.listNotFound(caveKept: CaveStore.keptIdsOnDisk().contains(cave)) == .caveNotOnServer {
            if notOnServer.insert(cave).inserted { Log.write("sync", "list: the server does not have this cave yet (only in CaveCAD on this phone); shots wait here") }
            offline = false
            return true
        } catch let e as SyncError {
            return stopped(e, cave: cave, what: "list")
        } catch { return stopped(.network, cave: cave, what: "list") }
        if serverMissing { Log.write("sync", "captures available on the server again") }
        serverMissing = false
        notOnServer.remove(cave)
        offline = false

        // deletes made here
        for id in current(cave).pendingDeletes {
            do {
                try await held { try await api.delete(cave: cave, shotId: id) }
                mutate(cave) { $0.pendingDeletes.removeAll { $0 == id } }
                // the list was taken before this delete: never fetch the shot straight back
                server.removeAll { $0.shotId == id }
                Log.write("sync", "deleted on the server: \(Self.short(id))")
            } catch let e as SyncError {
                if case .refused(403, _) = e {
                    mutate(cave) { $0.pendingDeletes.removeAll { $0 == id } }
                    Log.write("sync", "delete refused (403): view-only cave")
                    continue
                }
                return stopped(e, cave: cave, what: "delete")
            } catch { return stopped(.network, cave: cave, what: "delete") }
        }

        let d = SyncPlan.decide(local: local, server: server, state: current(cave))
        mutate(cave) { st in for (id, sha) in d.inSync { st.shots[id]?.syncedSha = sha } }
        publish(cave: cave, local: local)
        Log.write("sync", "list: \(server.count) on the server, \(local.count) here; \(d.upload.count) to upload, \(d.download.count) to download")

        // uploads, one at a time
        for shot in d.upload {
            guard signedIn(), !Task.isCancelled else { return true }
            if isDeleted(cave, shot.shotId) { continue }
            switch await upload(api, cave: cave, shot: shot, local: local) {
            case .done:
                mutate(cave) { st in
                    st.shots[shot.shotId]?.syncedSha = shot.sha
                    st.shots[shot.shotId]?.rejectedSha = nil
                }
            case .refused(let why):
                mutate(cave) { $0.shots[shot.shotId]?.rejectedSha = shot.sha }
                Log.write("sync", "upload refused \(Self.short(shot.shotId)): \(why)")
            case .changed:
                Log.write("sync", "upload \(Self.short(shot.shotId)): file changed while sending; will hash and send again")
                publish(cave: cave, local: local)
                queue.append(cave)
                return true
            case .stop(let e):
                publish(cave: cave, local: local)
                return stopped(e, cave: cave, what: "upload")
            }
            publish(cave: cave, local: local)
        }

        // downloads
        let folder = ShotFiles.folder(cave: cave)
        var arrived = 0
        defer { if arrived > 0 { bump(cave) } }
        for r in d.download {
            guard signedIn(), !Task.isCancelled else { return true }
            let final = folder.appendingPathComponent("\(r.shotId).cvcap"), part = folder.appendingPathComponent("\(r.shotId).cvcap.part")
            if isDeleted(cave, r.shotId) { try? FileManager.default.removeItem(at: part); continue }
            let partHave = CaptureAPI.size(part)
            guard SyncPlan.downloadFits(need: r.size - partHave, free: freeSpace()) else {
                Log.write("sync", "download skipped \(Self.short(r.shotId)) (\(r.size) bytes): keeps 2 GB free for captures")
                continue
            }
            Log.write("sync", "download start \(Self.short(r.shotId)) \(r.size) bytes" + (partHave > 0 ? ", resuming from \(partHave)" : ""))
            do {
                // one range per background task; a shot deleted here meanwhile stops coming down
                while try await held({ try await api.fetchNext(cave: cave, shot: r, part: part) }) == false {
                    if isDeleted(cave, r.shotId) || Task.isCancelled { break }
                }
            } catch let e as SyncError {
                switch e {
                case .notFound: Log.write("sync", "download \(Self.short(r.shotId)): gone from the server"); continue
                default: return stopped(e, cave: cave, what: "download")
                }
            } catch is URLError {
                return stopped(.network, cave: cave, what: "download")
            } catch {
                if Task.isCancelled { return stopped(.network, cave: cave, what: "download") }
                Log.write("sync", "download \(Self.short(r.shotId)) could not be written: \(error.localizedDescription)"); continue
            }
            if Task.isCancelled { return true }
            if isDeleted(cave, r.shotId) {
                try? FileManager.default.removeItem(at: part)
                Log.write("sync", "download \(Self.short(r.shotId)) dropped: deleted on this phone meanwhile")
                continue
            }
            let good = await held { await Task.detached { CaptureAPI.verified(part, shot: r) }.value }
            guard good else { Log.write("sync", "download \(Self.short(r.shotId)) failed its SHA-256 check; will start over"); continue }
            // checked again on the main actor right before the move, so a delete made meanwhile always wins
            if isDeleted(cave, r.shotId) {
                try? FileManager.default.removeItem(at: part)
                Log.write("sync", "download \(Self.short(r.shotId)) dropped: deleted on this phone meanwhile")
                continue
            }
            do { try CaptureAPI.install(part, to: final) } catch {
                Log.write("sync", "download \(Self.short(r.shotId)) could not be written: \(error.localizedDescription)"); continue
            }
            if r.capturedAt > 0 {
                try? FileManager.default.setAttributes([.creationDate: Date(timeIntervalSince1970: Double(r.capturedAt) / 1000)], ofItemAtPath: final.path)
            }
            let modified = ((try? FileManager.default.attributesOfItem(atPath: final.path))?[.modificationDate] as? Date)?.timeIntervalSince1970 ?? 0
            mutate(cave) { st in
                let was = st.shots[r.shotId]
                st.shots[r.shotId] = ShotRecord(sha: r.sha, size: r.size, modified: modified, syncedSha: r.sha, fromServer: was?.fromServer ?? true)
            }
            for t in ShotFiles.thumbnails(of: final) { try? FileManager.default.removeItem(at: t) }
            if !local.contains(where: { $0.shotId == r.shotId }) { local.append(LocalShot(shotId: r.shotId, sha: r.sha, size: r.size)) }
            else { local = local.map { $0.shotId == r.shotId ? LocalShot(shotId: r.shotId, sha: r.sha, size: r.size) : $0 } }
            arrived += 1
            Log.write("sync", "download done \(Self.short(r.shotId))")
            publish(cave: cave, local: local)
        }
        return true
    }

    private func bump(_ cave: String) { revisions[cave, default: 0] += 1 }

    /// Logs why a pass stopped; false (retry later) only when there was no connection or the server failed.
    private func stopped(_ e: SyncError, cave: String, what: String) -> Bool {
        // stopped by the end of background time, not by the network: resumes when the app is opened
        if paused || Task.isCancelled { return true }
        switch e {
        case .unavailable, .notFound:
            if !serverMissing { Log.write("sync", "\(what): the server has no capture storage (404); shots stay on this device") }
            serverMissing = true
            return true
        case .unauthorized:
            Log.write("sync", "\(what): 401, the sign-in ended (caves kept on the phone stay)")
            signInEnded()
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

    private func upload(_ api: CaptureAPI, cave: String, shot: LocalShot, local: [LocalShot]) async -> UploadOutcome {
        let file = ShotFiles.folder(cave: cave).appendingPathComponent("\(shot.shotId).cvcap")
        let id = Self.short(shot.shotId)
        let modified = current(cave).shots[shot.shotId]?.modified
        func map(_ e: Error) -> UploadOutcome {
            guard let e = e as? SyncError else { return .stop(.network) }
            switch e {
            case .refused(413, let c), .refused(422, let c): return .refused("\(c.isEmpty ? "too big" : c)")
            // a view-only member: this shot cannot go up, the rest of the pass (downloads) still runs
            case .refused(403, let c): return .refused("view-only in this cave (403\(c.isEmpty ? "" : " \(c)"))")
            default: Log.write("sync", "upload \(id) failed: \(e)"); return .stop(e)
            }
        }
        var have: Int64
        do { have = try await held { try await api.uploadHave(cave: cave, sha: shot.sha) } } catch { return map(error) }
        let known = current(cave).shots[shot.shotId]?.uploaded ?? 0
        Log.write("sync", have > 0 ? "upload resume \(id) from \(have) of \(shot.size) (this device had \(known))"
                                   : "upload start \(id) \(shot.size) bytes")
        var conflicts = 0
        while true {
            if Task.isCancelled { return .stop(.network) }
            if isDeleted(cave, shot.shotId) { return .done }
            // a pass rewrote the file meanwhile: what was hashed is not what would be sent
            let now = ((try? FileManager.default.attributesOfItem(atPath: file.path))?[.modificationDate] as? Date)?.timeIntervalSince1970
            if now != modified { return .changed }
            guard let r = SyncPlan.chunkToSend(have: have, total: shot.size) else { return .refused("empty file") }
            let chunk: Data
            do { chunk = try await Task.detached { try Self.read(file, r) }.value } catch { return .changed }
            let answer: PutAnswer
            do { answer = try await held { try await api.put(cave: cave, sha: shot.sha, offset: r.lowerBound, total: shot.size, chunk: chunk) } }
            catch {
                if case .refused(422, "bad-sha")? = error as? SyncError,
                   await Task.detached(operation: { try? ShotHash.file(file) }).value != shot.sha { return .changed }
                return map(error)
            }
            switch answer {
            case .done:
                Log.write("sync", "upload done \(id)")
                mutate(cave) { $0.shots[shot.shotId]?.uploaded = shot.size }
                return .done
            case .have(let n):
                have = n
                mutate(cave) { $0.shots[shot.shotId]?.uploaded = n }
                publish(cave: cave, local: local, uploading: (shot.shotId, Double(n) / Double(max(1, shot.size))))
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
        let s = serverMissing ? "server-missing" : paused ? "paused" : offline ? "offline" : "ok"
        let caves = summaries.sorted { $0.key < $1.key }.map { "\(Self.short($0.key)):\($0.value.shots)/\($0.value.toUpload)" }
        return "sync=\(s) running=\(worker != nil ? 1 : 0) queued=\(queue.count) shots/to-upload=[\(caves.joined(separator: " "))]"
    }
}
