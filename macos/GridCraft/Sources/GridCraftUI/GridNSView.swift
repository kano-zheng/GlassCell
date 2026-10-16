import AppKit
import GridCraftEngine
import SwiftUI

/// The worksheet grid: a layer-backed AppKit view that draws only the visible cells (the engine
/// lays them out) and scrolls itself, since a sheet is far bigger than any view.
@MainActor
public final class GridNSView: NSView {
    public var document: SpreadsheetDocument? {
        didSet { needsDisplay = true }
    }
    /// Sheet units scrolled past the frozen panes.
    public private(set) var scroll = CGPoint.zero
    /// The last frame's drawing time, in milliseconds.
    public private(set) var lastFrameMilliseconds: Double = 0
    public var onFrame: ((Double) -> Void)?
    /// The cell edit shared with the formula bar.
    public var editor: EditController? {
        didSet {
            editor?.grid = self
            editor?.cellEditor = cellEditor
            cellEditor.controller = editor
            popup.controller = editor
        }
    }

    private lazy var cellEditor: CellTextView = {
        let view = CellTextView(frame: .zero, formulaBar: false)
        view.isHidden = true
        addSubview(view)
        return view
    }()

    private lazy var popup: EditorPopupView = {
        let view = EditorPopupView(frame: .zero)
        view.isHidden = true
        view.onPick = { [weak self] index in
            self?.editor?.acceptAutocomplete(index)
            self?.cellEditor.sync(force: true)
        }
        addSubview(view)
        return view
    }()

    private let measurer = CoreTextMeasurer()
    private var lastReport: CFAbsoluteTime = 0
    private var layoutCache: GridRenderer.Layout?
    private var sheetIndex: UInt32?

    public override var isFlipped: Bool { true }
    public override var acceptsFirstResponder: Bool { true }
    public override var wantsUpdateLayer: Bool { false }

