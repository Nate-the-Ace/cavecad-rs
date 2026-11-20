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

/// ARKit mesh anchors handed to the card mesh queue (ARKit hands out a new anchor object per update, so these
/// are not changed while held; the ARFrame is not held).
private struct MeshAnchors: @unchecked Sendable { let anchors: [ARMeshAnchor] }

/// A station card to draw in the live view: where it was seen, in the capture frame (ARKit world).
struct CardMarker: Equatable, Sendable { let card: StationCard; let position: SIMD3<Float> }

/// Turns frames into JPEGs on one background queue.
private final class JpegEncoder: @unchecked Sendable {
    private let queue = DispatchQueue(label: "app.cavecad.capture.jpeg", qos: .userInitiated)
    private let context = CIContext()

    /// `width` x `height` is the size the JPEG is written at (the frame scaled down when it is bigger).
    func encode(_ px: PixelBuffer, width: Int, height: Int, done: @escaping @Sendable (Data?) -> Void) {
        queue.async { [context] in
            var image = CIImage(cvPixelBuffer: px.buffer)
            let sx = CGFloat(width) / image.extent.width, sy = CGFloat(height) / image.extent.height
            if sx != 1 || sy != 1 { image = image.transformed(by: CGAffineTransform(scaleX: sx, y: sy)) }
            let jpeg = CGColorSpace(name: CGColorSpace.sRGB).flatMap { srgb in
                context.jpegRepresentation(of: image, colorSpace: srgb,
                                           options: [kCGImageDestinationLossyCompressionQuality as CIImageRepresentationOption: 0.7])
            }
            done(jpeg)
        }
    }
}

/// Runs the shots: ARKit scene reconstruction with a compass-aligned frame, keyframes; turns each pass into a bundle
/// on Pause. One for the app's whole run (with its one ARSession), so leaving the capture screen and coming back to
/// the same station resumes the session.
@MainActor
final class ShotRecorder: NSObject, ObservableObject, ARSessionDelegate {
    enum Phase { case ready, capturing, saving, saved(URL), failed(String) }

    /// Photos kept per shot at most (memory and file size); the mesh keeps growing after.
    static let maxKeyframes = 240
    /// What to do before a pass; `resume` when it adds to the station's shot (the button reads Resume).
    static func readyMessage(resume: Bool) -> String {
        "Put the CURRENT card on this station (or stand the gimbal on it), FORWARD and REAR on the next and previous ones, then press \(resume ? "Resume" : "Start")."
    }
    static let capturingMessage = "Capturing. Walk or turn slowly and look at each card. Pause to save; Stop to finish."
    static let savedMessage = "Saved. Resume to fill gaps; Stop to finish."

    /// A button on the capture screen.
    enum Control: Equatable, Sendable { case start, resume, pause, stop, viewShot }

    /// The buttons for a phase, in screen order: Start or Resume when ready (Resume when there is a shot to add
    /// to); Pause and Stop while capturing; View shot, Resume and Stop while paused; Resume after a failed save.
    nonisolated static func controls(_ phase: Phase, hasParent: Bool) -> [Control] {
        switch phase {
        case .ready: [hasParent ? .resume : .start]
        case .capturing: [.pause, .stop]
        case .saving: []
        case .saved: [.viewShot, .resume, .stop]
        case .failed: [.resume]
        }
    }

    /// ARKit's live mesh is drawn only while capturing; paused, the scan shows as its ghost instead (ARKit keeps
    /// tracking and reconstructing underneath, unseen).
    nonisolated static func showsLiveMesh(_ phase: Phase) -> Bool {
        if case .capturing = phase { return true }
        return false
    }
    static let separateMessage = "Saved as a separate scan: it couldn't be lined up with the earlier one."
    /// Seconds between compass lines in the log while capturing (drift shows there).
    static let compassLogEvery: TimeInterval = 5
    /// Keyframe JPEGs are at most this wide (the camera format can be 4K; files must not balloon).
    static let maxKeyframeWidth = 1920
    /// Card detection runs at most this often (seconds apart), one at a time.
    static let cardEvery: TimeInterval = 1.0 / 3
    /// Seconds between card summaries in the log.
    static let cardLogEvery: TimeInterval = 5
    /// Seconds between passes that meet every card sight line with the scan as it now stands.
    static let cardMeshEvery: TimeInterval = 2

    @Published var phase: Phase = .ready { didSet { Log.write("shot", "phase \(phase)") } }
    @Published var message = ShotRecorder.readyMessage(resume: false) { didSet { if message != oldValue { Log.write("shot", message) } } }
    @Published var keyframeCount = 0
    /// Waiting for a heading before the session runs (Start is off meanwhile).
    @Published var findingNorth = false
    /// Station cards seen so far this shot, for the live view.
    @Published var cardMarkers: [CardMarker] = []
    /// The station's shot this capture adds to (nil: the next Pause makes a new shot).
    @Published private(set) var parent: URL?
    /// When the parent was made, for the screen.
    @Published private(set) var parentMade: Date?
    /// How this ARKit session's frame maps into the parent shot's frame (column-major 4x4). nil: unknown (the
    /// parent is from an earlier run, or tracking was reset): the cards may line the pass up, else Pause saves it
    /// as a separate shot. Identity: this session made (or continued) the shot in its own frame. Otherwise: the card fit, valid
    /// for every later pass of this session. Cleared whenever tracking is reset.
    private(set) var sessionToShot: [Double]? { didSet { ghostTransform = Self.ghostPlacement(sessionToShot) } }
    /// The parent's ghost for this pass (nil: none loaded, or the pass was saved and the file changed).
    @Published private(set) var ghost: Core.GhostMesh?
    /// Changes whenever a different ghost is loaded, so the live view builds each one once.
    @Published private(set) var ghostKey = 0
    /// Where the shot's frame sits in this session's: the inverse of `sessionToShot` (nil: hidden until lined up).
    @Published private(set) var ghostTransform: simd_float4x4?
    /// The parent file as read for this pass (mapped), for the card check and line-up.
    private var parentData: Data?
    /// Bumped per ghost load and whenever the loaded one is dropped, so a late load is not used.
    private var ghostLoad = 0
    /// The file a ghost load is running for (nil: none running), so Resume does not start a second one.
    private var ghostLoading: URL?
    /// The saved shot to open in the viewer after Stop (drives the capture screen's navigation).
    @Published var viewing: URL?
    /// The pass being saved was ended with Stop: once saved, the scan ends and the viewer opens.
    private var stopAfterSave = false
    /// Cards already warned about as moved this pass.
    private var warnedCards: Set<StationCard> = []
    /// The settled cards (id: position) the last check or line-up ran with; it runs again only when they change.
    private var checkedCards: [Int: SIMD3<Double>] = [:]
    /// One check or line-up in flight at most.
    private var checkingCards = false
    static let lookForGhostMessage = "Look at two cards to show the earlier scan."
    /// A ghost with more triangles than this is rebuilt on a coarser grid (it must not cost the scan its frame rate).
    nonisolated static let ghostMaxTriangles = 40_000
    nonisolated static let ghostCoarseCellM: Float = 0.08
    /// Still too many at the coarse grid: rebuilt once more on this one.
    nonisolated static let ghostCoarserCellM: Float = 0.12
    /// A settled card farther than this from where the shot has it warns, metres; also the worst misfit a card
    /// line-up accepts when the core gives no verdict ("ok") of its own.
    nonisolated static let cardTolerance = 0.05
    /// The shot `sessionToShot` maps into (a different parent makes it unknown).
    private var sessionShot: URL?
    /// The station the ARKit session ran over; nil until one has run (or after it failed). Coming back to the
    /// same station resumes the session without a reset, so the frame (and `sessionToShot`) is kept.
    private var sessionStation: String?
    /// The next `run` resumes the paused session instead of starting tracking afresh.
    private var resumeNext = false
    /// The cave and station the screen is over now (from `prepare`, `stationKey`); the session takes it when it runs.
    private var preparedStation: String?
    /// Bumped whenever tracking is reset, so a save that finishes after a reset does not claim the new frame.
    private var trackingEpoch = 0
    nonisolated static let identity: [Double] = [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]

