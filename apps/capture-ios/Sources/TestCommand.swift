#if DEBUG
import Foundation

/// A test command from the Mac, over the cable (Debug builds only; Release neither registers the URL scheme nor
/// compiles this file's contents):
///
///     xcrun devicectl device process launch --device <udid> --payload-url 'cavecadcapture://test?action=start' app.cavecad.capture
///
/// `apps/capture-ios/test-cmd.sh` wraps that. Every command is logged under `[test]` when it arrives and with its
/// result (ok, or refused and why); `live-log.sh` shows those lines as they happen.
enum TestCommand: Equatable, Sendable {
    /// Opens a kept cave by id or name, as tapping its row does.
    case open(cave: String)
    /// Picks the station on the station screen and goes to the capture screen, as tapping Capture does. With
    /// `other` the label goes through "Other station…": one the cave lacks is taken as a waiting station.
    case station(label: String, other: Bool = false)
    /// Flips the Gimbal switch.
    case gimbal(on: Bool)
    case start, pause, resume, stop
    /// One `[test]` log line with the screen, recorder and gimbal state.
    case status
    /// Leaves the capture screen (or, on the station screen, goes back to the cave list).
    case back

    static let scheme = "cavecadcapture"
    static let host = "test"

    static func parse(_ url: URL) -> Result<TestCommand, TestCommandError> {
        guard url.scheme?.lowercased() == scheme, url.host?.lowercased() == host,
              let items = URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems else {
            return .failure(.notATestURL)
        }
        func value(_ name: String) -> String? {
            items.first { $0.name == name }?.value.flatMap { $0.isEmpty ? nil : $0 }
        }
        guard let action = value("action") else { return .failure(.missing("action")) }
        switch action.lowercased() {
        case "open":
            guard let cave = value("cave") else { return .failure(.missing("cave")) }
            return .success(.open(cave: cave))
        case "station":
            guard let label = value("label") else { return .failure(.missing("label")) }
            let other = ["1", "true", "yes", "on"].contains(value("other")?.lowercased() ?? "")
            return .success(.station(label: label, other: other))
        case "gimbal":
            guard let on = value("on") else { return .failure(.missing("on")) }
            switch on.lowercased() {
            case "1", "true", "on", "yes": return .success(.gimbal(on: true))
            case "0", "false", "off", "no": return .success(.gimbal(on: false))
            default: return .failure(.bad("on", on))
            }
        case "start": return .success(.start)
        case "pause": return .success(.pause)
        case "resume": return .success(.resume)
        case "stop": return .success(.stop)
        case "status": return .success(.status)
        case "back": return .success(.back)
        default: return .failure(.unknownAction(action))
        }
    }

    /// The command as the log shows it.
    var text: String {
        switch self {
        case .open(let c): "open cave=\(c)"
        case .station(let l, let other): "station label=\(l)" + (other ? " other=1" : "")
        case .gimbal(let on): "gimbal on=\(on ? 1 : 0)"
        case .start: "start"
        case .pause: "pause"
        case .resume: "resume"
        case .stop: "stop"
        case .status: "status"
        case .back: "back"
        }
    }
}

enum TestCommandError: Error, Equatable, Sendable {
    case notATestURL
    case missing(String)
    case unknownAction(String)
    case bad(String, String)

    var why: String {
        switch self {
        case .notATestURL: "not a \(TestCommand.scheme)://\(TestCommand.host)?action=… URL"
        case .missing(let p): "missing \(p)="
        case .unknownAction(let a): "unknown action \(a)"
        case .bad(let p, let v): "bad \(p)=\(v)"
        }
    }
}

/// What a screen did with a command.
enum TestOutcome: Equatable, Sendable {
    case ok(String?)
    case refused(String)
}

/// Routes test commands to the screen that owns them. Each screen registers its handler when it appears and reports
/// the navigation state it owns (the open cave, the picked station, the capture screen shown), so a command for a
/// screen that is not up is refused here. The handlers call the very functions the buttons call.
@MainActor
final class TestRemote {
    static let shared = TestRemote()
    typealias Handler = @MainActor (TestCommand) -> TestOutcome

    weak var account: Account?
    weak var recorder: ShotRecorder?
    /// The open cave's name (the station screen is up), the station picked there, and the capture screen shown.
    private(set) var cave: String?
    var station: String?
    var capturing = false
    /// Handles open and back (from the station screen).
    var caveList: Handler?
    /// Handles station and back (from the capture screen).
    var stationScreen: Handler?
    /// Handles gimbal, start, pause, resume, stop.
    var captureScreen: Handler?
    /// The last periodic status line during a gimbal pass (ShotRecorder's clock).
    var lastPassStatus: TimeInterval = 0

    func caveOpened(_ name: String?) {
        cave = name
        if name == nil { station = nil; capturing = false }
    }

    var screen: String {
        if account?.token == nil { return "sign-in" }
        if capturing { return recorder?.viewing != nil ? "shot" : "capture" }
        return cave == nil ? "caves" : "stations"
    }

    func handle(_ url: URL) {
        switch TestCommand.parse(url) {
        case .failure(let e):
            Log.write("test", "refused \(url.absoluteString): \(e.why)")
        case .success(let c):
            Log.write("test", "got \(c.text)")
            if c == .status { logStatus("status"); return }
            switch route(c) {
            case .ok(let note): Log.write("test", "\(c.text) ok" + (note.map { ": \($0)" } ?? ""))
            case .refused(let why): Log.write("test", "\(c.text) refused: \(why)")
            }
        }
    }

    private func route(_ c: TestCommand) -> TestOutcome {
        let handler: Handler?
        switch c {
        case .open:
            guard account?.token != nil else { return .refused("signed out") }
            guard cave == nil else { return .refused("a cave is open (screen \(screen)): send back first") }
            handler = caveList
        case .station:
            guard cave != nil else { return .refused("no cave open (screen \(screen))") }
            guard !capturing else { return .refused("on the capture screen: send back first") }
            handler = stationScreen
        case .back:
            if capturing { handler = stationScreen }
            else if cave != nil { handler = caveList }
            else { return .refused("nothing to go back from (screen \(screen))") }
        case .gimbal, .start, .pause, .resume, .stop:
            guard capturing else { return .refused("not on the capture screen (screen \(screen))") }
            handler = captureScreen
        case .status:
            return .ok(nil)
        }
        guard let handler else { return .refused("that screen has not appeared") }
        return handler(c)
    }

    /// One line: screen, cave, station, recorder phase, gimbal, tracking, light.
    func logStatus(_ why: String) {
        let place = "screen=\(screen) cave=\(cave ?? "-") station=\(station ?? "-")"
        Log.write("test", "\(why): \(place) \(recorder?.testStatus() ?? "recorder=none")")
    }
}
#endif
