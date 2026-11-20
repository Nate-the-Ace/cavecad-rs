import Foundation

struct CaveInfo: Codable, Identifiable, Hashable { let id: String; let name: String }

/// A downloaded cave, kept on the phone for use underground.
struct KeptCave: Codable {
    let id: String
    let name: String
    let snapshot: String?
    let entries: [String]
    let last: Int
    let keptAt: Date
}

@MainActor
final class CaveStore: ObservableObject {
    @Published var caves: [CaveInfo] = []
    @Published var offline = false
    @Published var problem: String? { didSet { if let problem { Log.write("caves", problem) } } }
    /// Ids of the caves kept on the phone (one file each), so rows need not read the files.
    @Published private(set) var keptIds: Set<String> = []
    private let account: Account

    /// The stream (catch-up then live) waits this long for each read.
    static let streamReadTimeout: TimeInterval = 30

    init(account: Account) {
        self.account = account
        keptIds = Self.keptIdsOnDisk()
    }

    nonisolated static func keptIdsOnDisk() -> Set<String> {
        let files = (try? FileManager.default.contentsOfDirectory(at: folder, includingPropertiesForKeys: nil)) ?? []
        return Set(files.filter { $0.pathExtension == "json" }.map { $0.deletingPathExtension().lastPathComponent })
    }

    /// Removes every kept cave (sign-out). Captured shots live elsewhere and stay.
    nonisolated static func forgetKept() {
        let u = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0].appendingPathComponent("caves")
        try? FileManager.default.removeItem(at: u)
    }

    /// Re-reads the kept set from disk (after a sign-out, or anything else that changed the folder).
    func reloadKept() { keptIds = Self.keptIdsOnDisk() }

    nonisolated static var folder: URL {
        let u = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0].appendingPathComponent("caves")
        try? FileManager.default.createDirectory(at: u, withIntermediateDirectories: true)
        return u
    }

    nonisolated static func parseStream(_ body: String) -> (entries: [String], last: Int, caughtUp: Bool) {
        var entries: [String] = [], last = 0, live = false
        for line in body.split(separator: "\n") where !live {
            guard let o = try? JSONSerialization.jsonObject(with: Data(line.utf8)) as? [String: Any] else { continue }
            switch o["t"] as? String {
            case "entry": if let t = o["locked"] as? String { entries.append(t); last = o["number"] as? Int ?? last }
            case "live": live = true
            default: break
            }
        }
        return (entries, last, live)
    }

    /// `GET /api/caves` → the caves this app can read. Only open caves: a locked cave's history needs keys
    /// this app does not hold. An open cave's name is `label.locked`, the plain text `{"name","summary"}`.
    nonisolated static func parseCaves(_ data: Data) -> [CaveInfo] {
        let list = ((try? JSONSerialization.jsonObject(with: data)) as? [String: Any])?["caves"] as? [[String: Any]] ?? []
        return list.compactMap { c in
            guard let id = c["id"] as? String, c["storage"] as? String == "open" else { return nil }
            let text = (c["label"] as? [String: Any])?["locked"] as? String
            let label = text.flatMap { try? JSONSerialization.jsonObject(with: Data($0.utf8)) as? [String: Any] }
            let name = (label?["name"] as? String).flatMap { $0.isEmpty ? nil : $0 } ?? id
            return CaveInfo(id: id, name: name)
        }
    }

    func kept() -> [KeptCave] {
        let files = (try? FileManager.default.contentsOfDirectory(at: Self.folder, includingPropertiesForKeys: nil)) ?? []
        return files.compactMap { try? JSONDecoder().decode(KeptCave.self, from: Data(contentsOf: $0)) }
    }

    func refresh() async {
        reloadKept()
        do {
            caves = Self.parseCaves(try await account.get("api/caves"))
            offline = false
        } catch {
            offline = true
            caves = kept().map { CaveInfo(id: $0.id, name: $0.name) }
            Log.write("caves", "offline (\((error as? URLError)?.code.rawValue ?? 0)): listing \(caves.count) kept cave(s)")
        }
    }

    /// Reads the cave's stream until it says it is live. Runs off the main actor: a big cave is many lines.
    nonisolated static func readStream(_ req: URLRequest) async throws -> (response: URLResponse, entries: [String], last: Int, caughtUp: Bool) {
        let (bytes, resp) = try await URLSession.shared.bytes(for: req)
        guard (resp as? HTTPURLResponse)?.statusCode == 200 else { return (resp, [], 0, false) }
        var body = ""
        for try await line in bytes.lines {
            body += line + "\n"
            if line.contains(#""t":"live""#) { break }
        }
        let parsed = parseStream(body)
        return (resp, parsed.entries, parsed.last, parsed.caughtUp)
    }

    /// Downloads a cave (snapshot, then every entry until the stream says it is live) and keeps it.
    /// The kept copy is replaced only when the whole download succeeded.
    func download(_ cave: CaveInfo) async throws -> KeptCave {
        let snapData = try await account.get("api/caves/\(cave.id)/snapshot")
        let snap = (try JSONSerialization.jsonObject(with: snapData) as? [String: Any])?["snapshot"] as? [String: Any]
        let snapshot = snap?["locked"] as? String
        let after = snap?["upTo"] as? Int ?? 0
        var req = try await account.authorized("api/caves/\(cave.id)/stream", query: [URLQueryItem(name: "after", value: String(after))])
        req.timeoutInterval = Self.streamReadTimeout
        let streamed = try await Task.detached { try await Self.readStream(req) }.value
        try account.check(streamed.response)
        // A stream that ended before "live" is a partial cave: keep the older copy instead.
        guard streamed.caughtUp else { throw URLError(.networkConnectionLost) }
        let k = KeptCave(id: cave.id, name: cave.name, snapshot: snapshot, entries: streamed.entries,
                         last: max(after, streamed.last), keptAt: Date())
        let data = try JSONEncoder().encode(k)
        try data.write(to: Self.folder.appendingPathComponent("\(cave.id).json"), options: .atomic)
        keptIds.insert(cave.id)
        return k
    }

    func load(_ id: String) -> KeptCave? {
        try? JSONDecoder().decode(KeptCave.self, from: Data(contentsOf: Self.folder.appendingPathComponent("\(id).json")))
    }
}
