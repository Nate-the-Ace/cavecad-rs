// Swift twin of osmo-probe for macOS: once the Mac has bonded with an Osmo Mobile (it does, as a keyboard, for the
// shutter button), the gimbal stops advertising and btleplug cannot find it. CoreBluetooth can still hand us the link
// the system already holds (retrieveConnectedPeripherals), so this one works either way.
//
//   osmo-probe-swift steps wait:3000 speed:300:0:2000 abs:450:0:20 rel:-450:0:20 get listen:5000
import CoreBluetooth
import Foundation

let FFF0 = CBUUID(string: "FFF0"), FFF4 = CBUUID(string: "FFF4"), FFF5 = CBUUID(string: "FFF5")

// ---- DUML: reflected CRC-8 (0x31, init 0x77) on the header, reflected CRC-16 (0x1021, init 0x3692) over the whole message
let crc8Table: [UInt8] = (0..<256).map { i in
    var c = UInt8(i); for _ in 0..<8 { c = c & 1 != 0 ? (c >> 1) ^ 0x8C : c >> 1 }; return c
}
let crc16Table: [UInt16] = (0..<256).map { i in
    var c = UInt16(i); for _ in 0..<8 { c = c & 1 != 0 ? (c >> 1) ^ 0x8408 : c >> 1 }; return c
}
func crc8(_ d: [UInt8]) -> UInt8 { d.reduce(0x77) { crc8Table[Int($1 ^ $0)] } }
func crc16(_ d: [UInt8]) -> UInt16 { d.reduce(0x3692) { ($0 >> 8) ^ crc16Table[Int((UInt16($1) ^ $0) & 0xFF)] } }

// the OM7 drops a message whose sequence number matches the last one it took (measured 2026-11-16), so a run must
// not start where the last run started
var seq = UInt16.random(in: 2...0xFFF0)
func message(_ set: UInt8, _ cmd: UInt8, _ payload: [UInt8], attr: UInt8 = 0) -> [UInt8] {
    let len = UInt16(13 + payload.count) | (1 << 10)
    var out: [UInt8] = [0x55, UInt8(len & 0xFF), UInt8(len >> 8)]
    out.append(crc8(out))
    var body: [UInt8] = [0x02, 0x04, UInt8(seq & 0xFF), UInt8(seq >> 8), attr, set, cmd] + payload
    seq &+= 1
    let c = crc16(out + body); body += [UInt8(c & 0xFF), UInt8(c >> 8)]
    return out + body
}
func le(_ v: Int) -> [UInt8] { let x = UInt16(bitPattern: Int16(clamping: v)); return [UInt8(x & 0xFF), UInt8(x >> 8)] }
func rotate(_ yaw: Int, _ pitch: Int, roll: Int = 0, mode: UInt8, time: UInt8) -> [UInt8] {
    message(0x04, mode == 0x80 ? 0x0C : 0x14, le(yaw) + le(roll) + le(pitch) + [mode, time])
}
func hex(_ b: [UInt8]) -> String { b.map { String(format: "%02x", $0) }.joined(separator: " ") }

// ---- Cavway X1 (same wire format as crates/cave-instrument/src/cavway.rs): `data:` len payload CR LF; a shot packet is
// acknowledged with its flag | 0x55. Laser goes off after every remote shot, so each shot is laser on (36) + measure (38).
let NUS = CBUUID(string: "6E400001-B5A3-F393-E0A9-E50E24DCCA9E")
let X1_WRITE = CBUUID(string: "6E400002-B5A3-F393-E0A9-E50E24DCCA9E")
let X1_NOTIFY = CBUUID(string: "6E400003-B5A3-F393-E0A9-E50E24DCCA9E")
func x1Frame(_ p: [UInt8]) -> [UInt8] { [0x64, 0x61, 0x74, 0x61, 0x3a, UInt8(p.count)] + p + [0x0d, 0x0a] }
struct X1Shot { let flag: UInt8; let dist, az, incl, roll: Double }
func x1Decode(_ b: [UInt8]) -> X1Shot? {
    guard b.count >= 33, b[0] == 0x01 else { return nil }
    let u16 = { (i: Int) in Double(UInt16(b[i]) | UInt16(b[i + 1]) << 8) }
    let i16 = { (i: Int) in Double(Int16(bitPattern: UInt16(b[i]) | UInt16(b[i + 1]) << 8)) }
    let a = 360.0 / 65535.0
    let dist = Double(UInt32(b[2]) << 16 | UInt32(b[4]) << 8 | UInt32(b[3])) / 1000
    var incl = i16(7) * a
    if abs(incl) > 90 && abs(incl) < 90.01 { incl = incl < 0 ? -90 : 90 }
    return X1Shot(flag: b[1], dist: dist, az: u16(5) * a, incl: incl, roll: u16(9) * a)
}

