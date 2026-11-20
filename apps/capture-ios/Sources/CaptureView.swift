import ARKit
import RealityKit
import SwiftUI

struct ARMeshView: UIViewRepresentable {
    let session: ARSession
    func makeUIView(context: Context) -> ARView {
        let v = ARView(frame: .zero, cameraMode: .ar, automaticallyConfigureSession: false)
        v.session = session
        v.debugOptions.insert(.showSceneUnderstanding)
        return v
    }
    func updateUIView(_ uiView: ARView, context: Context) {}
}

struct CaptureView: View {
    let cave: KeptCave; let over: Station; let forward: Station
    @StateObject private var rec = ShotRecorder()

    var body: some View {
        ZStack {
            if ShotRecorder.hasLiDAR {
                ARMeshView(session: rec.session).ignoresSafeArea()
                Image(systemName: "plus").font(.system(size: 40, weight: .thin)).foregroundStyle(.yellow)
            }
            VStack {
                Text("Over \(over.label) · aim at \(forward.label)").padding(8).background(.thinMaterial, in: .capsule)
                Spacer()
                Text(rec.message).padding(8).background(.thinMaterial, in: .rect(cornerRadius: 8))
                HStack(spacing: 16) {
                    switch rec.phase {
                    case .aiming:
                        Button("Aim") { rec.aim() }.buttonStyle(.bordered)
                        Button("Start") { rec.start() }.buttonStyle(.borderedProminent).disabled(!rec.tapped)
                    case .capturing:
                        Text("\(rec.keyframeCount) photos")
                        Button("Stop") { rec.stop(cave: cave, over: over, forward: forward) }.buttonStyle(.borderedProminent).tint(.red)
                    case .saving: ProgressView()
                    case .saved(let url): NavigationLink("View shot") { ShotView(url: url, over: over, forward: forward) }
                    case .failed(let m): Text(m).foregroundStyle(.red)
                    }
                }.padding()
            }
        }
        .onAppear { rec.run() }
        .onDisappear { rec.session.pause() }
    }
}

struct ShotView: View {
    let url: URL; let over: Station; let forward: Station
    var body: some View {
        let s = (try? Data(contentsOf: url)).map { Core.summary(bundle: $0, pair: Core.pair(over, forward)) } ?? [:]
        List { ForEach(s.keys.sorted(), id: \.self) { k in LabeledContent(k, value: "\(s[k]!)") } }
    }
}
