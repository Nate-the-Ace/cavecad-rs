import SwiftUI

struct SignInView: View {
    @EnvironmentObject var account: Account
    var body: some View {
        VStack(spacing: 20) {
            Text("CaveCAD Capture").font(.largeTitle.bold())
            Text("Sign in with your CaveCAD account to pick a cave.").multilineTextAlignment(.center)
            Button("Sign in") { account.signIn() }.buttonStyle(.borderedProminent).controlSize(.large)
            if let p = account.problem { Text(p).foregroundStyle(.red) }
        }.padding()
    }
}