// ---- parse what the gimbal says (per characteristic; messages may span notifications)
struct Msg { let sender, receiver, attr, set, cmd: UInt8; let seq: UInt16; let payload: [UInt8] }
var buffers: [CBUUID: [UInt8]] = [:]
func parse(_ uuid: CBUUID, _ data: [UInt8]) -> [Msg] {
    var buf = (buffers[uuid] ?? []) + data, out: [Msg] = []
    while true {
        guard let i = buf.firstIndex(of: 0x55) else { buf = []; break }
        buf.removeFirst(i)
        if buf.count < 4 { break }
        let len = Int(UInt16(buf[1]) | UInt16(buf[2]) << 8) & 0x3FF
        if len < 13 || crc8(Array(buf[0..<3])) != buf[3] { buf.removeFirst(); continue }
        if buf.count < len { break }
        let raw = Array(buf[0..<len]); buf.removeFirst(len)
        guard crc16(Array(raw[0..<len - 2])) == UInt16(raw[len - 2]) | UInt16(raw[len - 1]) << 8 else { continue }
        out.append(Msg(sender: raw[4], receiver: raw[5], attr: raw[8], set: raw[9], cmd: raw[10],
                       seq: UInt16(raw[6]) | UInt16(raw[7]) << 8, payload: Array(raw[11..<len - 2])))
    }
    buffers[uuid] = buf
    return out
}

let t0 = Date()
func stamp() -> String { String(format: "+%.2fs", Date().timeIntervalSince(t0)) }
// the 04/57 stream from 0x27 and the 00/f1 heartbeat repeat endlessly: print a payload only when it changes
var lastPayload: [String: [UInt8]] = [:], counts: [String: Int] = [:]
func heard(_ uuid: CBUUID, _ data: Data) {
    let bytes = [UInt8](data)
    let msgs = parse(uuid, bytes)
    if msgs.isEmpty, uuid != FFF4 { print("\(stamp()) \(uuid) raw \(hex(bytes))"); return }
    for m in msgs {
        let key = String(format: "%02x→%02x %02x/%02x", m.sender, m.receiver, m.set, m.cmd)
        counts[key, default: 0] += 1
        if lastPayload[key] != m.payload {
            lastPayload[key] = m.payload
            print("\(stamp()) \(uuid.uuidString.prefix(8)) \(key) a=\(String(format: "%02x", m.attr)) #\(m.seq) [\(hex(m.payload))]")
        }
    }
}

final class Probe: NSObject, CBCentralManagerDelegate, CBPeripheralDelegate {
    var central: CBCentralManager!
    var osmo: CBPeripheral?
    var write: CBCharacteristic?
    var x1: CBPeripheral?
    var x1Write: CBCharacteristic?
    var wantX1 = false, osmoReady = false, x1Ready = false, started = false
    var recentX1: [[UInt8]] = []
    var onShot: ((X1Shot) -> Void)?
    let steps: [String]
    init(steps: [String]) {
        self.steps = steps
        wantX1 = steps.contains { $0.hasPrefix("shoot") || $0.hasPrefix("measure") || $0.hasPrefix("ring") }
        super.init(); central = CBCentralManager(delegate: self, queue: nil)
    }
    func startIfReady() {
        guard !started, osmoReady, !wantX1 || x1Ready else { return }
        started = true
        DispatchQueue.main.asyncAfter(deadline: .now() + 1.0) { self.run(0) }
    }