    /// How Pause saves.
    enum SaveMode: Equatable {
        /// No parent: a new shot file.
        case newShot
        /// A parent in an unknown frame (the cards did not line the pass up): a new shot over the same station,
        /// which becomes the parent. Pause never blocks.
        case separateShot
        /// The parent is in this session's frame: ARKit's whole mesh replaces the shot's.
        case sameSession
        /// Lined up by the cards: the pass is moved by the transform and its mesh added.
        case cards([Double])
    }

    /// Pause's choice, from whether there is a parent and how this session maps into it.
    nonisolated static func saveMode(hasParent: Bool, sessionToShot: [Double]?) -> SaveMode {
        guard hasParent else { return .newShot }
        guard let t = sessionToShot else { return .separateShot }
        return t == identity ? .sameSession : .cards(t)
    }

    /// The rule for a card line-up: the core's verdict `ok` (each card within its own limit: 5 cm, or 1.5% of its
    /// distance to the other cards when that is more); from a core without one, a worst misfit up to 5 cm. Accepted:
    /// the transform. Refused: the message names the worst card by misfit over its limit.
    nonisolated static func lineUp(fit: [String: Any]) -> (transform: [Double]?, message: String) {
        let worst = (fit["worst_m"] as? Double) ?? .infinity
        let ok = (fit["ok"] as? Bool) ?? (worst <= cardTolerance)
        guard ok, let t = fit["transform"] as? [Double], t.count == 16, t.allSatisfy(\.isFinite) else {
            var limits: [Int: Double] = [:]
            for l in fit["limits"] as? [[String: Any]] ?? [] {
                if let id = (l["id"] as? NSNumber)?.intValue, let m = l["m"] as? Double, m > 0 { limits[id] = m }
            }
            func ratio(_ o: [String: Any]) -> Double {
                let id = (o["id"] as? NSNumber)?.intValue ?? -1
                return (o["m"] as? Double ?? 0) / (limits[id] ?? cardTolerance)
            }
            let mis = (fit["misfits"] as? [[String: Any]] ?? []).max { ratio($0) < ratio($1) }
            let word = (mis?["id"] as? NSNumber).flatMap { StationCard(rawValue: $0.intValue) }?.word ?? "A card"
            let m = (mis?["m"] as? Double) ?? worst
            let cm = m.isFinite ? "\(Int((m * 100).rounded())) cm" : "too far"
            return (nil, "\(word) doesn't match where it was (\(cm)). Put it back; Pause will save this as a separate scan.")
        }
        return (t, String(format: "Lined up by cards (worst %.0f mm).", worst * 1000))
    }

    /// Where the ghost (the shot's frame) sits in this session's frame: the inverse of session -> shot, exactly
    /// identity for the same session; nil when the line-up is unknown or not a 4x4.
    nonisolated static func ghostPlacement(_ sessionToShot: [Double]?) -> simd_float4x4? {
        guard let t = sessionToShot, t.count == 16 else { return nil }
        if t == identity { return matrix_identity_float4x4 }
        let f = t.map(Float.init)
        let m = simd_float4x4(columns: (SIMD4(f[0], f[1], f[2], f[3]), SIMD4(f[4], f[5], f[6], f[7]),
                                        SIMD4(f[8], f[9], f[10], f[11]), SIMD4(f[12], f[13], f[14], f[15])))
        return m.inverse
    }

    /// The settled cards differ from those last checked: one came or went, or one moved more than `by` metres.
    nonisolated static func cardsChanged(last: [Int: SIMD3<Double>], now: [Int: SIMD3<Double>], by: Double = 0.02) -> Bool {
        guard Set(last.keys) == Set(now.keys) else { return true }
        return now.contains { id, p in last[id].map { simd_distance($0, p) > by } ?? true }
    }

    /// The cards the core's same-session offsets put more than 5 cm from the shot's, not yet warned about.
    nonisolated static func moved(offsets: [[String: Any]], warned: Set<StationCard>) -> [(card: StationCard, m: Double)] {
        offsets.compactMap { o in
            guard let id = (o["id"] as? NSNumber)?.intValue, let m = o["m"] as? Double, m > cardTolerance,
                  let c = StationCard(rawValue: id), !warned.contains(c) else { return nil }
            return (c, m)
        }
    }

