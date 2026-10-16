import AppKit
import GridCraftEngine
import Observation

/// One cell edit, shared by the in-cell editor and the formula bar (typing in one shows in the
/// other). Every rule is the engine's: where the edit starts, point mode, F4, autocomplete and
/// hints, the references to colour, and confirming the entry (`edit.commit`: formula syntax,
/// data validation).
@MainActor
@Observable
public final class EditController {
    /// Where the selection goes after the entry is confirmed.
    public enum Move {
        case none, down, up, right, left
        /// ⌘Enter: the entry goes into every selected cell.
        case fill
    }

    public let document: SpreadsheetDocument
    /// The interface's formula syntax (`en`, `es`).
    public var locale = "en"

    public private(set) var isEditing = false
    /// The cell being edited and the sheet it is on.
    public private(set) var cell = CellAddress(row: 0, col: 0)
    public private(set) var sheet: UInt32 = 0
    public private(set) var text = ""
    /// Caret, in UTF-16 units (as `NSTextView` counts).
    public private(set) var caret: UInt32 = 0
    /// The reference point mode inserted last (arrows and clicks replace it).
    public private(set) var point: TextSpan?
    /// Where point mode's keyboard cursor is, and the corner it started from (Shift+arrows).
    private var pointCell: CellAddress?
    private var pointAnchor: CellAddress?
    /// Started by typing (arrows confirm and move) rather than F2 or a click (arrows move the caret).
    public private(set) var enterMode = false
    public private(set) var fromFormulaBar = false
    public private(set) var autocomplete: [String] = []
    public var autocompleteIndex = 0
    public private(set) var hint: FunctionHint?
    public private(set) var references: [FormulaReference] = []
    /// Bumped when the text changes from code (point mode, F4, autocomplete): editors re-read it.
    public private(set) var revision = 0
    /// What the selected cell shows in the formula bar while nothing is being edited.
    public var idleText: String {
        _ = document.generation
        return (try? document.engine.editStart(locale: locale))?.text ?? ""
    }

    @ObservationIgnored private var functionNames: [String] = []
    /// The two editors and the grid, kept in step with each other.
    @ObservationIgnored public weak var cellEditor: CellTextView?
    @ObservationIgnored public weak var formulaBar: CellTextView?
    @ObservationIgnored public weak var grid: GridNSView?

    /// An editor changed the text or caret: the other editor follows, the grid redraws.
    public func changed(from source: CellTextView) {
        if source !== cellEditor { cellEditor?.sync() }
        if source !== formulaBar { formulaBar?.sync() }
        grid?.editorChanged()
    }

    /// The edit ended: the grid takes the keyboard again.
    public func ended() {
        formulaBar?.showIdleText()
        grid?.editorEnded()
    }

    public init(document: SpreadsheetDocument) {
        self.document = document
    }

    private var engine: Engine { document.engine }

    /// Starts editing the active cell: with `typing` replacing its content (Enter mode), or
    /// from its current text (F2, double-click, a click into the formula bar).
    public func begin(typing: String? = nil, fromFormulaBar: Bool = false) {
        guard !isEditing, let selection = try? engine.selection() else { return }
        let start = try? engine.editStart(locale: locale)
        guard start != nil || typing != nil else { return }
        cell = start?.cell ?? selection.active
        sheet = (try? engine.sheet().index) ?? 0
        text = typing ?? start?.text ?? ""
        caret = UInt32(text.utf16.count)
        point = nil
        pointCell = nil
        pointAnchor = nil
        enterMode = typing != nil
        self.fromFormulaBar = fromFormulaBar
        functionNames = (try? engine.functionNames(locale: locale)) ?? []
        isEditing = true
        refresh()
        revision += 1
    }

    /// The user typed or deleted in an editor.
    public func textChanged(_ text: String, caret: UInt32) {
        self.text = text
        self.caret = caret
        point = nil
        pointCell = nil
        refresh()
    }

    /// The whole text replaced from outside the editors (the control channel), caret at the end.
    public func replaceText(_ text: String) {
        guard isEditing else { return }
        textChanged(text, caret: UInt32(text.utf16.count))
        revision += 1
        cellEditor?.sync()
        formulaBar?.sync()
        grid?.editorChanged()
    }

    /// The caret moved in an editor (no text change).
    public func caretMoved(_ caret: UInt32) {
        guard caret != self.caret else { return }
        self.caret = caret
        hint = entryFunctionHint(text: text, caret: caret, locale: locale)
        autocomplete = entryAutocomplete(text: text, caret: caret, names: functionNames)
    }

    /// The other editor was clicked into: arrows move the caret there from now on.
    public func switchedEditor(toFormulaBar: Bool) {
        fromFormulaBar = toFormulaBar
        enterMode = false
    }

    private func refresh() {
        autocomplete = entryAutocomplete(text: text, caret: caret, names: functionNames)
        autocompleteIndex = min(autocompleteIndex, max(0, autocomplete.count - 1))
        hint = entryFunctionHint(text: text, caret: caret, locale: locale)
        references = entryIsFormula(text: text) ? entryFormulaReferences(text: text) : []
    }

    private func apply(_ edit: EntryEdit) {
        text = edit.text
        caret = edit.caret
        point = edit.point
        refresh()
        revision += 1
    }

    /// Column AutoComplete: the rest of the column entry the typed text completes to.
    public func completion() -> String? {
        guard isEditing, !entryIsFormula(text: text), caret == UInt32(text.utf16.count),
              let full = try? engine.columnCompletion(cell: cell, typed: text) else { return nil }
        return entryCompletionRest(full: full, typed: text)
    }

