import SwiftUI

/// A saved shot and the two stations it was taken from (nil when the cave no longer has one of them).
struct SavedShot: Identifiable, Sendable {
    let url: URL
    let made: Date?
    let over: Station?
    let forward: Station?
    var id: URL { url }
}

struct StationPickView: View {
    @State var cave: KeptCave
    let store: CaveStore
    @State private var stations: [Station] = []
    @State private var over: Station?
    @State private var forward: Station?
    @State private var problem: String?
    @State private var updating = false
    @State private var saved: [SavedShot] = []
    @AppStorage(CaptureSettings.heightKey) private var height = CaptureSettings.defaultHeight

    init(cave: KeptCave, store: CaveStore) {
        _cave = State(initialValue: cave)
        self.store = store
    }

    var body: some View {
        List {
            Section("The rig stands over") { picker($over) }
            Section("Forward station (you will aim at it)") { picker($forward) }
            Section("Phone height above the station mark") {
                HStack {
                    TextField("Height", value: $height, format: .number.precision(.fractionLength(0...2)))
                        .keyboardType(.decimalPad)
                    Text("m").foregroundStyle(.secondary)
                    Stepper("", value: $height, in: 0...3, step: 0.05).labelsHidden()
                }
            }
            if let over, let forward, over != forward {
                NavigationLink("Capture") {
                    CaptureView(cave: cave, over: over, forward: forward, height: CaptureSettings.clampHeight(height))
                }
                .font(.headline)
            }
            if !saved.isEmpty {
                // Each shot is shown with its own two stations, whatever is picked above.
                Section("Saved shots") {
                    ForEach(saved) { s in
                        if let o = s.over, let f = s.forward {
                            NavigationLink { ShotView(url: s.url, stations: [o, f]) } label: {
                                LabeledContent("\(o.label) → \(f.label)", value: s.made?.formatted(date: .abbreviated, time: .shortened) ?? "")
                            }
                        } else {
                            LabeledContent(String(s.url.deletingPathExtension().lastPathComponent.prefix(8)), value: "station gone")
                                .foregroundStyle(.secondary)
                        }
                    }
                }
            }
            if let problem { Text(problem).foregroundStyle(.red) }
            Section { Text("Kept \(cave.keptAt.formatted(date: .abbreviated, time: .shortened))").font(.caption).foregroundStyle(.secondary) }
        }
        .navigationTitle(cave.name)
        .toolbar {
            if updating { ProgressView() } else { Button("Get latest") { getLatest() } }
        }
        .onChange(of: height) { _, h in
            let c = CaptureSettings.clampHeight(h)
            if c != h { height = c }
        }
        .task(id: cave.keptAt) { await loadStations() }
        .onAppear { Task { await loadSaved() } }
    }

    /// Stations come from the Rust core, off the main actor; the result is published back here.
    private func loadStations() async {
        let snapshot = cave.snapshot, entries = cave.entries
        do {
            let s = try await Task.detached { try Core.stations(snapshot: snapshot, entries: entries) }.value
            stations = s
            problem = nil
            // keep picks that still exist in the newer copy
            if let o = over { over = s.first { $0.id == o.id } }
            if let f = forward { forward = s.first { $0.id == f.id } }
        } catch { problem = error.localizedDescription }
        await loadSaved()
    }

    /// The cave's saved shots, newest first, each with its own stations (header only: cheap).
    private func loadSaved() async {
        let caveId = cave.id, all = stations
        saved = await Task.detached {
            ShotFiles.list(cave: caveId).map { u in
                let made = try? u.resourceValues(forKeys: [.creationDateKey]).creationDate
                let head = (try? Data(contentsOf: u, options: .alwaysMapped))
                    .map { ShotSummary(Core.summary(bundle: $0, pair: nil)) }
                let pair = head?.stations(in: all)
                return SavedShot(url: u, made: made, over: pair?.over, forward: pair?.forward)
            }
        }.value
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

    private func picker(_ sel: Binding<Station?>) -> some View {
        Picker("Station", selection: sel) {
            Text("—").tag(Station?.none)
            ForEach(stations) { Text($0.label).tag(Station?.some($0)) }
        }
    }
}
