import SceneKit
import SwiftUI
import UIKit

/// A station card as a 3D object to turn in the hand: the laminated 4 in square (the dotted trim square of the
/// printed sheet: 8 glyph cells), the glyph on the front and the word on the back, as `CardSheet` prints them.
enum CardModel {
    /// The trimmed card's side, metres (8 cells of 12.7 mm: the 3 in black square plus one white cell round it).
    static let side = StationCard.glyphSide * 8 / 6
    /// Laminated thickness, metres.
    static let thickness = 0.0008

    /// The front: the glyph cells, white border cell included, black on white.
    static func frontImage(_ card: StationCard, pixels: Int = 512) -> UIImage {
        let grid = CardSheet.cells(card)
        let n = max(grid.count, 1)
        let size = CGSize(width: pixels, height: pixels)
        return UIGraphicsImageRenderer(size: size).image { ctx in
            UIColor.white.setFill(); ctx.fill(CGRect(origin: .zero, size: size))
            UIColor.black.setFill()
            let cell = CGFloat(pixels) / CGFloat(n)
            for (y, row) in grid.enumerated() {
                for (x, black) in row.enumerated() where black {
                    ctx.fill(CGRect(x: CGFloat(x) * cell, y: CGFloat(y) * cell, width: cell, height: cell))
                }
            }
        }
    }

    /// The back: the word, what the card is for, and the card's number, black on white.
    static func backImage(_ card: StationCard, pixels: Int = 512) -> UIImage {
        let w = CGFloat(pixels)
        let size = CGSize(width: w, height: w)
        let centred = NSMutableParagraphStyle(); centred.alignment = .center
        return UIGraphicsImageRenderer(size: size).image { ctx in
            UIColor.white.setFill(); ctx.fill(CGRect(origin: .zero, size: size))
            func draw(_ s: String, _ font: UIFont, at y: CGFloat) {
                (s as NSString).draw(in: CGRect(x: w * 0.04, y: y * w, width: w * 0.92, height: w),
                                     withAttributes: [.font: font, .paragraphStyle: centred, .foregroundColor: UIColor.black])
            }
            var big = w * 0.2
            while (card.word as NSString).size(withAttributes: [.font: UIFont.boldSystemFont(ofSize: big)]).width > w * 0.9 { big -= 1 }
            draw(card.word, .boldSystemFont(ofSize: big), at: 0.18)
            draw(card.note, .systemFont(ofSize: w * 0.065), at: 0.52)
            draw("CaveCAD Capture  #\(card.rawValue)", .systemFont(ofSize: w * 0.045), at: 0.84)
        }
    }

    /// The card standing upright, front facing +z (towards the viewer at first), back facing -z.
    static func scene(_ card: StationCard) -> SCNScene {
        let s = SCNScene()
        let box = SCNBox(width: side, height: side, length: thickness, chamferRadius: 0)
        func face(_ contents: Any) -> SCNMaterial {
            let m = SCNMaterial()
            m.diffuse.contents = contents
            m.lightingModel = .constant
            return m
        }
        let edge = face(UIColor(white: 0.92, alpha: 1))
        // SCNBox material order: front (+z), right, back (-z), left, top, bottom
        box.materials = [face(frontImage(card)), edge, face(backImage(card)), edge, edge, edge]
        let node = SCNNode(geometry: box)
        node.name = "card"
        s.rootNode.addChildNode(node)
        s.background.contents = UIColor(white: 0.15, alpha: 1)
        return s
    }
}

/// One card shown in 3D: drag to turn it over and see the back, pinch to zoom.
struct CardModelView: View {
    let card: StationCard
    @State private var scene: SCNScene?

    var body: some View {
        VStack(spacing: 0) {
            if let scene { OrbitView(scene: scene) } else { ProgressView().frame(maxHeight: .infinity) }
            Text("Drag to turn the card over. Front: the glyph the app looks for. Back: \(card.word).")
                .font(.footnote).foregroundStyle(.secondary).multilineTextAlignment(.center).padding()
        }
        .navigationTitle(card.word)
        .task { if scene == nil { scene = CardModel.scene(card) } }
    }
}
