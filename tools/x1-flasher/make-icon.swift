// Draws the app icon: an orange rounded square with a bolt over a chip. Run by build-app.sh.
import AppKit
let out = CommandLine.arguments[1]
let size = CGFloat(Int(CommandLine.arguments[2]) ?? 1024)
let img = NSImage(size: NSSize(width: size, height: size))
img.lockFocus()
let r = NSRect(x: size * 0.1, y: size * 0.1, width: size * 0.8, height: size * 0.8)
let bg = NSBezierPath(roundedRect: r, xRadius: size * 0.18, yRadius: size * 0.18)
NSGradient(starting: NSColor(red: 0.95, green: 0.50, blue: 0.20, alpha: 1), ending: NSColor(red: 0.80, green: 0.30, blue: 0.08, alpha: 1))!.draw(in: bg, angle: -90)
if let sym = NSImage(systemSymbolName: "bolt.fill", accessibilityDescription: nil)?
    .withSymbolConfiguration(.init(pointSize: size * 0.45, weight: .bold)) {
    let tinted = NSImage(size: sym.size, flipped: false) { rect in
        sym.draw(in: rect); NSColor.white.set(); rect.fill(using: .sourceAtop); return true
    }
    let s = tinted.size
    tinted.draw(in: NSRect(x: (size - s.width) / 2, y: (size - s.height) / 2, width: s.width, height: s.height))
}
img.unlockFocus()
let rep = NSBitmapImageRep(data: img.tiffRepresentation!)!
try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: out))
