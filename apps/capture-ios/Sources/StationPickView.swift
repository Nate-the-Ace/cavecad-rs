import SwiftUI

/// A saved shot and the station it was taken over (nil when the cave no longer has it); `forward` only for
/// an older heading-tap shot.
struct SavedShot: Identifiable, Sendable {
    let url: URL
    let made: Date?
    let over: Station?
    let forward: Station?
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

    init(cave: KeptCave, store: CaveStore) {
        _cave = State(initialValue: cave)
        self.store = store
    }

    var body: some View {
        List {
            Section("Station the gimbal stands on") { picker($over) }
            if let over {
                NavigationLink("Capture") { CaptureView(cave: cave, over: over) }.font(.headline)
            }
            if !saved.isEmpty {
                // Each shot is shown with its own station, whatever is picked above.
                Section("Saved shots") {
                    ForEach(saved) { s in
                        let made = s.made?.formatted(date: .abbreviated, time: .shortened) ?? ""
                        if let o = s.over {
                            NavigationLink { ShotView(url: s.url, stations: [o] + (s.forward.map { [$0] } ?? [])) } label: {
                                LabeledContent(s.title, value: made)
                            }
                        } else {
                            LabeledContent(s.title, value: made).foregroundStyle(.secondary)
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
        saved = await Task.detached {
            ShotFiles.list(cave: caveId).map { u in
                let made = try? u.resourceValues(forKeys: [.creationDateKey]).creationDate
                let head = (try? Data(contentsOf: u, options: .alwaysMapped))
                    .map { ShotSummary(Core.summary(bundle: $0, pair: nil)) }
                let found = head?.stations(in: all)
                return SavedShot(url: u, made: made, over: found?.over, forward: found?.forward)
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
