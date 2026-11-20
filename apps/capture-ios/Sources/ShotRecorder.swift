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

/// Capture settings the person sets once and the phone remembers.
enum CaptureSettings {
    /// UserDefaults key for the phone's height above the station mark, metres.
    static let heightKey = "instrumentHeightM"
    static let defaultHeight = 1.0
    /// A usable height: 0–3 m; anything not a number goes back to the default.
    static func clampHeight(_ h: Double) -> Double { h.isFinite ? min(max(h, 0), 3) : defaultHeight }
}

/// A camera frame's pixels handed to the JPEG queue (only that frame's buffer, never the ARFrame).
private struct PixelBuffer: @unchecked Sendable { let buffer: CVPixelBuffer }

/// Turns frames into JPEGs on one background queue.
private final class JpegEncoder: @unchecked Sendable {
    private let queue = DispatchQueue(label: "app.cavecad.capture.jpeg", qos: .userInitiated)
    private let context = CIContext()

    func encode(_ px: PixelBuffer, done: @escaping @Sendable (Data?) -> Void) {
        queue.async { [context] in
            let jpeg = CGColorSpace(name: CGColorSpace.sRGB).flatMap { srgb in
                context.jpegRepresentation(of: CIImage(cvPixelBuffer: px.buffer), colorSpace: srgb,
                                           options: [kCGImageDestinationLossyCompressionQuality as CIImageRepresentationOption: 0.7])
            }
            done(jpeg)
        }
    }
}

/// Runs one shot: ARKit scene reconstruction, heading tap, keyframes; turns it into a bundle on Stop.
@MainActor
final class ShotRecorder: NSObject, ObservableObject, ARSessionDelegate {
    enum Phase { case aiming, capturing, saving, saved(URL), failed(String) }

    /// Photos kept per shot at most (memory and file size); the mesh keeps growing after.
    static let maxKeyframes = 240
    static let aimMessage = "Put the crosshair on the forward station and tap Aim."

    @Published var phase: Phase = .aiming
    @Published var message = aimMessage
    @Published var tapped = false
    @Published var keyframeCount = 0

    let session = ARSession()
    private var tap: (origin: SIMD3<Float>, dir: SIMD3<Float>)?
    private var startCamera = SIMD3<Float>(0, 0, 0)
    private var keyframes: [ShotKeyframe] = []
    private var lastYaw: Float = .nan, lastTime: TimeInterval = 0
    private var worstTracking = "normal"
    private let encoder = JpegEncoder()
    /// One JPEG in flight at most; a frame that arrives meanwhile is skipped.
    private var encoding = false
    /// Bumped per shot, so a JPEG finishing after Stop is not added to the next shot.
    private var shotSerial = 0
    private var photoLimitShown = false
    private var spaceWarned = false

    nonisolated static func shouldKeep(lastYaw: Float, yaw: Float, lastTime: TimeInterval, time: TimeInterval) -> Bool {
        if time - lastTime < 0.25 { return false }
        if lastYaw.isNaN { return true }
        var d = abs(yaw - lastYaw).truncatingRemainder(dividingBy: 360)
        if d > 180 { d = 360 - d }
        return d >= 15 || time - lastTime >= 2
    }