    func centralManagerDidUpdateState(_ c: CBCentralManager) {
        guard c.state == .poweredOn else { if c.state != .unknown { print("Bluetooth not on: \(c.state.rawValue)") }; return }
        if let p = c.retrieveConnectedPeripherals(withServices: [FFF0]).first {
            print("using the Mac's existing link: \(p.name ?? "?")")
            take(p)
        }
        if wantX1, let p = c.retrieveConnectedPeripherals(withServices: [NUS]).first(where: { ($0.name ?? "").hasPrefix("CavwayX1") }) {
            takeX1(p)
        }
        if osmo == nil || (wantX1 && x1 == nil) {
            print("scanning…")
            c.scanForPeripherals(withServices: nil)
            DispatchQueue.main.asyncAfter(deadline: .now() + 15) {
                if self.osmo == nil { print("no Osmo found"); exit(1) }
                if self.wantX1 && self.x1 == nil { print("no X1 found (awake? not connected to the phone/iPad?)"); exit(1) }
            }
        }
    }
    func centralManager(_ c: CBCentralManager, didDiscover p: CBPeripheral, advertisementData ad: [String: Any], rssi: NSNumber) {
        let name = p.name ?? (ad[CBAdvertisementDataLocalNameKey] as? String) ?? ""
        if osmo == nil, name.uppercased().hasPrefix("OM") || name.uppercased().contains("OSMO") { take(p) }
        if wantX1, x1 == nil, name.hasPrefix("CavwayX1") { takeX1(p) }
        if osmo != nil && (!wantX1 || x1 != nil) { c.stopScan() }
    }
    func take(_ p: CBPeripheral) { osmo = p; p.delegate = self; central.connect(p) }
    func takeX1(_ p: CBPeripheral) { x1 = p; p.delegate = self; central.connect(p) }
    func centralManager(_ c: CBCentralManager, didConnect p: CBPeripheral) { print("connected \(p.name ?? "")"); p.discoverServices(nil) }
    func centralManager(_ c: CBCentralManager, didFailToConnect p: CBPeripheral, error: Error?) { print("connect failed: \(String(describing: error))"); exit(1) }
    func peripheral(_ p: CBPeripheral, didDiscoverServices error: Error?) {
        for s in p.services ?? [] { p.discoverCharacteristics(nil, for: s) }
    }
    var pending = 0
    func peripheral(_ p: CBPeripheral, didDiscoverCharacteristicsFor s: CBService, error: Error?) {
        if p == x1 {
            for ch in s.characteristics ?? [] {
                if ch.uuid == X1_WRITE { x1Write = ch }
                if ch.uuid == X1_NOTIFY { p.setNotifyValue(true, for: ch) }
            }
            if s.uuid == NUS { x1Ready = true; startIfReady() }
            return
        }
        for ch in s.characteristics ?? [] {
            print("  \(s.uuid) \(ch.uuid) props=\(ch.properties.rawValue)")
            if ch.uuid == FFF5 { write = ch }
            if ch.properties.contains(.notify) || ch.properties.contains(.indicate) { p.setNotifyValue(true, for: ch) }
        }
        if s.uuid == FFF0 { osmoReady = true; startIfReady() }
    }
    func peripheral(_ p: CBPeripheral, didUpdateValueFor ch: CBCharacteristic, error: Error?) {
        guard let v = ch.value else { return }
        if p == x1 {
            let b = [UInt8](v)
            guard let shot = x1Decode(b) else { return }
            if let w = x1Write { p.writeValue(Data(x1Frame([shot.flag | 0x55])), for: w, type: .withoutResponse) }
            if recentX1.contains(b) { return }
            recentX1.append(b); if recentX1.count > 8 { recentX1.removeFirst() }
            if let path = shotLog, let h = FileHandle(forWritingAtPath: path) ?? { FileManager.default.createFile(atPath: path, contents: nil); return FileHandle(forWritingAtPath: path) }() {
                h.seekToEndOfFile()
                h.write(String(format: "{\"t\":%.0f,\"d\":%.3f,\"az\":%.2f,\"inc\":%.2f,\"roll\":%.2f,\"flag\":%d}\n", Date().timeIntervalSince1970 * 1000, shot.dist, shot.az, shot.incl, shot.roll, Int(shot.flag)).data(using: .utf8)!)
                h.closeFile()
            }
            // accuracy block (bytes 45-53, cavway.rs parse_errors): first byte ff = none; bits 7..4 clear = absG, absM, dip, angle
            let errs: String = {
                guard b.count >= 54, b[45] != 0xff else { return "" }
                let names = [(0x80, "absG"), (0x40, "absM"), (0x20, "dip"), (0x10, "angle")].filter { Int(b[45]) & $0.0 == 0 }.map { $0.1 }
                return "  ACCURACY " + names.joined(separator: ",") + " [" + hex(Array(b[45..<54])) + "]"
            }()
            print(String(format: "\(stamp()) X1 shot  dist %.3f m  azimuth %.1f  inclination %.1f  roll %.1f  flag %02x", shot.dist, shot.az, shot.incl, shot.roll, shot.flag) + errs)
            if let f = onShot { onShot = nil; f(shot) }
            return
        }
        heard(ch.uuid, v)
    }

