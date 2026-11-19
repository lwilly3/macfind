// Dessine l'icône de macfind et écrit un fichier .icns.
// Usage : swift scripts/make-icon.swift <sortie.icns>
//
// Le dessin est fait à la main (loupe sur un petit réseau) : les symboles
// SF d'Apple ne sont pas autorisés dans une icône d'application.
import AppKit

guard CommandLine.arguments.count == 2 else {
    FileHandle.standardError.write(Data("Usage : make-icon.swift <sortie.icns>\n".utf8))
    exit(2)
}
let sortie = URL(fileURLWithPath: CommandLine.arguments[1])

/// Icône carrée de `cote` pixels.
func dessiner(cote: Int) -> Data {
    let rep = NSBitmapImageRep(
        bitmapDataPlanes: nil, pixelsWide: cote, pixelsHigh: cote, bitsPerSample: 8,
        samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
        bytesPerRow: 0, bitsPerPixel: 0)!
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
    let ctx = NSGraphicsContext.current!.cgContext
    // Tout est exprimé sur une grille de 1024, puis mis à l'échelle.
    ctx.scaleBy(x: CGFloat(cote) / 1024, y: CGFloat(cote) / 1024)

    // Fond : carré arrondi aux proportions des icônes macOS, dégradé bleu.
    let fond = CGRect(x: 100, y: 100, width: 824, height: 824)
    let forme = CGPath(roundedRect: fond, cornerWidth: 185, cornerHeight: 185, transform: nil)
    ctx.saveGState()
    ctx.addPath(forme)
    ctx.clip()
    let degrade = CGGradient(
        colorsSpace: CGColorSpaceCreateDeviceRGB(),
        colors: [
            CGColor(red: 0.10, green: 0.42, blue: 0.86, alpha: 1),
            CGColor(red: 0.05, green: 0.20, blue: 0.52, alpha: 1),
        ] as CFArray, locations: [0, 1])!
    ctx.drawLinearGradient(
        degrade, start: CGPoint(x: 512, y: 924), end: CGPoint(x: 512, y: 100), options: [])
    ctx.restoreGState()

    let blanc = CGColor(red: 1, green: 1, blue: 1, alpha: 1)
    let centre = CGPoint(x: 462, y: 562)
    let rayon: CGFloat = 215

    // Manche de la loupe.
    ctx.setStrokeColor(blanc)
    ctx.setLineCap(.round)
    ctx.setLineWidth(74)
    ctx.move(to: CGPoint(x: centre.x + 168, y: centre.y - 168))
    ctx.addLine(to: CGPoint(x: 770, y: 254))
    ctx.strokePath()

    // Verre : disque légèrement teinté, cerclé de blanc.
    ctx.setFillColor(CGColor(red: 1, green: 1, blue: 1, alpha: 0.16))
    ctx.fillEllipse(
        in: CGRect(x: centre.x - rayon, y: centre.y - rayon, width: 2 * rayon, height: 2 * rayon))
    ctx.setLineWidth(52)
    ctx.strokeEllipse(
        in: CGRect(x: centre.x - rayon, y: centre.y - rayon, width: 2 * rayon, height: 2 * rayon))

    // Dans le verre : trois équipements reliés, dont un mis en évidence.
    let noeuds = [
        CGPoint(x: centre.x - 92, y: centre.y - 62),
        CGPoint(x: centre.x + 96, y: centre.y - 40),
        CGPoint(x: centre.x - 6, y: centre.y + 98),
    ]
    ctx.setLineWidth(20)
    ctx.move(to: noeuds[0])
    ctx.addLine(to: noeuds[1])
    ctx.addLine(to: noeuds[2])
    ctx.closePath()
    ctx.strokePath()
    for (i, p) in noeuds.enumerated() {
        let r: CGFloat = i == 2 ? 44 : 34
        ctx.setFillColor(
            i == 2 ? CGColor(red: 1.0, green: 0.78, blue: 0.20, alpha: 1) : blanc)
        ctx.fillEllipse(in: CGRect(x: p.x - r, y: p.y - r, width: 2 * r, height: 2 * r))
    }

    NSGraphicsContext.restoreGraphicsState()
    return rep.representation(using: .png, properties: [:])!
}

// Jeu d'icônes attendu par iconutil.
let dossier = FileManager.default.temporaryDirectory
    .appendingPathComponent("macfind-\(ProcessInfo.processInfo.processIdentifier).iconset")
try? FileManager.default.removeItem(at: dossier)
try FileManager.default.createDirectory(at: dossier, withIntermediateDirectories: true)
for base in [16, 32, 128, 256, 512] {
    try dessiner(cote: base).write(to: dossier.appendingPathComponent("icon_\(base)x\(base).png"))
    try dessiner(cote: base * 2)
        .write(to: dossier.appendingPathComponent("icon_\(base)x\(base)@2x.png"))
}
let iconutil = Process()
iconutil.executableURL = URL(fileURLWithPath: "/usr/bin/iconutil")
iconutil.arguments = ["-c", "icns", "-o", sortie.path, dossier.path]
try iconutil.run()
iconutil.waitUntilExit()
try? FileManager.default.removeItem(at: dossier)
exit(iconutil.terminationStatus)
