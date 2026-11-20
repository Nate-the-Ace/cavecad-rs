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
    /// The earlier scan's ghost, `ghostKey` naming which one, and where it sits (nil: not shown).
    var ghost: Core.GhostMesh?
    var ghostKey = 0
    var ghostTransform: simd_float4x4?
    /// ARKit's live, still-growing mesh is drawn (only while capturing; ARKit keeps reconstructing either way).
    var liveMesh = true

    /// Which ghost the view built, so it is built once and afterwards only moved.
    final class Coordinator { var builtGhost: Int? }
    func makeCoordinator() -> Coordinator { Coordinator() }

    func makeUIView(context: Context) -> ARView {
        let v = ARView(frame: .zero, cameraMode: .ar, automaticallyConfigureSession: false)
        v.session = session
        if liveMesh { v.debugOptions.insert(.showSceneUnderstanding) }
        return v
    }
    func updateUIView(_ uiView: ARView, context: Context) {
        if liveMesh != uiView.debugOptions.contains(.showSceneUnderstanding) {
            if liveMesh { uiView.debugOptions.insert(.showSceneUnderstanding) } else { uiView.debugOptions.remove(.showSceneUnderstanding) }
        }
        updateGhost(uiView, context.coordinator)
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

    /// Builds the ghost once; afterwards only moves it. Rebuilding the mesh on every update would cost the scan
    /// its frame rate.
    private func updateGhost(_ uiView: ARView, _ c: Coordinator) {
        var old = uiView.scene.anchors.first { $0.name == "ghost" }
        if let o = old, c.builtGhost != ghostKey || ghost == nil || ghostTransform == nil {
            uiView.scene.removeAnchor(o)
            old = nil
            c.builtGhost = nil
        }
        guard let g = ghost, let t = ghostTransform else { return }
        if let old {
            if old.transform.matrix != t { old.transform = Transform(matrix: t) }
        } else {
            uiView.scene.addAnchor(GhostView.entity(g, transform: t))
            c.builtGhost = ghostKey
        }
    }
}

struct CaptureView: View {
    let cave: KeptCave; let over: Station
    /// The app's one recorder: it (and its ARSession) outlives this screen, so coming back to the same station
    /// resumes without a reset.
    @EnvironmentObject private var rec: ShotRecorder

    var body: some View {
        ZStack {
            if ShotRecorder.hasLiDAR { ARMeshView(session: rec.session, markers: rec.cardMarkers, ghost: rec.ghost, ghostKey: rec.ghostKey,
                           ghostTransform: rec.ghostTransform, liveMesh: ShotRecorder.showsLiveMesh(rec.phase)).ignoresSafeArea() }
            VStack {
                Text("Over \(over.label)").padding(8).background(.thinMaterial, in: .capsule)
                if let d = rec.parentMade {
                    Text("Adding to the scan from \(d.formatted(date: .omitted, time: .shortened))")
                        .font(.caption).padding(6).background(.thinMaterial, in: .capsule)
                }
                Spacer()
                // The controls sit on a solid dark panel with big, solid buttons: the live view behind is busy
                // with mesh lines and coloured markers.
                VStack(spacing: 12) {
                    Text(rec.message).font(.body.weight(.medium)).foregroundStyle(.white).multilineTextAlignment(.center)
                    switch rec.phase {
                    case .capturing: Text("\(rec.keyframeCount) photos").font(.subheadline).foregroundStyle(.white.opacity(0.8))
                    case .saving: ProgressView().tint(.white).controlSize(.large)
                    // a failed save keeps the session (and the earlier scan): another pass can try again
                    case .failed(let m): Text(m).foregroundStyle(.red)
                    default: EmptyView()
                    }
                    HStack(spacing: 12) {
                        ForEach(ShotRecorder.controls(rec.phase, hasParent: rec.parent != nil), id: \.self) { button($0) }
                    }
                }
                .padding(16)
                .frame(maxWidth: .infinity)
                .background(.black.opacity(0.72), in: .rect(cornerRadius: 20))
                .padding(.horizontal, 12).padding(.bottom, 8)
            }
        }
        .navigationDestination(item: $rec.viewing) { url in ShotView(url: url, stations: [over]) }
        .onAppear { rec.prepare(cave: cave, over: over); rec.run() }
        .onDisappear { rec.leave() }
    }

    @ViewBuilder private func button(_ c: ShotRecorder.Control) -> some View {
        switch c {
        case .start, .resume:
            // paused or failed: the next pass in the same session; ready: the pass the screen is ready for
            let ready: Bool = { if case .ready = rec.phase { true } else { false } }()
            if ShotRecorder.hasLiDAR || ready {
                Button(c == .start ? "Start" : "Resume") {
                    if !ready { rec.scanMore() }
                    rec.start()
                }
                .buttonStyle(BigButton(colour: .green)).disabled(rec.findingNorth)
            }
        case .pause:
            Button("Pause") { rec.pause(cave: cave, over: over) }.buttonStyle(BigButton(colour: .orange))
        case .stop:
            Button("Stop") { rec.finish(cave: cave, over: over) }.buttonStyle(BigButton(colour: .red))
        case .viewShot:
            if case .saved(let url) = rec.phase {
                NavigationLink("View") { ShotView(url: url, stations: [over]) }.buttonStyle(BigButton(colour: .blue))
            }
        }
    }
}

/// A capture control big and solid enough to find on a busy live view: bold white text on a solid colour,
/// at least 64 pt tall, sharing the row's width.
struct BigButton: ButtonStyle {
    let colour: Color
    @Environment(\.isEnabled) private var enabled

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.title2.weight(.bold))
            .foregroundStyle(.white)
            .frame(maxWidth: .infinity, minHeight: 64)
            .background(colour.opacity(enabled ? (configuration.isPressed ? 0.7 : 1) : 0.4), in: .rect(cornerRadius: 16))
            .overlay(RoundedRectangle(cornerRadius: 16).stroke(.white.opacity(0.9), lineWidth: 2))
    }
}
