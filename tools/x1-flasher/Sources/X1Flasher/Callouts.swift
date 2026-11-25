import SwiftUI

/// One row of the feature list being edited. A stable id keeps SwiftUI's text fields in place while rows move.
struct CalloutRow: Identifiable, Equatable {
    let id = UUID()
    var name: String
}

/// The X1's feature picker list (hold `<` with the laser on): edit it here, then build a firmware file with it and flash.
struct CalloutsCard: View {
    @EnvironmentObject var model: Model
    @Binding var confirming: Bool
    var goToFlash: () -> Void

    @State private var rows: [CalloutRow] = []
    @State private var loadedFor: String?
    @State private var problem: String?
    @State private var building = false

    static let maxNames = 32
    static let maxLength = 12

    var body: some View {
        Card(title: "Feature callouts", symbol: "list.bullet.rectangle") {
            Text("The list the X1 shows when you hold **<** with the laser on. **M** picks one for the next shot; it travels with the shot into CaveCAD.")
                .font(.callout).foregroundStyle(.secondary)
            if model.fileCallouts == nil {
                noList
            } else {
                editor
            }
        }
        .onAppear(perform: load)
        .onChange(of: model.firmware?.path) { _ in load() }
        .onChange(of: model.fileCallouts) { _ in load() }
    }

    // A firmware file without a list: offer the ones that have it.
    var noList: some View {
        VStack(alignment: .leading, spacing: 8) {
            Label(model.firmware == nil ? "Choose a firmware file that has a feature list." : "This firmware has no feature list (it needs 1.1.7-cc7 or later; 1.1.7-cc8 or later can also take a list without a reflash).",
                  systemImage: "info.circle").font(.callout)
            let withList = Model.library.filter { (Model.ccBuild($0.0) ?? 0) >= 7 }
            if withList.isEmpty {
                Text("None found. Build one with tools/x1-patch/build.py, or choose a .bin on the Flash page.").font(.callout).foregroundStyle(.secondary)
            }
            ForEach(withList, id: \.1) { v, path in
                Button("Use \(v)\(v.hasSuffix("m") ? " (with the extra marks)" : "")") { model.choose(path: path) }
            }
        }
    }

