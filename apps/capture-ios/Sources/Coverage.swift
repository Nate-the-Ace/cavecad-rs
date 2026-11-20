import simd

/// Which directions around the gimbal's pivot still see no surface, as yaw/pitch relative to home.
enum Coverage {
    /// +1 when target yaw turns the camera clockwise seen from above (right); the bench test sets it.
    nonisolated(unsafe) static var yawSign = 1.0
    static let cell = 10.0, reach = 5.0, elevations = (-45.0, 65.0)

    struct Direction: Equatable { let yaw: Double; let pitch: Double }

    /// Cell centres with no mesh hit within `reach` from `pivot`. `homeForward` is the camera's forward at home
    /// (ARKit frame, y up).
    static func missing(mesh: MeshSnapshot, pivot: SIMD3<Double>, homeForward: SIMD3<Double>) -> [Direction] {
        let f = simd_normalize(SIMD3(homeForward.x, 0, homeForward.z))
        let homeAz = atan2(f.x, -f.z) * 180 / .pi           // clockwise from -z
        var out: [Direction] = []
        var el = elevations.0 + cell / 2
        while el < elevations.1 {
            var az = cell / 2
            while az < 360 {
                let a = az * .pi / 180, e = el * .pi / 180
                let dir = SIMD3(sin(a) * cos(e), sin(e), -cos(a) * cos(e))
                let hit = mesh.hits(origin: pivot, dir: dir).first.map { $0 <= reach } ?? false
                if !hit {
                    var yaw = az - homeAz
                    yaw = (yaw + 540).truncatingRemainder(dividingBy: 360) - 180
                    out.append(Direction(yaw: yaw * yawSign, pitch: el))
                }
                az += cell
            }
            el += cell
        }
        return out
    }
}
