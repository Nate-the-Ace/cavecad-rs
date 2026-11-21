import Metal
import SceneKit
import SwiftUI
import UIKit

/// A scan as SceneKit points: the survey points (x east, y north, z up, metres) shown with y up. Pure
/// builders, usable off the main actor.
enum PointCloud {
    /// The box around survey points (min, max); nil when there are none.
    static func bounds(_ points: [Float]) -> (min: SIMD3<Float>, max: SIMD3<Float>)? {
        var i = 0, lo = SIMD3<Float>(repeating: .infinity), hi = SIMD3<Float>(repeating: -.infinity)
        while i + 2 < points.count {
            let p = SIMD3(points[i], points[i + 1], points[i + 2])
            lo = simd_min(lo, p); hi = simd_max(hi, p)
            i += 3
        }
        return lo.x <= hi.x ? (lo, hi) : nil
    }

    /// The same box in SceneKit's frame (y up): survey (x, y, z) shows as (x, z, -y).
    static func sceneBox(min lo: SIMD3<Float>, max hi: SIMD3<Float>) -> (min: SIMD3<Float>, max: SIMD3<Float>) {
        (SIMD3(lo.x, lo.z, -hi.y), SIMD3(hi.x, hi.z, -lo.y))
    }

    /// A node of points in one flat colour (no lighting), or shaded by height when `colour` is nil.
    static func node(points: [Float], colour: UIColor?, pointSize: CGFloat = 2, maxRadius: CGFloat = 3) -> SCNNode {
        let verts = ShotView.sceneVertices(points)
        var sources = [SCNGeometrySource(vertices: verts)]
        if colour == nil, !verts.isEmpty {
            // darker low, lighter high: gives a flat picture some depth
            let ys = verts.map(\.y)
            let lo = ys.min() ?? 0, span = max((ys.max() ?? 0) - lo, 0.01)
            var rgb: [Float] = []
            rgb.reserveCapacity(verts.count * 3)
            for y in ys {
                let t = Float((y - lo) / span)
                rgb.append(contentsOf: [0.45 + 0.5 * t, 0.47 + 0.48 * t, 0.5 + 0.4 * t])
            }
            let data = rgb.withUnsafeBufferPointer { Data(buffer: $0) }
            sources.append(SCNGeometrySource(data: data, semantic: .color, vectorCount: verts.count, usesFloatComponents: true,
                                             componentsPerVector: 3, bytesPerComponent: 4, dataOffset: 0, dataStride: 12))
        }
        let el = SCNGeometryElement(indices: Array(0..<Int32(verts.count)), primitiveType: .point)
        el.pointSize = pointSize; el.minimumPointScreenSpaceRadius = 1; el.maximumPointScreenSpaceRadius = maxRadius
        let g = SCNGeometry(sources: sources, elements: [el])
        g.firstMaterial?.diffuse.contents = colour ?? UIColor.white
        g.firstMaterial?.lightingModel = .constant
        return SCNNode(geometry: g)
    }
}

/// The small picture of a shot in the saved list: the scan from the 3/4 view looking down 30° that the
/// shot viewer opens with, kept beside the shot as <shot>.thumb.png.
enum ShotThumbnail {
    /// Side of the picture in points, and the pixels per point it is made at.
    static let side: CGFloat = 80
    static let scale: CGFloat = 3

    /// A picture must be made again when there is none, or when the shot changed after it was made (a
    /// Resume rewrites the shot). When the shot's date is unknown, a picture that exists is kept.
    static func isStale(shotModified: Date?, thumbModified: Date?) -> Bool {
        guard let thumbModified else { return true }
        guard let shotModified else { return false }
        return shotModified > thumbModified
    }

    static func modified(_ url: URL) -> Date? {
        (try? FileManager.default.attributesOfItem(atPath: url.path))?[.modificationDate] as? Date
    }

