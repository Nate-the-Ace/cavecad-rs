import SceneKit
import XCTest
@testable import CaveCAD_Capture

final class ShotViewTests: XCTestCase {
    /// Survey (x east, y north, z up) → SceneKit (y up): (x, z, -y); a trailing partial point is dropped.
    func testSurveyToSceneAxes() {
        let v = ShotView.sceneVertices([1, 2, 3, 4, 5, 6, 7])
        XCTAssertEqual(v.count, 2)
        XCTAssertEqual(v[0].x, 1); XCTAssertEqual(v[0].y, 3); XCTAssertEqual(v[0].z, -2)
        XCTAssertEqual(v[1].x, 4); XCTAssertEqual(v[1].y, 6); XCTAssertEqual(v[1].z, -5)
    }

    func testSummaryReading() {
        let placed = ShotSummary(["vertices": 3, "triangles": 1, "keyframes": 2, "placed": true, "turn_deg": 12.5])
        XCTAssertEqual(placed, ShotSummary(vertices: 3, triangles: 1, keyframes: 2, placed: true, turnDeg: 12.5, error: nil,
                                           stationId: nil, targetStationId: nil))
        let named = ShotSummary(["station_id": "a", "target_station_id": "b", "placed": false])
        XCTAssertEqual(named.stationId, "a")
        XCTAssertEqual(named.targetStationId, "b")
        XCTAssertFalse(ShotSummary(["vertices": 0, "placed": false]).placed)
        XCTAssertEqual(ShotSummary(["error": "bad bundle"]).error, "bad bundle")
    }
}
