import SwiftUI

@main
struct CaptureApp: App {
    @StateObject private var account = Account()
    var body: some Scene {
        WindowGroup {
            Group {
                if account.token == nil { SignInView() } else { Text(account.email ?? "Signed in").task { await account.loadMe() } }
            }.environmentObject(account)
        }
    }
}
