import SwiftUI

/// A saved shot and the station it was taken over (nil when the cave no longer has it, or does not have it
/// yet: see `waitingLabel`); `forward` only for an older heading-tap shot.
struct SavedShot: Identifiable, Sendable {
    let url: URL
    let made: Date?
    let over: Station?
    let forward: Station?
    /// The compass state, for compass shots ("true north ±5°", "compass unsure", …).
    let compassLabel: String?
    /// The six numbers to draw it with (`ShotSummary.viewPair`): its placement, or around its own spot while it
    /// waits for its station; nil when a station it needs is gone.
    var pair: [Double]? = nil
    /// When the shot file was last written (a Resume rewrites it).
    var modified: Date? = nil
    /// The typed label of a shot whose station the cave does not have yet: shown unplaced, never "station gone".
    var waitingLabel: String? = nil
    /// Saved waiting, now placed because the cave carries its label (`over` is that station).
    var fromWaiting = false
    var id: URL { url }

    var title: String {
        if let waitingLabel { return "\(waitingLabel) · waiting for station" }
        guard let over else { return "station gone" }
        return "over \(over.label)" + (forward.map { " → \($0.label)" } ?? "")
    }

    /// Log lines for waiting shots that are placed now, each once (`logged` remembers them by shot id).
    static func newlyPlaced(_ shots: [SavedShot], logged: inout Set<String>) -> [String] {
        shots.compactMap { s in
            guard s.fromWaiting, let o = s.over else { return nil }
            let id = s.url.deletingPathExtension().lastPathComponent
            return logged.insert(id).inserted ? "waiting shot \(id) placed at \(o.label)" : nil
        }
    }
}

/// Waiting shots already logged as placed in this run of the app.
@MainActor private var placedWaitingLogged = Set<String>()

struct StationPickView: View {
    @State var cave: KeptCave
    let store: CaveStore
    @State private var stations: [Station] = []
    @State private var over: Station?
    @State private var problem: String?
    @State private var updating = false
    @State private var saved: [SavedShot] = []
    @State private var savedBytes: Int64 = 0
    @State private var askDeleteAll = false
    /// The capture screen is shown (Capture tapped, or a test command).
    @State private var capturing = false
    /// The "Other station…" box is up, and what is typed in it.
    @State private var askOther = false
    @State private var typed = ""

    init(cave: KeptCave, store: CaveStore) {
        _cave = State(initialValue: cave)
        self.store = store
    }

