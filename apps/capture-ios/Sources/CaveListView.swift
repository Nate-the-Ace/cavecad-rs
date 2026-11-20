import SwiftUI

struct CaveListView: View {
    @EnvironmentObject var account: Account
    @StateObject var store: CaveStore
    @State private var opened: KeptCave?
    @State private var busy: String?

    var body: some View {
        NavigationStack {
            List(store.caves) { cave in
                Button {
                    Task {
                        busy = cave.id
                        defer { busy = nil }
                        opened = store.offline ? store.load(cave.id) : ((try? await store.download(cave)) ?? store.load(cave.id))
                        if opened == nil { store.problem = "This cave has not been kept on the phone yet." }
                    }
                } label: {
                    HStack {
                        Text(cave.name)
                        Spacer()
                        if busy == cave.id { ProgressView() }
                        else if store.load(cave.id) != nil { Text("kept for offline").font(.caption).foregroundStyle(.secondary) }
                    }
                }
            }
            .navigationTitle("Caves")
            .toolbar { Button("Sign out") { account.signOut() } }
            .refreshable { await store.refresh() }
            .task { await store.refresh() }
            .overlay(alignment: .bottom) { if let p = store.problem { Text(p).padding().background(.thinMaterial) } }
            .navigationDestination(item: Binding(get: { opened.map { CaveInfo(id: $0.id, name: $0.name) } },
                                                 set: { if $0 == nil { opened = nil } })) { _ in
                if let opened { StationPickView(cave: opened) }
            }
        }
    }
}