    /// Tracking was (or is about to be) reset: this session's frame no longer relates to any shot.
    private func dropFrame(_ why: String) {
        trackingEpoch += 1
        if sessionToShot != nil { Log.write("shot", "line-up with the shot dropped: \(why)") }
        sessionToShot = nil; sessionShot = nil
    }

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
    /// Bumped per shot, so a JPEG finishing after Pause is not added to the next shot.
    private var shotSerial = 0
    private var photoLimitShown = false
    private var spaceWarned = false
    private let cardDetector = CardDetector()
    private var cardTracker = CardTracker()
    /// One card detection in flight at most; a frame that arrives meanwhile is skipped.
    private var detectingCards = false
    /// Every card seen is settled: detection runs slowly. Refreshed only where the tracker changes.
    private var calm = false
    private var frameCount = 0, lastFpsTime: TimeInterval = 0, lastFrameTime: TimeInterval = 0
    private var lastCardTime: TimeInterval = -1, lastCardLog: TimeInterval = 0
    private var cardRuns = 0, cardMillis = 0.0, cardMaxMillis = 0.0
    private var cardFrameLogged = false
    /// One mesh pass in flight at most: every card sight line met with the latest scan, off the main thread.
    private var meshing = false
    private var lastMeshTime: TimeInterval = 0
    private var meshRuns = 0, meshMillis = 0.0, meshCopyMillis = 0.0
    /// Card reads ignored as too weak this shot, per card.
    private var weakReads: [StationCard: Int] = [:]
    /// Each seen card's ARKit anchor; ARKit moves anchors when it corrects its map, and the card's sightings move
    /// with it.
    private var cardAnchors: [StationCard: UUID] = [:]
    /// How far map corrections have moved each card's anchor this shot, metres.
    private var anchorMoves: [StationCard: Double] = [:]
    /// Sight lines met with the mesh per pass at most.
    static let meshBatchLimit = 40
    /// Mesh anchors whose origin is this far from every card are not copied for the card passes, metres.
    nonisolated static let meshAnchorReach = 4.0
    private static let meshQueue = DispatchQueue(label: "app.cavecad.capture.cards.mesh", qos: .utility)
    /// The highest camera format is in use (a failed start falls back to the default for the app's run).
    private var usingBestFormat = false
    private var bestFormatFailed = false

    nonisolated static func shouldKeep(lastYaw: Float, yaw: Float, lastTime: TimeInterval, time: TimeInterval) -> Bool {
        if time - lastTime < 0.25 { return false }
        if lastYaw.isNaN { return true }
        var d = abs(yaw - lastYaw).truncatingRemainder(dividingBy: 360)
        if d > 180 { d = 360 - d }
        return d >= 15 || time - lastTime >= 2
    }

    /// The camera format to prefer: most pixels (range for the cards doubles with resolution), then the lower
    /// frame rate (lighter for LiDAR and the detector). Index into the list, nil when it is empty.
    nonisolated static func bestFormatIndex(_ formats: [(width: Int, height: Int, fps: Int)]) -> Int? {
        formats.indices.max { a, b in
            let (x, y) = (formats[a], formats[b])
            if x.width * x.height != y.width * y.height { return x.width * x.height < y.width * y.height }
            return x.fps > y.fps
        }
    }

    /// A frame's JPEG size: as it is when no wider than `maxWidth`, else scaled to that width.
    nonisolated static func keyframeSize(width: Int, height: Int, maxWidth: Int) -> (width: Int, height: Int) {
        guard width > maxWidth, maxWidth > 0, width > 0 else { return (width, height) }
        return (maxWidth, max(1, Int((Double(height) * Double(maxWidth) / Double(width)).rounded())))
    }

    /// Intrinsics for the same frame written at `from` -> `to` pixels.
    nonisolated static func scaledIntrinsics(_ k: simd_float3x3, from: (width: Int, height: Int), to: (width: Int, height: Int)) -> simd_float3x3 {
        guard from.width > 0, from.height > 0 else { return k }
        let sx = Float(to.width) / Float(from.width), sy = Float(to.height) / Float(from.height)
        var o = k
        o.columns.0.x *= sx; o.columns.1.y *= sy; o.columns.2.x *= sx; o.columns.2.y *= sy
        return o
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

    /// Called when the capture screen opens (or comes back) over a station, before `run`: the station's newest
    /// shot becomes the parent. Back at the same station after a session ran (say from viewing the shot), the
    /// next `run` resumes that session and keeps how it maps into the shot; otherwise tracking starts afresh and
    /// a pass into an existing shot must be lined up by the cards.
    ///
    /// The recorder outlives the screen: at another station whatever the last screen left (the pass, its markers,
    /// the saved or failed state, the message) is cleared; a save still running finishes and is then let go.
    func prepare(cave: KeptCave, over: Station) {
        let key = Self.stationKey(cave: cave.id, station: over.id)
        parent = ShotFiles.parent(cave: cave.id, station: over.id)
        parentMade = parent.flatMap { try? $0.resourceValues(forKeys: [.creationDateKey]).creationDate }
        let sameScreen = preparedStation == key
        preparedStation = key
        resumeNext = sessionStation == key
        if !resumeNext {
            dropFrame("another station")
        } else if parent != sessionShot {
            dropFrame("the station's shot changed")
        }
        // Back from viewing the shot in the same session: the paused scan's ghost stays; otherwise it goes.
        if !(sameScreen && resumeNext && parent != nil && parent == sessionShot) { dropGhost() }
        if !sameScreen { viewing = nil; stopAfterSave = false }
        if case .saving = phase {
            // the save lands (or is let go) when it finishes
        } else if !sameScreen {
            clearPass()
            phase = .ready
            message = readyText
        } else if case .ready = phase {
            message = readyText
        }
        if let parent {
            Log.write("shot", "adding to the station's shot \(parent.lastPathComponent) (\(sessionToShot == nil ? "needs a card line-up" : "lined up in this session"))")
        }
    }

    /// Which station a session or screen is over: station ids are only unique within a cave.
    nonisolated static func stationKey(cave: String, station: String) -> String { "\(cave)/\(station)" }

    /// The ready message for the station's state now.
    private var readyText: String { Self.readyMessage(resume: parent != nil) }

    /// Forgets the pass in hand (photos, cards, markers, warnings); the session and the line-up are kept.
    private func clearPass() {
        shotSerial += 1
        keyframes = []; keyframeCount = 0
        cardTracker = CardTracker(); cardMarkers = []; calm = false
        photoLimitShown = false
        warnedCards = []; checkedCards = [:]
    }

    func run() {
        guard Self.hasLiDAR else {
            phase = .failed("This phone has no LiDAR.")
            message = "This phone has no LiDAR."
            return
        }
        compass.start()
        runSerial += 1
        if resumeNext {
            // ARKit keeps its world, mesh and anchors over a pause and relocalizes; north was fixed at the first run.
            resumeNext = false
            findingNorth = false
            runSession(reset: false)
            return
        }
        // .gravityAndHeading fixes north when the session runs: start the compass first and wait (up to 3 s)
        // for a valid heading, so the alignment has one and the header records that very reading.
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
            message = readyText
            runSession(reset: true)
        }
    }

