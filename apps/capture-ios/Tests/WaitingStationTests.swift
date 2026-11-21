import simd
import XCTest
@testable import CaveCAD_Capture

/// A shot taken over a typed station label the cave does not have yet ("waiting for station").
final class WaitingStationTests: XCTestCase {
    let a7 = Station(id: "s7", label: "A7", x: 10, y: 20, z: 5)
    let b1 = Station(id: "b", label: "B 1", x: 0, y: 0, z: 0)

    func testTypedLabelIsTrimmedNonEmptyAndShort() {
        XCTAssertEqual(StationLabel.clean("  A7 \n"), "A7")
        XCTAssertNil(StationLabel.clean("   "))
        XCTAssertNil(StationLabel.clean(""))
        XCTAssertEqual(StationLabel.clean(String(repeating: "x", count: 40)), String(repeating: "x", count: 40))
        XCTAssertNil(StationLabel.clean(String(repeating: "x", count: 41)))
    }

    func testLabelsMatchIgnoringCaseAndSpaces() {
        // the same rule as the core's cave_capture::label_key
        XCTAssertEqual(StationLabel.key(" a 7 "), "a7")
        XCTAssertEqual(StationLabel.find("a7", in: [b1, a7]), a7)
        XCTAssertEqual(StationLabel.find("b1", in: [b1, a7]), b1)
        XCTAssertEqual(StationLabel.find(" A 7", in: [b1, a7]), a7)
        XCTAssertNil(StationLabel.find("A71", in: [b1, a7]))
        XCTAssertNil(StationLabel.find("", in: [Station(id: "e", label: "", x: 0, y: 0, z: 0)]))
        // a waiting station is never a match: it has no place
        XCTAssertNil(StationLabel.find("A9", in: [Station.waiting(label: "A9")]))
    }

    func testPickUsesTheCaveStationElseWaits() {
        XCTAssertEqual(StationLabel.pick("a 7", in: [a7]), a7)
        let w = StationLabel.pick(" A9 ", in: [a7])
        XCTAssertEqual(w, Station.waiting(label: "A9"))
        XCTAssertTrue(w?.isWaiting ?? false)
        XCTAssertEqual(w?.id, "")
        XCTAssertNil(StationLabel.pick("  ", in: [a7]))
        XCTAssertFalse(a7.isWaiting)
    }

    func testWaitingSummaryResolvesByLabel() {
        let compass: [String: Any] = ["true_north": true, "accuracy_deg": 5.0]
        let w = ShotSummary(["station_id": "", "station_label": "a7", "waiting": true, "compass": compass])
        XCTAssertTrue(w.isWaiting)
        XCTAssertEqual(w.stationLabel, "a7")
        // found: placed at the cave's station, like any compass shot
        XCTAssertEqual(w.stations(in: [b1, a7])?.over, a7)
        XCTAssertEqual(w.pair(in: [b1, a7]), Core.pair(a7, a7))
        XCTAssertEqual(w.waitingLabel(in: [b1, a7]), nil)
        // not found: waiting, no placement; drawn around its own spot
        XCTAssertNil(w.stations(in: [b1]))
        XCTAssertNil(w.pair(in: [b1]))
        XCTAssertEqual(w.waitingLabel(in: [b1]), "a7")
        XCTAssertEqual(w.viewPair(in: [b1]), [0, 0, 0, 0, 0, 0])
        XCTAssertEqual(w.viewPair(in: [a7]), Core.pair(a7, a7))
        // an older file (no "waiting" key) with an empty station id is waiting too
        XCTAssertTrue(ShotSummary(["station_id": "", "station_label": "A7"]).isWaiting)
        // a shot with a station is never resolved by label, even when the label matches
        let gone = ShotSummary(["station_id": "old", "station_label": "A7", "compass": compass])
        XCTAssertFalse(gone.isWaiting)
        XCTAssertNil(gone.stations(in: [a7]))
        XCTAssertNil(gone.waitingLabel(in: [a7]))
        XCTAssertNil(gone.viewPair(in: [a7]))
    }

    func testSavedShotTitle() {
        let u = URL(fileURLWithPath: "/x.cvcap")
        XCTAssertEqual(SavedShot(url: u, made: nil, over: nil, forward: nil, compassLabel: nil, waitingLabel: "A7").title,
                       "A7 · waiting for station")
        XCTAssertEqual(SavedShot(url: u, made: nil, over: nil, forward: nil, compassLabel: nil).title, "station gone")
        XCTAssertEqual(SavedShot(url: u, made: nil, over: a7, forward: nil, compassLabel: nil).title, "over A7")
    }

    func testWaitingShotIsWrittenAndBecomesAParentOnlyOncePlaced() throws {
        let cave = "test-cave-waiting"
        defer { try? FileManager.default.removeItem(at: ShotFiles.folder(cave: cave)) }
        let job = ShotJob(caveId: cave, over: .waiting(label: "A7"), parts: [ShotRecorderTests.tri], keyframes: [], startCamera: .zero,
                          tracking: "normal", systemVersion: "27", compass: CompassReading(trueNorth: true, accuracyDeg: 5, headingDeg: 0))
        guard case .success(let u) = job.write() else { return XCTFail("waiting shot not written") }
        let head = ShotSummary(Core.summary(bundle: try Data(contentsOf: u), pair: nil))
        XCTAssertNil(head.error)
        XCTAssertTrue(head.isWaiting)
        XCTAssertEqual(head.stationLabel, "A7")
        // waiting at the capture screen: no parent, not even itself
        XCTAssertNil(ShotFiles.parent(cave: cave, station: .waiting(label: "A7")))
        // the cave got A7 (another id, a label typed differently): the waiting shot is that station's shot now
        XCTAssertEqual(ShotFiles.parent(cave: cave, station: Station(id: "new", label: "a 7", x: 0, y: 0, z: 0)), u)
        XCTAssertNil(ShotFiles.parent(cave: cave, station: b1))
    }

    func testWaitingStationKeysByLabel() {
        XCTAssertEqual(ShotRecorder.stationKey(cave: "c", station: a7), "c/s7")
        XCTAssertEqual(ShotRecorder.stationKey(cave: "c", station: .waiting(label: "A 9")),
                       ShotRecorder.stationKey(cave: "c", station: .waiting(label: "a9")))
        XCTAssertNotEqual(ShotRecorder.stationKey(cave: "c", station: .waiting(label: "A9")),
                          ShotRecorder.stationKey(cave: "c", station: .waiting(label: "A8")))
    }

    func testPlacedWaitingShotsAreLoggedOnce() {
        var seen = Set<String>()
        let w = URL(fileURLWithPath: "/s/w1.cvcap")
        let shots = [
            SavedShot(url: w, made: nil, over: a7, forward: nil, compassLabel: nil, fromWaiting: true),
            SavedShot(url: URL(fileURLWithPath: "/s/w2.cvcap"), made: nil, over: nil, forward: nil, compassLabel: nil, waitingLabel: "A9"),
            SavedShot(url: URL(fileURLWithPath: "/s/p.cvcap"), made: nil, over: a7, forward: nil, compassLabel: nil),
        ]
        XCTAssertEqual(SavedShot.newlyPlaced(shots, logged: &seen), ["waiting shot w1 placed at A7"])
        XCTAssertEqual(SavedShot.newlyPlaced(shots, logged: &seen), [])
    }
}
