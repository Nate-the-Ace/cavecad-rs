import Foundation

/// Shots kept on the phone: Documents/shots/<caveId>/<shotId>.cvcap. Sign-out leaves them alone.
enum ShotFiles {
    static func folder(cave: String) -> URL {
        let u = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("shots").appendingPathComponent(cave)
        try? FileManager.default.createDirectory(at: u, withIntermediateDirectories: true)
        return u
    }
    static func save(_ bundle: Data, cave: String, shot: String) throws -> URL {
        let u = folder(cave: cave).appendingPathComponent("\(shot).cvcap")
        try bundle.write(to: u, options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])
        return u
    }
    /// Deletes one shot file from the phone. Refuses anything that is not a .cvcap in the shots folder.
    @discardableResult
    static func delete(_ url: URL) -> Bool {
        let root = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("shots").standardizedFileURL.path + "/"
        let path = url.standardizedFileURL.path
        guard url.pathExtension == "cvcap", path.hasPrefix(root) else { return false }
        do { try FileManager.default.removeItem(at: url); return true } catch { return false }
    }

    /// Deletes every shot of the cave from the phone; returns how many went.
    @discardableResult
    static func deleteAll(cave: String) -> Int { list(cave: cave).filter(delete).count }

    /// Bytes the cave's shots take on the phone.
    static func bytes(cave: String) -> Int64 {
        list(cave: cave).reduce(0) { $0 + Int64((try? $1.resourceValues(forKeys: [.fileSizeKey]).fileSize) ?? 0) }
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
