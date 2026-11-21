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
            .toolbar {
                ToolbarItem(placement: .topBarLeading) { NavigationLink("Station cards") { CardSheetView() } }
                ToolbarItem(placement: .topBarTrailing) { Button("Sign out") { account.signOut(); store.reloadKept() } }
            }
            .refreshable { await store.refresh() }
            .task { await store.refresh() }
            .overlay(alignment: .bottom) { if let p = store.problem { Text(p).padding().background(.thinMaterial) } }
            .navigationDestination(item: Binding(get: { opened.map { CaveInfo(id: $0.id, name: $0.name) } },
                                                 set: { if $0 == nil { opened = nil } })) { _ in
                if let opened { StationPickView(cave: opened, store: store) }
            }
            #if DEBUG
            .onAppear { TestRemote.shared.caveList = testCommand }
            .onChange(of: opened?.id, initial: true) { TestRemote.shared.caveOpened(opened?.name) }
            #endif
        }
    }

    #if DEBUG
    /// Test commands for this screen (TestRemote): open a cave as tapping its row does; back from its station screen.
    private func testCommand(_ c: TestCommand) -> TestOutcome {
        switch c {
        case .open(let key):
            if busy != nil { return .refused("a download is running") }
            guard let cave = store.caves.first(where: { $0.id == key })
                    ?? store.caves.first(where: { $0.name.localizedCaseInsensitiveCompare(key) == .orderedSame }) else {
                return .refused("no cave \"\(key)\" in the list (\(store.caves.count) listed\(store.offline ? ", offline" : ""))")
            }
            open(cave)
            if opened != nil { return .ok("opened the kept copy") }
            return .ok("not kept: downloading (watch for [caves] lines)")
        case .back:
            opened = nil
            return .ok(nil)
        default:
            return .refused("not a cave list command")
        }
    }
    #endif

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