    /// Renders thinned survey points (relative to the station) to PNG bytes; nil when there is nothing to show.
    static func render(points: [Float], with renderer: SCNRenderer) -> Data? {
        guard let b = PointCloud.bounds(points) else { return nil }
        let (lo, hi) = PointCloud.sceneBox(min: b.min, max: b.max)
        let scene = SCNScene()
        scene.background.contents = UIColor(white: 0.13, alpha: 1)
        scene.rootNode.addChildNode(PointCloud.node(points: points, colour: nil, pointSize: 1.5, maxRadius: 1.5))
        let rig = OrbitRig.framing(min: lo, max: hi)
        let cam = SCNNode()
        let c = SCNCamera()
        c.fieldOfView = CGFloat(OrbitRig.fovDeg)
        c.projectionDirection = .vertical
        c.zNear = Double(max(0.005, rig.distance * 0.01))
        c.zFar = Double(rig.distance * 4 + OrbitRig.radius(min: lo, max: hi) * 2)
        cam.camera = c
        cam.simdPosition = rig.cameraPosition
        cam.simdEulerAngles = SIMD3(rig.pitch, rig.yaw, 0)
        scene.rootNode.addChildNode(cam)
        renderer.scene = scene
        renderer.pointOfView = cam
        let px = side * scale
        let img = renderer.snapshot(atTime: 0, with: CGSize(width: px, height: px), antialiasingMode: .multisampling4X)
        renderer.scene = nil
        guard let cg = img.cgImage else { return nil }
        return UIImage(cgImage: cg, scale: scale, orientation: .up).pngData()
    }
}

/// Makes shot pictures one at a time, off the main actor, with its own Metal renderer.
actor ThumbnailMaker {
    static let shared = ThumbnailMaker()
    private var renderer: SCNRenderer?

    /// The shot's picture as PNG bytes: the kept one while it is fresh, else made now and kept. Nil when the
    /// shot cannot be read or placed with `pair`. A shot placed by its chain is turned by the chain's fix
    /// (`chain`); its picture is kept per whole degree of that turn.
    func picture(shot: URL, pair: [Double], chain: ChainPlace? = nil) -> Data? {
        let yaw = Int((chain?.fixYawDeg ?? 0).rounded())
        let thumb = ShotFiles.thumbnail(of: shot, yawDeg: yaw)
        if !ShotThumbnail.isStale(shotModified: ShotThumbnail.modified(shot), thumbModified: ShotThumbnail.modified(thumb)),
           let d = try? Data(contentsOf: thumb) {
            return d
        }
        guard let data = try? Data(contentsOf: shot, options: .alwaysMapped) else { return nil }
        let raw = Core.placedPointsThin(bundle: data, pair: pair)
        let points = chain?.apply(raw) ?? raw
        if renderer == nil, let device = MTLCreateSystemDefaultDevice() { renderer = SCNRenderer(device: device, options: nil) }
        guard let renderer, let png = ShotThumbnail.render(points: points, with: renderer) else { return nil }
        try? png.write(to: thumb, options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])
        // pictures made for an earlier turn of the chain are out of date
        for old in ShotFiles.thumbnails(of: shot) where old.lastPathComponent != thumb.lastPathComponent {
            try? FileManager.default.removeItem(at: old)
        }
        return png
    }
}

/// A saved shot's picture, with a spinner while it is made and a plain mark when there is none.
struct ShotThumbView: View {
    let url: URL
    /// Nil when the shot cannot be placed (its station is gone): the plain mark, nothing made.
    let pair: [Double]?
    /// The shot file's date, so a rewritten shot gets a new picture.
    let modified: Date?
    /// Its place after card chaining; the picture follows the chain's turn.
    var chain: ChainPlace? = nil

    private enum Shown { case making, picture(UIImage), none }
    @State private var shown = Shown.making

    var body: some View {
        ZStack {
            switch shown {
            case .making: ProgressView()
            case .picture(let i): Image(uiImage: i).resizable().scaledToFill()
            case .none: Image(systemName: "cube.transparent").font(.title2).foregroundStyle(.secondary)
            }
        }
        .frame(width: ShotThumbnail.side, height: ShotThumbnail.side)
        .background(Color(white: 0.13))
        .clipShape(RoundedRectangle(cornerRadius: 8))
        .task(id: "\(url.path)|\(modified?.timeIntervalSince1970 ?? 0)|\(Int((chain?.fixYawDeg ?? 0).rounded()))|\(pair ?? [])") {
            guard let pair else { shown = .none; return }
            let png = await ThumbnailMaker.shared.picture(shot: url, pair: pair, chain: chain)
            shown = png.flatMap { UIImage(data: $0, scale: ShotThumbnail.scale) }.map(Shown.picture) ?? .none
        }
    }
}