    /// Runs the session: `reset` starts tracking afresh (a new frame), otherwise a paused session resumes.
    private func runSession(reset: Bool) {
        let c = ARWorldTrackingConfiguration()
        c.sceneReconstruction = .mesh
        c.frameSemantics = [.sceneDepth]
        c.isLightEstimationEnabled = true
        // x east, y up, z south: the compass turns the shot, no heading tap needed
        c.worldAlignment = .gravityAndHeading
        let formats = ARWorldTrackingConfiguration.supportedVideoFormats
        usingBestFormat = false
        if !bestFormatFailed, let i = Self.bestFormatIndex(formats.map {
            (Int($0.imageResolution.width), Int($0.imageResolution.height), $0.framesPerSecond)
        }) {
            c.videoFormat = formats[i]
            usingBestFormat = true
            Log.write("ar", "video format \(Int(formats[i].imageResolution.width))x\(Int(formats[i].imageResolution.height)) @\(formats[i].framesPerSecond) fps (best of \(formats.count))")
        } else {
            Log.write("ar", "video format default \(Int(c.videoFormat.imageResolution.width))x\(Int(c.videoFormat.imageResolution.height)) @\(c.videoFormat.framesPerSecond) fps")
        }
        session.delegate = self
        sessionStation = preparedStation
        if reset {
            dropFrame("tracking started afresh")
            Log.write("ar", "started fresh")
            session.run(c, options: [.resetTracking, .removeExistingAnchors])
        } else {
            Log.write("ar", "resumed without reset")
            session.run(c)
        }
    }

    /// After an interruption ARKit relocalizes into the same world, so the frame (and `sessionToShot`) holds.
    nonisolated func sessionShouldAttemptRelocalization(_ session: ARSession) -> Bool { true }

    func start() {
        guard !findingNorth, let f = session.currentFrame else { return }
        if (f.lightEstimate?.ambientIntensity ?? 0) < 150 { message = "Too dark to track. Turn the light on."; return }
        guard case .normal = f.camera.trackingState else { message = "Hold still until tracking settles."; return }
        // Warn once when space is short; a second tap on Start goes ahead.
        if !spaceWarned, let free = try? URL(fileURLWithPath: NSHomeDirectory())
            .resourceValues(forKeys: [.volumeAvailableCapacityForImportantUsageKey]).volumeAvailableCapacityForImportantUsage,
            free < 2_000_000_000 {
            spaceWarned = true
            message = "Less than 2 GB free on the phone. Tap again to capture anyway."
            return
        }
        let t = f.camera.transform
        startCamera = SIMD3(t.columns.3.x, t.columns.3.y, t.columns.3.z)
        keyframes = []; keyframeCount = 0; lastYaw = .nan; lastTime = 0; worstTracking = "normal"
        photoLimitShown = false
        shotSerial += 1
        cardTracker = CardTracker(); cardMarkers = []
        detectingCards = false; lastCardTime = -1; lastCardLog = f.timestamp
        calm = false; frameCount = 0; lastFpsTime = f.timestamp; lastFrameTime = f.timestamp
        cardRuns = 0; cardMillis = 0; cardMaxMillis = 0; cardFrameLogged = false
        meshing = false; lastMeshTime = f.timestamp; meshRuns = 0; meshMillis = 0; meshCopyMillis = 0; weakReads = [:]
        removeCardAnchors()
        lastCompassLog = f.timestamp
        warnedCards = []; checkedCards = [:]; checkingCards = false
        Log.write("compass", "at start: \(compass.latest.logLine) (aligned with \(compassAtAlignment.logLine))")
        phase = .capturing
        UIApplication.shared.isIdleTimerDisabled = true
        // After a Pause the saved file's ghost is loaded (or loading) already: it is this pass's parent.
        if let p = parent, parentData == nil, ghostLoading != p { loadGhost(p) }
        // An unsure compass does not stop the shot, but it must be seen.
        let going = parent != nil && sessionToShot == nil
            ? Self.lookForGhostMessage
            : Self.capturingMessage
        message = compassAtAlignment.warning.map { "\($0) \(going)" } ?? going
    }

    /// Reads the parent and builds its ghost off the main actor (once per saved file: after each save, and at a
    /// pass's start when none is loaded).
    private func loadGhost(_ p: URL) {
        ghostLoad += 1
        let n = ghostLoad
        ghostLoading = p
        Task {
            let t0 = Date()
            let (data, g, coarse) = await Task.detached { () -> (Data?, Core.GhostMesh?, Bool) in
                guard let d = try? Data(contentsOf: p, options: .alwaysMapped) else { return (nil, nil, false) }
                var g = Core.ghost(bundle: d)
                var coarse = false
                // finer grids first; each one still over the cap is rebuilt on the next
                for (from, to) in [(Float(0.05), Self.ghostCoarseCellM), (Self.ghostCoarseCellM, Self.ghostCoarserCellM)] {
                    guard let big = g, big.indices.count / 3 > Self.ghostMaxTriangles else { break }
                    Log.write("ghost", "\(big.indices.count / 3) triangles at \(Int((from * 100).rounded())) cm: rebuilding at \(Int((to * 100).rounded())) cm")
                    g = Core.ghost(bundle: d, cellM: to)
                    coarse = true
                }
                return (d, g, coarse)
            }.value
            guard n == ghostLoad, p == parent else { return }
            ghostLoading = nil
            parentData = data
            ghost = g
            ghostKey += 1
            if let g {
                Log.write("ghost", "\(g.indices.count / 3) triangles, \(g.outlines.count) outlines, \(Int(Date().timeIntervalSince(t0) * 1000)) ms\(coarse ? " (coarse)" : "")")
            } else {
                Log.write("ghost", data == nil ? "the earlier scan could not be read" : "no ghost from the earlier scan")
            }
            // Cards may have settled while it loaded.
            checkCards()
        }
    }

    /// Forgets the loaded parent and its ghost (the file changed, or another parent): the next pass reloads.
    private func dropGhost() {
        ghostLoad += 1
        ghostLoading = nil
        parentData = nil
        if ghost != nil { ghost = nil; ghostKey += 1 }
    }

