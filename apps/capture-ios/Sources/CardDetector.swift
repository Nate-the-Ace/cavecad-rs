import CoreVideo
import Foundation
import simd

/// A camera frame's luma and depth, as handed to the detector queue: the luma pixel buffer (held only while
/// the detection runs, never the ARFrame) and the depth map already copied out.
struct CardFrame: @unchecked Sendable {
    let luma: CVPixelBuffer
    let depth: DepthGrid?
    let intrinsics: simd_float3x3
    let camera: simd_float4x4
}

struct CardResult: Sendable {
    let found: [CardSighting]
    let noDepth: [StationCard]
    /// How long the detector took on this frame, milliseconds.
    let millis: Double
    let width: Int, height: Int
}

extension DepthGrid {
    /// Copies an ARKit depth map (32-bit float metres) out; nil for any other layout.
    init?(copying buffer: CVPixelBuffer) {
        guard CVPixelBufferGetPixelFormatType(buffer) == kCVPixelFormatType_DepthFloat32 else { return nil }
        CVPixelBufferLockBaseAddress(buffer, .readOnly)
        defer { CVPixelBufferUnlockBaseAddress(buffer, .readOnly) }
        guard let base = CVPixelBufferGetBaseAddress(buffer) else { return nil }
        let w = CVPixelBufferGetWidth(buffer), h = CVPixelBufferGetHeight(buffer)
        let stride = CVPixelBufferGetBytesPerRow(buffer)
        var out = [Float](repeating: 0, count: w * h)
        for y in 0..<h {
            let row = base.advanced(by: y * stride).assumingMemoryBound(to: Float.self)
            for x in 0..<w { out[y * w + x] = row[x] }
        }
        self.init(width: w, height: h, values: out)
    }
}

/// Finds the station cards (AprilTag tag16h5) in camera frames, on its own serial queue. The caller keeps at
/// most one detection in flight; the C detector is only ever touched from that queue.
final class CardDetector: @unchecked Sendable {
    private let queue = DispatchQueue(label: "app.cavecad.capture.cards", qos: .utility)
    private let detector: UnsafeMutablePointer<apriltag_detector_t>
    private let family: UnsafeMutablePointer<apriltag_family_t>

    init() {
        family = tag16h5_create()
        detector = apriltag_detector_create()
        // bits_corrected 0: only an exact read counts as a card, and it is faster.
        apriltag_detector_add_family_bits(detector, family, 0)
        // full resolution: range is the point; the log reports how long each frame takes
        detector.pointee.quad_decimate = 1.0
        detector.pointee.nthreads = 2
        detector.pointee.refine_edges = true
    }

    deinit {
        apriltag_detector_destroy(detector)
        tag16h5_destroy(family)
    }

    func detect(_ frame: CardFrame, done: @escaping @Sendable (CardResult) -> Void) {
        queue.async { [self] in
            let t0 = DispatchTime.now().uptimeNanoseconds
            let raw = Self.run(detector, frame.luma)
            let w = CVPixelBufferGetWidthOfPlane(frame.luma, 0), h = CVPixelBufferGetHeightOfPlane(frame.luma, 0)
            let r = CardMath.sightings(from: raw, depth: frame.depth, imageW: w, imageH: h,
                                       intrinsics: frame.intrinsics, camera: frame.camera)
            let ms = Double(DispatchTime.now().uptimeNanoseconds - t0) / 1e6
            done(CardResult(found: r.found, noDepth: r.noDepth, millis: ms, width: w, height: h))
        }
    }

    /// Detects on the buffer's luma plane in place (no copy; the plane's row stride is passed on).
    private static func run(_ td: UnsafeMutablePointer<apriltag_detector_t>, _ buffer: CVPixelBuffer) -> [RawDetection] {
        CVPixelBufferLockBaseAddress(buffer, .readOnly)
        defer { CVPixelBufferUnlockBaseAddress(buffer, .readOnly) }
        guard let base = CVPixelBufferGetBaseAddressOfPlane(buffer, 0) else { return [] }
        var im = image_u8_t(width: Int32(CVPixelBufferGetWidthOfPlane(buffer, 0)),
                            height: Int32(CVPixelBufferGetHeightOfPlane(buffer, 0)),
                            stride: Int32(CVPixelBufferGetBytesPerRowOfPlane(buffer, 0)),
                            buf: base.assumingMemoryBound(to: UInt8.self))
        return detections(td, &im)
    }

    /// Runs the detector on an image and copies the results out.
    static func detections(_ td: UnsafeMutablePointer<apriltag_detector_t>, _ im: UnsafeMutablePointer<image_u8_t>) -> [RawDetection] {
        guard let list = apriltag_detector_detect(td, im) else { return [] }
        defer { apriltag_detections_destroy(list) }
        var out: [RawDetection] = []
        for i in 0..<Int(zarray_size(list)) {
            var d: UnsafeMutablePointer<apriltag_detection_t>?
            zarray_get(list, Int32(i), &d)
            guard let det = d?.pointee else { continue }
            let p = det.p
            out.append(RawDetection(id: Int(det.id), hamming: Int(det.hamming), margin: Double(det.decision_margin),
                                    center: SIMD2(det.c.0, det.c.1),
                                    corners: [SIMD2(p.0.0, p.0.1), SIMD2(p.1.0, p.1.1), SIMD2(p.2.0, p.2.1), SIMD2(p.3.0, p.3.1)]))
        }
        return out
    }

    /// Test hook: detect on an 8-bit grey image held by the caller.
    func detectGrey(_ pixels: inout [UInt8], width: Int, height: Int) -> [RawDetection] {
        pixels.withUnsafeMutableBufferPointer { p in
            var im = image_u8_t(width: Int32(width), height: Int32(height), stride: Int32(width), buf: p.baseAddress)
            return queue.sync { Self.detections(detector, &im) }
        }
    }
}
