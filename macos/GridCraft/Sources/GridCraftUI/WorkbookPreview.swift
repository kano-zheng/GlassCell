import AppKit
import CoreGraphics
import GridCraftEngine

/// A workbook file as a picture of its active sheet's top-left corner (Quick Look, sharing),
/// drawn by the grid's own renderer.
public enum WorkbookPreview {
    /// Renders `data` (a file named `name`) at `size` points, `scale`× for Retina.
    @MainActor
    public static func render(data: Data, name: String, size: CGSize = CGSize(width: 1000, height: 640), scale: CGFloat = 2) throws -> CGImage {
        let engine = Engine()
        _ = try engine.execute(id: "file.open", params: JSON.encode(["name": name, "base64": data.base64EncodedString()]))
        guard let ctx = CGContext(data: nil, width: Int(size.width * scale), height: Int(size.height * scale), bitsPerComponent: 8, bytesPerRow: 0,
                                  space: CGColorSpace(name: CGColorSpace.sRGB) ?? CGColorSpaceCreateDeviceRGB(),
                                  bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
        else { throw CocoaError(.fileReadUnknown) }
        // The renderer draws top-down, like the app's flipped grid view.
        ctx.translateBy(x: 0, y: size.height * scale)
        ctx.scaleBy(x: scale, y: -scale)
        try GridRenderer(engine: engine, measurer: CoreTextMeasurer()).draw(in: ctx, size: size, scroll: .zero)
        guard let image = ctx.makeImage() else { throw CocoaError(.fileReadUnknown) }
        return image
    }
}
