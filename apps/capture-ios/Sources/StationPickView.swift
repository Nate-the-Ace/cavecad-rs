import SwiftUI

struct StationPickView: View {
    let cave: KeptCave
    @State private var stations: [Station] = []
    @State private var over: Station?
    @State private var forward: Station?
    @State private var problem: String?

    var body: some View {
        List {
            Section("The rig stands over") { picker($over) }
            Section("Forward station (you will aim at it)") { picker($forward) }
            if let over, let forward, over != forward {
                NavigationLink("Capture") { CaptureView(cave: cave, over: over, forward: forward) }
                    .font(.headline)
            }
            if let problem { Text(problem).foregroundStyle(.red) }
        }
        .navigationTitle(cave.name)
        .task {
            do { stations = try Core.stations(snapshot: cave.snapshot, entries: cave.entries) }
            catch { problem = error.localizedDescription }
        }
    }

    private func picker(_ sel: Binding<Station?>) -> some View {
        Picker("Station", selection: sel) {
            Text("—").tag(Station?.none)
            ForEach(stations) { Text($0.label).tag(Station?.some($0)) }
        }
    }
}
