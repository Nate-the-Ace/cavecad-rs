import AppKit
import SwiftUI
import UniformTypeIdentifiers
import WebKit

@main
struct X1FlasherApp: App {
    @StateObject private var model = Model()

    init() {
        // launched as a bare binary (swift run) it still gets a Dock icon and a front window
        NSApplication.shared.setActivationPolicy(.regular)
    }

    var body: some Scene {
        WindowGroup("X1 Flasher", id: "main") {
            MainView()
                .environmentObject(model)
                .frame(minWidth: 560, minHeight: 640)
                .onAppear { model.start(); NSApp.activate(ignoringOtherApps: true) }
        }
        .windowResizability(.contentMinSize)
        .commands {
            CommandGroup(replacing: .newItem) {}
            CommandGroup(after: .help) { RecoveryMenuItem() }
        }
        Window("X1 Recovery Card", id: "recovery") {
            RecoveryView().frame(minWidth: 520, minHeight: 600)
        }
        .defaultSize(width: 760, height: 900)
    }
}

struct RecoveryMenuItem: View {
    @Environment(\.openWindow) private var openWindow
    var body: some View {
        Button("X1 Recovery Card") { openWindow(id: "recovery") }
            .keyboardShortcut("r", modifiers: [.command, .shift])
    }
}

// MARK: - Main window

struct MainView: View {
    @EnvironmentObject var model: Model
    @Environment(\.openWindow) private var openWindow
    @State private var confirming = false
    @State private var importing = false
    @State private var showLog = false

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                DeviceCard()
                FirmwareCard(importing: $importing)
                FlashCard(confirming: $confirming)
                DisclosureGroup("Log", isExpanded: $showLog) {
                    ScrollView {
                        Text(model.log.joined(separator: "\n"))
                            .font(.system(.caption, design: .monospaced))
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .textSelection(.enabled)
                    }
                    .frame(height: 140)
                }
            }
            .padding(20)
        }
        .toolbar {
            ToolbarItem {
                Button { model.refresh() } label: { Label("Check X1", systemImage: "arrow.clockwise") }
                    .help("Look for the X1 on the cable again")
            }
            ToolbarItem {
                Button { openWindow(id: "recovery") } label: { Label("Recovery Card", systemImage: "lifepreserver") }
                    .help("What to do if something goes wrong (⇧⌘R)")
            }
        }
        .fileImporter(isPresented: $importing, allowedContentTypes: [UTType(filenameExtension: "bin") ?? .data]) { r in
            if case .success(let url) = r { model.choose(path: url.path) }
        }
        .onDrop(of: [.fileURL], isTargeted: nil) { items in
            _ = items.first?.loadObject(ofClass: URL.self) { url, _ in
                if let url { Task { @MainActor in model.choose(path: url.path) } }
            }
            return true
        }
        .sheet(isPresented: $confirming) { ConfirmSheet(confirming: $confirming) }
    }
}

struct Card<Content: View>: View {
    let title: String
    let symbol: String
    @ViewBuilder var content: Content
    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Label(title, systemImage: symbol).font(.headline)
            content
        }
        .padding(16)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(RoundedRectangle(cornerRadius: 10).fill(Color(nsColor: .controlBackgroundColor)))
        .overlay(RoundedRectangle(cornerRadius: 10).stroke(Color(nsColor: .separatorColor)))
    }
}

struct Pill: View {
    let text: String
    let color: Color
    var body: some View {
        Text(text)
            .font(.caption.weight(.semibold))
            .padding(.horizontal, 8).padding(.vertical, 3)
            .background(Capsule().fill(color.opacity(0.18)))
            .foregroundStyle(color)
    }
}

// MARK: Device

struct DeviceCard: View {
    @EnvironmentObject var model: Model

