import CaptureCore
import Foundation
import simd

/// Something about a shot's chaining a person should see (`cc_chains` "flags").
struct ChainFlag: Equatable, Sendable {
    /// `leg_misfit`, `cards_misfit` or `one_point`.
    let kind: String
    /// The other shot's station label.
    let with: String
    var offM: Double?, limitM: Double?, lidarM: Double?, surveyM: Double?

    init(kind: String, with: String, offM: Double? = nil, limitM: Double? = nil, lidarM: Double? = nil, surveyM: Double? = nil) {
        self.kind = kind; self.with = with; self.offM = offM; self.limitM = limitM; self.lidarM = lidarM; self.surveyM = surveyM
    }

    init?(_ d: [String: Any]) {
        guard let k = d["kind"] as? String else { return nil }
        let n = { (key: String) in (d[key] as? NSNumber)?.doubleValue }
        self.init(kind: k, with: d["with"] as? String ?? "?", offM: n("off_m"), limitM: n("limit_m"),
                  lidarM: n("lidar_m"), surveyM: n("survey_m"))
    }

    /// A misfit (shown in orange; that pair is not used).
    var isMisfit: Bool { kind != "one_point" }

    /// What to show, with the numbers.
    func text(own: String) -> String {
        let m = { (v: Double?) in v.map { String(format: "%.2f m", $0) } ?? "?" }
        switch kind {
        case "leg_misfit":
            return "\(own)–\(with) misfit: LiDAR \(m(lidarM)), survey \(m(surveyM)) (off \(m(offM)), limit \(m(limitM)))"
        case "cards_misfit":
            return "cards disagree with \(with): off \(m(offM)), limit \(m(limitM))"
        default:
            return "joined to \(with) by one card and the compass"
        }
    }
}

/// One shot's place after card chaining (`cc_chains`).
struct ChainPlace: Equatable, Sendable {
    /// `chain`, `card+compass`, `compass`, `tap`, `floating`, `waiting` or `unplaced`.
    var source: String
    /// Placed in the cave.
    var placed: Bool
    /// Its chain (index into `ChainResult.chains`).
    var chain: Int?
    /// The six numbers to place it with the old calls; nil when it cannot be drawn.
    var pair: [Double]?
    /// Turn (degrees, counter-clockwise from above) and shift moving the old placement's points, relative to
    /// `pair[0...2]`, to the chain's place.
    var fixYawDeg: Double = 0
    var fixShift: SIMD3<Double> = .zero
    /// The shot's own station point after the fix, relative to `pair[0...2]`.
    var spot: SIMD3<Double>?
    /// Station labels of the neighbouring shots it is joined to.
    var joined: [String] = []
    var label: String = ""
    var flags: [ChainFlag] = []

    init(source: String, placed: Bool, chain: Int? = nil, pair: [Double]? = nil, fixYawDeg: Double = 0,
         fixShift: SIMD3<Double> = .zero, spot: SIMD3<Double>? = nil, joined: [String] = [], label: String = "",
         flags: [ChainFlag] = []) {
        self.source = source; self.placed = placed; self.chain = chain; self.pair = pair; self.fixYawDeg = fixYawDeg
        self.fixShift = fixShift; self.spot = spot; self.joined = joined; self.label = label; self.flags = flags
    }

    init?(_ d: [String: Any]) {
        guard let source = d["source"] as? String else { return nil }
        let v3 = { (a: Any?) -> SIMD3<Double>? in
            guard let a = a as? [NSNumber], a.count == 3 else { return nil }
            return SIMD3(a[0].doubleValue, a[1].doubleValue, a[2].doubleValue)
        }
        let fix = d["fix"] as? [String: Any]
        self.init(source: source, placed: d["placed"] as? Bool ?? false, chain: (d["chain"] as? NSNumber)?.intValue,
                  pair: (d["pair"] as? [NSNumber]).flatMap { $0.count == 6 ? $0.map(\.doubleValue) : nil },
                  fixYawDeg: (fix?["yaw_deg"] as? NSNumber)?.doubleValue ?? 0, fixShift: v3(fix?["shift"]) ?? .zero,
                  spot: v3(d["spot"]), joined: d["joined"] as? [String] ?? [], label: d["label"] as? String ?? "",
                  flags: (d["flags"] as? [[String: Any]] ?? []).compactMap(ChainFlag.init))
    }

    var isChained: Bool { source == "chain" }
    var isFloating: Bool { source == "floating" }
    var misfits: [ChainFlag] { flags.filter(\.isMisfit) }
    /// The fix does nothing.
    var isIdentity: Bool { abs(fixYawDeg) < 1e-9 && fixShift == .zero }

