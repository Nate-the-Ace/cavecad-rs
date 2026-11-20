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

    /// The core's summary of a shot. With no pair only the header is read: counts and which stations it was
    /// taken from (`station_id`, `target_station_id`), not placed.
    static func summary(bundle: Data, pair: [Double]?) -> [String: Any] {
        let d = bundle.withUnsafeBytes { b in
            let base = b.bindMemory(to: UInt8.self).baseAddress
            guard let pair, pair.count == 6 else { return take(cc_shot_summary(base, b.count, nil)) }
            return pair.withUnsafeBufferPointer { s in take(cc_shot_summary(base, b.count, s.baseAddress)) }
        }
        return (try? JSONSerialization.jsonObject(with: d) as? [String: Any]) ?? ["error": "unreadable"]
    }

    /// Placed mesh points, x y z per vertex, relative to the station over (`pair[0...2]`).
    static func placedPositions(bundle: Data, pair: [Double]) -> [Float] {
        guard pair.count == 6 else { return [] }
        let d = bundle.withUnsafeBytes { b in pair.withUnsafeBufferPointer { s in
            take(cc_shot_placed_positions(b.bindMemory(to: UInt8.self).baseAddress, b.count, s.baseAddress))
        } }
        return floats(d)
    }

    /// Placed points thinned on a `cellM` grid (one per occupied cell, the cell mean), x y z per point,
    /// relative to the station over (`pair[0...2]`). Empty when the shot cannot be placed.
    static func placedPointsThin(bundle: Data, pair: [Double], cellM: Float = 0.05) -> [Float] {
        guard pair.count == 6 else { return [] }
        let d = bundle.withUnsafeBytes { b in pair.withUnsafeBufferPointer { s in
            take(cc_shot_placed_points_thin(b.bindMemory(to: UInt8.self).baseAddress, b.count, s.baseAddress, cellM))
        } }
        return floats(d)
    }

    /// Little-endian float32s. Copied rather than rebound: the bytes carry no alignment promise.
    private static func floats(_ d: Data) -> [Float] {
        var out = [Float](repeating: 0, count: d.count / MemoryLayout<Float>.size)
        _ = out.withUnsafeMutableBytes { d.copyBytes(to: $0) }
        return out.map { Float(bitPattern: UInt32(littleEndian: $0.bitPattern)) }
    }

    /// A shot's ghost: simplified mesh and hole outlines, capture frame.
    struct GhostMesh: Sendable { var positions: [SIMD3<Float>]; var indices: [UInt32]; var outlines: [[SIMD3<Float>]] }

    static func addPass(bundle: Data, pass: Data, positions: [Float], normals: [Float], indices: [UInt32], jpegs: Data) -> Data {
        guard let pj = String(data: pass, encoding: .utf8) else { return Data() }
        return bundle.withUnsafeBytes { b in pj.withCString { pp in
            positions.withUnsafeBufferPointer { p in normals.withUnsafeBufferPointer { n in
                indices.withUnsafeBufferPointer { i in jpegs.withUnsafeBytes { j in
                    take(cc_shot_add_pass(b.bindMemory(to: UInt8.self).baseAddress, b.count, pp,
                                          p.baseAddress, p.count, n.baseAddress, n.count, i.baseAddress, i.count,
                                          j.bindMemory(to: UInt8.self).baseAddress, j.count))
                } }
            } }
        } }
    }

    /// The ghost of a shot (`cc_shot_ghost`), parsed in one bounds-checked pass over the bytes; nil
    /// when the core gave nothing or the counts do not add up.
    static func ghost(bundle: Data, cellM: Float = 0.05, minOutlineM: Float = 0.15) -> GhostMesh? {
        let d = bundle.withUnsafeBytes { b in take(cc_shot_ghost(b.bindMemory(to: UInt8.self).baseAddress, b.count, cellM, minOutlineM)) }
        guard d.count >= 12 else { return nil }
        return d.withUnsafeBytes { raw -> GhostMesh? in
            var at = 0
            func u32() -> UInt32? {
                guard raw.count - at >= 4 else { return nil }
                defer { at += 4 }
                return UInt32(littleEndian: raw.loadUnaligned(fromByteOffset: at, as: UInt32.self))
            }
            /// `count` words ahead, or nil when fewer remain (checked before any allocation).
            func words(_ count: Int) -> Bool { count >= 0 && count <= (raw.count - at) / 4 }
            guard let nf = u32(), let ni = u32(), let no = u32(), nf % 3 == 0, words(Int(nf)) else { return nil }
            var g = GhostMesh(positions: [], indices: [], outlines: [])
            g.positions.reserveCapacity(Int(nf) / 3)
            for _ in 0..<(Int(nf) / 3) {
                g.positions.append(SIMD3(Float(bitPattern: u32()!), Float(bitPattern: u32()!), Float(bitPattern: u32()!)))
            }
            guard words(Int(ni)) else { return nil }
            g.indices.reserveCapacity(Int(ni))
            for _ in 0..<Int(ni) { g.indices.append(u32()!) }
            for _ in 0..<Int(no) {
                guard let n = u32(), words(Int(n) * 3) else { return nil }
                var line: [SIMD3<Float>] = []
                line.reserveCapacity(Int(n))
                for _ in 0..<Int(n) {
                    line.append(SIMD3(Float(bitPattern: u32()!), Float(bitPattern: u32()!), Float(bitPattern: u32()!)))
                }
                g.outlines.append(line)
            }
            return g
        }
    }

    /// The core's card comparison (`cc_cards_fit`) as parsed JSON.
    static func cardsFit(bundle: Data, cards: [CardSeenRecord], sameSession: Bool) -> [String: Any] {
        guard let j = try? JSONSerialization.data(withJSONObject: cards.map(\.json)), let t = String(data: j, encoding: .utf8) else { return ["error": "cards"] }
        let d = bundle.withUnsafeBytes { b in t.withCString { c in
            take(cc_cards_fit(b.bindMemory(to: UInt8.self).baseAddress, b.count, c, sameSession ? 1 : 0))
        } }
        return (try? JSONSerialization.jsonObject(with: d) as? [String: Any]) ?? ["error": "unreadable"]
    }

    // MARK: gimbal (cc_gimbal_*: one runner in the core; these return at once, poll gimbalStatus)

    /// Looks for the gimbal for `timeoutMs` and connects.
    static func gimbalConnect(timeoutMs: UInt32 = 15000) { cc_gimbal_connect(timeoutMs) }
    /// Queues a plan (lists of `["yaw","roll","pitch","seconds"]` targets). nil on success, else the core's error text.
    static func gimbalRun(_ lists: [[[String: Double]]]) -> String? {
        guard let d = try? JSONSerialization.data(withJSONObject: lists), let t = String(data: d, encoding: .utf8) else { return "plan" }
        let out = t.withCString { take(cc_gimbal_run($0)) }
        let j = (try? JSONSerialization.jsonObject(with: out) as? [String: Any]) ?? [:]
        return j["ok"] as? Bool == true ? nil : (j["error"] as? String ?? "gimbal")
    }
    static func gimbalHold() { cc_gimbal_hold() }
    static func gimbalGoOn() { cc_gimbal_go_on() }
    static func gimbalStopHome() { cc_gimbal_stop_home() }
    static func gimbalDisconnect() { cc_gimbal_disconnect() }
    /// `{"connected","state","step","steps","mount_ok","joints","error"}` (see capture_core.h).
    static func gimbalStatus() -> [String: Any] {
        (try? JSONSerialization.jsonObject(with: take(cc_gimbal_status())) as? [String: Any]) ?? [:]
    }
}
