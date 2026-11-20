import XCTest
@testable import CaveCAD_Capture

final class AccountTests: XCTestCase {
    func testPkceVector() {
        XCTAssertEqual(PKCE.challenge(for: "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
                       "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM")
    }
    func testSignInUrl() {
        let url = Account.signInURL(server: URL(string: "https://cavecad.com")!, challenge: "c1", state: "s1")
        let q = URLComponents(url: url, resolvingAgainstBaseURL: false)!.queryItems!
        XCTAssertEqual(url.path, "/signin")
        XCTAssertEqual(q.first { $0.name == "desktop" }?.value, "1")
        XCTAssertEqual(q.first { $0.name == "challenge" }?.value, "c1")
        XCTAssertEqual(q.first { $0.name == "state" }?.value, "s1")
    }
    func testCallbackStateChecked() {
        XCTAssertEqual(Account.code(from: URL(string: "cavecad://auth?code=abc&state=s1")!, state: "s1"), "abc")
        XCTAssertNil(Account.code(from: URL(string: "cavecad://auth?code=abc&state=zz")!, state: "s1"))
    }
}