    nonisolated static func mergeAnchors(_ parts: [MeshPart]) -> MergedMesh {
        var out = MergedMesh(positions: [], normals: [], indices: [])
        let n = parts.reduce(0) { $0 + $1.positions.count }
        out.positions.reserveCapacity(n * 3); out.normals.reserveCapacity(n * 3)
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
                let r = rot * n
                let len = simd_length(r)
                // a zero or non-finite ARKit normal would make the whole mesh unwritable: point it up instead
                let w = len.isFinite && len > 1e-6 ? r / len : SIMD3<Float>(0, 1, 0)
                out.normals += [w.x, w.y, w.z]
            }
            out.indices += p.indices.map { $0 + base }
        }
        return out
    }

    /// True when the anchors hold any mesh at all.
    nonisolated static func hasMesh(_ parts: [MeshPart]) -> Bool { parts.contains { !$0.positions.isEmpty } }

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
        // Warn once when space is short; a second tap on Start goes ahead.
        if !spaceWarned, let free = try? URL(fileURLWithPath: NSHomeDirectory())
            .resourceValues(forKeys: [.volumeAvailableCapacityForImportantUsageKey]).volumeAvailableCapacityForImportantUsage,
            free < 2_000_000_000 {
            spaceWarned = true
            message = "Less than 2 GB free on the phone. Tap Start again to capture anyway."
            return
        }
        let t = f.camera.transform
        startCamera = SIMD3(t.columns.3.x, t.columns.3.y, t.columns.3.z)
        keyframes = []; keyframeCount = 0; lastYaw = .nan; lastTime = 0; worstTracking = "normal"
        photoLimitShown = false
        shotSerial += 1
        phase = .capturing
        UIApplication.shared.isIdleTimerDisabled = true
        message = "Capturing. Turn slowly; Stop when the room is covered."
    }

    // ARSession calls this on the main queue (no delegateQueue is set).
    nonisolated func session(_ session: ARSession, didUpdate frame: ARFrame) {
        let pose = frame.camera.transform
        let yaw = atan2(-pose.columns.2.x, -pose.columns.2.z) * 180 / .pi
        let time = frame.timestamp
        let state = frame.camera.trackingState
        let px = PixelBuffer(buffer: frame.capturedImage)
        let intrinsics = frame.camera.intrinsics
        MainActor.assumeIsolated {
            guard case .capturing = phase else { return }
            switch state {
            case .normal: break
            case .notAvailable: worstTracking = "lost"
            case .limited: if worstTracking == "normal" { worstTracking = "limited" }
            }
            if keyframes.count >= Self.maxKeyframes {
                if !photoLimitShown { photoLimitShown = true; message = "Photo limit reached; mesh still capturing." }
                return
            }
            guard !encoding, Self.shouldKeep(lastYaw: lastYaw, yaw: yaw, lastTime: lastTime, time: time) else { return }
            encoding = true
            lastYaw = yaw; lastTime = time
            let serial = shotSerial
            let w = CVPixelBufferGetWidth(px.buffer), h = CVPixelBufferGetHeight(px.buffer)
            encoder.encode(px) { [weak self] jpeg in
                Task { @MainActor [weak self] in
                    guard let self else { return }
                    self.encoding = false
                    guard serial == self.shotSerial, case .capturing = self.phase, let jpeg,
                          self.keyframes.count < Self.maxKeyframes else { return }
                    self.keyframes.append(ShotKeyframe(pose: pose, intrinsics: intrinsics, w: w, h: h, jpeg: jpeg))
                    self.keyframeCount = self.keyframes.count
                }
            }
        }
    }

    func stop(cave: KeptCave, over: Station, forward: Station, height: Double) {
        guard case .capturing = phase, let tap else { return }
        // Copy the meshes out while the anchors are still valid.
        let anchors = session.currentFrame?.anchors.compactMap { $0 as? ARMeshAnchor } ?? []
        let parts = anchors.compactMap(Self.part)
        guard Self.hasMesh(parts) else {
            // the session is still running: carry on capturing
            message = "No room mesh yet — keep turning before Stop."
            return
        }
        session.pause()
        UIApplication.shared.isIdleTimerDisabled = false
        phase = .saving
        message = "Saving the shot…"
        shotSerial += 1
        let job = ShotJob(caveId: cave.id, over: over, forward: forward, parts: parts, keyframes: keyframes,
                          tapOrigin: tap.origin, tapDir: tap.dir, startCamera: startCamera, tracking: worstTracking,
                          systemVersion: UIDevice.current.systemVersion, instrumentHeight: CaptureSettings.clampHeight(height))
        keyframes = []  // the job now holds the only reference, and drops each photo as it is written
        Task {
            let result = await Task.detached { job.write() }.value
            switch result {
            case .success(let url): phase = .saved(url); message = "Shot saved."
            case .failure(let m): phase = .failed(m.message); message = m.message
            }
        }
    }

    /// Ready for another shot from the same station.
    func reset() {
        shotSerial += 1
        tap = nil; tapped = false
        keyframes = []; keyframeCount = 0
        photoLimitShown = false
        phase = .aiming
        message = Self.aimMessage
        run()
    }

    /// The capture screen went away.
    func leave() {
        shotSerial += 1
        session.pause()
        UIApplication.shared.isIdleTimerDisabled = false
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

/// Everything Stop needs to turn a shot into a file, so the heavy part (merge, encode, write) runs off the main
/// actor. A class so the photos have one owner: `write` lets each go as soon as it is in the joined buffer.
/// Used by one task at a time.
final class ShotJob: @unchecked Sendable {
    let caveId: String
    let over: Station
    let forward: Station
    private var parts: [MeshPart]
    private var keyframes: [ShotKeyframe]
    let tapOrigin: SIMD3<Float>
    let tapDir: SIMD3<Float>
    let startCamera: SIMD3<Float>
    let tracking: String
    let systemVersion: String
    let instrumentHeight: Double

    init(caveId: String, over: Station, forward: Station, parts: [MeshPart], keyframes: [ShotKeyframe],
         tapOrigin: SIMD3<Float>, tapDir: SIMD3<Float>, startCamera: SIMD3<Float>, tracking: String,
         systemVersion: String, instrumentHeight: Double) {
        self.caveId = caveId; self.over = over; self.forward = forward; self.parts = parts; self.keyframes = keyframes
        self.tapOrigin = tapOrigin; self.tapDir = tapDir; self.startCamera = startCamera; self.tracking = tracking
        self.systemVersion = systemVersion; self.instrumentHeight = instrumentHeight
    }

    private static let unwritable = ShotFailure(message: "The shot could not be written.")

    /// Writes the shot once; the photos and mesh are gone from the job afterwards.
    func write() -> Result<URL, ShotFailure> {
        var mesh = ShotRecorder.mergeAnchors(parts)
        parts = []
        let shotId = UUID().uuidString.lowercased()

        // Join the photos, letting each one go as soon as it is copied in.
        var photos = keyframes
        keyframes = []
        var jpegs = Data()
        jpegs.reserveCapacity(photos.reduce(0) { $0 + $1.jpeg.count })
        var kfs: [[String: Any]] = []
        var numbers: [Double] = [instrumentHeight]
        photos.reverse()
        while let k = photos.popLast() {
            jpegs.append(k.jpeg)
            let p = k.pose, i = k.intrinsics
            let pose = [p.columns.0, p.columns.1, p.columns.2, p.columns.3].flatMap { [$0.x, $0.y, $0.z, $0.w] }.map(Double.init)
            let intr = [i.columns.0, i.columns.1, i.columns.2].flatMap { [$0.x, $0.y, $0.z] }.map(Double.init)
            numbers += pose + intr
            kfs.append(["pose": pose, "intrinsics": intr, "width": k.w, "height": k.h, "jpeg_len": k.jpeg.count])
        }
        let start = [startCamera.x, startCamera.y, startCamera.z].map(Double.init)
        let origin = [tapOrigin.x, tapOrigin.y, tapOrigin.z].map(Double.init)
        let dir = [tapDir.x, tapDir.y, tapDir.z].map(Double.init)
        numbers += start + origin + dir
        let header: [String: Any] = [
            "shot_id": shotId, "cave_id": caveId, "station_id": over.id, "station_label": over.label,
            "captured_at_ms": Int64(Date().timeIntervalSince1970 * 1000), "device": Self.model(),
            "instrument_height_m": instrumentHeight, "start_camera": start,
            "taps": [["target_station_id": forward.id, "target_label": forward.label, "origin": origin, "dir": dir]],
            "tracking": tracking, "arkit_version": systemVersion,
            "gravity": [0.0, -1.0, 0.0], "keyframes": kfs,
        ]
        guard numbers.allSatisfy(\.isFinite), JSONSerialization.isValidJSONObject(header),
              let headerData = try? JSONSerialization.data(withJSONObject: header)
        else { return .failure(Self.unwritable) }
        let bundle = Core.encodeShot(header: headerData, positions: mesh.positions, normals: mesh.normals,
                                     indices: mesh.indices, jpegs: jpegs)
        // The bundle holds its own copy now.
        jpegs = Data()
        mesh = MergedMesh(positions: [], normals: [], indices: [])
        guard !bundle.isEmpty else { return .failure(Self.unwritable) }
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