    var editor: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Text("\(rows.count) of \(Self.maxNames)").font(.callout.monospacedDigit()).foregroundStyle(.secondary)
                Spacer()
                Button("Reset to the file's list") { reset() }.disabled(model.isFlashing || building)
            }
            List {
                ForEach($rows) { $row in
                    HStack {
                        TextField("Name", text: $row.name)
                            .textFieldStyle(.roundedBorder)
                            .onChange(of: row.name) { new in
                                if new.count > Self.maxLength { row.name = String(new.prefix(Self.maxLength)) }
                            }
                        Text("\(row.name.count)/\(Self.maxLength)").font(.caption.monospacedDigit())
                            .foregroundStyle(row.name.count >= Self.maxLength ? .orange : .secondary).frame(width: 40, alignment: .trailing)
                        if let why = rowProblem(row) {
                            Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(.red).help(why)
                        }
                    }
                }
                .onMove { rows.move(fromOffsets: $0, toOffset: $1) }
                .onDelete { rows.remove(atOffsets: $0) }
            }
            .frame(minHeight: 220, maxHeight: 340)
            .listStyle(.bordered)
            .disabled(model.isFlashing || building)
            HStack {
                Button { rows.append(CalloutRow(name: "")) } label: { Label("Add", systemImage: "plus") }
                    .disabled(rows.count >= Self.maxNames || model.isFlashing)
                Text("Drag to reorder, swipe or ⌫ to delete. Plain letters, digits and punctuation, up to 12 characters, no repeats.")
                    .font(.caption).foregroundStyle(.secondary)
            }
            if let problem { Label(problem, systemImage: "xmark.octagon").font(.callout).foregroundStyle(.red) }
            if let fw = model.firmware, model.builtFromList {
                Label("Ready: \(fw.name). Flash it from here, or from the Flash page.", systemImage: "checkmark.circle").font(.callout).foregroundStyle(.green)
            }
            pushRow
            HStack {
                Button {
                    build(thenFlash: true)
                } label: {
                    Label(buttonTitle, systemImage: "bolt.fill").frame(minWidth: 150)
                }
                .buttonStyle(.borderedProminent).tint(.orange).controlSize(.large)
                .disabled(!listOK || building || model.isFlashing || (model.firmware == nil))
                Button("Build file only") { build(thenFlash: false) }
                    .disabled(!listOK || building || model.isFlashing)
                    .help("Make the firmware file with this list, then check the X1 and flash on the Flash page")
            }
            Text(model.canFlash || model.device?.mode == .boot
                 ? "Flashing replaces the firmware and erases stored shots on the X1."
                 : "Put the X1 in boot mode first (hold < and >, press DIST). The buttons above stay off until it's there.")
                .font(.caption).foregroundStyle(.secondary)
        }
    }

    /// "Update List on X1": straight into the running X1, no reflash.
    var pushRow: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Button {
                    model.push = .idle
                    model.pushList(names: rows.map { $0.name.trimmingCharacters(in: .whitespaces) })
                } label: {
                    Label("Update List on X1", systemImage: "arrow.up.circle.fill").frame(minWidth: 150)
                }
                .buttonStyle(.borderedProminent).controlSize(.large)
                .disabled(!listOK || !model.canPush || model.isFlashing || isPushing)
                .help("Writes the list into the X1 while it runs: no reflash, and the stored shots stay")
                switch model.push {
                case .running(let d, let t):
                    ProgressView(value: Double(d), total: Double(max(t, 1))).frame(width: 120)
                case .ok(let m): Label(m, systemImage: "checkmark.circle.fill").foregroundStyle(.green).font(.callout)
                case .failed(let m): Label(m, systemImage: "xmark.octagon.fill").foregroundStyle(.red).font(.callout)
                case .idle: EmptyView()
                }
            }
            Text(pushHint).font(.caption).foregroundStyle(.secondary)
        }
    }

    var isPushing: Bool { if case .running = model.push { return true } else { return false } }

    var pushHint: String {
        guard let d = model.device else { return "Start the X1 normally (not boot mode) and check it above, to update its list without flashing." }
        if d.mode != .app { return "The X1 is in boot mode: start it normally to update the list without flashing." }
        if !model.canPush { return "This X1 runs \(d.version ?? "?"). Flash 1.1.7-cc8m once; after that the list can be updated here with no flash." }
        return "Updating the list keeps the stored shots. A full flash may put the list back to the one built into the file (not yet confirmed on a real X1)."
    }

    var buttonTitle: String { model.device?.mode == .boot ? "Build and Flash…" : "Build, then Flash…" }

    /// The list is fine to build: at least one name, and no row has a problem.
    var listOK: Bool { !rows.isEmpty && rows.allSatisfy { rowProblem($0) == nil } }

    func rowProblem(_ r: CalloutRow) -> String? {
        let n = r.name
        if n.isEmpty { return "A name is empty" }
        if n.count > Self.maxLength { return "Longer than 12 characters" }
        if !n.unicodeScalars.allSatisfy({ $0.value >= 0x20 && $0.value < 0x7f }) { return "Only plain keyboard characters (no accents or symbols)" }
        if n.hasPrefix(" ") || n.hasSuffix(" ") { return "Starts or ends with a space" }
        if rows.contains(where: { $0.id != r.id && $0.name.lowercased() == n.lowercased() }) { return "Appears twice (names are compared ignoring case)" }
        return nil
    }

    // MARK: actions

    /// Starts the editor from what the last session saved, else from the file's own list.
    func load() {
        guard let names = model.fileCallouts, let path = model.firmware?.path else { rows = []; return }
        if loadedFor == path, !rows.isEmpty { return }
        loadedFor = path
        if let saved = UserDefaults.standard.stringArray(forKey: "calloutList"), !saved.isEmpty, !model.builtFromList {
            rows = saved.map { CalloutRow(name: $0) }
        } else {
            rows = names.map { CalloutRow(name: $0) }
        }
    }

    func reset() {
        rows = (model.fileCallouts ?? []).map { CalloutRow(name: $0) }
        problem = nil
    }

    func build(thenFlash: Bool) {
        guard let base = model.firmware?.path else { return }
        // always build from the unmodified base, never from a previously built "-list" file
        let source = base.hasSuffix("-list.bin") ? Self.baseFor(base) : base
        let names = rows.map { $0.name.trimmingCharacters(in: .whitespaces) }
        building = true
        problem = nil
        Task {
            defer { building = false }
            if let err = await model.buildList(base: source, names: names) {
                problem = err
                return
            }
            UserDefaults.standard.set(names, forKey: "calloutList")
            if thenFlash {
                if model.canFlash { confirming = true } else { goToFlash() }
            }
        }
    }

    /// The base file a "...-list.bin" was built from (same name without "-list", looked up in the library).
    static func baseFor(_ listFile: String) -> String {
        let stem = ((listFile as NSString).lastPathComponent as NSString).deletingPathExtension.replacingOccurrences(of: "-list", with: "")
        return Model.library.first { (($0.1 as NSString).lastPathComponent as NSString).deletingPathExtension == stem }?.1 ?? listFile
    }
}
