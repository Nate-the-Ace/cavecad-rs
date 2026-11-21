import XCTest
@testable import CaveCAD_Capture

final class TestCommandTests: XCTestCase {
    private func parse(_ s: String) -> Result<TestCommand, TestCommandError> { TestCommand.parse(URL(string: s)!) }

    func testGoodCommands() {
        XCTAssertEqual(parse("cavecadcapture://test?action=open&cave=Truitt%20Cave"), .success(.open(cave: "Truitt Cave")))
        XCTAssertEqual(parse("cavecadcapture://test?action=open&cave=abc123"), .success(.open(cave: "abc123")))
        XCTAssertEqual(parse("cavecadcapture://test?action=station&label=A3"), .success(.station(label: "A3")))
        XCTAssertEqual(parse("cavecadcapture://test?action=gimbal&on=1"), .success(.gimbal(on: true)))
        XCTAssertEqual(parse("cavecadcapture://test?action=gimbal&on=0"), .success(.gimbal(on: false)))
        XCTAssertEqual(parse("cavecadcapture://test?on=off&action=gimbal"), .success(.gimbal(on: false)))
        XCTAssertEqual(parse("cavecadcapture://test?action=start"), .success(.start))
        XCTAssertEqual(parse("cavecadcapture://test?action=pause"), .success(.pause))
        XCTAssertEqual(parse("cavecadcapture://test?action=resume"), .success(.resume))
        XCTAssertEqual(parse("cavecadcapture://test?action=stop"), .success(.stop))
        XCTAssertEqual(parse("cavecadcapture://test?action=status"), .success(.status))
        XCTAssertEqual(parse("cavecadcapture://test?action=back"), .success(.back))
        XCTAssertEqual(parse("CaveCADCapture://TEST?action=STOP"), .success(.stop))
    }

    func testUnknownAction() {
        XCTAssertEqual(parse("cavecadcapture://test?action=fly"), .failure(.unknownAction("fly")))
    }

    func testMissingParams() {
        XCTAssertEqual(parse("cavecadcapture://test"), .failure(.notATestURL))
        XCTAssertEqual(parse("cavecadcapture://test?cave=x"), .failure(.missing("action")))
        XCTAssertEqual(parse("cavecadcapture://test?action="), .failure(.missing("action")))
        XCTAssertEqual(parse("cavecadcapture://test?action=open"), .failure(.missing("cave")))
        XCTAssertEqual(parse("cavecadcapture://test?action=open&cave="), .failure(.missing("cave")))
        XCTAssertEqual(parse("cavecadcapture://test?action=station"), .failure(.missing("label")))
        XCTAssertEqual(parse("cavecadcapture://test?action=gimbal"), .failure(.missing("on")))
        XCTAssertEqual(parse("cavecadcapture://test?action=gimbal&on=maybe"), .failure(.bad("on", "maybe")))
    }

    func testOtherURLsAreNotCommands() {
        XCTAssertEqual(parse("cavecad://auth?code=x&state=y"), .failure(.notATestURL))
        XCTAssertEqual(parse("cavecadcapture://other?action=start"), .failure(.notATestURL))
    }

    func testLogText() {
        XCTAssertEqual(TestCommand.open(cave: "Truitt").text, "open cave=Truitt")
        XCTAssertEqual(TestCommand.gimbal(on: true).text, "gimbal on=1")
        XCTAssertEqual(TestCommandError.missing("label").why, "missing label=")
    }
}