    var body: some View {
        List {
            Section {
                picker($over)
                Button("Other station…") { typed = ""; askOther = true }
            } footer: {
                if let o = over, o.isWaiting {
                    Text("\(o.label) is not in this copy of the cave yet. The shot is saved waiting for its station and placed once the cave has it.")
                }
            }
            if over != nil {
                // a button driving `capturing` (not a NavigationLink), so a test command takes the same path
                Button { capture() } label: {
                    HStack {
                        Text("Capture").font(.headline).foregroundStyle(.primary)
                        Spacer()
                        Image(systemName: "chevron.right").font(.footnote.weight(.semibold)).foregroundStyle(.tertiary)
                    }
                }
            }
            if !saved.isEmpty {
                // Each shot is shown with its own station, whatever is picked above.
                Section {
                    NavigationLink { OverallView(shots: saved, stations: stations) } label: {
                        Label("Overall", systemImage: "cube.transparent").font(.headline)
                    }
                    ForEach(saved) { s in
                        let made = [s.made?.formatted(date: .abbreviated, time: .shortened), s.compassLabel]
                            .compactMap { $0 }.joined(separator: " · ")
                        if let o = s.over {
                            NavigationLink { ShotView(url: s.url, stations: [o] + (s.forward.map { [$0] } ?? [])) } label: {
                                row(s, made: made)
                            }
                        } else if s.waitingLabel != nil {
                            // no station to place it by: the viewer draws it around its own spot
                            NavigationLink { ShotView(url: s.url, stations: []) } label: { row(s, made: made) }
                        } else {
                            row(s, made: made).foregroundStyle(.secondary)
                        }
                    }
                    .onDelete { rows in
                        let gone = rows.map { saved[$0] }
                        // the Overall row comes first, outside this ForEach: rows here index `saved` directly
                        for s in gone { Log.write("shots", "deleted one shot: \(ShotFiles.delete(s.url) ? "ok" : "failed")") }
                        Task { await loadSaved() }
                    }
                } header: {
                    Text("Saved shots")
                } footer: {
                    Text("Overall shows every shot at its own station; shots waiting for their station are left out until the cave has it. Swipe a shot left to delete it from this phone.")
                }
                Section {
                    Button("Delete all shots of this cave (\(ByteCountFormatter.string(fromByteCount: savedBytes, countStyle: .file)))",
                           role: .destructive) { askDeleteAll = true }
                }
            }
            if let problem { Text(problem).foregroundStyle(.red) }
        }
        .navigationDestination(isPresented: $capturing) {
            if let over { CaptureView(cave: cave, over: over) }
        }
        #if DEBUG
        .onAppear { TestRemote.shared.stationScreen = testCommand }
        .onChange(of: over?.label, initial: true) { TestRemote.shared.station = over?.label }
        .onChange(of: capturing, initial: true) { TestRemote.shared.capturing = capturing }
        #endif
        .navigationTitle(cave.name)
        // when this cave was last downloaded to the phone (the kept offline copy)
        .modifier(Subtitle(text: "Last Synced on: \(cave.keptAt.formatted(.dateTime.month(.twoDigits).day(.twoDigits).year(.twoDigits)))"))
        .confirmationDialog("Delete all \(saved.count) shots of \(cave.name) from this phone?", isPresented: $askDeleteAll,
                            titleVisibility: .visible) {
            Button("Delete all shots", role: .destructive) {
                let caveId = cave.id
                Log.write("shots", "deleted all shots of a cave: \(ShotFiles.deleteAll(cave: caveId))")
                Task { await loadSaved() }
            }
        } message: {
            Text("Shots not yet sent anywhere are lost. This cannot be undone.")
        }
        .alert("Other station", isPresented: $askOther) {
            TextField("Station label", text: $typed)
                .textInputAutocapitalization(.characters).autocorrectionDisabled()
                .onChange(of: typed) { _, t in if t.count > StationLabel.maxLength { typed = String(t.prefix(StationLabel.maxLength)) } }
            Button("Use") { _ = useTyped(typed) }.disabled(StationLabel.clean(typed) == nil)
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("Type the station's label. If this copy of the cave does not have it yet, the shot waits for it and is placed once it arrives.")
        }
        .toolbar {
            if updating { ProgressView() } else { Button("Get latest") { getLatest() } }
        }
        .task(id: cave.keptAt) { await loadStations() }
        .onChange(of: problem) { _, p in if let p { Log.write("stations", p) } }
        .onAppear { if !stations.isEmpty { Task { await loadSaved() } } }
    }

    /// A typed label: the cave's station with that label, else a waiting one. Nil (nothing picked) when the
    /// text is not a usable label.
    private func useTyped(_ text: String) -> Station? {
        guard let s = StationLabel.pick(text, in: stations) else { return nil }
        Log.write("stations", "station typed: \(s.label) (\(s.isWaiting ? "waiting" : "found in cave"))")
        over = s
        return s
    }

    /// Capture: the capture screen over the picked station.
    private func capture() {
        guard over != nil else { return }
        capturing = true
    }

