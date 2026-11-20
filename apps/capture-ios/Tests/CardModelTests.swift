import SceneKit
import XCTest
@testable import CaveCAD_Capture

final class CardModelTests: XCTestCase {
    /// The 3D card's front is the real glyph: the app's own detector reads it as that card.
    func testFrontReadsAsItsCard() throws {
        for card in StationCard.allCases {
            let img = try XCTUnwrap(CardModel.frontImage(card, pixels: 400).cgImage)
            // the trim square has one white cell round the glyph; pad more white so the detector sees a quiet zone
            let w = 600
            var px = [UInt8](repeating: 255, count: w * w)
            px.withUnsafeMutableBytes { buf in
                let g = CGContext(data: buf.baseAddress, width: w, height: w, bitsPerComponent: 8, bytesPerRow: w,
                                  space: CGColorSpaceCreateDeviceGray(), bitmapInfo: CGImageAlphaInfo.none.rawValue)!
                g.setFillColor(gray: 1, alpha: 1); g.fill(CGRect(x: 0, y: 0, width: w, height: w))
                g.draw(img, in: CGRect(x: 100, y: 100, width: 400, height: 400))
            }
            let found = CardDetector().detectGrey(&px, width: w, height: w)
                .filter { StationCard.accepting(id: $0.id, hamming: $0.hamming) != nil }
            XCTAssertEqual(found.map(\.id), [card.rawValue], card.word)
        }
    }

    func testCardIsFourInchesWithFrontAndBack() {
        XCTAssertEqual(CardModel.side, 0.1016, accuracy: 1e-9)
        let box = CardModel.scene(.rear).rootNode.childNode(withName: "card", recursively: false)?.geometry as? SCNBox
        XCTAssertEqual(box?.materials.count, 6)
        XCTAssertNotNil(box?.materials[0].diffuse.contents as? UIImage)
        XCTAssertNotNil(box?.materials[2].diffuse.contents as? UIImage)
    }
}