    func send(_ b: [UInt8]) {
        guard let p = osmo, let w = write else { return }
        var i = 0
        while i < b.count { p.writeValue(Data(b[i..<min(i + 20, b.count)]), for: w, type: .withoutResponse); i += 20 }
    }


    // MARK: closed-loop LRUD ring (X1 laser out of the clamp's top; FINDINGS.md "Weight, step size and speed")
    // phi = the laser's angle from straight up, positive toward the gimbal's long roll side (B). The X1's own roll
    // figure says which side it is on (~270 on B, ~90 on A); its inclination says how far.
    let RATE = 30.0, SETTLE = 0.3
    var timeGain = 0.87      // move time per degree asked, learned as it goes (short moves run long: 10° → 11.5°)
    func phi(_ s: X1Shot) -> Double { (s.roll > 180 ? 1 : -1) * (90 - s.incl) }
    func wrap(_ d: Double) -> Double { var x = d.truncatingRemainder(dividingBy: 360); if x > 180 { x -= 360 }; if x < -180 { x += 360 }; return x }
    func pause(_ s: Double) async { try? await Task.sleep(nanoseconds: UInt64(max(0, s) * 1e9)) }
    func laserOn() { if let p = x1, let w = x1Write { p.writeValue(Data(x1Frame([0x36])), for: w, type: .withoutResponse) } }
    func turn(yaw: Double = 0, roll: Double = 0, secs: Double) async {
        let end = Date().addingTimeInterval(secs)
        while Date() < end {
            send(rotate(Int((yaw * 10).rounded()), 0, roll: Int((roll * 10).rounded()), mode: 0x80, time: 2))
            await pause(min(0.1, end.timeIntervalSinceNow))
        }
        send(rotate(0, 0, mode: 0x80, time: 1))
    }
    func measure() async -> X1Shot? {
        guard let p = x1, let w = x1Write else { return nil }
        return await withCheckedContinuation { (c: CheckedContinuation<X1Shot?, Never>) in
            var done = false
            onShot = { s in if !done { done = true; c.resume(returning: s) } }
            p.writeValue(Data(x1Frame([0x38])), for: w, type: .withoutResponse)
            DispatchQueue.main.asyncAfter(deadline: .now() + 4) { if !done { done = true; self.onShot = nil; c.resume(returning: nil) } }
        }
    }
    /// Laser on, then the move (the X1 warms up meanwhile), settle, measure; one retry if the X1 stays silent.
    func moveAndShoot(yaw: Double = 0, roll: Double = 0, secs: Double) async -> X1Shot? {
        laserOn()
        if secs > 0 { await turn(yaw: yaw, roll: roll, secs: secs) }
        await pause(SETTLE)
        if let s = await measure() { return s }
        print("\(stamp()) no shot, retrying"); laserOn(); await pause(0.3)
        return await measure()
    }
    var ringShots: [(X1Shot, Double)] = []
    /// Rolls to `target` phi from `from`; returns the shot (and learns the time gain).
    var snapped = false
    func rollTo(_ target: Double, from: Double) async -> (X1Shot, Double)? {
        let asked = target - from
        guard let s = await moveAndShoot(roll: asked >= 0 ? RATE : -RATE, secs: abs(asked) / RATE * timeGain) else { return nil }
        let now = phi(s), got = now - from
        if abs(asked) > 3, got.sign != asked.sign, abs(got) > 15 { snapped = true; print(String(format: "  SNAPPED back to phi %.1f (asked %+.1f): end of this side's reach", now, asked)) }
        if abs(asked) > 3, abs(got) > 2, asked.sign == got.sign { timeGain = max(0.5, min(1.6, timeGain * (0.5 + 0.5 * asked / got))) }
        print(String(format: "  phi %6.1f (asked %+5.1f, got %+5.1f, gain %.2f)  %.2f m", now, asked, got, timeGain, s.dist))
        ringShots.append((s, now))
        return (s, now)
    }
    func ring(step: Double) async {
        // deepest: with the X1's weight the long side snaps back past about phi 130-140 (laser ~45° below level)
        let t0 = Date(), deepest = 130.0
        print("ring: \(step)° steps")
        // look along the passage: pan -90 (the gimbal was set down with its centre facing a wall)
        guard let s0 = await moveAndShoot(yaw: -RATE, secs: 3.0) else { print("ring: X1 silent"); return }
        var at = phi(s0); ringShots.append((s0, at)); print(String(format: "  start phi %.1f  %.2f m", at, s0.dist))
        // side 1: down the long side
        var target = (at / step).rounded(.down) * step + step
        snapped = false
        while target <= deepest && !snapped { if let r = await rollTo(target, from: at) { at = r.1 }; target += step }
        guard let level = ringShots.min(by: { abs($0.1 - 90) < abs($1.1 - 90) }) else { return }
        let az1 = level.0.az
        // back to level, pan 180, steer the pan by the X1's compass
        if let r = await rollTo(90, from: at) { at = r.1 }
        var yawSign = 1.0, err = 0.0
        if let s = await moveAndShoot(yaw: RATE, secs: 6.0) {
            at = phi(s); err = wrap(az1 + 180 - s.az); ringShots.append((s, at))
            print(String(format: "  after pan: azimuth %.1f, want %.1f (off %+.1f)", s.az, wrap(az1 + 180) < 0 ? wrap(az1 + 180) + 360 : wrap(az1 + 180), err))
        }
        for _ in 0..<4 where abs(err) > 4 {
            guard let s = await moveAndShoot(yaw: (err >= 0 ? RATE : -RATE) * yawSign, secs: abs(err) / RATE) else { break }
            let e2 = wrap(az1 + 180 - s.az); at = phi(s); ringShots.append((s, at))
            print(String(format: "  pan fix: azimuth %.1f (off %+.1f)", s.az, e2))
            if abs(e2) > abs(err) { yawSign = -yawSign }
            err = e2
        }
        // side 2: down from level to the deepest, then back up to vertical, shooting both ways
        target = (at / step).rounded(.down) * step + step
        snapped = false
        while target <= deepest && !snapped { if let r = await rollTo(target, from: at) { at = r.1 }; target += step }
        target = 90 - step
        while target >= 0 { if let r = await rollTo(target, from: at) { at = r.1 }; target -= step }
        // home: pan back
        laserOn(); await turn(yaw: -RATE, secs: 3.0)
        print(String(format: "ring done: %d shots in %.0f s (%.1f s a shot), pan off by %.1f°, final gain %.2f", ringShots.count, Date().timeIntervalSince(t0), Date().timeIntervalSince(t0) / Double(max(1, ringShots.count)), err, timeGain))
    }

