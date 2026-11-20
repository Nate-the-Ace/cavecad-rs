import SceneKit
import SwiftUI
import UIKit

/// Where the orbit camera is: it looks at `target` from `distance` away, turned `yaw` about the vertical and tilted
/// `pitch` (negative looks down). SceneKit frame: y up. Pure values, so the maths can be tested.
struct OrbitRig: Equatable {
    var target: SIMD3<Float>
    var yaw: Float
    var pitch: Float
    var distance: Float

    static let maxPitch: Float = 89 * .pi / 180
    static let fovDeg: Float = 60
    static var fov: Float { fovDeg * .pi / 180 }
    /// One-finger orbit: radians per point dragged.
    static let orbitPerPoint: Float = 0.3 * .pi / 180

    static func clampPitch(_ p: Float) -> Float { min(max(p, -maxPitch), maxPitch) }

    /// From the target towards the camera, unit length. yaw 0, pitch 0: the camera is on +z looking down -z.
    static func back(yaw: Float, pitch: Float) -> SIMD3<Float> {
        SIMD3(cos(pitch) * sin(yaw), -sin(pitch), cos(pitch) * cos(yaw))
    }

    /// The camera's right and up directions (match a node with eulerAngles (pitch, yaw, 0)).
    static func right(yaw: Float) -> SIMD3<Float> { SIMD3(cos(yaw), 0, -sin(yaw)) }
    static func up(yaw: Float, pitch: Float) -> SIMD3<Float> {
        SIMD3(sin(pitch) * sin(yaw), cos(pitch), sin(pitch) * cos(yaw))
    }

    static func cameraPosition(target: SIMD3<Float>, yaw: Float, pitch: Float, distance: Float) -> SIMD3<Float> {
        target + distance * back(yaw: yaw, pitch: pitch)
    }

    var cameraPosition: SIMD3<Float> { Self.cameraPosition(target: target, yaw: yaw, pitch: pitch, distance: distance) }

    /// How far the target moves for a two-finger drag of (dx, dy) screen points (UIKit: y down), so the scene
    /// follows the fingers: one point is 2·d·tan(fov/2)/viewHeight of world at the target's distance.
    static func panShift(dx: Float, dy: Float, yaw: Float, pitch: Float, distance: Float, fov: Float,
                         viewHeight: Float) -> SIMD3<Float> {
        guard viewHeight > 0 else { return .zero }
        let perPoint = 2 * distance * tan(fov / 2) / viewHeight
        return (-dx * right(yaw: yaw) + dy * up(yaw: yaw, pitch: pitch)) * perPoint
    }

    /// Bounding sphere radius of a box (at least a few centimetres so an empty scene still frames).
    static func radius(min lo: SIMD3<Float>, max hi: SIMD3<Float>) -> Float {
        let r = simd_length(hi - lo) / 2
        return r.isFinite && r > 0.05 ? r : 0.05
    }

    /// Distance that fits the box's bounding sphere in the (vertical) field of view, with a little margin.
    static func framingDistance(min lo: SIMD3<Float>, max hi: SIMD3<Float>, fov: Float) -> Float {
        1.1 * radius(min: lo, max: hi) / sin(fov / 2)
    }

    /// Zoom limits from the scene size: in close enough to look at a wall, out to a few times the whole.
    static func distanceRange(radius r: Float) -> ClosedRange<Float> { max(0.05, r * 0.01)...max(1, r * 10) }

    /// The starting view: the box's centre, the whole box in view, looking a little down.
    static func framing(min lo: SIMD3<Float>, max hi: SIMD3<Float>) -> OrbitRig {
        OrbitRig(target: (lo + hi) / 2, yaw: 0, pitch: -30 * .pi / 180, distance: framingDistance(min: lo, max: hi, fov: fov))
    }
}

/// A SceneKit view with our own orbit camera: one finger orbits, two fingers pan with the scene under them,
/// pinch zooms about the target, double-tap goes back to the start. No roll.
struct OrbitView: UIViewRepresentable {
    let scene: SCNScene

    func makeCoordinator() -> Coordinator { Coordinator() }

    func makeUIView(context: Context) -> SCNView {
        let v = SCNView()
        v.allowsCameraControl = false
        v.autoenablesDefaultLighting = true
        context.coordinator.attach(to: v)
        context.coordinator.show(scene)
        return v
    }

    func updateUIView(_ v: SCNView, context: Context) {
        // only a new scene resets the camera; other SwiftUI updates leave it be
        if v.scene !== scene { context.coordinator.show(scene) }
    }

