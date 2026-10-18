import AppKit
import GridCraftEngine

/// The text view of a cell edit, in the cell and in the formula bar. It types like any Mac text
/// field (input methods, dictation, services); Enter, Tab, Esc, the arrows (point mode) and F4
/// act on the shared `EditController`.
@MainActor
public final class CellTextView: NSTextView, NSTextViewDelegate {
    public weak var controller: EditController?
    /// This is the formula bar's editor (clicking into it starts or moves the edit there).
    public var isFormulaBar = false
    /// The cell's vertical alignment: the editor's text sits where the grid draws it.
    public var valign: VAlign = .bottom
    public var baseFont = NSFont.systemFont(ofSize: 13) {
        didSet { applyStyle() }
    }
    /// Called after the text or the caret changed.
    private func onChange() {
        controller?.changed(from: self)
    }

    private var syncing = false
    private var syncedRevision = -1
    /// The last change typed text at the end (not a deletion): AutoComplete may suggest.
    private var typedAtEnd = false

    public init(frame: NSRect, formulaBar: Bool) {
        let storage = NSTextStorage()
        let layout = NSLayoutManager()
        storage.addLayoutManager(layout)
        let container = NSTextContainer(size: NSSize(width: CGFloat.greatestFiniteMagnitude, height: CGFloat.greatestFiniteMagnitude))
        container.widthTracksTextView = false
        layout.addTextContainer(container)
        super.init(frame: frame, textContainer: container)
        isFormulaBar = formulaBar
        delegate = self
        isRichText = false
        importsGraphics = false
        allowsUndo = true
        isAutomaticQuoteSubstitutionEnabled = false
        isAutomaticDashSubstitutionEnabled = false
        isAutomaticTextReplacementEnabled = false
        isAutomaticSpellingCorrectionEnabled = false
        isContinuousSpellCheckingEnabled = false
        isHorizontallyResizable = true
        isVerticallyResizable = false
        textContainerInset = NSSize(width: 2, height: 1)
        drawsBackground = true
        backgroundColor = .textBackgroundColor
        applyStyle()
    }

    @available(*, unavailable)
    public required init?(coder: NSCoder) {
        nil
    }

    // MARK: - Shared state

    /// Shows the controller's text when it changed from code or from the other editor.
    public func sync(force: Bool = false) {
        guard let controller, controller.isEditing else { return }
        guard force || controller.revision != syncedRevision || (string != controller.text && window?.firstResponder !== self) else { return }
        syncing = true
        defer { syncing = false }
        if string != controller.text {
            string = controller.text
        }
        let caret = Int(min(controller.caret, UInt32(controller.text.utf16.count)))
        setSelectedRange(NSRange(location: caret, length: 0))
        syncedRevision = controller.revision
        applyStyle()
        onChange()
    }

    public override func becomeFirstResponder() -> Bool {
        let ok = super.becomeFirstResponder()
        if ok, let controller {
            if !controller.isEditing {
                controller.begin(fromFormulaBar: isFormulaBar)
                sync(force: true)
            } else if controller.fromFormulaBar != isFormulaBar {
                controller.switchedEditor(toFormulaBar: isFormulaBar)
            }
        }
        return ok
    }

    public func textDidChange(_ notification: Notification) {
        guard !syncing, let controller else { return }
        controller.textChanged(string, caret: UInt32(selectedRange().location))
        syncedRevision = controller.revision
        // Column AutoComplete: the rest of the entry, selected after the caret, so typing on
        // replaces it and Enter takes it.
        if typedAtEnd, !hasMarkedText(), let rest = controller.completion(), !rest.isEmpty {
            let at = (string as NSString).length
            syncing = true
            textStorage?.append(NSAttributedString(string: rest, attributes: typingAttributes))
            setSelectedRange(NSRange(location: at, length: (rest as NSString).length))
            syncing = false
            controller.textChanged(string, caret: UInt32(at))
        }
        applyStyle()
        onChange()
    }

    public func textView(_ textView: NSTextView, shouldChangeTextIn range: NSRange, replacementString: String?) -> Bool {
        typedAtEnd = !(replacementString ?? "").isEmpty && range.location + range.length == (string as NSString).length
        return true
    }