    /// With the parent in this session's frame, warns about cards that moved; with no line-up yet, tries to line
    /// up by the settled cards. Runs off the main actor, one at a time, and only when the settled cards change.
    private func checkCards() {
        guard case .capturing = phase, parent != nil, let d = parentData, !checkingCards else { return }
        let same = sessionToShot == Self.identity
        // lined up by cards already: nothing to do
        guard same || sessionToShot == nil else { return }
        let settled = cardTracker.seen.filter { $0.track.settled }
        guard settled.count >= (same ? 1 : 2) else { return }
        let now = Dictionary(settled.map { ($0.track.card.rawValue, $0.estimate.point) }, uniquingKeysWith: { a, _ in a })
        guard Self.cardsChanged(last: checkedCards, now: now) else { return }
        checkedCards = now
        let recs = cardTracker.records.filter { now[$0.id] != nil }
        checkingCards = true
        let serial = shotSerial, epoch = trackingEpoch, into = parent, warned = warnedCards
        Task {
            let out = await Task.detached { () -> CardCheck in
                let r = Core.cardsFit(bundle: d, cards: recs, sameSession: same)
                if same { return .moved(Self.moved(offsets: r["offsets"] as? [[String: Any]] ?? [], warned: warned)) }
                guard let fit = r["fit"] as? [String: Any] else {
                    return .noFit(r["error"] as? String ?? "the cards do not match the earlier scan's, or are too close together")
                }
                let l = Self.lineUp(fit: fit)
                return .lineUp(transform: l.transform, message: l.message, yawDeg: (fit["yaw_rad"] as? Double ?? 0) * 180 / .pi,
                               worstMm: (fit["worst_m"] as? Double ?? 0) * 1000)
            }.value
            checkingCards = false
            guard serial == shotSerial, epoch == trackingEpoch, into == parent, case .capturing = phase else { return }
            switch out {
            case .moved(let cards):
                for (c, m) in cards where !warnedCards.contains(c) {
                    warnedCards.insert(c)
                    message = "\(c.word) is \(Int((m * 100).rounded())) cm from where it was: card moved, or tracking slipped?"
                    Log.write("cards", "check: \(c.word) \(Int((m * 1000).rounded())) mm from the earlier scan")
                }
            case .lineUp(let t, let msg, let yaw, let worst):
                message = msg
                if let t, sessionToShot == nil {
                    sessionShot = into
                    sessionToShot = t
                    Log.write("cards", String(format: "lined up by cards: yaw %.1f deg, worst %.0f mm", yaw, worst))
                } else {
                    Log.write("cards", "card line-up refused: \(msg)")
                }
            case .noFit(let why):
                Log.write("cards", "card line-up: no fit (\(why))")
            }
            // Cards may have changed while it ran.
            checkCards()
        }
    }

    // ARSession calls this on the main queue (no delegateQueue is set).
    nonisolated func session(_ session: ARSession, cameraDidChangeTrackingState camera: ARCamera) {
        Log.write("ar", "tracking \(camera.trackingState)")
    }

    nonisolated func session(_ session: ARSession, didFailWithError error: Error) {
        Log.write("ar", "session failed: \(error.localizedDescription)")
        MainActor.assumeIsolated {
            // A failed session has no world to come back to: the next run starts afresh.
            sessionStation = nil
            dropFrame("the session failed")
            // The best camera format may not run with scene reconstruction and depth: try the default, once.
            guard usingBestFormat, !bestFormatFailed, case .ready = phase else { return }
            bestFormatFailed = true
            Log.write("ar", "best video format failed to run; falling back to the default format")
            runSession(reset: true)
        }
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
        let depthBuffer = frame.sceneDepth.map { PixelBuffer(buffer: $0.depthMap) }
        let confidenceBuffer = frame.sceneDepth?.confidenceMap.map { PixelBuffer(buffer: $0) }
        MainActor.assumeIsolated {
            guard case .capturing = phase else { return }
            frameCount += 1; lastFrameTime = time
            switch state {
            case .normal: break
            case .notAvailable: worstTracking = "lost"
            case .limited: if worstTracking == "normal" { worstTracking = "limited" }
            }
            if time - lastCompassLog >= Self.compassLogEvery {
                lastCompassLog = time
                Log.write("compass", compass.latest.logLine)
            }
            detectCards(px: px, depth: depthBuffer, confidence: confidenceBuffer, intrinsics: intrinsics, pose: pose, time: time)
            meshCards(time: time)
            if keyframes.count >= Self.maxKeyframes {
                if !photoLimitShown { photoLimitShown = true; message = "Photo limit reached; mesh still capturing." }
                return
            }
            guard !encoding, Self.shouldKeep(lastYaw: lastYaw, yaw: yaw, lastTime: lastTime, time: time) else { return }
            encoding = true
            lastYaw = yaw; lastTime = time
            let serial = shotSerial
            let full = (width: CVPixelBufferGetWidth(px.buffer), height: CVPixelBufferGetHeight(px.buffer))
            let out = Self.keyframeSize(width: full.width, height: full.height, maxWidth: Self.maxKeyframeWidth)
            let (w, h) = (out.width, out.height)
            let k = Self.scaledIntrinsics(intrinsics, from: full, to: out)
            encoder.encode(px, width: w, height: h) { [weak self] jpeg in
                Task { @MainActor [weak self] in
                    guard let self else { return }
                    self.encoding = false
                    guard serial == self.shotSerial, case .capturing = self.phase, let jpeg,
                          self.keyframes.count < Self.maxKeyframes else { return }
                    self.keyframes.append(ShotKeyframe(pose: pose, intrinsics: k, w: w, h: h, jpeg: jpeg))
                    self.keyframeCount = self.keyframes.count
                }
            }
        }
    }

    /// Hands this frame's luma (and a copy of its depth) to the card detector, at most 3 a second, one at a time.
    private func detectCards(px: PixelBuffer, depth: PixelBuffer?, confidence: PixelBuffer?, intrinsics: simd_float3x3,
                             pose: simd_float4x4, time: TimeInterval) {
        if time - lastCardLog >= Self.cardLogEvery {
            lastCardLog = time
            logCards("summary", time: time)
        }
        guard !detectingCards, time - lastCardTime >= (calm ? 2.0 : Self.cardEvery) else { return }
        detectingCards = true
        lastCardTime = time
        let serial = shotSerial
        guard let luma = GreyImage(lumaOf: px.buffer) else { detectingCards = false; return }
        let frame = CardFrame(luma: luma, depth: depth.flatMap { DepthGrid(copying: $0.buffer, confidence: confidence?.buffer) },
                              intrinsics: intrinsics, camera: pose)
        cardDetector.detect(frame, decimate: calm ? 2 : 1, threads: calm ? 1 : 2) { [weak self] result in
            Task { @MainActor [weak self] in self?.cardsDone(result, serial: serial) }
        }
    }

    private func cardsDone(_ r: CardResult, serial: Int) {
        detectingCards = false
        guard serial == shotSerial, case .capturing = phase else { return }
        if !cardFrameLogged {
            cardFrameLogged = true
            Log.write("cards", "detecting on \(r.width)x\(r.height) luma, depth \(r.depthWidth)x\(r.depthHeight)")
            // Depth is mapped to the image by scaling each axis; a different aspect means a crop, and cards
            // would be placed with the wrong depth.
            if r.depthWidth > 0, r.depthHeight > 0,
               abs(Double(r.width) / Double(r.height) - Double(r.depthWidth) / Double(r.depthHeight)) > 0.02 {
                Log.write("cards", "WARNING depth aspect differs from the image: card depths may be wrong")
            }
        }
        cardRuns += 1; cardMillis += r.millis; cardMaxMillis = max(cardMaxMillis, r.millis)
        for c in r.noRange { cardTracker.addMissed(c) }
        for c in r.weak { weakReads[c, default: 0] += 1 }
        for s in r.found {
            if cardTracker.add(s) {
                Log.write("cards", String(format: "first sighting: %d %@, size says %@, lidar says %@, margin %.1f",
                                          s.card.rawValue, s.card.word, Self.metres(s.sizeRange), Self.metres(s.lidarRange), s.margin))
            }
        }
        if !r.found.isEmpty { pinNewCards(); updateCardMarkers() }
        refreshCalm()
        checkCards()
    }