    @MainActor
    final class Coordinator: NSObject, UIGestureRecognizerDelegate {
        weak var view: SCNView?
        let camera = SCNNode()
        var rig = OrbitRig(target: .zero, yaw: 0, pitch: 0, distance: 1)
        var start = OrbitRig(target: .zero, yaw: 0, pitch: 0, distance: 1)
        var range: ClosedRange<Float> = 0.05...100
        weak var twoPan: UIPanGestureRecognizer?
        weak var pinch: UIPinchGestureRecognizer?

        override init() {
            let c = SCNCamera()
            c.fieldOfView = CGFloat(OrbitRig.fovDeg)
            c.projectionDirection = .vertical
            camera.camera = c
            camera.name = "orbit-camera"
            super.init()
        }

        func attach(to v: SCNView) {
            view = v
            let orbit = UIPanGestureRecognizer(target: self, action: #selector(orbit(_:)))
            orbit.minimumNumberOfTouches = 1; orbit.maximumNumberOfTouches = 1
            let pan = UIPanGestureRecognizer(target: self, action: #selector(pan(_:)))
            pan.minimumNumberOfTouches = 2; pan.maximumNumberOfTouches = 2
            let pinch = UIPinchGestureRecognizer(target: self, action: #selector(pinch(_:)))
            let tap = UITapGestureRecognizer(target: self, action: #selector(reset(_:)))
            tap.numberOfTapsRequired = 2
            for g in [orbit, pan, pinch, tap] as [UIGestureRecognizer] { g.delegate = self; v.addGestureRecognizer(g) }
            twoPan = pan; self.pinch = pinch
        }

        func show(_ scene: SCNScene) {
            guard let v = view else { return }
            camera.removeFromParentNode()
            let (lo, hi) = scene.rootNode.boundingBox
            let l = SIMD3<Float>(Float(lo.x), Float(lo.y), Float(lo.z)), h = SIMD3<Float>(Float(hi.x), Float(hi.y), Float(hi.z))
            let ok = l.x <= h.x && l.y <= h.y && l.z <= h.z
            let a = ok ? l : SIMD3<Float>(repeating: -1), b = ok ? h : SIMD3<Float>(repeating: 1)
            let r = OrbitRig.radius(min: a, max: b)
            range = OrbitRig.distanceRange(radius: r)
            start = OrbitRig.framing(min: a, max: b)
            start.distance = min(max(start.distance, range.lowerBound), range.upperBound)
            camera.camera?.zNear = Double(max(0.005, range.lowerBound * 0.1))
            camera.camera?.zFar = Double(range.upperBound + 2 * r)
            scene.rootNode.addChildNode(camera)
            v.scene = scene
            v.pointOfView = camera
            rig = start
            apply()
        }

        func apply() {
            camera.simdPosition = rig.cameraPosition
            camera.simdEulerAngles = SIMD3(rig.pitch, rig.yaw, 0)
        }

        @objc func orbit(_ g: UIPanGestureRecognizer) {
            guard let v = view, g.state == .changed else { return }
            let t = g.translation(in: v); g.setTranslation(.zero, in: v)
            rig.yaw -= Float(t.x) * OrbitRig.orbitPerPoint
            rig.pitch = OrbitRig.clampPitch(rig.pitch - Float(t.y) * OrbitRig.orbitPerPoint)
            apply()
        }

        @objc func pan(_ g: UIPanGestureRecognizer) {
            guard let v = view, g.state == .changed else { return }
            let t = g.translation(in: v); g.setTranslation(.zero, in: v)
            rig.target += OrbitRig.panShift(dx: Float(t.x), dy: Float(t.y), yaw: rig.yaw, pitch: rig.pitch,
                                            distance: rig.distance, fov: OrbitRig.fov, viewHeight: Float(v.bounds.height))
            apply()
        }

        @objc func pinch(_ g: UIPinchGestureRecognizer) {
            guard g.state == .changed, g.scale > 0 else { return }
            rig.distance = min(max(rig.distance / Float(g.scale), range.lowerBound), range.upperBound)
            g.scale = 1
            apply()
        }

        @objc func reset(_ g: UITapGestureRecognizer) {
            rig = start
            apply()
        }

        func gestureRecognizer(_ a: UIGestureRecognizer, shouldRecognizeSimultaneouslyWith b: UIGestureRecognizer) -> Bool {
            (a === twoPan && b === pinch) || (a === pinch && b === twoPan)
        }
    }
}
