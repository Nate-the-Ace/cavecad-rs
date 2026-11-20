import SwiftUI
struct CaptureView: View {
    let cave: KeptCave; let over: Station; let forward: Station
    var body: some View { Text("Capture over \(over.label)") }
}
