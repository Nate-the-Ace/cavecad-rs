import Foundation

/// Shots kept on the phone: Documents/shots/<caveId>/<shotId>.cvcap, each with its picture beside it
/// (<shotId>.thumb.png, made on demand). Sign-out leaves them alone.
enum ShotFiles {
    /// Posted (object: the cave id) when a shot is saved or rewritten here, so sync sends it.
    static let changed = Notification.Name("app.cavecad.capture.shotChanged")

    /// A shot's id: its file name without `.cvcap` (the header's `shot_id`).
    static func shotId(of url: URL) -> String { url.deletingPathExtension().lastPathComponent }

    static func folder(cave: String) -> URL {
        let u = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("shots").appendingPathComponent(cave)
        try? FileManager.default.createDirectory(at: u, withIntermediateDirectories: true)
        return u
    }
    static func save(_ bundle: Data, cave: String, shot: String) throws -> URL {
        let u = folder(cave: cave).appendingPathComponent("\(shot).cvcap")
        try bundle.write(to: u, options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])
        NotificationCenter.default.post(name: changed, object: cave)
        return u
    }
    /// The shot's picture, beside it: <shotId>.thumb.png; for a shot its chain turns by `yawDeg` (whole degrees),
    /// <shotId>.y<yaw>.thumb.png, so a picture is made again when the chain turns the shot differently.
    static func thumbnail(of shot: URL, yawDeg: Int = 0) -> URL {
        let base = shot.deletingPathExtension()
        return yawDeg == 0 ? base.appendingPathExtension("thumb.png")
            : base.deletingLastPathComponent().appendingPathComponent("\(base.lastPathComponent).y\(yawDeg).thumb.png")
    }

    /// Every picture of the shot (any turn).
    static func thumbnails(of shot: URL) -> [URL] {
        let id = shot.deletingPathExtension().lastPathComponent
        let all = (try? FileManager.default.contentsOfDirectory(at: shot.deletingLastPathComponent(), includingPropertiesForKeys: nil)) ?? []
        return all.filter { u in
            let n = u.lastPathComponent
            return n == "\(id).thumb.png" || (n.hasPrefix("\(id).y") && n.hasSuffix(".thumb.png"))
        }
    }

    /// Deletes one shot file (and its picture) from the phone. Refuses anything that is not a .cvcap in the
    /// shots folder.
    @discardableResult
    static func delete(_ url: URL) -> Bool {
        let root = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("shots").standardizedFileURL.path + "/"
        let path = url.standardizedFileURL.path
        guard url.pathExtension == "cvcap", path.hasPrefix(root) else { return false }
        for t in thumbnails(of: url) { try? FileManager.default.removeItem(at: t) }
        do { try FileManager.default.removeItem(at: url); return true } catch { return false }
    }

    /// Deletes every shot of the cave from the phone (and any picture left without its shot); returns how
    /// many shots went.
    @discardableResult
    static func deleteAll(cave: String) -> Int {
        let n = list(cave: cave).filter(delete).count
        let left = (try? FileManager.default.contentsOfDirectory(at: folder(cave: cave), includingPropertiesForKeys: nil)) ?? []
        for u in left where u.lastPathComponent.hasSuffix(".thumb.png") { try? FileManager.default.removeItem(at: u) }
        return n
    }

    /// Bytes the cave's shots take on the phone.
    static func bytes(cave: String) -> Int64 {
        list(cave: cave).reduce(0) { $0 + Int64((try? $1.resourceValues(forKeys: [.fileSizeKey]).fileSize) ?? 0) }
    }

    /// What a capture over a station adds to.
    enum ParentPick: Equatable {
        /// No shot over the station: a new shot.
        case none
        /// The station's newest shot, readable: passes are added to it.
        case parent(URL)
        /// The station's newest shot cannot be read (`reason`, the core's word: `Truncated`, `NewerFormat(2)`, …):
        /// the capture saves as a new shot and that file is never touched.
        case unreadable(URL, reason: String)
    }

    /// The newest shot of the cave taken over `station`, or nil (none, or the newest is unreadable: see `parentPick`).
    static func parent(cave: String, station: Station) -> URL? {
        if case .parent(let u) = parentPick(cave: cave, station: station) { return u }
        return nil
    }

    /// The station's newest shot and whether it can be read, from each file's header alone (mapped, cheap). A shot
    /// saved waiting for its station counts once `station` (from the cave) carries its label; a waiting station
    /// itself has no parent, so a waiting shot is never added to before it is placed.
    static func parentPick(cave: String, station: Station) -> ParentPick {
        guard !station.isWaiting else { return .none }
        let heads = list(cave: cave).lazy.map { u -> (URL, [String: Any]) in
            guard let d = try? Data(contentsOf: u, options: .alwaysMapped) else { return (u, ["error": "unreadable"]) }
            return (u, Core.summary(bundle: d, pair: nil))
        }
        return pickParent(heads, station: station)
    }

    /// The rule, over header summaries newest first (`cc_shot_summary` with no pair): the first shot over `station`
    /// decides. Readable: it is the parent. Refused by the header check but its header still names the station
    /// (`"unreadable"`): the capture saves as a new shot. A file that does not say whose it is is passed over.
    static func pickParent<S: Sequence>(_ heads: S, station: Station) -> ParentPick where S.Element == (URL, [String: Any]) {
        guard !station.isWaiting else { return .none }
        for (u, sum) in heads {
            if let e = sum["error"] as? String {
                guard let head = sum["unreadable"] as? [String: Any],
                      ShotSummary(head).stations(in: [station])?.over == station else { continue }
                return .unreadable(u, reason: e)
            }
            if ShotSummary(sum).stations(in: [station])?.over == station { return .parent(u) }
        }
        return .none
    }

    /// Writes `data` beside `url`, then swaps it in; the old file stays as it was if anything fails.
    @discardableResult
    static func replace(_ url: URL, with data: Data) -> Bool {
        replace(url) { tmp in
            (try? data.write(to: tmp, options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])) != nil
        }
    }

    /// Has `write` fill a temporary file beside `url` (true: written), then swaps it in with one rename; the old
    /// file stays as it was if anything fails, and no temporary file is left behind.
    @discardableResult
    static func replace(_ url: URL, writing write: (URL) -> Bool) -> Bool {
        let tmp = url.deletingLastPathComponent().appendingPathComponent(".\(UUID().uuidString).tmp")
        defer { try? FileManager.default.removeItem(at: tmp) }
        guard write(tmp) else { return false }
        try? FileManager.default.setAttributes([.protectionKey: FileProtectionType.completeUntilFirstUserAuthentication],
                                               ofItemAtPath: tmp.path)
        do {
            _ = try FileManager.default.replaceItemAt(url, withItemAt: tmp)
            NotificationCenter.default.post(name: changed, object: url.deletingLastPathComponent().lastPathComponent)
            return true
        } catch {
            return false
        }
    }

    /// The cave's shots, newest first.
    static func list(cave: String) -> [URL] {
        let files = ((try? FileManager.default.contentsOfDirectory(at: folder(cave: cave), includingPropertiesForKeys: [.creationDateKey])) ?? [])
            .filter { $0.pathExtension == "cvcap" }
        let made = { (u: URL) in (try? u.resourceValues(forKeys: [.creationDateKey]).creationDate) ?? .distantPast }
        return files.map { ($0, made($0)) }
            .sorted { $0.1 != $1.1 ? $0.1 > $1.1 : $0.0.lastPathComponent < $1.0.lastPathComponent }
            .map(\.0)
    }
}
