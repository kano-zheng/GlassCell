import AppKit
import GridCraftEngine

/// Under the cell editor: the functions the typed word may start (autocomplete), or the
/// signature of the function the caret is in, with the current argument in bold.
@MainActor
final class EditorPopupView: NSView {
    weak var controller: EditController?
    var onPick: ((Int) -> Void)?

    override var isFlipped: Bool { true }

    private let font = NSFont.systemFont(ofSize: 12)
    private let rowHeight: CGFloat = 18

    /// The size the popup needs now (zero: nothing to show).
    func fittingSize() -> NSSize {
        guard let controller, controller.isEditing else { return .zero }
        if !controller.autocomplete.isEmpty {
            let width = controller.autocomplete.map { ($0 as NSString).size(withAttributes: [.font: font]).width }.max() ?? 0
            return NSSize(width: max(140, width + 24), height: CGFloat(controller.autocomplete.count) * rowHeight + 6)
        }
        if let hint = controller.hint {
            let text = hintText(hint)
            let size = text.boundingRect(with: NSSize(width: 420, height: 400), options: [.usesLineFragmentOrigin]).size
            return NSSize(width: ceil(size.width) + 16, height: ceil(size.height) + 10)
        }
        return .zero
    }

    private func hintText(_ hint: FunctionHint) -> NSAttributedString {
        let out = NSMutableAttributedString()
        let signature = hint.signature ?? "\(hint.name)()"
        // Bold the argument the caret is in: arguments are split by the list separator at depth 1.
        var depth = 0, argument: UInt32 = 0
        let separators: Set<Character> = [",", ";"]
        for ch in signature {
            var bold = false
            if ch == "(" { depth += 1 } else if ch == ")" { depth -= 1 } else if depth == 1, separators.contains(ch) { argument += 1 } else if depth == 1, argument == hint.argument { bold = true }
            out.append(NSAttributedString(string: String(ch), attributes: [.font: bold ? NSFont.boldSystemFont(ofSize: 12) : font, .foregroundColor: NSColor.labelColor]))
        }
        if let description = hint.description {
            out.append(NSAttributedString(string: "\n" + description, attributes: [.font: NSFont.systemFont(ofSize: 11), .foregroundColor: NSColor.secondaryLabelColor]))
        }
        return out
    }

    override func draw(_ dirtyRect: NSRect) {
        guard let controller else { return }
        let box = NSBezierPath(roundedRect: bounds.insetBy(dx: 0.5, dy: 0.5), xRadius: 5, yRadius: 5)
        NSColor.windowBackgroundColor.setFill()
        box.fill()
        NSColor.separatorColor.setStroke()
        box.stroke()
        if !controller.autocomplete.isEmpty {
            for (i, name) in controller.autocomplete.enumerated() {
                let row = NSRect(x: 3, y: 3 + CGFloat(i) * rowHeight, width: bounds.width - 6, height: rowHeight)
                let selected = i == controller.autocompleteIndex
                if selected {
                    NSColor.selectedContentBackgroundColor.setFill()
                    NSBezierPath(roundedRect: row, xRadius: 4, yRadius: 4).fill()
                }
                let color: NSColor = selected ? .alternateSelectedControlTextColor : .labelColor
                (name as NSString).draw(at: NSPoint(x: row.minX + 8, y: row.minY + 1), withAttributes: [.font: font, .foregroundColor: color])
            }
        } else if let hint = controller.hint {
            hintText(hint).draw(with: bounds.insetBy(dx: 8, dy: 5), options: [.usesLineFragmentOrigin])
        }
    }

    override func mouseDown(with event: NSEvent) {
        guard let controller, !controller.autocomplete.isEmpty else { return }
        let p = convert(event.locationInWindow, from: nil)
        let i = Int((p.y - 3) / rowHeight)
        if controller.autocomplete.indices.contains(i) {
            onPick?(i)
        }
    }
}
