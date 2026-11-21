import SwiftUI

@main
struct CaptureApp: App {
    @StateObject private var account = Account()
    /// One recorder (and ARSession) for the app's whole run, shared by every capture screen.
    @StateObject private var recorder = ShotRecorder()
    init() { Log.launched() }
    var body: some Scene {
        WindowGroup {
            Group {
                if account.token == nil { SignInView() } else { CaveListView(store: CaveStore(account: account)) }
            }.environmentObject(account).environmentObject(recorder)
            #if DEBUG
            // test commands from the Mac (TestCommand.swift); the URL scheme is registered in Debug only
            .onOpenURL { url in
                let remote = TestRemote.shared
                remote.account = account
                remote.recorder = recorder
                remote.handle(url)
            }
            #endif
        }
    }
}
