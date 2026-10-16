import CoreText
import Foundation
import Synchronization

/// Measures cell text with Core Text, for the engine's viewport layout (`####`, General
/// narrowing, Shrink to fit, overflow). Fonts the Mac doesn't have fall back to the system font.
public final class CoreTextMeasurer: TextMeasurer {
    private struct FontKey: Hashable {
        let face: FontFace
        let size: Float
    }

    private let fonts = Mutex<[FontKey: CTFont]>([:])

    public init() {}

    public func textWidth(text: String, face: FontFace, size: Float) -> Float {
        guard size.isFinite, size > 0, !text.isEmpty else { return 0 }
        let font = self.font(face, size: size)
        let attributed = NSAttributedString(string: text, attributes: [.init(kCTFontAttributeName as String): font])
        let line = CTLineCreateWithAttributedString(attributed)
        let width = CTLineGetTypographicBounds(line, nil, nil, nil)
        return width.isFinite ? Float(width) : 0
    }

    /// The Core Text font for a cell font face at `size` points, cached.
    public func font(_ face: FontFace, size: Float) -> CTFont {
        let key = FontKey(face: face, size: size)
        return fonts.withLock { cache in
            if let font = cache[key] {
                return font
            }
            let font = Self.makeFont(face, size: CGFloat(size))
            if cache.count > 512 {
                cache.removeAll()
            }
            cache[key] = font
            return font
        }
    }

    private static func makeFont(_ face: FontFace, size: CGFloat) -> CTFont {
        let named = CTFontCreateWithName(face.name as CFString, size, nil)
        // Core Text substitutes a default face for an unknown name; use the system font then.
        let found = (CTFontCopyFamilyName(named) as String).caseInsensitiveCompare(face.name) == .orderedSame
            || (CTFontCopyPostScriptName(named) as String).caseInsensitiveCompare(face.name) == .orderedSame
        let base = found ? named : CTFontCreateUIFontForLanguage(.system, size, nil) ?? named
        var traits: CTFontSymbolicTraits = []
        if face.bold {
            traits.insert(.traitBold)
        }
        if face.italic {
            traits.insert(.traitItalic)
        }
        if traits.isEmpty {
            return base
        }
        return CTFontCreateCopyWithSymbolicTraits(base, size, nil, traits, traits) ?? base
    }
}
