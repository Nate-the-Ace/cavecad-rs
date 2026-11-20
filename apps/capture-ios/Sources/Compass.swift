import CoreLocation
import Foundation

/// The compass when a shot starts. ARKit's `.gravityAndHeading` frame already points its axes at
/// north from this same compass; the reading is kept so a spoiled heading shows in the shot and the log.
struct CompassReading: Equatable, Sendable {
    /// iOS gave a true-north heading (location allowed); false means magnetic north.
    let trueNorth: Bool
    /// iOS's heading accuracy, degrees; negative when iOS called the heading invalid.
    let accuracyDeg: Double
    /// The heading itself, degrees clockwise from north (log only).
    let headingDeg: Double

    init(trueNorth: Bool, accuracyDeg: Double, headingDeg: Double) {
        self.trueNorth = trueNorth; self.accuracyDeg = accuracyDeg; self.headingDeg = headingDeg
    }

    /// From a `CLHeading`'s numbers: a negative true heading means iOS had no true north.
    init(trueHeading: Double, magneticHeading: Double, accuracy: Double) {
        trueNorth = trueHeading >= 0
        accuracyDeg = accuracy
        headingDeg = trueHeading >= 0 ? trueHeading : magneticHeading
    }

    /// No heading at all yet.
    static let none = CompassReading(trueNorth: false, accuracyDeg: -1, headingDeg: 0)

    /// Worse than this many degrees counts as unsure.
    static let unsureAboveDeg = 20.0

    /// What to tell the person when the compass is unsure (the shot still saves), else nil.
    var warning: String? {
        guard accuracyDeg < 0 || accuracyDeg > Self.unsureAboveDeg else { return nil }
        let n = accuracyDeg < 0 ? "?" : String(Int(accuracyDeg.rounded()))
        return "Compass unsure (±\(n)°). The gimbal's magnet may be pulling it; the shot will still save."
    }

    var logLine: String {
        String(format: "heading %.1f° %@ ±%@", headingDeg, trueNorth ? "true" : "magnetic",
               accuracyDeg < 0 ? "invalid" : String(format: "%.0f°", accuracyDeg))
    }
}

/// Watches the phone's compass. Asks for When-In-Use location so iOS can give true north (underground
/// with no fix it falls back to magnetic, which the reading records).
@MainActor
final class CompassWatcher: NSObject, CLLocationManagerDelegate {
    private let manager = CLLocationManager()
    private(set) var latest = CompassReading.none

    override init() {
        super.init()
        manager.delegate = self
        manager.desiredAccuracy = kCLLocationAccuracyKilometer
        manager.headingFilter = 1
    }

    func start() {
        guard CLLocationManager.headingAvailable() else { Log.write("compass", "no compass on this phone"); return }
        if manager.authorizationStatus == .notDetermined { manager.requestWhenInUseAuthorization() }
        manager.startUpdatingLocation()
        manager.startUpdatingHeading()
    }

    func stop() {
        manager.stopUpdatingHeading()
        manager.stopUpdatingLocation()
    }

    // The manager was made on the main thread, so it calls back there.
    nonisolated func locationManager(_ manager: CLLocationManager, didUpdateHeading h: CLHeading) {
        let r = CompassReading(trueHeading: h.trueHeading, magneticHeading: h.magneticHeading, accuracy: h.headingAccuracy)
        MainActor.assumeIsolated { latest = r }
    }

    nonisolated func locationManagerDidChangeAuthorization(_ manager: CLLocationManager) {
        let status = manager.authorizationStatus
        Log.write("compass", "location permission \(status.rawValue)")
    }

    nonisolated func locationManager(_ manager: CLLocationManager, didFailWithError error: Error) {
        Log.write("compass", "location failed: \(error.localizedDescription)")
    }
}
