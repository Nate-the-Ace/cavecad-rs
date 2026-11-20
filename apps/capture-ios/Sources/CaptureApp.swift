import SwiftUI

@main
struct CaptureApp: App {
    @StateObject private var account = Account()
    var body: some Scene {
        WindowGroup {
            Group {
                if account.token == nil { SignInView() } else { CaveListView(store: CaveStore(account: account)) }
            }.environmentObject(account)
        }
    }
}
