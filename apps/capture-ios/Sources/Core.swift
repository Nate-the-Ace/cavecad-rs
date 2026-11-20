import CaptureCore
import Foundation

struct Station: Codable, Hashable, Identifiable {
    let id: String
    let label: String
    let x: Double, y: Double, z: Double
}

struct CoreError: Error, LocalizedError { let message: String; var errorDescription: String? { message } }

/// Swift face of the Rust core (`capture_core.h`). Every buffer is copied out and freed here.
enum Core {
    private static func take(_ b: CcBuf) -> Data {
        defer { cc_free(b) }
        guard let p = b.ptr, b.len > 0 else { return Data() }
        return Data(bytes: p, count: b.len)
    }

    static func stations(snapshot: String?, entries: [String]) throws -> [Station] {
        let entriesJSON = String(data: try JSONSerialization.data(withJSONObject: entries), encoding: .utf8)!
        let data: Data = entriesJSON.withCString { e in
            if let snapshot { return snapshot.withCString { s in take(cc_stations(s, e)) } }
            return take(cc_stations(nil, e))
        }
        if let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any], let m = obj["error"] as? String {
            throw CoreError(message: m)
        }
        return try JSONDecoder().decode([Station].self, from: data)
    }

    /// Station and target as six doubles for the placement calls.
    static func pair(_ over: Station, _ forward: Station) -> [Double] {
        [over.x, over.y, over.z, forward.x, forward.y, forward.z]
    }

    static func encodeShot(header: Data, positions: [Float], normals: [Float], indices: [UInt32], jpegs: Data) -> Data {
        guard let h = String(data: header, encoding: .utf8) else { return Data() }
        return h.withCString { hp in
            positions.withUnsafeBufferPointer { p in normals.withUnsafeBufferPointer { n in
                indices.withUnsafeBufferPointer { i in jpegs.withUnsafeBytes { j in
                    take(cc_shot_encode(hp, p.baseAddress, p.count, n.baseAddress, n.count, i.baseAddress, i.count,
                                        j.bindMemory(to: UInt8.self).baseAddress, j.count))
                } }
            } }
        }
    }

    static func summary(bundle: Data, pair: [Double]) -> [String: Any] {
        let d = bundle.withUnsafeBytes { b in pair.withUnsafeBufferPointer { s in
            take(cc_shot_summary(b.bindMemory(to: UInt8.self).baseAddress, b.count, s.baseAddress))
        } }
        return (try? JSONSerialization.jsonObject(with: d) as? [String: Any]) ?? ["error": "unreadable"]
    }

    static func placedPositions(bundle: Data, pair: [Double]) -> [Float] {
        let d = bundle.withUnsafeBytes { b in pair.withUnsafeBufferPointer { s in
            take(cc_shot_placed_positions(b.bindMemory(to: UInt8.self).baseAddress, b.count, s.baseAddress))
        } }
        return d.withUnsafeBytes { Array($0.bindMemory(to: Float.self)) }
    }
}