    #if DEBUG
    /// Test commands for this screen (TestRemote): pick a station and Capture; back from the capture screen.
    private func testCommand(_ c: TestCommand) -> TestOutcome {
        switch c {
        case .station(let label, let other):
            if other {
                // the "Other station…" entry: a label the cave lacks is taken as a waiting station
                if stations.isEmpty, problem == nil { return .refused("stations not loaded yet") }
                guard let s = useTyped(label) else { return .refused("not a usable label: \(label)") }
                capture()
                return .ok("over \(s.label)" + (s.isWaiting ? " (waiting for station)" : ""))
            }
            if stations.isEmpty { return .refused(problem.map { "no stations: \($0)" } ?? "stations not loaded yet") }
            guard let s = stations.first(where: { $0.label == label })
                    ?? stations.first(where: { $0.label.localizedCaseInsensitiveCompare(label) == .orderedSame }) else {
                return .refused("no station \(label) (\(stations.count) stations; other=1 takes it as waiting)")
            }
            over = s
            capture()
            return .ok("over \(s.label)")
        case .back:
            capturing = false
            return .ok(nil)
        default:
            return .refused("not a station screen command")
        }
    }
    #endif

    /// Stations come from the Rust core, off the main actor; the result is published back here.
    private func loadStations() async {
        let snapshot = cave.snapshot, entries = cave.entries
        do {
            let s = try await Task.detached { try Core.stations(snapshot: snapshot, entries: entries) }.value
            stations = s
            problem = nil
            // keep picks that still exist in the newer copy; a waiting pick becomes the station once it has come
            if let o = over { over = o.isWaiting ? (StationLabel.find(o.label, in: s) ?? o) : s.first(where: { $0.id == o.id }) }
        } catch { problem = error.localizedDescription }
        await loadSaved()
    }

    /// The cave's saved shots, newest first, each with its own stations (header only: cheap).
    private func loadSaved() async {
        let caveId = cave.id, all = stations
        savedBytes = await Task.detached { ShotFiles.bytes(cave: caveId) }.value
        saved = await Task.detached {
            ShotFiles.list(cave: caveId).map { u in
                let made = try? u.resourceValues(forKeys: [.creationDateKey]).creationDate
                let head = (try? Data(contentsOf: u, options: .alwaysMapped))
                    .map { ShotSummary(Core.summary(bundle: $0, pair: nil)) }
                let found = head?.stations(in: all)
                return SavedShot(url: u, made: made, over: found?.over, forward: found?.forward, compassLabel: head?.compassLabel,
                                 pair: head?.viewPair(in: all), modified: ShotThumbnail.modified(u),
                                 waitingLabel: head?.waitingLabel(in: all), fromWaiting: head?.isWaiting == true && found != nil)
            }
        }.value
        for line in SavedShot.newlyPlaced(saved, logged: &placedWaitingLogged) { Log.write("shots", line) }
    }

    /// Downloads the cave again in the background; the kept copy changes only on full success.
    private func getLatest() {
        updating = true
        Task {
            defer { updating = false }
            do { cave = try await store.download(CaveInfo(id: cave.id, name: cave.name)) }
            catch { problem = "Could not get the latest copy; still using the kept one." }
        }
    }

    private func row(_ s: SavedShot, made: String) -> some View {
        HStack(spacing: 12) {
            ShotThumbView(url: s.url, pair: s.pair, modified: s.modified)
            VStack(alignment: .leading, spacing: 4) {
                Text(s.title)
                if !made.isEmpty { Text(made).font(.caption).foregroundStyle(.secondary) }
            }
        }
    }

    private func picker(_ sel: Binding<Station?>) -> some View {
        Picker("Station", selection: sel) {
            Text("—").tag(Station?.none)
            ForEach(stations) { Text($0.label).tag(Station?.some($0)) }
            if let o = over, o.isWaiting { Text("\(o.label) (waiting)").tag(Station?.some(o)) }
        }
    }
}

/// A small line under the navigation title: the system subtitle on iOS 26 and later, else a small caption at the
/// top of the page.
struct Subtitle: ViewModifier {
    let text: String

    func body(content: Content) -> some View {
        if #available(iOS 26.0, *) {
            content.navigationSubtitle(text)
        } else {
            content.safeAreaInset(edge: .top, spacing: 0) {
                Text(text).font(.caption).foregroundStyle(.secondary).frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 20).padding(.bottom, 4)
            }
        }
    }
}
