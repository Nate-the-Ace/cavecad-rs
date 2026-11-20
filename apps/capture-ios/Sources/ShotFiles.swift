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
