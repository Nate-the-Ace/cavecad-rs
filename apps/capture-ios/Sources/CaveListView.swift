import SwiftUI

struct CaveListView: View {
    @EnvironmentObject var account: Account
    @StateObject var store: CaveStore
    @State private var opened: KeptCave?
    @State private var busy: String?

    var body: some View {
        NavigationStack {
            List(store.caves) { cave in
                Button { open(cave) } label: {
                    HStack {
                        Text(cave.name)
                        Spacer()
                        if busy == cave.id { ProgressView() }
                        else if store.keptIds.contains(cave.id) { Text("kept for offline").font(.caption).foregroundStyle(.secondary) }
                    }
                }
            }
            .navigationTitle("Caves")
            .toolbar { Button("Sign out") { account.signOut(); store.reloadKept() } }
            .refreshable { await store.refresh() }
            .task { await store.refresh() }
            .overlay(alignment: .bottom) { if let p = store.problem { Text(p).padding().background(.thinMaterial) } }
            .navigationDestination(item: Binding(get: { opened.map { CaveInfo(id: $0.id, name: $0.name) } },
                                                 set: { if $0 == nil { opened = nil } })) { _ in
                if let opened { StationPickView(cave: opened, store: store) }
            }
        }
    }

    /// A kept cave opens from the phone at once (no signal needed); "Get latest" on the next screen refreshes it.
    /// A cave not kept yet is downloaded first.
    private func open(_ cave: CaveInfo) {
        store.problem = nil
        if store.keptIds.contains(cave.id), let k = store.load(cave.id) { Log.write("caves", "opened kept copy (\(k.entries.count) entries)"); opened = k; return }
        Task {
            busy = cave.id
            defer { busy = nil }
            do { opened = try await store.download(cave) }
            catch { store.problem = "This cave has not been kept on the phone yet, and it could not be downloaded." }
        }
    }
}
