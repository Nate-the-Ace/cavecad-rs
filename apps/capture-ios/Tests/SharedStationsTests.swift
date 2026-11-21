import XCTest
@testable import CaveCAD_Capture

/// The stations CaveCAD on this phone shares through the keychain (SharedStations.swift), and how they merge into
/// the kept copy's stations.
final class SharedStationsTests: XCTestCase {
    /// MUST equal `crates/cave-capture/tests/fixtures/shared-stations.json` byte for byte (the Rust side encodes the
    /// same input to this text). When the fixture changes, paste it here.
    static let fixture = #"{"v":1,"caveId":"01JCAVE0000000000000000000","name":"Sullivan Cave","updatedAt":1763700000000,"stations":[{"id":"00000000000000000000000001","label":"A1","x":0.0,"y":0.0,"z":0.0},{"id":"00000000000000000J6HB7H6NW","label":"A2","x":10.123,"y":-3.0,"z":2.0},{"id":"0ZZZZZZZZZZZZZZZZZZZZZZZZZ","label":"Z 7","x":1234567.891,"y":4321098.765,"z":-12.346}]}"#

    let a1 = Station(id: "01JDST00000000000000000001", label: "A1", x: 0, y: 0, z: 0)
    let a2 = Station(id: "01JDST00000000000000000002", label: "A2", x: 3.2, y: -1.5, z: 0)

    func shared(updatedAt: Int64, _ stations: [Station]) -> SharedStations {
        SharedStations(v: 1, caveId: "cave", name: "Cave", updatedAt: updatedAt, stations: stations)
    }

    func testDecodesTheSharedFixture() throws {
        let s = try XCTUnwrap(SharedStations.decode(Data(Self.fixture.utf8)))
        XCTAssertEqual(s.v, 1)
        XCTAssertEqual(s.caveId, "01JCAVE0000000000000000000")
        XCTAssertEqual(s.name, "Sullivan Cave")
        XCTAssertEqual(s.updatedAt, 1_763_700_000_000)
        XCTAssertEqual(s.stations.map(\.label), ["A1", "A2", "Z 7"])
        XCTAssertEqual(s.stations[1], Station(id: "00000000000000000J6HB7H6NW", label: "A2", x: 10.123, y: -3.0, z: 2.0))
        XCTAssertEqual(s.stations[2], Station(id: "0ZZZZZZZZZZZZZZZZZZZZZZZZZ", label: "Z 7", x: 1234567.891, y: 4321098.765, z: -12.346))
        // a typed "z7" finds the CaveCAD station by label, as the core's label_key does
        XCTAssertEqual(StationLabel.find("z7", in: s.stations)?.id, "0ZZZZZZZZZZZZZZZZZZZZZZZZZ")
        XCTAssertEqual(s.updatedDate, Date(timeIntervalSince1970: 1_763_700_000))
    }

