import SwiftUI

@main
struct CaptureApp: App {
    @StateObject private var account = Account()
    /// One recorder (and ARSession) for the app's whole run, shared by every capture screen.
    @StateObject private var recorder = ShotRecorder()
    /// Shot sync for the whole run: carries on while screens change (ShotSync.swift).
    @StateObject private var sync = ShotSync()
    @Environment(\.scenePhase) private var phase
    init() { Log.launched() }
    var body: some Scene {
        WindowGroup {
            Group {
                if account.token == nil { SignInView() } else { CaveListView(store: CaveStore(account: account)) }
            }.environmentObject(account).environmentObject(recorder).environmentObject(sync)
            // send what waits (and resume an upload a relaunch cut off) at launch, sign-in and coming back
            .onChange(of: account.token, initial: true) { _, t in sync.attach(account); if t != nil { sync.syncAll() } }
            .onChange(of: phase) { _, p in if p == .active, account.token != nil { sync.syncAll() } }
            #if DEBUG
            // test commands from the Mac (TestCommand.swift); the URL scheme is registered in Debug only
            .onOpenURL { url in
                let remote = TestRemote.shared
                remote.account = account
                remote.recorder = recorder
                remote.sync = sync
                remote.handle(url)
            }
            #endif
        }
    }
}
