import Foundation
import Security

/// One cave's stations as CaveCAD on this phone shares them, offline: a keychain generic password (service
/// `cavecad.stations`, account = the server cave id, group `<team>.app.cavecad.shared`) holding this JSON, written by
/// CaveCAD (`crates/cave-capture/src/shared.rs`). Positions are the adjusted ones, rounded to the millimetre, with
/// the same ids the cave's document uses, so a shot over one is placed exactly as over a kept station.
struct SharedStations: Decodable, Equatable, Sendable {
    let v: Int
    let caveId: String
    let name: String
    /// When the cave last changed in CaveCAD, milliseconds since 1970.
    let updatedAt: Int64
    /// `Station`'s Codable keys are already id, label, x, y, z.
    let stations: [Station]

    var updatedDate: Date { Date(timeIntervalSince1970: Double(updatedAt) / 1000) }

    static func decode(_ data: Data) -> SharedStations? { try? JSONDecoder().decode(SharedStations.self, from: data) }
}

/// Reads CaveCAD's shared stations from the keychain, and merges them into a kept copy's.
enum SharedStore {
    static let service = "cavecad.stations"

    private static func query(_ extra: [String: Any]) -> [String: Any] {
        var q: [String: Any] = [kSecClass as String: kSecClassGenericPassword, kSecAttrService as String: service,
                                kSecReturnData as String: true]
        // unsigned builds know no team prefix: then any item of the service (only CaveCAD writes it)
        if let g = Keychain.sharedGroup { q[kSecAttrAccessGroup as String] = g }
        return q.merging(extra) { _, new in new }
    }

    /// Every cave CaveCAD on this phone shares; unreadable items are left out.
    static func all() -> [SharedStations] {
        var out: CFTypeRef?
        let st = SecItemCopyMatching(query([kSecMatchLimit as String: kSecMatchLimitAll,
                                            kSecReturnAttributes as String: true]) as CFDictionary, &out)
        guard st == errSecSuccess, let items = out as? [[String: Any]] else {
            if st != errSecItemNotFound { Log.write("stations", "shared stations unreadable (keychain \(st))") }
            return []
        }
        return items.compactMap { ($0[kSecValueData as String] as? Data).flatMap(SharedStations.decode) }
    }

    /// The stations CaveCAD on this phone shares for one cave; nil when it shares none.
    static func of(cave: String) -> SharedStations? {
        var out: CFTypeRef?
        let st = SecItemCopyMatching(query([kSecAttrAccount as String: cave, kSecMatchLimit as String: kSecMatchLimitOne]) as CFDictionary, &out)
        guard st == errSecSuccess, let d = out as? Data else {
            if st != errSecItemNotFound { Log.write("stations", "shared stations unreadable (keychain \(st))") }
            return nil
        }
        guard let s = SharedStations.decode(d), s.caveId == cave else {
            Log.write("stations", "shared stations item for this cave did not decode; ignored")
            return nil
        }
        return s
    }

    /// The kept copy's stations with CaveCAD's: for the same id, CaveCAD's when it changed after the copy was kept,
    /// else the kept one; stations only CaveCAD has are appended in its order and their ids returned in `fromCaveCAD`.
    static func merge(kept: [Station], shared: SharedStations?, keptAt: Date) -> (stations: [Station], fromCaveCAD: Set<String>) {
        guard let shared else { return (kept, []) }
        let newer = shared.updatedDate > keptAt
        var byId: [String: Station] = [:]
        for s in shared.stations where !s.id.isEmpty && byId[s.id] == nil { byId[s.id] = s }
        let keptIds = Set(kept.map(\.id))
        var out = kept.map { k in newer ? (byId[k.id] ?? k) : k }
        var from = Set<String>()
        for s in shared.stations where !s.id.isEmpty && !keptIds.contains(s.id) && from.insert(s.id).inserted {
            out.append(s)
        }
        return (out, from)
    }
}

extension KeptCave {
    /// A cave only in CaveCAD on this phone (never downloaded here): an empty copy, so all its stations come from
    /// CaveCAD's shared item. Not written to disk; "Get latest" keeps the real one once the server has it.
    static func onlyInCaveCAD(id: String, name: String) -> KeptCave {
        KeptCave(id: id, name: name, snapshot: nil, entries: [], last: 0, keptAt: .distantPast)
    }

    var isOnlyInCaveCAD: Bool { keptAt == .distantPast && snapshot == nil && entries.isEmpty }
}
