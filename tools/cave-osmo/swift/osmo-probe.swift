// Swift twin of osmo-probe for macOS: once the Mac has bonded with an Osmo Mobile (it does, as a keyboard, for the
// shutter button), the gimbal stops advertising and btleplug cannot find it. CoreBluetooth can still hand us the link
// the system already holds (retrieveConnectedPeripherals), so this one works either way.
//
//   osmo-probe-swift steps wait:3000 speed:300:0:2000 abs:450:0:20 rel:-450:0:20 get listen:5000
import CoreBluetooth
import Foundation

let FFF0 = CBUUID(string: "FFF0"), FFF4 = CBUUID(string: "FFF4"), FFF5 = CBUUID(string: "FFF5")

// ---- DUML: reflected CRC-8 (0x31, init 0x77) on the header, reflected CRC-16 (0x1021, init 0xDF0C) on the rest
let crc8Table: [UInt8] = (0..<256).map { i in
    var c = UInt8(i); for _ in 0..<8 { c = c & 1 != 0 ? (c >> 1) ^ 0x8C : c >> 1 }; return c
}
let crc16Table: [UInt16] = (0..<256).map { i in
    var c = UInt16(i); for _ in 0..<8 { c = c & 1 != 0 ? (c >> 1) ^ 0x8408 : c >> 1 }; return c
}
func crc8(_ d: [UInt8]) -> UInt8 { d.reduce(0x77) { crc8Table[Int($1 ^ $0)] } }
func crc16(_ d: [UInt8]) -> UInt16 { d.reduce(0xDF0C) { ($0 >> 8) ^ crc16Table[Int((UInt16($1) ^ $0) & 0xFF)] } }

// the OM7 drops a message whose sequence number matches the last one it took (measured 2026-11-16), so a run must
// not start where the last run started
var seq = UInt16.random(in: 2...0xFFF0)
func message(_ set: UInt8, _ cmd: UInt8, _ payload: [UInt8], attr: UInt8 = 0) -> [UInt8] {
    let len = UInt16(13 + payload.count) | (1 << 10)
    var out: [UInt8] = [0x55, UInt8(len & 0xFF), UInt8(len >> 8)]
    out.append(crc8(out))
    var body: [UInt8] = [0x02, 0x04, UInt8(seq & 0xFF), UInt8(seq >> 8), attr, set, cmd] + payload
    seq &+= 1
    let c = crc16(body); body += [UInt8(c & 0xFF), UInt8(c >> 8)]
    return out + body
}
func le(_ v: Int) -> [UInt8] { let x = UInt16(bitPattern: Int16(clamping: v)); return [UInt8(x & 0xFF), UInt8(x >> 8)] }
func rotate(_ yaw: Int, _ pitch: Int, roll: Int = 0, mode: UInt8, time: UInt8) -> [UInt8] {
    message(0x04, mode == 0x80 ? 0x0C : 0x14, le(yaw) + le(roll) + le(pitch) + [mode, time])
}
func hex(_ b: [UInt8]) -> String { b.map { String(format: "%02x", $0) }.joined(separator: " ") }

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
        guard crc16(Array(raw[4..<len - 2])) == UInt16(raw[len - 2]) | UInt16(raw[len - 1]) << 8 else { continue }
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
    let steps: [String]
    init(steps: [String]) { self.steps = steps; super.init(); central = CBCentralManager(delegate: self, queue: nil) }

    func centralManagerDidUpdateState(_ c: CBCentralManager) {
        guard c.state == .poweredOn else { if c.state != .unknown { print("Bluetooth not on: \(c.state.rawValue)") }; return }
        if let p = c.retrieveConnectedPeripherals(withServices: [FFF0]).first {
            print("using the Mac's existing link: \(p.name ?? "?")")
            take(p)
        } else {
            print("scanning…")
            c.scanForPeripherals(withServices: nil)
            DispatchQueue.main.asyncAfter(deadline: .now() + 12) { if self.osmo == nil { print("no Osmo found"); exit(1) } }
        }
    }
    func centralManager(_ c: CBCentralManager, didDiscover p: CBPeripheral, advertisementData ad: [String: Any], rssi: NSNumber) {
        let name = p.name ?? (ad[CBAdvertisementDataLocalNameKey] as? String) ?? ""
        if name.uppercased().hasPrefix("OM") || name.uppercased().contains("OSMO") { c.stopScan(); take(p) }
    }
    func take(_ p: CBPeripheral) { osmo = p; p.delegate = self; central.connect(p) }
    func centralManager(_ c: CBCentralManager, didConnect p: CBPeripheral) { print("connected \(p.name ?? "")"); p.discoverServices(nil) }
    func centralManager(_ c: CBCentralManager, didFailToConnect p: CBPeripheral, error: Error?) { print("connect failed: \(String(describing: error))"); exit(1) }
    func peripheral(_ p: CBPeripheral, didDiscoverServices error: Error?) {
        for s in p.services ?? [] { p.discoverCharacteristics(nil, for: s) }
    }
    var pending = 0
    func peripheral(_ p: CBPeripheral, didDiscoverCharacteristicsFor s: CBService, error: Error?) {
        for ch in s.characteristics ?? [] {
            print("  \(s.uuid) \(ch.uuid) props=\(ch.properties.rawValue)")
            if ch.uuid == FFF5 { write = ch }
            if ch.properties.contains(.notify) || ch.properties.contains(.indicate) { p.setNotifyValue(true, for: ch) }
        }
        if s.uuid == FFF0 { DispatchQueue.main.asyncAfter(deadline: .now() + 1.5) { self.run(0) } }
    }
    func peripheral(_ p: CBPeripheral, didUpdateValueFor ch: CBCharacteristic, error: Error?) {
        if let v = ch.value { heard(ch.uuid, v) }
    }

    func send(_ b: [UInt8]) {
        guard let p = osmo, let w = write else { return }
        var i = 0
        while i < b.count { p.writeValue(Data(b[i..<min(i + 20, b.count)]), for: w, type: .withoutResponse); i += 20 }
    }

    func run(_ i: Int) {
        guard i < steps.count else {
            print("--- counts: " + counts.sorted { $0.key < $1.key }.map { "\($0.key) ×\($0.value)" }.joined(separator: ", "))
            if let p = osmo { central.cancelPeripheralConnection(p) }
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
                else { t.invalidate(); self.send(rotate(0, 0, mode: 0x80, time: 1)); next(2) }
            }
        case "mode": send(message(0x04, 0x4C, [UInt8(n(1, 0)), 0])); next(2)
        case "raw":  // raw:<set hex>:<cmd hex>:<payload hex>:<attr hex>
            let b = (p.count > 3 ? p[3] : "")
            let payload = stride(from: 0, to: b.count - 1, by: 2).compactMap { UInt8(String(Array(b)[$0...$0 + 1]), radix: 16) }
            let m = message(UInt8(p[1], radix: 16) ?? 4, UInt8(p[2], radix: 16) ?? 0, payload, attr: UInt8(p.count > 4 ? p[4] : "40", radix: 16) ?? 0x40)
            print("sent \(hex(m))"); send(m); next(3)
        default: print("unknown step"); next(0)
        }
    }
}

let probe = Probe(steps: Array(CommandLine.arguments.dropFirst(CommandLine.arguments.count > 1 && CommandLine.arguments[1] == "steps" ? 2 : 1)))
RunLoop.main.run()
