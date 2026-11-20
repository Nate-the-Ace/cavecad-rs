import SwiftUI
import UIKit

/// The station cards as a printable PDF, drawn from the detector's own tag16h5 bits so a print always reads.
/// Letter pages, actual size: each card's front (the glyph, black square exactly 3 in, with a dotted trim
/// square one cell outside it and a 100 mm check bar) then its back (word and use), for double-sided printing.
enum CardSheet {
    static let pageSize = CGSize(width: 612, height: 792)   // US Letter, points
    static func pt(mm: Double) -> CGFloat { CGFloat(mm / 25.4 * 72) }

    /// The glyph's cells, white border included (8 x 8 for tag16h5): true = black.
    static func cells(_ card: StationCard) -> [[Bool]] {
        guard let fam = tag16h5_create() else { return [] }
        defer { tag16h5_destroy(fam) }
        guard let im = apriltag_to_image(fam, UInt32(card.rawValue)) else { return [] }
        defer { image_u8_destroy(im) }
        let w = Int(im.pointee.width), h = Int(im.pointee.height), s = Int(im.pointee.stride)
        return (0..<h).map { y in (0..<w).map { x in im.pointee.buf[y * s + x] < 128 } }
    }

    static func pdf() -> Data {
        let r = UIGraphicsPDFRenderer(bounds: CGRect(origin: .zero, size: pageSize))
        return r.pdfData { ctx in
            for card in StationCard.allCases {
                ctx.beginPage(); front(card, ctx.cgContext)
                ctx.beginPage(); back(card)
            }
        }
    }

    /// The trim square (8 cells): centred on the page, the same on both sides.
    static var box: CGRect {
        let side = pt(mm: StationCard.glyphSide * 1000 * 8 / 6)
        return CGRect(x: (pageSize.width - side) / 2, y: (pageSize.height - side) / 2 - 40, width: side, height: side)
    }

    private static func front(_ card: StationCard, _ g: CGContext) {
        let grid = cells(card)
        let n = grid.count
        guard n > 0 else { return }
        let b = box, cell = b.width / CGFloat(n)
        g.setFillColor(UIColor.black.cgColor)
        for (y, row) in grid.enumerated() {
            for (x, black) in row.enumerated() where black {
                g.fill(CGRect(x: b.minX + CGFloat(x) * cell, y: b.minY + CGFloat(y) * cell, width: cell, height: cell))
            }
        }
        g.setStrokeColor(UIColor(white: 0.6, alpha: 1).cgColor)
        g.setLineWidth(0.5)
        g.setLineDash(phase: 0, lengths: [3, 3])
        g.stroke(b)
        g.setLineDash(phase: 0, lengths: [])
        let small = [NSAttributedString.Key.font: UIFont.systemFont(ofSize: 9)]
        let lines = [
            "\(card.word) · tag16h5 #\(card.rawValue) · CaveCAD Capture station card",
            "Print at 100 % (Actual Size), double-sided. The black square must measure exactly 3 in (76.2 mm):",
            "the app works out the card's distance from that size. Trim no closer than the dotted square.",
        ]
        var y = b.maxY + 24
        for l in lines { (l as NSString).draw(at: CGPoint(x: b.minX - 60, y: y), withAttributes: small); y += 13 }
        // check bar: 100 mm with 10 mm ticks
        let bar = pt(mm: 100), x0 = (pageSize.width - bar) / 2
        y += 16
        g.setStrokeColor(UIColor.black.cgColor); g.setLineWidth(1)
        g.move(to: CGPoint(x: x0, y: y)); g.addLine(to: CGPoint(x: x0 + bar, y: y))
        for i in 0...10 {
            let x = x0 + pt(mm: Double(i) * 10)
            g.move(to: CGPoint(x: x, y: y - (i % 5 == 0 ? 6 : 3))); g.addLine(to: CGPoint(x: x, y: y + (i % 5 == 0 ? 6 : 3)))
        }
        g.strokePath()
        ("this bar is 100 mm when printed at actual size" as NSString)
            .draw(at: CGPoint(x: x0, y: y + 8), withAttributes: small)
    }

    private static func back(_ card: StationCard) {
        let b = box
        let centred = NSMutableParagraphStyle(); centred.alignment = .center
        func draw(_ s: String, size: CGFloat, bold: Bool, at y: CGFloat) {
            let font = bold ? UIFont.boldSystemFont(ofSize: size) : UIFont.systemFont(ofSize: size)
            (s as NSString).draw(in: CGRect(x: b.minX, y: b.minY + y * b.height, width: b.width, height: b.height),
                                 withAttributes: [.font: font, .paragraphStyle: centred])
        }
        var size = b.width * 0.2
        while (card.word as NSString).size(withAttributes: [.font: UIFont.boldSystemFont(ofSize: size)]).width > b.width * 0.92 { size -= 1 }
        draw(card.word, size: size, bold: true, at: 0.18)
        draw(card.note, size: b.width * 0.065, bold: false, at: 0.52)
        draw("CaveCAD Capture  #\(card.rawValue)", size: b.width * 0.045, bold: false, at: 0.84)
    }

    /// The PDF written to a temporary file, for sharing or saving to Files.
    static func file() -> URL? {
        let u = FileManager.default.temporaryDirectory.appendingPathComponent("CaveCAD station cards.pdf")
        do { try pdf().write(to: u, options: .atomic); return u } catch { return nil }
    }
}

/// Print or share the station cards.
struct CardSheetView: View {
    @State private var url: URL?

    var body: some View {
        List {
            Section {
                ForEach(StationCard.allCases, id: \.self) { c in
                    LabeledContent(c.word, value: c.note)
                }
            } footer: {
                Text("Three cards, used at every station. Print double-sided on Letter paper at 100 % (Actual Size); the black square must measure exactly 3 in. Check with the 100 mm bar on each front, then trim and laminate.")
            }
            Section {
                Button("Print cards") { print() }
                if let url { ShareLink("Share or save the PDF", item: url) }
            }
        }
        .navigationTitle("Station cards")
        .task { url = await Task.detached { CardSheet.file() }.value }
    }

    private func print() {
        let p = UIPrintInteractionController.shared
        let info = UIPrintInfo.printInfo()
        info.jobName = "CaveCAD station cards"
        info.outputType = .grayscale
        info.duplex = .longEdge
        p.printInfo = info
        p.printingItem = CardSheet.pdf()
        p.present(animated: true)
        Log.write("cards", "print sheet opened")
    }
}
