import ARKit
import RealityKit
import SwiftUI

extension StationCard {
    /// One colour per card, in the live view and in the shot viewer.
    var colour: UIColor {
        switch self { case .current: .systemGreen; case .forward: .systemBlue; case .rear: .systemOrange }
    }
}

struct ARMeshView: UIViewRepresentable {
    let session: ARSession
    /// Station cards seen so far, drawn as a coloured ball and the card's word at the 3D point.
    var markers: [CardMarker] = []
    func makeUIView(context: Context) -> ARView {
        let v = ARView(frame: .zero, cameraMode: .ar, automaticallyConfigureSession: false)
        v.session = session
        v.debugOptions.insert(.showSceneUnderstanding)
        return v
    }
    func updateUIView(_ uiView: ARView, context: Context) {
        let existing = uiView.scene.anchors.filter { $0.name.hasPrefix("card-") }
        let wanted = Set(markers.map { "card-\($0.card.rawValue)" })
        for a in existing where !wanted.contains(a.name) { uiView.scene.removeAnchor(a) }
        let camera = uiView.cameraTransform.translation
        for m in markers {
            let name = "card-\(m.card.rawValue)"
            let anchor: AnchorEntity
            let old = existing.first(where: { $0.name == name }) as? AnchorEntity
            // A world anchor is pinned where it was made: when the estimate moves, make a new one there.
            if let old, simd_distance(old.position(relativeTo: nil), m.position) < 0.01 {
                anchor = old
            } else {
                if let old { uiView.scene.removeAnchor(old) }
                anchor = AnchorEntity(world: m.position)
                anchor.name = name
                let ball = ModelEntity(mesh: .generateSphere(radius: 0.05),
                                       materials: [UnlitMaterial(color: m.card.colour)])
                let text = ModelEntity(mesh: .generateText(m.card.word, extrusionDepth: 0.002,
                                                           font: .systemFont(ofSize: 0.08, weight: .bold)),
                                       materials: [UnlitMaterial(color: m.card.colour)])
                text.name = "label"
                text.position = SIMD3(-0.15, 0.08, 0)
                anchor.addChild(ball)
                anchor.addChild(text)
                uiView.scene.addAnchor(anchor)
            }
            // keep the word facing the phone
            if let label = anchor.findEntity(named: "label"), simd_distance(camera, m.position) > 0.01 {
                label.look(at: camera, from: label.position(relativeTo: nil), relativeTo: nil)
                label.transform.rotation *= simd_quatf(angle: .pi, axis: SIMD3(0, 1, 0))
            }
        }
    }
}

struct CaptureView: View {
    let cave: KeptCave; let over: Station
    @StateObject private var rec = ShotRecorder()

    var body: some View {
        ZStack {
            if ShotRecorder.hasLiDAR { ARMeshView(session: rec.session, markers: rec.cardMarkers).ignoresSafeArea() }
            VStack {
                Text("Over \(over.label)").padding(8).background(.thinMaterial, in: .capsule)
                if let d = rec.parentMade {
                    Text("Adding to the scan from \(d.formatted(date: .omitted, time: .shortened))")
                        .font(.caption).padding(6).background(.thinMaterial, in: .capsule)
                }
                Spacer()
                Text(rec.message).padding(8).background(.thinMaterial, in: .rect(cornerRadius: 8))
                HStack(spacing: 16) {
                    switch rec.phase {
                    case .ready:
                        Button(rec.parent == nil ? "Start" : "Scan more") { rec.start() }
                            .buttonStyle(.borderedProminent).disabled(rec.findingNorth)
                    case .capturing:
                        Text("\(rec.keyframeCount) photos")
                        Button("Stop") { rec.stop(cave: cave, over: over) }.buttonStyle(.borderedProminent).tint(.red)
                    case .saving: ProgressView()
                    case .saved(let url):
                        NavigationLink("View shot") { ShotView(url: url, stations: [over]) }
                        Button("Scan more") { rec.scanMore() }.buttonStyle(.borderedProminent)
                    case .failed(let m):
                        Text(m).foregroundStyle(.red)
                        // a failed save keeps the session (and the earlier scan): another pass can try again
                        if ShotRecorder.hasLiDAR { Button("Scan more") { rec.scanMore() }.buttonStyle(.bordered) }
                    }
                }.padding()
            }
        }
        .onAppear { rec.prepare(cave: cave, over: over); rec.run() }
        .onDisappear { rec.leave() }
    }
}
