import SwiftUI

struct StationPickView: View {
    @State var cave: KeptCave
    let store: CaveStore
    @State private var stations: [Station] = []
    @State private var over: Station?
    @State private var forward: Station?
    @State private var problem: String?
    @State private var updating = false

    init(cave: KeptCave, store: CaveStore) {
        _cave = State(initialValue: cave)
        self.store = store
    }

    var body: some View {
        List {
            Section("The rig stands over") { picker($over) }
            Section("Forward station (you will aim at it)") { picker($forward) }
            if let over, let forward, over != forward {
                NavigationLink("Capture") { CaptureView(cave: cave, over: over, forward: forward) }
                    .font(.headline)
            }
            if let problem { Text(problem).foregroundStyle(.red) }
            Section { Text("Kept \(cave.keptAt.formatted(date: .abbreviated, time: .shortened))").font(.caption).foregroundStyle(.secondary) }
        }
        .navigationTitle(cave.name)
        .toolbar {
            if updating { ProgressView() } else { Button("Get latest") { getLatest() } }
        }
        .task(id: cave.keptAt) { await loadStations() }
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
