import RealityKit
import simd
import UIKit

/// The earlier scan drawn over the live view: a see-through grey shell and red hole outlines.
enum GhostView {
    /// Outline strips as one triangle mesh.
    struct Ribbon { var positions: [SIMD3<Float>]; var indices: [UInt32] }

    /// Each outline segment as two thin strips at right angles (for a level segment one upright, one flat), so it
    /// shows from any side. Zero-length segments are skipped.
    static func ribbon(outlines: [[SIMD3<Float>]], width: Float) -> Ribbon {
        var r = Ribbon(positions: [], indices: [])
        let up = SIMD3<Float>(0, 1, 0)
        let half = width / 2
        for line in outlines where line.count >= 2 {
            for (a, b) in zip(line, line.dropFirst()) {
                let d = b - a
                guard simd_length(d) > 1e-6 else { continue }
                // across the segment, level; an upright segment takes east
                var side = simd_cross(d, up)
                side = simd_length(side) > 1e-6 ? simd_normalize(side) : SIMD3(1, 0, 0)
                // across both the segment and `side` (up for a level segment)
                let lift = simd_normalize(simd_cross(side, d))
                for off in [lift * half, side * half] {
                    let base = UInt32(r.positions.count)
                    r.positions += [a - off, a + off, b + off, b - off]
                    r.indices += [base, base + 1, base + 2, base, base + 2, base + 3]
                }
            }
        }
        return r
    }

    /// One anchor named "ghost" holding the shell and the outlines, at `transform` (the shot's frame placed in this
    /// session's frame). Both faces of everything draw: the shell is seen from inside the cave.
    @MainActor static func entity(_ g: Core.GhostMesh, transform: simd_float4x4) -> AnchorEntity {
        let anchor = AnchorEntity(world: transform)
        anchor.name = "ghost"
        if !g.indices.isEmpty {
            var shell = MeshDescriptor(name: "shell")
            shell.positions = MeshBuffers.Positions(g.positions)
            shell.primitives = .triangles(g.indices)
            var grey = UnlitMaterial(color: UIColor(white: 0.85, alpha: 1))
            grey.blending = .transparent(opacity: 0.3)
            grey.faceCulling = .none
            if let m = try? MeshResource.generate(from: [shell]) { anchor.addChild(ModelEntity(mesh: m, materials: [grey])) }
        }
        let rib = ribbon(outlines: g.outlines, width: 0.015)
        if !rib.indices.isEmpty {
            var edges = MeshDescriptor(name: "holes")
            edges.positions = MeshBuffers.Positions(rib.positions)
            edges.primitives = .triangles(rib.indices)
            var red = UnlitMaterial(color: .systemRed)
            red.faceCulling = .none
            if let m = try? MeshResource.generate(from: [edges]) {
                anchor.addChild(ModelEntity(mesh: m, materials: [red]))
            }
        }
        return anchor
    }
}