    var body: some View {
        Card(title: "X1 on the cable", symbol: "cable.connector") {
            HStack(spacing: 10) {
                switch model.device?.mode {
                case .app?: Pill(text: "Running", color: .green)
                case .boot?: Pill(text: "Boot mode: ready to flash", color: .orange)
                case .none?: Pill(text: "Not answering", color: .secondary)
                case nil: Pill(text: model.ports.isEmpty ? "Not plugged in" : "Looking…", color: .secondary)
                }
                if model.checking { ProgressView().controlSize(.small) }
                Spacer()
                if model.ports.count > 1 {
                    Picker("Port", selection: Binding(get: { model.port ?? "" }, set: { model.port = $0; model.device = nil; model.refresh() })) {
                        ForEach(model.ports, id: \.self) { Text(($0 as NSString).lastPathComponent).tag($0) }
                    }
                    .frame(maxWidth: 220)
                }
            }
            if let d = model.device {
                Grid(alignment: .leading, horizontalSpacing: 16, verticalSpacing: 4) {
                    if d.mode == .app {
                        GridRow { Text("Firmware").foregroundStyle(.secondary); Text(d.version ?? "?").monospacedDigit() }
                        GridRow { Text("Serial").foregroundStyle(.secondary); Text(d.serial.map(String.init) ?? "?") }
                    }
                    if let b = d.bootloader, d.mode != .none {
                        GridRow { Text("Bootloader").foregroundStyle(.secondary); Text(String(format: "0x%02X", b)).monospacedDigit() }
                    }
                    GridRow { Text("Port").foregroundStyle(.secondary); Text(d.port).font(.caption.monospaced()) }
                }
                .font(.callout)
            }
            switch model.device?.mode {
            case .app?:
                Button("Set Clock to This Mac") { model.setClock() }
                    .disabled(model.isFlashing)
                    .help("The X1 keeps local time; shots are stamped with it")
                BootModeHelp(intro: "To flash, power the X1 off (hold CLR), then:")
            case .none?:
                Text("The cable is there but the X1 isn't answering. Wake it with DIST, or put it in boot mode:")
                    .font(.callout).foregroundStyle(.secondary)
                BootModeHelp(intro: nil)
            case .boot?:
                HStack {
                    Text("To leave without flashing, press CLR on the X1, or:").font(.callout).foregroundStyle(.secondary)
                    Button("Power off X1") { model.powerOff() }.disabled(model.isFlashing)
                }
            case nil:
                Text("Plug the X1 into this Mac with a USB data cable and switch it on.")
                    .font(.callout).foregroundStyle(.secondary)
            }
        }
    }
}

struct BootModeHelp: View {
    let intro: String?
    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            if let intro { Text(intro).font(.callout).foregroundStyle(.secondary) }
            HStack(alignment: .center, spacing: 18) {
                Keypad(held: ["<", ">"], tapped: ["DIST"])
                VStack(alignment: .leading, spacing: 4) {
                    Text("1. Hold both arrows, **<** and **>**")
                    Text("2. Keep holding, press **DIST**")
                    Text("3. Let go when it shows **BOOT MODE**")
                }
                .font(.callout)
            }
        }
    }
}

/// The X1's five keys, drawn by label (not by position on the case).
struct Keypad: View {
    var held: Set<String> = []
    var tapped: Set<String> = []
    var body: some View {
        VStack(spacing: 6) {
            HStack(spacing: 6) { key("<"); key("M"); key(">") }
            HStack(spacing: 6) { key("DIST", width: 86); key("CLR") }
        }
        .padding(8)
        .background(RoundedRectangle(cornerRadius: 8).fill(Color(nsColor: .windowBackgroundColor)))
        .accessibilityLabel("Hold \(held.sorted().joined(separator: " and ")), then press \(tapped.sorted().joined(separator: ", "))")
    }

    func key(_ k: String, width: CGFloat = 40) -> some View {
        let isHeld = held.contains(k), isTap = tapped.contains(k)
        return Text(k)
            .font(.system(size: 13, weight: .semibold))
            .frame(width: width, height: 30)
            .foregroundStyle(isHeld ? Color.white : (isTap ? Color.orange : Color.secondary))
            .background(RoundedRectangle(cornerRadius: 7).fill(isHeld ? Color.orange : Color(nsColor: .controlColor)))
            .overlay(
                RoundedRectangle(cornerRadius: 7)
                    .strokeBorder(isTap ? Color.orange : Color(nsColor: .separatorColor),
                                  style: StrokeStyle(lineWidth: isTap ? 2 : 1, dash: isTap ? [4, 3] : [])))
    }
}

// MARK: Firmware

struct FirmwareCard: View {
    @EnvironmentObject var model: Model
    @Binding var importing: Bool

    var body: some View {
        Card(title: "Firmware file", symbol: "doc.zipper") {
            if let fw = model.firmware {
                HStack(alignment: .firstTextBaseline) {
                    Text(fw.version).font(.title2.weight(.semibold)).monospacedDigit()
                    Text(fw.name).foregroundStyle(.secondary).lineLimit(1).truncationMode(.middle)
                }
                Text("Built \(fw.built.formatted(date: .abbreviated, time: .shortened)) · \(fw.bytes.formatted()) bytes · \(fw.packets) packets")
                    .font(.callout).foregroundStyle(.secondary)
                ForEach(fw.warnings, id: \.self) { w in
                    Label(w, systemImage: "exclamationmark.triangle").font(.callout).foregroundStyle(.orange)
                }
                if let d = model.device, d.mode == .app, let v = d.version, v != fw.version {
                    Label("The X1 has \(v). Flashing replaces it with \(fw.version)\(older(fw.version, than: v) ? ", an older version" : "").",
                          systemImage: "arrow.left.arrow.right")
                        .font(.callout)
                        .foregroundStyle(older(fw.version, than: v) ? .orange : .secondary)
                }
            } else if let e = model.firmwareError {
                Label(e, systemImage: "xmark.octagon").foregroundStyle(.red).font(.callout)
            } else {
                Text("Choose a Cavway .bin update file, or drop one on this window.").foregroundStyle(.secondary).font(.callout)
            }
            HStack {
                Button("Choose File…") { importing = true }
                if !Model.library.isEmpty {
                    Menu("Cavway Release") {
                        ForEach(Model.library, id: \.1) { v, path in
                            Button("\(v)\(v.contains("-cc") ? " (CaveCAD scan control)" : v == "1.1.7" ? " (latest stock)" : v == "1.1.4" ? " (no scan mode)" : "")") { model.choose(path: path) }
                        }
                    }
                    .fixedSize()
                    .help("Restore one of Cavway's published releases")
                }
            }
            .disabled(model.isFlashing)
        }
    }

