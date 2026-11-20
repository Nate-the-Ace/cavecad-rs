import ARKit
import CoreImage
import Foundation
import simd
import UIKit

/// One anchor's mesh, pulled out of ARKit so merging can be tested without a device.
struct MeshPart: Sendable {
    var positions: [SIMD3<Float>]
    var normals: [SIMD3<Float>]
    var indices: [UInt32]
    var transform: simd_float4x4
}

struct MergedMesh: Sendable { var positions: [Float]; var normals: [Float]; var indices: [UInt32] }

/// One kept camera frame.
struct ShotKeyframe: Sendable { let pose: simd_float4x4; let intrinsics: simd_float3x3; let w: Int; let h: Int; let jpeg: Data }

/// Runs one shot: ARKit scene reconstruction, heading tap, keyframes; turns it into a bundle on Stop.
@MainActor
final class ShotRecorder: NSObject, ObservableObject, ARSessionDelegate {
    enum Phase { case aiming, capturing, saving, saved(URL), failed(String) }

    @Published var phase: Phase = .aiming
    @Published var message = "Put the crosshair on the forward station and tap Aim."
    @Published var tapped = false
    @Published var keyframeCount = 0

    let session = ARSession()
    private var tap: (origin: SIMD3<Float>, dir: SIMD3<Float>)?
    private var startCamera = SIMD3<Float>(0, 0, 0)
    private var keyframes: [ShotKeyframe] = []
    private var lastYaw: Float = .nan, lastTime: TimeInterval = 0
    private var worstTracking = "normal"
    private let context = CIContext()

    nonisolated static func shouldKeep(lastYaw: Float, yaw: Float, lastTime: TimeInterval, time: TimeInterval) -> Bool {
        if time - lastTime < 0.25 { return false }
        if lastYaw.isNaN { return true }
        var d = abs(yaw - lastYaw).truncatingRemainder(dividingBy: 360)
        if d > 180 { d = 360 - d }
        return d >= 15 || time - lastTime >= 2
    }

    nonisolated static func mergeAnchors(_ parts: [MeshPart]) -> MergedMesh {
        var out = MergedMesh(positions: [], normals: [], indices: [])
        for p in parts {
            let base = UInt32(out.positions.count / 3)
            let rot = simd_float3x3(SIMD3(p.transform.columns.0.x, p.transform.columns.0.y, p.transform.columns.0.z),
                                    SIMD3(p.transform.columns.1.x, p.transform.columns.1.y, p.transform.columns.1.z),
                                    SIMD3(p.transform.columns.2.x, p.transform.columns.2.y, p.transform.columns.2.z))
            for v in p.positions {
                let w = p.transform * SIMD4(v, 1)
                out.positions += [w.x, w.y, w.z]
            }
            for n in p.normals {
                let w = simd_normalize(rot * n)
                out.normals += [w.x, w.y, w.z]
            }
            out.indices += p.indices.map { $0 + base }
        }
        return out
    }

    /// LiDAR phones only: scene reconstruction needs the depth sensor (the simulator has none either).
    static var hasLiDAR: Bool { ARWorldTrackingConfiguration.supportsSceneReconstruction(.mesh) }

    func run() {
        guard Self.hasLiDAR else {
            phase = .failed("This phone has no LiDAR.")
            message = "This phone has no LiDAR."
            return
        }
        let c = ARWorldTrackingConfiguration()
        c.sceneReconstruction = .mesh
        c.frameSemantics = [.sceneDepth]
        c.isLightEstimationEnabled = true
        c.worldAlignment = .gravity
        session.delegate = self
        session.run(c, options: [.resetTracking, .removeExistingAnchors])
    }

    func aim() {
        guard let f = session.currentFrame else { return }
        let t = f.camera.transform
        let origin = SIMD3(t.columns.3.x, t.columns.3.y, t.columns.3.z)
        let fwd = -SIMD3(t.columns.2.x, t.columns.2.y, t.columns.2.z)
        tap = (origin, simd_normalize(fwd))
        tapped = true
        message = "Aimed. Press Start and turn slowly around the room."
    }

    func start() {
        guard let f = session.currentFrame else { return }
        if (f.lightEstimate?.ambientIntensity ?? 0) < 150 { message = "Too dark to track. Turn the light on."; return }
        guard case .normal = f.camera.trackingState else { message = "Hold still until tracking settles."; return }
        guard tapped else { message = "Aim at the forward station first."; return }
        let t = f.camera.transform
        startCamera = SIMD3(t.columns.3.x, t.columns.3.y, t.columns.3.z)
        keyframes = []; keyframeCount = 0; lastYaw = .nan; lastTime = 0; worstTracking = "normal"
        phase = .capturing
        message = "Capturing. Turn slowly; Stop when the room is covered."
    }

    // ARSession calls this on the main queue (no delegateQueue is set).
    nonisolated func session(_ session: ARSession, didUpdate frame: ARFrame) {
        let pose = frame.camera.transform
        let yaw = atan2(-pose.columns.2.x, -pose.columns.2.z) * 180 / .pi
        let time = frame.timestamp
        let state = frame.camera.trackingState
        let image = frame.capturedImage
        let intrinsics = frame.camera.intrinsics
        MainActor.assumeIsolated {
            guard case .capturing = phase else { return }
            switch state {
            case .normal: break
            case .notAvailable: worstTracking = "lost"
            case .limited: if worstTracking == "normal" { worstTracking = "limited" }
            }
            guard Self.shouldKeep(lastYaw: lastYaw, yaw: yaw, lastTime: lastTime, time: time),
                  let srgb = CGColorSpace(name: CGColorSpace.sRGB) else { return }
            let ci = CIImage(cvPixelBuffer: image)
            guard let jpeg = context.jpegRepresentation(of: ci, colorSpace: srgb,
                                                        options: [kCGImageDestinationLossyCompressionQuality as CIImageRepresentationOption: 0.7])
            else { return }
            keyframes.append(ShotKeyframe(pose: pose, intrinsics: intrinsics, w: CVPixelBufferGetWidth(image),
                                          h: CVPixelBufferGetHeight(image), jpeg: jpeg))
            keyframeCount = keyframes.count
            lastYaw = yaw; lastTime = time
        }
    }