    func run(_ i: Int) {
        guard i < steps.count else {
            print("--- counts: " + counts.sorted { $0.key < $1.key }.map { "\($0.key) ×\($0.value)" }.joined(separator: ", "))
            if let p = osmo { central.cancelPeripheralConnection(p) }
            if let p = x1 { central.cancelPeripheralConnection(p) }
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) { exit(0) }
            return
        }
        let p = steps[i].split(separator: ":").map(String.init)
        let n = { (k: Int, d: Int) in k < p.count ? Int(p[k]) ?? d : d }
        print("=== \(stamp()) step \(steps[i])")
        let next = { (after: Double) in DispatchQueue.main.asyncAfter(deadline: .now() + after) { self.run(i + 1) } }
        switch p[0] {
        case "wait", "listen": next(Double(n(1, 1000)) / 1000)
        case "get": let m = message(0x04, 0x02, [], attr: 0x40); print("sent \(hex(m))"); send(m); next(3)
        case "abs", "rel":
            let m = rotate(n(1, 0), n(2, 0), mode: p[0] == "abs" ? 0x05 : 0x04, time: UInt8(clamping: n(3, 20)))
            print("sent \(hex(m))"); send(m); next(Double(n(3, 20)) / 10 + 2)
        case "speed":  // speed:<yaw>:<pitch>:<ms>[:<roll>], tenths of a degree per second
            let end = Date().addingTimeInterval(Double(n(3, 500)) / 1000)
            print("sent \(hex(rotate(n(1, 0), n(2, 0), roll: n(4, 0), mode: 0x80, time: 2))) every 100 ms")
            Timer.scheduledTimer(withTimeInterval: 0.1, repeats: true) { t in
                if Date() < end { self.send(rotate(n(1, 0), n(2, 0), roll: n(4, 0), mode: 0x80, time: 2)) }
                else { t.invalidate(); self.send(rotate(0, 0, mode: 0x80, time: 1)); next(0) }
            }
        case "mode": send(message(0x04, 0x4C, [UInt8(n(1, 0)), 0])); next(2)
        case "level":  // roll the laser to level (phi 90, long side), steered by the X1
            Task { @MainActor in
                laserOn(); await pause(1.5)
                if var at = await measure().map({ phi($0) }) {
                    for _ in 0..<3 where abs(at - 90) > 3 { if let r = await rollTo(90, from: at) { at = r.1 } }
                    print(String(format: "level: phi %.1f", at))
                }
                next(0)
            }
        case "ring":  // ring[:<step°>]: closed-loop LRUD ring with the X1 (see ring())
            Task { @MainActor in await self.ring(step: Double(n(1, 10))); next(0) }
        case "laser":  // laser on now (send before a move: the X1 warms up while the gimbal turns)
            guard let p = x1, let w = x1Write else { print("no X1"); next(0); return }
            p.writeValue(Data(x1Frame([0x36])), for: w, type: .withoutResponse); next(0)
        case "measure":  // measure with the laser already on; wait for the shot
            guard let p = x1, let w = x1Write else { print("no X1"); next(0); return }
            var done = false
            onShot = { _ in if !done { done = true; next(0) } }
            p.writeValue(Data(x1Frame([0x38])), for: w, type: .withoutResponse)
            DispatchQueue.main.asyncAfter(deadline: .now() + 4) { if !done { done = true; self.onShot = nil; print("\(stamp()) X1 no shot"); next(0) } }
        case "shoot":  // laser on + measure, wait for the shot (X1 FINDINGS: ~1.45 s, give up at 4 s)
            guard let p = x1, let w = x1Write else { print("no X1"); next(0); return }
            var done = false
            onShot = { _ in if !done { done = true; next(0.3) } }
            p.writeValue(Data(x1Frame([0x36])), for: w, type: .withoutResponse)
            p.writeValue(Data(x1Frame([0x38])), for: w, type: .withoutResponse)
            DispatchQueue.main.asyncAfter(deadline: .now() + 4) { if !done { done = true; self.onShot = nil; print("\(stamp()) X1 no shot"); next(0) } }
        case "raw":  // raw:<set hex>:<cmd hex>:<payload hex>:<attr hex>
            let b = (p.count > 3 ? p[3] : "")
            let payload = stride(from: 0, to: b.count - 1, by: 2).compactMap { UInt8(String(Array(b)[$0...$0 + 1]), radix: 16) }
            let m = message(UInt8(p[1], radix: 16) ?? 4, UInt8(p[2], radix: 16) ?? 0, payload, attr: UInt8(p.count > 4 ? p[4] : "40", radix: 16) ?? 0x40)
            print("sent \(hex(m))"); send(m); next(3)
        default: print("unknown step"); next(0)
        }
    }
}

// osmo-probe-swift [--shots <file.jsonl>] steps …   (--shots: every X1 shot appended as one JSON line, for watch/view.html)
var argv = Array(CommandLine.arguments.dropFirst())
var shotLog: String? = nil
if argv.first == "--shots", argv.count > 1 { shotLog = argv[1]; argv.removeFirst(2) }
if argv.first == "steps" { argv.removeFirst() }
let probe = Probe(steps: argv)
RunLoop.main.run()
