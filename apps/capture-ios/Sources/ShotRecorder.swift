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

/// The capture rig.
enum Rig {
    /// The phone's height above the station mark: the gimbal's roll pivot, metres. Mirrors
    /// `cave_capture::frame::PIVOT_HEIGHT_M` (crates/cave-capture/src/frame.rs); change both together.
    static let pivotHeightM = 0.30
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

/// Runs one shot: ARKit scene reconstruction with a compass-aligned frame, keyframes; turns it into a bundle on Stop.
@MainActor
final class ShotRecorder: NSObject, ObservableObject, ARSessionDelegate {
    enum Phase { case ready, capturing, saving, saved(URL), failed(String) }

    /// Photos kept per shot at most (memory and file size); the mesh keeps growing after.
    static let maxKeyframes = 240
    static let readyMessage = "Stand the gimbal on the station, then press Start and let it turn."
    /// Seconds between compass lines in the log while capturing (drift shows there).
    static let compassLogEvery: TimeInterval = 5

    @Published var phase: Phase = .ready { didSet { Log.write("shot", "phase \(phase)") } }
    @Published var message = readyMessage { didSet { if message != oldValue { Log.write("shot", message) } } }
    @Published var keyframeCount = 0
    /// Waiting for a heading before the session runs (Start is off meanwhile).
    @Published var findingNorth = false

    let session = ARSession()
    private let compass = CompassWatcher()
    /// The compass when ARKit fixed north (the session's run): what the shot's frame really used.
    private var compassAtAlignment = CompassReading.none
    /// Bumped on every run and leave, so a late heading wait does not start a session that was left.
    private var runSerial = 0
    private var lastCompassLog: TimeInterval = 0
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
        // .gravityAndHeading fixes north when the session runs: start the compass first and wait (up to 3 s)
        // for a valid heading, so the alignment has one and the header records that very reading.
        compass.start()
        runSerial += 1
        let serial = runSerial
        findingNorth = true
        message = "Finding north…"
        Task {
            let found = await compass.firstValid(timeout: 3)
            guard serial == runSerial else { return }
            findingNorth = false
            if let found {
                compassAtAlignment = found
                Log.write("compass", "at alignment: \(found.logLine)")
            } else {
                compassAtAlignment = CompassReading(trueNorth: false, accuracyDeg: -1, headingDeg: compass.latest.headingDeg)
                Log.write("compass", "no heading at alignment")
            }
            message = Self.readyMessage
            runSession()
        }
    }

    private func runSession() {
        let c = ARWorldTrackingConfiguration()
        c.sceneReconstruction = .mesh
        c.frameSemantics = [.sceneDepth]
        c.isLightEstimationEnabled = true
        // x east, y up, z south: the compass turns the shot, no heading tap needed
        c.worldAlignment = .gravityAndHeading
        session.delegate = self
        session.run(c, options: [.resetTracking, .removeExistingAnchors])
    }

    func start() {
        guard !findingNorth, let f = session.currentFrame else { return }
        if (f.lightEstimate?.ambientIntensity ?? 0) < 150 { message = "Too dark to track. Turn the light on."; return }
        guard case .normal = f.camera.trackingState else { message = "Hold still until tracking settles."; return }
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
        lastCompassLog = f.timestamp
        Log.write("compass", "at start: \(compass.latest.logLine) (aligned with \(compassAtAlignment.logLine))")
        phase = .capturing
        UIApplication.shared.isIdleTimerDisabled = true
        // An unsure compass does not stop the shot, but it must be seen.
        message = compassAtAlignment.warning ?? "Capturing. Turn slowly; Stop when the room is covered."
    }

    // ARSession calls this on the main queue (no delegateQueue is set).
    nonisolated func session(_ session: ARSession, cameraDidChangeTrackingState camera: ARCamera) {
        Log.write("ar", "tracking \(camera.trackingState)")
    }

    nonisolated func session(_ session: ARSession, didFailWithError error: Error) {
        Log.write("ar", "session failed: \(error.localizedDescription)")
    }

    nonisolated func sessionWasInterrupted(_ session: ARSession) { Log.write("ar", "session interrupted") }

    nonisolated func sessionInterruptionEnded(_ session: ARSession) { Log.write("ar", "interruption ended") }

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
            if time - lastCompassLog >= Self.compassLogEvery {
                lastCompassLog = time
                Log.write("compass", compass.latest.logLine)
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

    func stop(cave: KeptCave, over: Station) {
        guard case .capturing = phase else { return }
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
        Log.write("compass", "at stop: \(compass.latest.logLine)")
        let job = ShotJob(caveId: cave.id, over: over, parts: parts, keyframes: keyframes, startCamera: startCamera,
                          tracking: worstTracking, systemVersion: UIDevice.current.systemVersion, compass: compassAtAlignment)
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
        keyframes = []; keyframeCount = 0
        photoLimitShown = false
        phase = .ready
        message = Self.readyMessage
        run()
    }

    /// The capture screen went away.
    func leave() {
        shotSerial += 1
        runSerial += 1
        findingNorth = false
        session.pause()
        compass.stop()
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
    private var parts: [MeshPart]
    private var keyframes: [ShotKeyframe]
    let startCamera: SIMD3<Float>
    let tracking: String
    let systemVersion: String
    let compass: CompassReading
    /// Always the gimbal's pivot height: every shot stands on the same gimbal.
    let instrumentHeight = Rig.pivotHeightM

    init(caveId: String, over: Station, parts: [MeshPart], keyframes: [ShotKeyframe], startCamera: SIMD3<Float>,
         tracking: String, systemVersion: String, compass: CompassReading) {
        self.caveId = caveId; self.over = over; self.parts = parts; self.keyframes = keyframes
        self.startCamera = startCamera; self.tracking = tracking; self.systemVersion = systemVersion; self.compass = compass
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
        numbers += start + [compass.accuracyDeg]
        let header: [String: Any] = [
            "shot_id": shotId, "cave_id": caveId, "station_id": over.id, "station_label": over.label,
            "captured_at_ms": Int64(Date().timeIntervalSince1970 * 1000), "device": Self.model(),
            "instrument_height_m": instrumentHeight, "start_camera": start,
            // a compass shot: the frame is gravityAndHeading, so no taps
            "taps": [Any](), "compass": ["true_north": compass.trueNorth, "accuracy_deg": compass.accuracyDeg],
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