    /// A point relative to the pair's station moved by the fix.
    func apply(_ p: SIMD3<Double>) -> SIMD3<Double> {
        let a = fixYawDeg * .pi / 180, c = cos(a), s = sin(a)
        return SIMD3(c * p.x - s * p.y, s * p.x + c * p.y, p.z) + fixShift
    }

    /// Points (x y z per point, relative to the pair's station) moved by the fix.
    func apply(_ points: [Float]) -> [Float] {
        if isIdentity { return points }
        var out = points
        var i = 0
        while i + 2 < out.count {
            let q = apply(SIMD3(Double(out[i]), Double(out[i + 1]), Double(out[i + 2])))
            out[i] = Float(q.x); out[i + 1] = Float(q.y); out[i + 2] = Float(q.z)
            i += 3
        }
        return out
    }
}

/// A chain of two or more shots.
struct ChainInfo: Equatable, Sendable {
    let k: Int
    /// Indices into the shots given.
    let shots: [Int]
    let labels: [String]
    let placed: Bool

    /// "A7–A9": its first and last station labels in label order.
    var span: String {
        let l = labels.sorted { $0.localizedStandardCompare($1) == .orderedAscending }
        guard let a = l.first, let b = l.last else { return "" }
        return a == b ? a : "\(a)–\(b)"
    }
}

/// Every shot's chaining (`cc_chains`), in the order the shots were given.
struct ChainResult: Equatable, Sendable {
    var places: [ChainPlace?] = []
    var chains: [ChainInfo] = []
    var byChain = 0, flagged = 0

    init(places: [ChainPlace?] = [], chains: [ChainInfo] = [], byChain: Int = 0, flagged: Int = 0) {
        self.places = places; self.chains = chains; self.byChain = byChain; self.flagged = flagged
    }

    init(_ d: [String: Any], count: Int) {
        let shots = d["shots"] as? [[String: Any]] ?? []
        places = (0..<count).map { i in i < shots.count && shots[i]["error"] == nil ? ChainPlace(shots[i]) : nil }
        chains = (d["chains"] as? [[String: Any]] ?? []).compactMap { c in
            guard let k = (c["k"] as? NSNumber)?.intValue else { return nil }
            return ChainInfo(k: k, shots: (c["shots"] as? [NSNumber] ?? []).map(\.intValue),
                             labels: c["labels"] as? [String] ?? [], placed: c["placed"] as? Bool ?? false)
        }
        let counts = d["counts"] as? [String: Any] ?? [:]
        byChain = (counts["by_chain"] as? NSNumber)?.intValue ?? 0
        flagged = (counts["flagged"] as? NSNumber)?.intValue ?? 0
    }

    /// The log line written on every refresh of the saved list.
    var logLine: String {
        "chains: \(places.count) shots, \(chains.count) chains, \(byChain) placed by chain, \(flagged) flagged"
    }

    func info(_ p: ChainPlace?) -> ChainInfo? {
        p?.chain.flatMap { k in chains.first { $0.k == k } }
    }
}

extension Core {
    /// Chains every given shot (`cc_chains`): headers only, the files mapped, nothing copied. `stations` as the
    /// cave has them (waiting ones left out); `legs` (station id pairs, from → to) when known.
    static func chains(stations: [Station], legs: [(String, String)] = [], shots: [URL]) -> ChainResult {
        let st: [[String: Any]] = stations.filter { !$0.isWaiting }
            .map { ["id": $0.id, "label": $0.label, "x": $0.x, "y": $0.y, "z": $0.z] }
        let body: [String: Any] = ["stations": st, "legs": legs.map { [$0.0, $0.1] }]
        guard let j = try? JSONSerialization.data(withJSONObject: body), let text = String(data: j, encoding: .utf8) else {
            return ChainResult()
        }
        // NSData keeps one stable pointer per mapped file for the whole call
        let files: [NSData] = shots.map { (try? NSData(contentsOf: $0, options: .alwaysMapped)) ?? NSData() }
        var ptrs: [UnsafePointer<UInt8>?] = files.map { $0.length > 0 ? $0.bytes.assumingMemoryBound(to: UInt8.self) : nil }
        var lens: [Int] = files.map(\.length)
        let d: Data = text.withCString { t in
            ptrs.withUnsafeMutableBufferPointer { p in lens.withUnsafeMutableBufferPointer { l in
                let b = cc_chains(t, p.baseAddress, l.baseAddress, files.count)
                defer { cc_free(b) }
                guard let bp = b.ptr, b.len > 0 else { return Data() }
                return Data(bytes: bp, count: b.len)
            } }
        }
        withExtendedLifetime(files) {}
        let obj = (try? JSONSerialization.jsonObject(with: d) as? [String: Any]) ?? [:]
        return ChainResult(obj, count: shots.count)
    }
}