    func testJunkAndMissingKeysDoNotDecode() {
        XCTAssertNil(SharedStations.decode(Data("{}".utf8)))
        XCTAssertNil(SharedStations.decode(Data("not json".utf8)))
        XCTAssertNil(SharedStations.decode(Data(#"{"v":1,"caveId":"c","name":"n","updatedAt":1}"#.utf8)))
    }

    func testNoSharedItemKeepsTheKeptList() {
        let m = SharedStore.merge(kept: [a1, a2], shared: nil, keptAt: Date())
        XCTAssertEqual(m.stations, [a1, a2])
        XCTAssertTrue(m.fromCaveCAD.isEmpty)
    }

    func testStationOnlyInCaveCADIsAppendedAndMarked() {
        let z1 = Station(id: "z1", label: "Z1", x: 9, y: 9, z: 9)
        let m = SharedStore.merge(kept: [a1], shared: shared(updatedAt: 0, [a1, z1]), keptAt: Date())
        XCTAssertEqual(m.stations, [a1, z1])
        XCTAssertEqual(m.fromCaveCAD, ["z1"])
    }

    func testSameIdNewerCaveCADWins() {
        let keptAt = Date(timeIntervalSince1970: 1000)
        let moved = Station(id: a2.id, label: "A2", x: 3.25, y: -1.5, z: -0.012)
        let newer = SharedStore.merge(kept: [a1, a2], shared: shared(updatedAt: 1_000_001, [moved]), keptAt: keptAt)
        XCTAssertEqual(newer.stations, [a1, moved])
        XCTAssertTrue(newer.fromCaveCAD.isEmpty, "a station the kept copy has is not 'from CaveCAD'")
        // same time or older: the kept copy wins
        XCTAssertEqual(SharedStore.merge(kept: [a1, a2], shared: shared(updatedAt: 1_000_000, [moved]), keptAt: keptAt).stations, [a1, a2])
        XCTAssertEqual(SharedStore.merge(kept: [a1, a2], shared: shared(updatedAt: 5, [moved]), keptAt: keptAt).stations, [a1, a2])
    }

    func testOrderIsKeptFirstThenCaveCADOnlyInTheirOrder() {
        let z2 = Station(id: "z2", label: "Z2", x: 2, y: 0, z: 0), z1 = Station(id: "z1", label: "Z1", x: 1, y: 0, z: 0)
        let m = SharedStore.merge(kept: [a2, a1], shared: shared(updatedAt: 0, [z2, a1, z1]), keptAt: Date())
        XCTAssertEqual(m.stations.map(\.id), [a2.id, a1.id, "z2", "z1"])
        XCTAssertEqual(m.fromCaveCAD, ["z2", "z1"])
    }

    func testSharedStationsWithNoIdOrRepeatedIdsAreSkipped() {
        let noId = Station(id: "", label: "Q", x: 0, y: 0, z: 0)
        let z1 = Station(id: "z1", label: "Z1", x: 1, y: 0, z: 0), z1again = Station(id: "z1", label: "Z1b", x: 5, y: 0, z: 0)
        let m = SharedStore.merge(kept: [a1], shared: shared(updatedAt: 0, [noId, z1, z1again]), keptAt: Date())
        XCTAssertEqual(m.stations, [a1, z1])
    }

    /// A cave only in CaveCAD on this phone: an empty kept copy, so every station comes from CaveCAD.
    func testEmptyKeptCopyTakesEveryCaveCADStation() throws {
        let s = try XCTUnwrap(SharedStations.decode(Data(Self.fixture.utf8)))
        let k = KeptCave.onlyInCaveCAD(id: s.caveId, name: s.name)
        XCTAssertNil(k.snapshot)
        XCTAssertEqual(k.entries, [])
        XCTAssertEqual(try Core.stations(snapshot: k.snapshot, entries: k.entries), [])
        let m = SharedStore.merge(kept: [], shared: s, keptAt: k.keptAt)
        XCTAssertEqual(m.stations.map(\.label), ["A1", "A2", "Z 7"])
        XCTAssertEqual(m.fromCaveCAD.count, 3)
        XCTAssertTrue(k.isOnlyInCaveCAD)
        XCTAssertFalse(KeptCave(id: "c", name: "n", snapshot: nil, entries: [], last: 0, keptAt: Date()).isOnlyInCaveCAD)
    }

    /// A shot saved waiting for "A9" (typed, the kept copy lacks it) is placed once CaveCAD on this phone has A9:
    /// it resolves against the merged list (WaitingStationTests, ShotSummary.stations(in:)).
    func testWaitingLabelFoundThroughTheMergedList() {
        let a9 = Station(id: "a9", label: "A9", x: 4, y: 4, z: 0)
        XCTAssertNil(StationLabel.find("a 9", in: [a1, a2]))
        let m = SharedStore.merge(kept: [a1, a2], shared: shared(updatedAt: 0, [a9]), keptAt: Date())
        XCTAssertEqual(StationLabel.find("a 9", in: m.stations), a9)
        XCTAssertEqual(StationLabel.pick("A9", in: m.stations), a9)
        XCTAssertFalse(StationLabel.pick("A9", in: m.stations)?.isWaiting ?? true)
    }

    // MARK: cave list

    func testOfflineListsKeptThenCaveCADOnlyCaves() {
        let kept = [CaveInfo(id: "k1", name: "Kept")]
        let shared = [CaveInfo(id: "k1", name: "Kept (CaveCAD)"), CaveInfo(id: "n1", name: "New in CaveCAD")]
        let l = CaveStore.listing(server: nil, kept: kept, shared: shared)
        XCTAssertEqual(l.caves, [CaveInfo(id: "k1", name: "Kept"), CaveInfo(id: "n1", name: "New in CaveCAD")])
        XCTAssertEqual(l.onlyInCaveCAD, ["n1"])
    }

    func testOnlineListsServerThenCaveCADOnlyCavesTheServerLacks() {
        let server = [CaveInfo(id: "s1", name: "Server")]
        let kept = [CaveInfo(id: "k1", name: "Kept, gone from the server")]
        let shared = [CaveInfo(id: "s1", name: "Server"), CaveInfo(id: "k1", name: "k"), CaveInfo(id: "n1", name: "New")]
        let l = CaveStore.listing(server: server, kept: kept, shared: shared)
        // a kept cave is never "only in CaveCAD" (it opens from its kept copy); online lists stay as before
        XCTAssertEqual(l.caves.map(\.id), ["s1", "n1"])
        XCTAssertEqual(l.onlyInCaveCAD, ["n1"])
    }

    func testNoSharedCavesListsAsBefore() {
        let kept = [CaveInfo(id: "k1", name: "Kept")]
        XCTAssertEqual(CaveStore.listing(server: nil, kept: kept, shared: []).caves, kept)
        XCTAssertEqual(CaveStore.listing(server: [], kept: kept, shared: []).caves, [])
    }

    // MARK: keychain groups

    func testAccessGroupsFromThePrefix() {
        XCTAssertEqual(Keychain.prefix(from: "2ANLK72UUS."), "2ANLK72UUS.")
        XCTAssertEqual(Keychain.prefix(from: "2ANLK72UUS"), "2ANLK72UUS.")
        // unsigned builds (simulator tests) leave it empty or unexpanded: no group then
        XCTAssertNil(Keychain.prefix(from: nil))
        XCTAssertNil(Keychain.prefix(from: ""))
        XCTAssertNil(Keychain.prefix(from: "$(AppIdentifierPrefix)"))
        XCTAssertEqual(Keychain.group("app.cavecad.capture", prefix: "T."), "T.app.cavecad.capture")
        XCTAssertNil(Keychain.group("app.cavecad.capture", prefix: nil))
    }

    /// Writes name the app's own group, so the token never lands in the shared one (the default group is the first
    /// listed); reads do not filter by group, so a token saved before the entitlement existed still loads.
    func testSaveNamesTheOwnGroupAndLoadDoesNot() {
        let save = Keychain.saveQuery("v", service: "s", group: "T.app.cavecad.capture")
        XCTAssertEqual(save[kSecAttrAccessGroup as String] as? String, "T.app.cavecad.capture")
        XCTAssertEqual(save[kSecAttrService as String] as? String, "s")
        XCTAssertEqual(save[kSecAttrAccessible as String] as? String, kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly as String)
        XCTAssertNil(Keychain.saveQuery("v", service: "s", group: nil)[kSecAttrAccessGroup as String])
        XCTAssertNil(Keychain.loadQuery(service: "s")[kSecAttrAccessGroup as String])
        XCTAssertNil(Keychain.deleteQuery(service: "s")[kSecAttrAccessGroup as String])
    }

    /// The real keychain round trip: what the token goes through. An unsigned simulator build has no keychain at all
    /// (errSecMissingEntitlement), so it is skipped there; it runs in a signed build.
    func testSaveLoadRoundTrip() throws {
        let service = "app.cavecad.capture.test-\(UUID().uuidString)"
        defer { Keychain.delete(service: service) }
        let probe = SecItemAdd(Keychain.saveQuery("probe", service: service, group: nil) as CFDictionary, nil)
        Keychain.delete(service: service)
        if probe == errSecMissingEntitlement { throw XCTSkip("no keychain in an unsigned build") }
        XCTAssertTrue(Keychain.save("one", service: service))
        XCTAssertEqual(Keychain.load(service: service), "one")
        XCTAssertTrue(Keychain.save("two", service: service))
        XCTAssertEqual(Keychain.load(service: service), "two")
    }

    // MARK: sync of a cave the server lacks

    func testListNotFoundForACaveOnlyHereWaitsInsteadOfMarkingTheServerMissing() {
        XCTAssertEqual(SyncPlan.listNotFound(caveKept: true), .serverMissing)
        XCTAssertEqual(SyncPlan.listNotFound(caveKept: false), .caveNotOnServer)
    }
}