    /// Re-reads whether every card is settled; call wherever the tracker changes, not every frame.
    private func refreshCalm() {
        let now = cardTracker.allSettled
        guard now != calm else { return }
        calm = now
        Log.write("cards", now ? "settled, backing off" : "searching at full rate")
    }

    /// Gives each newly seen card an ARKit anchor where it is placed now.
    private func pinNewCards() {
        for (t, e) in cardTracker.seen where cardAnchors[t.card] == nil {
            var m = matrix_identity_float4x4
            m.columns.3 = SIMD4(Float(e.point.x), Float(e.point.y), Float(e.point.z), 1)
            let a = ARAnchor(name: "card-\(t.card.rawValue)", transform: m)
            session.add(anchor: a)
            cardAnchors[t.card] = a.identifier
            cardTracker.pin(t.card, Self.double(m))
        }
    }

    private func removeCardAnchors() {
        let ids = Set(cardAnchors.values)
        for a in session.currentFrame?.anchors ?? [] where ids.contains(a.identifier) { session.remove(anchor: a) }
        cardAnchors = [:]; anchorMoves = [:]
    }

    nonisolated static func double(_ m: simd_float4x4) -> simd_double4x4 {
        simd_double4x4(SIMD4<Double>(m.columns.0), SIMD4<Double>(m.columns.1), SIMD4<Double>(m.columns.2), SIMD4<Double>(m.columns.3))
    }

    // ARKit corrected its map and moved some anchors (main queue).
    nonisolated func session(_ session: ARSession, didUpdate anchors: [ARAnchor]) {
        let moved = anchors.compactMap { a in a.name?.hasPrefix("card-") == true ? (a.identifier, a.transform) : nil }
        guard !moved.isEmpty else { return }
        MainActor.assumeIsolated {
            guard case .capturing = phase else { return }
            var any = false
            for (id, m) in moved {
                guard let card = cardAnchors.first(where: { $0.value == id })?.key else { continue }
                let d = cardTracker.moveAnchor(card, Self.double(m))
                if d > 0 { anchorMoves[card, default: 0] += d; any = true }
            }
            if any { updateCardMarkers(); refreshCalm(); checkCards() }
        }
    }

    nonisolated static func metres(_ v: Double?) -> String { v.map { String(format: "%.2f m", $0) } ?? "nothing" }

    /// The live markers follow the estimates as they are now (new sightings, a better mesh).
    private func updateCardMarkers() {
        let m = cardTracker.seen.map { t, e in CardMarker(card: t.card, position: SIMD3(Float(e.point.x), Float(e.point.y), Float(e.point.z))) }
        if m != cardMarkers { cardMarkers = m }
    }

    /// Every couple of seconds, meets card sight lines with the scan as it stands now, so a card's place improves
    /// as the mesh does. Only the anchor list is taken here; copying the mesh and the line tests run on their own
    /// queue, off the main thread.
    private func meshCards(time: TimeInterval) {
        guard !meshing, time - lastMeshTime >= Self.cardMeshEvery else { return }
        let sightings = cardTracker.meshBatch(limit: Self.meshBatchLimit, skipSettled: true)
        guard !sightings.isEmpty else { return }
        lastMeshTime = time
        let near = cardTracker.points
        let held = MeshAnchors(anchors: session.currentFrame?.anchors.compactMap { $0 as? ARMeshAnchor } ?? [])
        meshing = true
        let serial = shotSerial
        Self.meshQueue.async { [weak self] in
            let c0 = DispatchTime.now().uptimeNanoseconds
            // Copy only anchors near a card (an anchor's mesh lies within a few metres of its origin); the
            // snapshot then keeps only the parts whose box reaches within 1 m of one.
            let parts = held.anchors.filter { a in
                let o = SIMD3<Double>(Double(a.transform.columns.3.x), Double(a.transform.columns.3.y), Double(a.transform.columns.3.z))
                return near.contains { simd_distance($0, o) <= Self.meshAnchorReach }
            }.compactMap(Self.part)
            let t0 = DispatchTime.now().uptimeNanoseconds
            let ranges = Self.hasMesh(parts) ? CardTracker.meshRanges(sightings, MeshSnapshot(parts, near: near)) : [:]
            let copyMs = Double(t0 - c0) / 1e6, ms = Double(DispatchTime.now().uptimeNanoseconds - t0) / 1e6
            Task { @MainActor [weak self] in
                guard let self else { return }
                self.meshing = false
                guard serial == self.shotSerial, case .capturing = self.phase, !ranges.isEmpty else { return }
                self.meshRuns += 1; self.meshMillis += ms; self.meshCopyMillis += copyMs
                self.cardTracker.applyMesh(ranges, pass: self.meshRuns)
                self.updateCardMarkers()
                self.refreshCalm()
                self.checkCards()
            }
        }
    }

    /// The card summary in the log: every seen card, then how the detector is doing.
    private func logCards(_ when: String, time: TimeInterval) {
        if time > lastFpsTime, lastFpsTime > 0 {
            Log.write("cards", String(format: "%@: ARKit %.0f fps", when, Double(frameCount) / (time - lastFpsTime)))
        }
        lastFpsTime = time; frameCount = 0
        let lines = cardTracker.logLines
        if lines.isEmpty { Log.write("cards", "\(when): none seen yet") } else { lines.forEach { Log.write("cards", "\(when): \($0)") } }
        if cardRuns > 0 {
            Log.write("cards", String(format: "%@: detector %d runs, avg %.0f ms, max %.0f ms", when, cardRuns, cardMillis / Double(cardRuns), cardMaxMillis))
        }
        if !weakReads.isEmpty {
            let w = weakReads.sorted { $0.key.rawValue < $1.key.rawValue }.map { "\($0.key.word) \($0.value)" }.joined(separator: ", ")
            Log.write("cards", "\(when): weak reads ignored: \(w)")
        }
        if !anchorMoves.isEmpty {
            let m = anchorMoves.sorted { $0.key.rawValue < $1.key.rawValue }
                .map { String(format: "%@ %.0f mm", $0.key.word, $0.value * 1000) }.joined(separator: ", ")
            Log.write("cards", "\(when): ARKit map corrections moved card anchors: \(m)")
        }
        if meshRuns > 0 {
            Log.write("cards", String(format: "%@: mesh passes %d, avg %.0f ms (+ %.0f ms copying the mesh, off the main thread)", when, meshRuns,
                                      meshMillis / Double(meshRuns), meshCopyMillis / Double(meshRuns)))
        }
    }

