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
        }
    }
}