    func stop(cave: KeptCave, over: Station, forward: Station) {
        guard case .capturing = phase, let tap else { return }
        phase = .saving
        message = "Saving the shot…"
        // Copy the meshes out while the anchors are still valid, then pause.
        let anchors = session.currentFrame?.anchors.compactMap { $0 as? ARMeshAnchor } ?? []
        let parts = anchors.compactMap(Self.part)
        session.pause()
        let job = ShotJob(caveId: cave.id, over: over, forward: forward, parts: parts, keyframes: keyframes,
                          tapOrigin: tap.origin, tapDir: tap.dir, startCamera: startCamera, tracking: worstTracking,
                          systemVersion: UIDevice.current.systemVersion)
        keyframes = []
        Task {
            let result = await Task.detached { job.write() }.value
            switch result {
            case .success(let url): phase = .saved(url); message = "Shot saved."
            case .failure(let m): phase = .failed(m.message); message = m.message
            }
        }
    }

    /// One ARKit mesh anchor as plain arrays; nil for a layout this code does not read (LiDAR meshes are
    /// float3 vertices/normals and 32-bit triangle indices).
    private static func part(_ a: ARMeshAnchor) -> MeshPart? {
        let g = a.geometry
        let f = g.faces
        guard f.bytesPerIndex == 4, f.indexCountPerPrimitive == 3,
              g.vertices.format == .float3, g.normals.format == .float3, g.normals.count == g.vertices.count
        else { return nil }
        func vec3(_ s: ARGeometrySource) -> [SIMD3<Float>] {
            (0..<s.count).map { i in
                let p = s.buffer.contents().advanced(by: s.offset + s.stride * i).assumingMemoryBound(to: Float.self)
                return SIMD3(p[0], p[1], p[2])
            }
        }
        let idx = UnsafeBufferPointer(start: f.buffer.contents().assumingMemoryBound(to: UInt32.self), count: f.count * 3)
        return MeshPart(positions: vec3(g.vertices), normals: vec3(g.normals), indices: Array(idx), transform: a.transform)
    }
}

struct ShotFailure: Error { let message: String }

/// Everything Stop needs to turn a shot into a file, so the heavy part (merge, encode, write) runs off the main actor.
struct ShotJob: Sendable {
    let caveId: String
    let over: Station
    let forward: Station
    let parts: [MeshPart]
    let keyframes: [ShotKeyframe]
    let tapOrigin: SIMD3<Float>
    let tapDir: SIMD3<Float>
    let startCamera: SIMD3<Float>
    let tracking: String
    let systemVersion: String

    func write() -> Result<URL, ShotFailure> {
        let mesh = ShotRecorder.mergeAnchors(parts)
        let shotId = UUID().uuidString.lowercased()
        var jpegs = Data()
        var kfs: [[String: Any]] = []
        for k in keyframes {
            jpegs.append(k.jpeg)
            let p = k.pose, i = k.intrinsics
            kfs.append(["pose": [p.columns.0, p.columns.1, p.columns.2, p.columns.3].flatMap { [$0.x, $0.y, $0.z, $0.w] }.map(Double.init),
                        "intrinsics": [i.columns.0, i.columns.1, i.columns.2].flatMap { [$0.x, $0.y, $0.z] }.map(Double.init),
                        "width": k.w, "height": k.h, "jpeg_len": k.jpeg.count])
        }
        let header: [String: Any] = [
            "shot_id": shotId, "cave_id": caveId, "station_id": over.id, "station_label": over.label,
            "captured_at_ms": Int64(Date().timeIntervalSince1970 * 1000), "device": Self.model(),
            "instrument_height_m": 0.0, "start_camera": [startCamera.x, startCamera.y, startCamera.z].map(Double.init),
            "taps": [["target_station_id": forward.id, "target_label": forward.label,
                      "origin": [tapOrigin.x, tapOrigin.y, tapOrigin.z].map(Double.init),
                      "dir": [tapDir.x, tapDir.y, tapDir.z].map(Double.init)]],
            "tracking": tracking, "arkit_version": systemVersion,
            "gravity": [0.0, -1.0, 0.0], "keyframes": kfs,
        ]
        guard let headerData = try? JSONSerialization.data(withJSONObject: header) else {
            return .failure(ShotFailure(message: "The shot could not be written."))
        }
        let bundle = Core.encodeShot(header: headerData, positions: mesh.positions, normals: mesh.normals,
                                     indices: mesh.indices, jpegs: jpegs)
        guard !bundle.isEmpty else { return .failure(ShotFailure(message: "The shot could not be written.")) }
        do { return .success(try ShotFiles.save(bundle, cave: caveId, shot: shotId)) }
        catch { return .failure(ShotFailure(message: "Saving failed: \(error.localizedDescription)")) }
    }

    private static func model() -> String {
        var s = utsname(); uname(&s)
        return withUnsafeBytes(of: s.machine) { b in
            b.bindMemory(to: CChar.self).baseAddress.map { String(cString: $0) } ?? "iPhone"
        }
    }
}