    /// Pause: saves the pass (into the station's shot, or as a new or separate one); tracking keeps running for
    /// Resume.
    func pause(cave: KeptCave, over: Station) {
        guard case .capturing = phase else { return }
        // Never refused for want of a line-up: a pass the cards did not line up becomes a separate shot.
        let mode = Self.saveMode(hasParent: parent != nil, sessionToShot: sessionToShot)
        // Copy the meshes out while the anchors are still valid.
        let anchors = session.currentFrame?.anchors.compactMap { $0 as? ARMeshAnchor } ?? []
        let parts = anchors.compactMap(Self.part)
        guard Self.hasMesh(parts) else {
            // the session is still running: carry on capturing
            message = "No room mesh yet — keep turning before Pause."
            return
        }
        // The session keeps running (tracking kept for Resume); only leaving the screen pauses it.
        UIApplication.shared.isIdleTimerDisabled = false
        phase = .saving
        message = "Saving the shot…"
        shotSerial += 1
        Log.write("compass", "at stop: \(compass.latest.logLine)")
        logCards("at stop", time: lastFrameTime)
        let job = ShotJob(caveId: cave.id, over: over, parts: parts, keyframes: keyframes, startCamera: startCamera,
                          tracking: worstTracking, systemVersion: UIDevice.current.systemVersion, compass: compassAtAlignment,
                          cards: cardTracker)
        keyframes = []  // the job now holds the only reference, and drops each photo as it is written
        let into = parent
        // A tracking reset while saving means this session's frame is no longer the shot's.
        let epoch = trackingEpoch
        // Leaving for another station while saving lets the result go (the next screen is not its).
        let screen = preparedStation
        Task {
            let (result, passes, separate) = await Task.detached { () -> (Result<URL, ShotFailure>, Int, Bool) in
                var r: Result<URL, ShotFailure>
                var separate = false
                switch (mode, into) {
                case (.sameSession, let u?): r = job.writePass(into: u, sameSession: true, transform: nil)
                // v1 limit: in a card-lined session ARKit's mesh is cumulative, so every later pass of the session
                // adds the session's earlier geometry again (overlapping triangles are allowed by the spec).
                case (.cards(let t), let u?): r = job.writePass(into: u, sameSession: false, transform: t)
                case (.separateShot, _): r = job.write(); separate = true
                default: r = job.write()
                }
                // The earlier scan is gone or unreadable: the pass is kept as a shot of its own instead.
                if case .failure(let f) = r, f.parentUnreadable {
                    Log.write("shot", "the earlier scan could not be read: saving the pass as a separate shot")
                    r = job.write(); separate = true
                }
                guard case .success(let url) = r else { return (r, 0, separate) }
                let n = (try? Data(contentsOf: url, options: .alwaysMapped)).map { Core.summary(bundle: $0, pair: nil)["passes"] as? Int ?? 0 } ?? 0
                return (r, n, separate)
            }.value
            switch result {
            case .success(let url):
                // A fresh shot (first or separate) is in this session's own frame; a pass leaves the line-up as it was.
                if mode == .newShot || separate, epoch == trackingEpoch { sessionToShot = Self.identity }
                if epoch == trackingEpoch, sessionToShot != nil { sessionShot = url }
                Log.write("shot", separate ? "saved as a separate shot (no line-up)" : "saved pass \(max(passes, 1))")
                let thenEnd = stopAfterSave
                stopAfterSave = false
                guard screen == preparedStation else {
                    phase = .ready; message = readyText
                    return
                }
                parent = url
                parentMade = separate ? Date() : (parentMade ?? Date())
                // The file changed: its new ghost replaces the old one.
                dropGhost()
                if thenEnd { end(); viewing = url; return }
                // Shown while paused (ARKit's live mesh is hidden then) and kept as the next pass's parent.
                loadGhost(url)
                phase = .saved(url); message = separate ? Self.separateMessage : Self.savedMessage
            case .failure(let m):
                stopAfterSave = false
                guard screen == preparedStation else { phase = .ready; message = readyText; return }
                phase = .failed(m.message); message = m.message
            }
        }
    }

    /// Stop: capturing, saves the pass exactly as Pause does and then ends; paused, ends at once. Ending opens the
    /// saved shot in the viewer (`viewing`). A pass with no mesh yet is refused as Pause refuses it.
    func finish(cave: KeptCave, over: Station) {
        switch phase {
        case .capturing:
            stopAfterSave = true
            pause(cave: cave, over: over)
            // refused (no mesh yet): still capturing
            if case .capturing = phase { stopAfterSave = false }
        case .saved(let url):
            end()
            viewing = url
        default:
            break
        }
    }

    /// Ends the scan at this station: ARKit pauses and the line-up with the shot is forgotten (the next visit
    /// starts tracking afresh), the ghost and the pass are dropped, and the screen is ready again.
    private func end() {
        Log.write("shot", "stopped, opening the viewer")
        runSerial += 1
        findingNorth = false
        session.pause()
        compass.stop()
        UIApplication.shared.isIdleTimerDisabled = false
        sessionStation = nil
        resumeNext = false
        dropFrame("stopped")
        dropGhost()
        clearPass()
        phase = .ready
        message = readyText
    }

    /// Ready for another pass at the same station, in the same ARKit session (tracking and the line-up with the
    /// shot are kept). Does not run the session again.
    func scanMore() {
        clearPass()
        phase = .ready
        message = readyText
    }

    /// Ready for another shot from the same station, with tracking started afresh.
    func reset() {
        scanMore()
        resumeNext = false
        run()
    }

    /// The capture screen went away. The session pauses (battery) and a pass in hand is dropped; the recorder,
    /// the session's world and the line-up are kept for coming back to the same station.
    func leave() {
        if case .capturing = phase {
            Log.write("shot", "left the capture screen mid-pass: the pass is dropped")
            clearPass()
            phase = .ready
        }
        shotSerial += 1
        runSerial += 1
        findingNorth = false
        session.pause()
        compass.stop()
        UIApplication.shared.isIdleTimerDisabled = false
    }

