// Renders a workbook's grid offscreen to a PNG and times the frames, without a window:
//   swift run grid-snapshot [--sample sales | --in book.xlsx] [--rows 100000] [--size 1400x900]
//                           [--cmd 'id={json}']... [--scroll x,y] [--frames 60] out.png
import AppKit
import GridCraftEngine
import GridCraftUI

@MainActor
func run() -> Int32 {
    var args = Array(CommandLine.arguments.dropFirst())
    var size = CGSize(width: 1400, height: 900)
    var scroll = CGPoint.zero
    var frames = 1
    var commands: [(String, String)] = []
    var output = "grid.png"
    let document = SpreadsheetDocument()
    func next() -> String? { args.isEmpty ? nil : args.removeFirst() }
    while let arg = next() {
        switch arg {
        case "--sample":
            commands.append(("file.new", JSON.encode(["sample": next() ?? "sales"])))
        case "--in":
            guard let path = next(), let data = FileManager.default.contents(atPath: path) else {
                print("can't read the --in file")
                return 1
            }
            commands.append(("file.open", JSON.encode(["name": (path as NSString).lastPathComponent, "base64": data.base64EncodedString()])))
        case "--rows":
            let rows = Int(next() ?? "") ?? 100_000
            commands.append(("range.fill", JSON.encode(["range": "A1:J\(rows)", "input": "=ROW()*COLUMN()"])))
            commands.append(("range.fill", JSON.encode(["range": "K1:K\(rows)", "input": "=\"Row \"&ROW()&\" of the long sheet\""])))
        case "--size":
            let parts = (next() ?? "").split(separator: "x").compactMap { Double($0) }
            if parts.count == 2 { size = CGSize(width: parts[0], height: parts[1]) }
        case "--scroll":
            let parts = (next() ?? "").split(separator: ",").compactMap { Double($0) }
            if parts.count == 2 { scroll = CGPoint(x: parts[0], y: parts[1]) }
        case "--frames":
            frames = max(1, Int(next() ?? "") ?? 1)
        case "--cmd":
            let spec = next() ?? ""
            if let eq = spec.firstIndex(of: "=") {
                commands.append((String(spec[..<eq]), String(spec[spec.index(after: eq)...])))
            } else {
                commands.append((spec, ""))
            }
        default:
            output = arg
        }
    }
    for (id, params) in commands {
        do {
            _ = try document.engine.execute(id: id, params: params)
        } catch {
            print("\(id): \(error)")
        }
    }
    let scale: CGFloat = 2
    guard let ctx = CGContext(data: nil, width: Int(size.width * scale), height: Int(size.height * scale), bitsPerComponent: 8, bytesPerRow: 0,
                              space: CGColorSpace(name: CGColorSpace.sRGB) ?? CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
    else {
        print("no bitmap context")
        return 1
    }
    // The renderer draws top-down, like the flipped view.
    ctx.translateBy(x: 0, y: size.height * scale)
    ctx.scaleBy(x: scale, y: -scale)
    let renderer = GridRenderer(engine: document.engine, measurer: CoreTextMeasurer())
    var times: [Double] = []
    for frame in 0..<frames {
        let start = CFAbsoluteTimeGetCurrent()
        do {
            // Scroll a little each frame, as a trackpad does.
            try renderer.draw(in: ctx, size: size, scroll: CGPoint(x: scroll.x, y: scroll.y + CGFloat(frame) * 7))
        } catch {
            print("draw: \(error)")
            return 1
        }
        times.append((CFAbsoluteTimeGetCurrent() - start) * 1000)
    }
    let sorted = times.sorted()
    let median = sorted[sorted.count / 2]
    print(String(format: "%d frame(s) at %.0fx%.0f: median %.2f ms, worst %.2f ms", frames, size.width, size.height, median, sorted.last ?? 0))
    guard let image = ctx.makeImage(), let png = NSBitmapImageRep(cgImage: image).representation(using: .png, properties: [:]) else {
        print("no image")
        return 1
    }
    do {
        try png.write(to: URL(fileURLWithPath: output))
    } catch {
        print("write: \(error)")
        return 1
    }
    print("wrote \(output)")
    return 0
}

exit(MainActor.assumeIsolated { run() })
