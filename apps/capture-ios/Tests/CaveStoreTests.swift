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

@MainActor
final class SignOutTests: XCTestCase {
    func testSignOutForgetsKeptCaves() throws {
        let k = KeptCave(id: "kept-test-cave", name: "Test", snapshot: nil, entries: [], last: 0, keptAt: Date())
        try JSONEncoder().encode(k).write(to: CaveStore.folder.appendingPathComponent("kept-test-cave.json"))
        let account = Account()
        let store = CaveStore(account: account)
        XCTAssertTrue(store.keptIds.contains("kept-test-cave"))
        account.signOut()
        store.reloadKept()
        XCTAssertTrue(store.kept().isEmpty)
        XCTAssertTrue(store.keptIds.isEmpty)
    }
}

@MainActor
final class SignInEndedTests: XCTestCase {
    func testSignInEndingKeepsKeptCaves() throws {
        let k = KeptCave(id: "kept-ended-cave", name: "Test", snapshot: nil, entries: [], last: 0, keptAt: Date())
        let url = CaveStore.folder.appendingPathComponent("kept-ended-cave.json")
        try JSONEncoder().encode(k).write(to: url)
        defer { try? FileManager.default.removeItem(at: url) }
        let account = Account()
        account.signInEnded()
        XCTAssertNil(account.token)
        XCTAssertNotNil(account.problem, "says the sign-in ended")
        XCTAssertTrue(CaveStore.keptIdsOnDisk().contains("kept-ended-cave"), "a 401 never deletes the offline copies")
    }
}