    /// One ARKit mesh anchor as plain arrays; nil for a layout this code does not read (LiDAR meshes are
    /// float3 vertices/normals and 32-bit triangle indices).
    nonisolated private static func part(_ a: ARMeshAnchor) -> MeshPart? {
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

struct ShotFailure: Error {
    let message: String
    /// A pass could not read the shot it adds to (nothing was written): Pause saves a separate shot instead.
    var parentUnreadable = false
}

/// What a card check or line-up found, handed back from off the main actor.
private enum CardCheck: Sendable {
    /// Same session: cards more than 5 cm from where the shot has them.
    case moved([(card: StationCard, m: Double)])
    /// A card fit: its transform when accepted (pass -> shot), else nil, and the message for the screen.
    case lineUp(transform: [Double]?, message: String, yawDeg: Double, worstMm: Double)
    /// No fit could be made (unmatched cards, or too close together).
    case noFit(String)
}

/// Everything Pause needs to turn a shot into a file, so the heavy part (merge, encode, write) runs off the main
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
    /// Pass flags (`gimbal`, `tracking lost`, `gimbal lost`, `low battery`): the shot's header for a new shot, the pass record for a pass.
    let flags: [String]
    /// Station cards seen during the scan; their sight lines meet the final mesh in `write`.
    private var cards: CardTracker
    /// Always the gimbal's pivot height: every shot stands on the same gimbal.
    let instrumentHeight = Rig.pivotHeightM

    init(caveId: String, over: Station, parts: [MeshPart], keyframes: [ShotKeyframe], startCamera: SIMD3<Float>,
         tracking: String, systemVersion: String, compass: CompassReading, cards: CardTracker = CardTracker(),
         flags: [String] = []) {
        self.cards = cards; self.flags = flags
        self.caveId = caveId; self.over = over; self.parts = parts; self.keyframes = keyframes
        self.startCamera = startCamera; self.tracking = tracking; self.systemVersion = systemVersion; self.compass = compass
    }

    private static let unwritable = ShotFailure(message: "The shot could not be written.")

    /// Writes the shot once; the photos and mesh are gone from the job afterwards.
    func write() -> Result<URL, ShotFailure> {
        // The cards' last word: every sight line met with the final mesh.
        if !cards.allSightings.isEmpty {
            cards.applyMesh(CardTracker.meshRanges(cards.allSightings, MeshSnapshot(parts, near: cards.points)))
            cards.logLines.forEach { Log.write("cards", "final (full mesh): \($0)") }
        }
        var mesh = ShotRecorder.mergeAnchors(parts)
        parts = []
        let shotId = UUID().uuidString.lowercased()

        var (kfs, jpegs, numbers) = joinedPhotos()
        numbers.append(instrumentHeight)
        let start = [startCamera.x, startCamera.y, startCamera.z].map(Double.init)
        numbers += start + [compass.accuracyDeg]
        let header: [String: Any] = [
            "shot_id": shotId, "cave_id": caveId, "station_id": over.id, "station_label": over.label,
            "captured_at_ms": Int64(Date().timeIntervalSince1970 * 1000), "device": Self.model(),
            "instrument_height_m": instrumentHeight, "start_camera": start,
            // a compass shot: the frame is gravityAndHeading, so no taps
            "taps": [Any](), "compass": ["true_north": compass.trueNorth, "accuracy_deg": compass.accuracyDeg],
            "tracking": tracking, "arkit_version": systemVersion,
            "gravity": [0.0, -1.0, 0.0], "keyframes": kfs, "cards": cards.records.map(\.json),
            "flags": flags,
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

    /// Adds this job as a pass to the shot at `parent`. Same session: the mesh replaces the shot's (ARKit's
    /// whole current mesh, which holds the earlier passes) and the transform is identity. Otherwise `transform`
    /// (column-major, this pass's frame to the shot's, from the cards) moves the pass and its mesh is added.
    /// The parent is rewritten through a temporary file; on any failure it is left as it was.
    func writePass(into parent: URL, sameSession: Bool, transform: [Double]?) -> Result<URL, ShotFailure> {
        guard let old = try? Data(contentsOf: parent) else {
            return .failure(ShotFailure(message: "The earlier scan could not be read.", parentUnreadable: true))
        }
        if !cards.allSightings.isEmpty {
            cards.applyMesh(CardTracker.meshRanges(cards.allSightings, MeshSnapshot(parts, near: cards.points)))
            cards.logLines.forEach { Log.write("cards", "final (full mesh): \($0)") }
        }
        var mesh = ShotRecorder.mergeAnchors(parts)
        parts = []
        var (kfs, jpegs, numbers) = joinedPhotos()
        let identity: [Double] = [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]
        let t = sameSession ? identity : (transform ?? identity)
        numbers += t
        let pass: [String: Any] = [
            "captured_at_ms": Int64(Date().timeIntervalSince1970 * 1000),
            "aligned_by": sameSession ? "same_session" : "cards",
            "transform": t, "replace": sameSession, "tracking": tracking,
            "cards": cards.records.map(\.json), "keyframes": kfs, "flags": flags,
        ]
        guard t.count == 16, numbers.allSatisfy(\.isFinite), JSONSerialization.isValidJSONObject(pass),
              let pj = try? JSONSerialization.data(withJSONObject: pass)
        else { return .failure(Self.unwritable) }
        let bundle = Core.addPass(bundle: old, pass: pj, positions: mesh.positions, normals: mesh.normals,
                                  indices: mesh.indices, jpegs: jpegs)
        jpegs = Data()
        mesh = MergedMesh(positions: [], normals: [], indices: [])
        guard !bundle.isEmpty else { return .failure(Self.unwritable) }
        return ShotFiles.replace(parent, with: bundle)
            ? .success(parent) : .failure(ShotFailure(message: "Saving the pass failed; the earlier scan is kept."))
    }

    /// Joins the photos into one JPEG run with their header entries, letting each photo go as soon as it is
    /// copied in. `numbers` holds every pose and intrinsics value, for the finite check.
    private func joinedPhotos() -> (kfs: [[String: Any]], jpegs: Data, numbers: [Double]) {
        var photos = keyframes
        keyframes = []
        var jpegs = Data()
        jpegs.reserveCapacity(photos.reduce(0) { $0 + $1.jpeg.count })
        var kfs: [[String: Any]] = []
        var numbers: [Double] = []
        photos.reverse()
        while let k = photos.popLast() {
            jpegs.append(k.jpeg)
            let p = k.pose, i = k.intrinsics
            let pose = [p.columns.0, p.columns.1, p.columns.2, p.columns.3].flatMap { [$0.x, $0.y, $0.z, $0.w] }.map(Double.init)
            let intr = [i.columns.0, i.columns.1, i.columns.2].flatMap { [$0.x, $0.y, $0.z] }.map(Double.init)
            numbers += pose + intr
            kfs.append(["pose": pose, "intrinsics": intr, "width": k.w, "height": k.h, "jpeg_len": k.jpeg.count])
        }
        return (kfs, jpegs, numbers)
    }

    private static func model() -> String {
        var s = utsname(); uname(&s)
        return withUnsafeBytes(of: s.machine) { b in
            b.bindMemory(to: CChar.self).baseAddress.map { String(cString: $0) } ?? "iPhone"
        }
    }
}