    func older(_ a: String, than b: String) -> Bool {
        let x = a.split(separator: ".").compactMap { Int($0) }, y = b.split(separator: ".").compactMap { Int($0) }
        return x.lexicographicallyPrecedes(y)
    }
}

// MARK: Flash

struct FlashCard: View {
    @EnvironmentObject var model: Model
    @Binding var confirming: Bool

    var body: some View {
        Card(title: "Flash", symbol: "bolt.horizontal") {
            switch model.flash {
            case .running(let sim):
                ProgressView(value: Double(model.done), total: Double(max(model.total, 1)))
                Text(model.phase).font(.callout)
                if !sim {
                    Label("Keep the cable in and the X1 on until it says done.", systemImage: "exclamationmark.triangle")
                        .font(.callout).foregroundStyle(.orange)
                }
            case .succeeded(let sim):
                Label(sim ? "Rehearsal went through cleanly. Nothing was sent to the X1."
                          : "Done: UPGRADE SUCCESS. Press CLR to power off, then start with DIST.",
                      systemImage: "checkmark.seal.fill")
                    .foregroundStyle(.green)
                Button("OK") { model.flash = .idle }
            case .failed(let msg, let untouched):
                Label(msg, systemImage: "xmark.octagon.fill").foregroundStyle(.red)
                Text(untouched ? "The X1's existing firmware was not changed. You can try again."
                               : "Not sure the install finished. If the X1 won't start, enter boot mode and flash again.")
                    .font(.callout).foregroundStyle(.secondary)
                Button("OK") { model.flash = .idle }
            case .idle:
                Text(hint).font(.callout).foregroundStyle(.secondary)
                HStack {
                    Button {
                        confirming = true
                    } label: {
                        Label("Flash X1…", systemImage: "bolt.fill").frame(minWidth: 120)
                    }
                    .buttonStyle(.borderedProminent)
                    .tint(.orange)
                    .controlSize(.large)
                    .disabled(!model.canFlash)
                    Button("Rehearse") { model.startFlash(simulated: true) }
                        .disabled(model.firmware == nil)
                        .help("Run the whole flash against a pretend X1. Nothing is sent.")
                }
            }
        }
    }

    var hint: String {
        if model.firmware == nil { return "Choose a firmware file first." }
        switch model.device?.mode {
        case .boot?: return "Ready. Flashing takes about half a minute."
        case .app?: return "The X1 is running normally. Put it in boot mode to flash."
        default: return "Connect the X1 and put it in boot mode."
        }
    }
}

struct ConfirmSheet: View {
    @EnvironmentObject var model: Model
    @Binding var confirming: Bool
    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Label("Replace the X1's firmware?", systemImage: "bolt.trianglebadge.exclamationmark").font(.title3.weight(.semibold))
            if let fw = model.firmware {
                Text("Installs **\(fw.version)** from \(fw.name).")
            }
            VStack(alignment: .leading, spacing: 6) {
                Label("Shots stored on the X1 are erased. Send or download them first.", systemImage: "trash")
                Label("Keep the cable in and the X1 on until it says done (about 30 s).", systemImage: "cable.connector")
                Label("If anything goes wrong, the bootloader stays intact: you can always flash again.", systemImage: "lifepreserver")
            }
            .font(.callout)
            HStack {
                Spacer()
                Button("Cancel", role: .cancel) { confirming = false }.keyboardShortcut(.cancelAction)
                Button("Flash") {
                    confirming = false
                    model.startFlash(simulated: false)
                }
                .buttonStyle(.borderedProminent).tint(.orange)
            }
        }
        .padding(22)
        .frame(width: 440)
    }
}

// MARK: - Recovery card

struct RecoveryView: View {
    var body: some View {
        if let url = Model.recoveryCard {
            WebView(url: url)
        } else {
            Text("The recovery card is missing from this build.").padding()
        }
    }
}

struct WebView: NSViewRepresentable {
    let url: URL
    func makeNSView(context: Context) -> WKWebView {
        let w = WKWebView()
        w.loadFileURL(url, allowingReadAccessTo: url.deletingLastPathComponent())
        return w
    }
    func updateNSView(_ nsView: WKWebView, context: Context) {}
}
