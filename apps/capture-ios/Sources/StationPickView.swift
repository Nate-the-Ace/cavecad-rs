import SwiftUI

/// A saved shot and the station it was taken over (nil when the cave no longer has it); `forward` only for
/// an older heading-tap shot.
struct SavedShot: Identifiable, Sendable {
    let url: URL
    let made: Date?
    let over: Station?
    let forward: Station?
    /// The compass state, for compass shots ("true north ±5°", "compass unsure", …).
    let compassLabel: String?
    /// The six numbers that place it (`ShotSummary.pair`); nil when a station it needs is gone.
    var pair: [Double]? = nil
    /// When the shot file was last written (a Resume rewrites it).
    var modified: Date? = nil
    var id: URL { url }

    var title: String {
        guard let over else { return "station gone" }
        return "over \(over.label)" + (forward.map { " → \($0.label)" } ?? "")
    }
}

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

    init(cave: KeptCave, store: CaveStore) {
        _cave = State(initialValue: cave)
        self.store = store
    }

    var body: some View {
        List {
            Section("Station you capture from") { picker($over) }
            if let over {
                NavigationLink("Capture") { CaptureView(cave: cave, over: over) }.font(.headline)
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
                    Text("Overall shows every shot at its own station. Swipe a shot left to delete it from this phone.")
                }
                Section {
                    Button("Delete all shots of this cave (\(ByteCountFormatter.string(fromByteCount: savedBytes, countStyle: .file)))",
                           role: .destructive) { askDeleteAll = true }
                }
            }
            if let problem { Text(problem).foregroundStyle(.red) }
            Section { Text("Kept \(cave.keptAt.formatted(date: .abbreviated, time: .shortened))").font(.caption).foregroundStyle(.secondary) }
        }
        .navigationTitle(cave.name)
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
        .toolbar {
            if updating { ProgressView() } else { Button("Get latest") { getLatest() } }
        }
        .task(id: cave.keptAt) { await loadStations() }
        .onChange(of: problem) { _, p in if let p { Log.write("stations", p) } }
        .onAppear { if !stations.isEmpty { Task { await loadSaved() } } }
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
                                 pair: head?.pair(in: all), modified: ShotThumbnail.modified(u))
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
        }
    }
}