    // MARK: - Point mode

    /// A click or an arrow inserts a reference now.
    public var canPoint: Bool {
        isEditing && entryCanPoint(text: text, caret: caret, point: point)
    }

    /// Inserts a reference to `range` on the active sheet (sheet-qualified when another sheet's
    /// cell is being edited).
    public func insertReference(_ range: CellRange) {
        guard let reference = try? engine.pointReference(range: range, editingSheet: sheet) else { return }
        apply(entryInsertReference(text: text, caret: caret, point: point, reference: reference))
    }

    /// Point mode by keyboard: moves the pointed cell (`extend`: from where it started).
    public func pointMove(dr: Int, dc: Int, extend: Bool) {
        let from = pointCell ?? (try? engine.selection().active) ?? cell
        let to = CellAddress(row: UInt32(clamping: Int(from.row) + dr), col: UInt32(clamping: Int(from.col) + dc))
        if !extend || pointAnchor == nil {
            pointAnchor = extend ? from : to
        }
        pointCell = to
        insertReference(CellRange(first: pointAnchor ?? to, last: to))
    }

    /// A click in point mode starts pointing at `cell`; a drag extends from it.
    public func pointClick(_ cell: CellAddress, extendingTo end: CellAddress? = nil) {
        pointAnchor = cell
        pointCell = end ?? cell
        insertReference(CellRange(first: cell, last: end ?? cell))
    }

    // MARK: - Editing operations

    /// F4.
    public func cycleAnchor() {
        if let edit = entryCycleAnchor(text: text, caret: caret) {
            apply(edit)
        }
    }

    /// Tab (or a click) on the autocomplete list.
    @discardableResult
    public func acceptAutocomplete(_ index: Int? = nil) -> Bool {
        let i = index ?? autocompleteIndex
        guard autocomplete.indices.contains(i) else { return false }
        apply(entryAcceptAutocomplete(text: text, caret: caret, name: autocomplete[i]))
        autocomplete = []
        return true
    }

    /// Confirms the entry; `false` when the engine refused it (a Stop validation rule, a formula
    /// that doesn't parse), and editing goes on.
    @discardableResult
    public func commit(_ move: Move = .none, array: Bool = false) -> Bool {
        guard isEditing else { return true }
        let params: [String: Any] = ["cell": cellName(cell: cell), "input": text, "locale": locale, "array": array, "fill": move == .fill]
        guard let result = document.run("edit.commit", params) else {
            // `run` put the engine's message in `document.problem`.
            return false
        }
        let object = JSON.decode(result)
        let validation = object["validation"] as? [String: Any]
        if object["committed"] as? Bool == false {
            document.problem = validation?["message"] as? String ?? "The entry wasn't accepted."
            return false
        }
        if let message = validation?["message"] as? String {
            document.notice = message
        }
        finish()
        switch move {
        case .down: moveAfterEntry(dr: 1, dc: 0)
        case .up: moveAfterEntry(dr: -1, dc: 0)
        case .right: moveAfterEntry(dr: 0, dc: 1)
        case .left: moveAfterEntry(dr: 0, dc: -1)
        case .none, .fill: break
        }
        return true
    }

    /// Before a menu command: an edit in progress is confirmed first (`false`: it was refused).
    public func commitIfEditing() -> Bool {
        guard isEditing else { return true }
        let ok = commit()
        if ok { ended() }
        return ok
    }

    /// Esc.
    public func cancel() {
        finish()
    }

    private func finish() {
        isEditing = false
        text = ""
        point = nil
        autocomplete = []
        hint = nil
        references = []
        revision += 1
    }

    /// Enter / Tab: inside a multi-cell selection to its next cell, otherwise one cell on.
    private func moveAfterEntry(dr: Int, dc: Int) {
        guard let selection = try? engine.selection() else { return }
        let single = selection.ranges.count == 1 && selection.ranges.first.map { $0.first == $0.last } == true
        if single {
            document.run("selection.move", ["dr": dr, "dc": dc])
        } else {
            document.run("selection.next", ["forward": dr > 0 || dc > 0, "byRow": dc != 0])
        }
    }
}

/// The colours references get in the editor and on the grid (our own palette).
public enum ReferenceColors {
    public static let all: [NSColor] = [
        NSColor(srgbRed: 0x2E / 255, green: 0x6F / 255, blue: 0xD8 / 255, alpha: 1),
        NSColor(srgbRed: 0xD0 / 255, green: 0x3C / 255, blue: 0x3C / 255, alpha: 1),
        NSColor(srgbRed: 0x7A / 255, green: 0x4F / 255, blue: 0xC9 / 255, alpha: 1),
        NSColor(srgbRed: 0x2B / 255, green: 0x9A / 255, blue: 0x4A / 255, alpha: 1),
        NSColor(srgbRed: 0xC2 / 255, green: 0x5E / 255, blue: 0x1E / 255, alpha: 1),
        NSColor(srgbRed: 0x1D / 255, green: 0x92 / 255, blue: 0xA6 / 255, alpha: 1),
        NSColor(srgbRed: 0xB8 / 255, green: 0x3B / 255, blue: 0x8E / 255, alpha: 1),
        NSColor(srgbRed: 0x8A / 255, green: 0x7A / 255, blue: 0x1F / 255, alpha: 1),
    ]

    public static func color(_ index: UInt32) -> NSColor {
        all[Int(index) % all.count]
    }
}