    public override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        layerContentsRedrawPolicy = .onSetNeedsDisplay
    }

    @available(*, unavailable)
    public required init?(coder: NSCoder) {
        nil
    }

    /// The grid has the keyboard when its window opens (not the Name Box, SwiftUI's first field).
    public override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        guard window != nil else { return }
        Task { @MainActor [weak self] in
            guard let self, let window = self.window else { return }
            window.makeFirstResponder(self)
        }
    }

    public override func draw(_ dirtyRect: NSRect) {
        guard let document, let ctx = NSGraphicsContext.current?.cgContext else { return }
        let start = CFAbsoluteTimeGetCurrent()
        if let sheet = try? document.engine.sheet(), sheet.index != sheetIndex {
            // Another sheet opens where its file left it.
            sheetIndex = sheet.index
            if let saved = try? document.engine.savedScroll() {
                scroll = CGPoint(x: CGFloat(saved.x), y: CGFloat(saved.y))
            }
        }
        var failure: String?
        do {
            var renderer = GridRenderer(engine: document.engine, measurer: measurer)
            renderer.editing = editor?.isEditing == true
            renderer.formulaLocale = editor?.locale ?? "en"
            let layout = try renderer.draw(in: ctx, size: bounds.size, scroll: scroll)
            layoutCache = layout
            drawReferenceOutlines(ctx, layout: layout)
        } catch {
            failure = error.localizedDescription
        }
        lastFrameMilliseconds = (CFAbsoluteTimeGetCurrent() - start) * 1000
        // SwiftUI state must not change while AppKit is drawing (it throws "needs update
        // constraints" mid-display and the app goes down): report after this display pass.
        let now = CFAbsoluteTimeGetCurrent()
        let report = now - lastReport > 0.5
        if report {
            lastReport = now
        }
        guard failure != nil || report else { return }
        let ms = lastFrameMilliseconds
        Task { @MainActor [weak self, weak document] in
            if let failure {
                document?.problem = failure
            }
            if report {
                self?.onFrame?(ms)
            }
        }
    }

    /// While a formula is edited: its references outlined in their colours.
    private func drawReferenceOutlines(_ ctx: CGContext, layout: GridRenderer.Layout) {
        guard let editor, editor.isEditing, !editor.references.isEmpty, let sheet = try? document?.engine.sheet(), sheet.index == editor.sheet else { return }
        let cells = CGRect(x: layout.headerWidth, y: layout.headerHeight, width: bounds.width - layout.headerWidth, height: bounds.height - layout.headerHeight)
        ctx.saveGState()
        ctx.clip(to: cells)
        for reference in editor.references where reference.sheet == nil || reference.sheet?.caseInsensitiveCompare(sheet.name) == .orderedSame {
            let r = cgRect(layout.geometry.rangeRect(range: reference.range))
            let color = ReferenceColors.color(reference.color)
            ctx.setFillColor(color.withAlphaComponent(0.08).cgColor)
            ctx.fill(r)
            ctx.setStrokeColor(color.cgColor)
            ctx.setLineWidth(1.5)
            ctx.stroke(r.insetBy(dx: 0.5, dy: 0.5))
        }
        ctx.restoreGState()
    }

    // MARK: - The cell editor

    /// Starts editing the active cell: typing (`event` goes on to the editor, so input methods
    /// compose from the first key) or F2 / double-click (`event` nil: from its current text).
    public func beginEditing(with event: NSEvent? = nil) {
        guard let editor else { return }
        editor.begin(typing: event == nil ? nil : "")
        guard editor.isEditing else { return }
        showEditor()
        if let event {
            cellEditor.interpretKeyEvents([event])
        }
    }

    private func showEditor() {
        guard let editor, let document else { return }
        // The editor writes in the cell's font, at the zoom.
        if let format = try? document.engine.selectionFormat(), let layout = layoutCache {
            let size = CGFloat(format.active.fontSize ?? 11) * 96 / 72 * CGFloat(layout.scale)
            cellEditor.baseFont = NSFont(name: format.active.fontName ?? "", size: size) ?? .systemFont(ofSize: size)
        }
        cellEditor.isHidden = false
        cellEditor.sync(force: true)
        positionEditor()
        if !editor.fromFormulaBar {
            window?.makeFirstResponder(cellEditor)
        }
    }

    /// The editor sits over the edited cell and grows to the right with its text.
    private func positionEditor() {
        guard let editor, editor.isEditing, let document, let layout = try? GridRenderer(engine: document.engine, measurer: measurer).layout(size: bounds.size, scroll: scroll) else {
            cellEditor.isHidden = true
            popup.isHidden = true
            return
        }
        let r = cgRect(layout.geometry.cellRect(cell: editor.cell))
        cellEditor.layoutManager?.ensureLayout(for: cellEditor.textContainer ?? NSTextContainer())
        let used = cellEditor.layoutManager?.usedRect(for: cellEditor.textContainer ?? NSTextContainer()).size ?? .zero
        let width = min(max(r.width, used.width + 12), max(r.width, bounds.width - r.minX))
        let height = max(r.height, used.height + 4)
        cellEditor.frame = NSRect(x: r.minX, y: r.minY, width: width, height: height)
        cellEditor.isHidden = r.maxX < layout.headerWidth || r.minY > bounds.height
        let size = popup.fittingSize()
        popup.isHidden = size == .zero || cellEditor.isHidden
        popup.frame = NSRect(x: r.minX, y: cellEditor.frame.maxY + 2, width: size.width, height: size.height)
        popup.needsDisplay = true
    }

    /// The edit's text or caret changed (in the cell or in the formula bar).
    public func editorChanged() {
        guard let editor else { return }
        if editor.isEditing, cellEditor.isHidden {
            showEditor()
        }
        positionEditor()
        needsDisplay = true
    }

    /// The edit ended (confirmed or cancelled).
    public func editorEnded() {
        cellEditor.isHidden = true
        popup.isHidden = true
        window?.makeFirstResponder(self)
        reveal()
        needsDisplay = true
    }

    // MARK: - Scrolling and zoom

    public func scroll(to point: CGPoint) {
        scroll = CGPoint(x: max(0, point.x), y: max(0, point.y))
        try? document?.engine.storeScroll(scroll: GridCraftUI.point(scroll.x, scroll.y))
        positionEditor()
        needsDisplay = true
    }

    public override func scrollWheel(with event: NSEvent) {
        guard let layout = layoutCache else { return }
        if event.modifierFlags.contains(.command) {
            zoom(by: event.scrollingDeltaY > 0 ? 1.1 : 1 / 1.1)
            return
        }
        let scale = CGFloat(layout.scale)
        var dx = event.scrollingDeltaX, dy = event.scrollingDeltaY
        if !event.hasPreciseScrollingDeltas {
            dx *= 20
            dy *= 20
        }
        scroll(to: CGPoint(x: scroll.x - dx / scale, y: scroll.y - dy / scale))
    }

    public override func magnify(with event: NSEvent) {
        zoom(by: 1 + event.magnification)
    }

    private func zoom(by factor: CGFloat) {
        guard let document, let sheet = try? document.engine.sheet() else { return }
        let percent = Int((CGFloat(sheet.zoom) * factor).rounded())
        document.run("view.zoom", ["percent": max(10, min(400, percent))])
        needsDisplay = true
    }

    // MARK: - Selection

    /// What a drag that started with the mouse down extends.
    private enum DragKind {
        case none, cells, rows, columns, pointing
    }

    private var drag = DragKind.none
    /// The cell where the drag started (the selection's anchor).
    private var dragAnchor = CellAddress(row: 0, col: 0)
    /// Areas selected before a ⌘-click started adding one.
    private var keptRanges: [CellRange] = []

    private static let lastRow: UInt32 = 1_048_575
    private static let lastCol: UInt32 = 16_383

    public override func mouseDown(with event: NSEvent) {
        window?.makeFirstResponder(self)
        guard let document, let layout = layoutCache else { return }
        let p = convert(event.locationInWindow, from: nil)
        let flags = event.modifierFlags
        keptRanges = flags.contains(.command) ? ((try? document.engine.selection().ranges) ?? []) : []
        let geometry = layout.geometry
        if let editor, editor.isEditing {
            let inCells = p.x >= layout.headerWidth && p.y >= layout.headerHeight
            if inCells, editor.canPoint {
                // Point mode: the click inserts a reference; a drag extends it.
                let cell = CellAddress(row: geometry.rowAt(y: Float(p.y)), col: geometry.colAt(x: Float(p.x)))
                dragAnchor = cell
                drag = .pointing
                editor.pointClick(cell)
                cellEditor.sync(force: true)
                window?.makeFirstResponder(editor.fromFormulaBar ? editor.formulaBar : cellEditor)
                return
            }
            guard editor.commit() else { return }
            editorEnded()
        }
        if event.clickCount == 2, p.x >= layout.headerWidth, p.y >= layout.headerHeight {
            beginEditing()
            return
        }
        let inColumnHeader = p.y < layout.headerHeight && p.x >= layout.headerWidth
        let inRowHeader = p.x < layout.headerWidth && p.y >= layout.headerHeight
        if p.x < layout.headerWidth && p.y < layout.headerHeight {
            document.run("edit.selectAll")
            drag = .none
        } else if inColumnHeader {
            let col = geometry.colAt(x: Float(p.x))
            dragAnchor = shiftAnchor(flags) ?? CellAddress(row: 0, col: col)
            drag = .columns
            selectColumns(dragAnchor.col, col)
        } else if inRowHeader {
            let row = geometry.rowAt(y: Float(p.y))
            dragAnchor = shiftAnchor(flags) ?? CellAddress(row: row, col: 0)
            drag = .rows
            selectRows(dragAnchor.row, row)
        } else {
            switch geometry.hit(x: Float(p.x), y: Float(p.y)) {
            case let .cell(cell), let .filterButton(cell):
                dragAnchor = shiftAnchor(flags) ?? cell
                drag = .cells
                select(CellRange(first: dragAnchor, last: cell))
            default:
                drag = .none
            }
        }
        needsDisplay = true
    }

    public override func mouseDragged(with event: NSEvent) {
        guard drag != .none, let layout = layoutCache else { return }
        let p = convert(event.locationInWindow, from: nil)
        // Dragging past the edge scrolls (and selects) further.
        autoscroll(towards: p, layout: layout)
        let geometry = layout.geometry
        let cell = CellAddress(row: geometry.rowAt(y: Float(max(p.y, layout.headerHeight + 1))), col: geometry.colAt(x: Float(max(p.x, layout.headerWidth + 1))))
        switch drag {
        case .cells: select(CellRange(first: dragAnchor, last: cell))
        case .rows: selectRows(dragAnchor.row, cell.row)
        case .columns: selectColumns(dragAnchor.col, cell.col)
        case .pointing:
            editor?.pointClick(dragAnchor, extendingTo: cell)
            cellEditor.sync(force: true)
        case .none: break
        }
        needsDisplay = true
    }

    public override func mouseUp(with event: NSEvent) {
        drag = .none
    }

    private func shiftAnchor(_ flags: NSEvent.ModifierFlags) -> CellAddress? {
        guard flags.contains(.shift), let document else { return nil }
        return try? document.engine.selection().active
    }

    private func autoscroll(towards p: CGPoint, layout: GridRenderer.Layout) {
        let step = 20 / CGFloat(layout.scale)
        var target = scroll
        if p.x > bounds.maxX { target.x += step }
        if p.y > bounds.maxY { target.y += step }
        if p.x < layout.headerWidth { target.x -= step }
        if p.y < layout.headerHeight { target.y -= step }
        if target != scroll {
            scroll(to: target)
        }
    }

    /// Selects `area` (keeping the areas a ⌘-click adds to), with the anchor as the active cell.
    private func select(_ area: CellRange) {
        guard let document else { return }
        let ordered = CellRange(first: CellAddress(row: min(area.first.row, area.last.row), col: min(area.first.col, area.last.col)),
                                last: CellAddress(row: max(area.first.row, area.last.row), col: max(area.first.col, area.last.col)))
        let names = (keptRanges + [ordered]).map { rangeName(range: $0) }
        document.run("selection.set", ["range": names.joined(separator: ","), "active": cellName(cell: dragAnchor)])
    }

    private func selectRows(_ a: UInt32, _ b: UInt32) {
        select(CellRange(first: CellAddress(row: a, col: 0), last: CellAddress(row: b, col: Self.lastCol)))
    }

    private func selectColumns(_ a: UInt32, _ b: UInt32) {
        select(CellRange(first: CellAddress(row: 0, col: a), last: CellAddress(row: Self.lastRow, col: b)))
    }

    // MARK: - Keyboard

    public override func keyDown(with event: NSEvent) {
        guard let document else { return }
        let flags = event.modifierFlags
        let shift = flags.contains(.shift), command = flags.contains(.command)
        let page = max(1, Int((bounds.height - (layoutCache?.headerHeight ?? 0)) / max(1, 16 * CGFloat(layoutCache?.scale ?? 0.8) / 0.8)) - 1)
        func move(_ dr: Int, _ dc: Int) {
            document.run("selection.move", ["dr": dr, "dc": dc, "extend": shift, "jump": command])
        }
        if event.keyCode == 120 {
            // F2: edit the active cell.
            beginEditing()
            return
        }
        if event.specialKey == nil, !command, !flags.contains(.control), let chars = event.characters, !chars.isEmpty,
           chars.unicodeScalars.allSatisfy({ !CharacterSet.controlCharacters.contains($0) }) {
            // Typing replaces the active cell's content.
            beginEditing(with: event)
            return
        }
        if event.specialKey == .backspace || event.specialKey == .delete, !command {
            // Backspace clears the cell and starts editing it.
            document.run("edit.clearContents")
            beginEditing(with: nil)
            return
        }
        switch event.specialKey {
        case .upArrow?: move(-1, 0)
        case .downArrow?: move(1, 0)
        case .leftArrow?: move(0, -1)
        case .rightArrow?: move(0, 1)
        case .pageDown? where command: document.run("sheet.next")
        case .pageUp? where command: document.run("sheet.previous")
        case .pageDown?: move(page, 0)
        case .pageUp?: move(-page, 0)
        case .home? where command: document.run("selection.set", ["cell": "A1"])
        case .home?:
            if let active = try? document.engine.selection().active {
                document.run("selection.set", ["cell": cellName(cell: CellAddress(row: active.row, col: 0))])
            }
        case .carriageReturn?, .enter?: moveAfterEntry(down: !shift)
        case .tab?: moveAfterEntry(right: !shift)
        case .deleteForward?: document.run("edit.clearContents")
        default:
            super.keyDown(with: event)
            return
        }
        reveal()
    }

    /// Enter / Tab: within a selection of several cells, to the next one in it; otherwise one cell on.
    private func moveAfterEntry(down: Bool? = nil, right: Bool? = nil) {
        guard let document, let selection = try? document.engine.selection() else { return }
        let single = selection.ranges.count == 1 && selection.ranges.first.map { $0.first == $0.last } == true
        if single {
            if let down {
                document.run("selection.move", ["dr": down ? 1 : -1, "dc": 0])
            } else if let right {
                document.run("selection.move", ["dr": 0, "dc": right ? 1 : -1])
            }
        } else {
            document.run("selection.next", ["forward": (down ?? right) ?? true, "byRow": right != nil])
        }
    }

    /// Scrolls the active cell into view.
    public func reveal() {
        guard let document, let layout = layoutCache, let active = try? document.engine.selection().active else { return }
        let target = layout.geometry.reveal(cell: active)
        scroll(to: CGPoint(x: CGFloat(target.x), y: CGFloat(target.y)))
    }

    // MARK: - Edit menu

    @objc public func copy(_ sender: Any?) {
        copyToPasteboard(cut: false)
    }

    @objc public func cut(_ sender: Any?) {
        copyToPasteboard(cut: true)
    }

    @objc public func paste(_ sender: Any?) {
        guard let document else { return }
        let pasteboard = NSPasteboard.general
        let text = pasteboard.string(forType: .string)
        let source = (try? document.engine.pasteForCommand(system: text)) ?? .internal
        switch source {
        case .internal:
            document.run("edit.paste")
        case let .text(text):
            var params: [String: Any] = ["text": text]
            if let html = pasteboard.string(forType: .html) {
                params["html"] = html
            }
            document.run("edit.paste", params)
        }
        needsDisplay = true
    }

    @objc public override func selectAll(_ sender: Any?) {
        document?.run("edit.selectAll")
        needsDisplay = true
    }

    @objc public func delete(_ sender: Any?) {
        document?.run("edit.clearContents")
        needsDisplay = true
    }

    /// Copies (or cuts) the selection to the system clipboard as plain text and HTML.
    private func copyToPasteboard(cut: Bool) {
        guard let document, let result = document.run(cut ? "edit.cut" : "edit.copy", ["html": true]) else { return }
        let object = JSON.decode(result)
        guard let text = object["text"] as? String else { return }
        let pasteboard = NSPasteboard.general
        pasteboard.clearContents()
        pasteboard.setString(text, forType: .string)
        if let html = object["html"] as? String {
            pasteboard.setString(html, forType: .html)
        }
        try? document.engine.clipboardPublished(cut: cut, text: text)
        needsDisplay = true
    }
}

extension GridNSView: NSMenuItemValidation {
    public func validateMenuItem(_ item: NSMenuItem) -> Bool {
        guard document != nil else { return false }
        switch item.action {
        case #selector(paste(_:)):
            return NSPasteboard.general.string(forType: .string) != nil || (try? document?.engine.commands().first { $0.id == "edit.paste" }?.enabled) == true
        default:
            return true
        }
    }
}

/// The grid in SwiftUI.
public struct GridView: NSViewRepresentable {
    @ObservedObject var document: SpreadsheetDocument
    let editor: EditController
    var onFrame: ((Double) -> Void)?

    public init(document: SpreadsheetDocument, editor: EditController, onFrame: ((Double) -> Void)? = nil) {
        self.document = document
        self.editor = editor
        self.onFrame = onFrame
    }

    public func makeNSView(context: Context) -> GridNSView {
        let view = GridNSView(frame: .zero)
        view.document = document
        view.editor = editor
        view.onFrame = onFrame
        return view
    }

    public func updateNSView(_ view: GridNSView, context: Context) {
        _ = document.generation
        view.document = document
        view.onFrame = onFrame
        view.needsDisplay = true
    }
}