    public func textViewDidChangeSelection(_ notification: Notification) {
        guard !syncing, let controller, controller.isEditing else { return }
        controller.caretMoved(UInt32(selectedRange().location))
        onChange()
    }

    /// Colours the references of a formula like their outlines on the grid.
    private func applyStyle() {
        guard let storage = textStorage else { return }
        let all = NSRange(location: 0, length: storage.length)
        storage.beginEditing()
        storage.setAttributes([.font: baseFont, .foregroundColor: NSColor.textColor], range: all)
        for reference in controller?.references ?? [] {
            let range = NSRange(location: Int(reference.span.start), length: Int(reference.span.end) - Int(reference.span.start))
            if NSMaxRange(range) <= storage.length {
                storage.addAttribute(.foregroundColor, value: ReferenceColors.color(reference.color), range: range)
            }
        }
        storage.endEditing()
        typingAttributes = [.font: baseFont, .foregroundColor: NSColor.textColor]
    }

    // MARK: - Keys

    public override func keyDown(with event: NSEvent) {
        if event.keyCode == 118, let controller {
            // F4: cycle the reference's $ anchors.
            controller.cycleAnchor()
            sync(force: true)
            return
        }
        super.keyDown(with: event)
    }

    public func textView(_ textView: NSTextView, doCommandBy selector: Selector) -> Bool {
        guard let controller else { return false }
        let flags = NSApp.currentEvent?.modifierFlags ?? []
        switch selector {
        case #selector(insertNewline(_:)):
            if flags.contains(.option) {
                // Option+Enter: a line break in the cell.
                insertText("\n", replacementRange: selectedRange())
                return true
            }
            confirm(flags.contains(.command) ? .fill : (flags.contains(.shift) ? .up : .down), array: flags.contains(.command) && flags.contains(.shift))
            return true
        case #selector(insertTab(_:)):
            if !controller.autocomplete.isEmpty {
                controller.acceptAutocomplete()
                sync(force: true)
            } else {
                confirm(.right)
            }
            return true
        case #selector(insertBacktab(_:)):
            confirm(.left)
            return true
        case #selector(cancelOperation(_:)):
            controller.cancel()
            endEditing()
            return true
        case #selector(moveUp(_:)), #selector(moveDown(_:)):
            let down = selector == #selector(moveDown(_:))
            if !controller.autocomplete.isEmpty {
                let n = controller.autocomplete.count
                controller.autocompleteIndex = (controller.autocompleteIndex + (down ? 1 : n - 1)) % n
                onChange()
                return true
            }
            return arrow(dr: down ? 1 : -1, dc: 0, extend: false)
        case #selector(moveLeft(_:)):
            return arrow(dr: 0, dc: -1, extend: false)
        case #selector(moveRight(_:)):
            return arrow(dr: 0, dc: 1, extend: false)
        case #selector(moveUpAndModifySelection(_:)):
            return arrow(dr: -1, dc: 0, extend: true)
        case #selector(moveDownAndModifySelection(_:)):
            return arrow(dr: 1, dc: 0, extend: true)
        case #selector(moveLeftAndModifySelection(_:)):
            return arrow(dr: 0, dc: -1, extend: true)
        case #selector(moveRightAndModifySelection(_:)):
            return arrow(dr: 0, dc: 1, extend: true)
        default:
            return false
        }
    }

    /// Arrows: point mode inserts a reference; in Enter mode they confirm and move; otherwise
    /// the caret moves as in any text field.
    private func arrow(dr: Int, dc: Int, extend: Bool) -> Bool {
        guard let controller, controller.enterMode, !isFormulaBar else { return false }
        if controller.canPoint {
            controller.pointMove(dr: dr, dc: dc, extend: extend)
            sync(force: true)
            return true
        }
        guard !extend else { return false }
        confirm(dr > 0 ? .down : dr < 0 ? .up : dc > 0 ? .right : .left)
        return true
    }

    private func confirm(_ move: EditController.Move, array: Bool = false) {
        guard let controller else { return }
        if controller.commit(move, array: array) {
            endEditing()
        }
    }

    private func endEditing() {
        controller?.ended()
    }

    /// The formula bar between edits: the selected cell's content.
    public func showIdleText() {
        guard isFormulaBar, let controller, !controller.isEditing else { return }
        syncing = true
        string = controller.idleText
        syncing = false
        applyStyle()
    }
}
