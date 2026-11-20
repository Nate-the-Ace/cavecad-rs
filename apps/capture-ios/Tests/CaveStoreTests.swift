import XCTest
@testable import CaveCAD_Capture

final class CaveStoreTests: XCTestCase {
    func testParseStream() {
        let body = """
        {"t":"entry","number":5,"keyVersion":0,"locked":"{\\"c\\":\\"d\\",\\"ops\\":[]}","authorDevice":"d"}
        {"t":"entry","number":6,"keyVersion":0,"locked":"{\\"c\\":\\"e\\",\\"ops\\":[]}","authorDevice":"e"}
        {"t":"live"}
        {"t":"ping"}
        """
        let r = CaveStore.parseStream(body)
        XCTAssertEqual(r.entries.count, 2)
        XCTAssertTrue(r.entries[1].contains("\"e\""))
        XCTAssertEqual(r.last, 6)
        XCTAssertTrue(r.caughtUp)
    }

    func testStationsFromEmptyCave() throws {
        let s = try Core.stations(snapshot: #"{"upTo":0,"ops":[]}"#, entries: [])
        XCTAssertEqual(s.count, 0)
    }

    func testCoreReportsBadEntries() {
        XCTAssertThrowsError(try Core.stations(snapshot: nil, entries: ["{bad"]))
    }

    /// `GET /api/caves` (rooms.ts myCaves): an open cave's name is in `label.locked`, the plain text `{name, summary}`.
    /// Locked caves cannot be read here and are left out.
    func testCaveListNames() {
        let body = #"""
        {"caves":[
          {"id":"cave-open-1","storage":"open","label":{"keyVersion":0,"locked":"{\"name\":\"Truitt Cave\",\"summary\":\"\"}"}},
          {"id":"cave-open-2","storage":"open","label":null},
          {"id":"cave-locked","storage":"locked","label":{"keyVersion":1,"locked":"xxxx"}}
        ],"gone":[],"deleted":[]}
        """#
        let caves = CaveStore.parseCaves(Data(body.utf8))
        XCTAssertEqual(caves, [CaveInfo(id: "cave-open-1", name: "Truitt Cave"), CaveInfo(id: "cave-open-2", name: "cave-open-2")])
    }
}
