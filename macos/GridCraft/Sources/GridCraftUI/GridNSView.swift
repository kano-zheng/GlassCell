import AppKit
import CoreImage
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
    /// Points at the bottom covered by the window's floating status bar: revealing a cell keeps
    /// it above them.
    public var bottomInset: CGFloat = 0
    /// Points at the right covered by the floating inspector: revealing a cell keeps it left of them.
    public var trailingInset: CGFloat = 0
    /// Where the grid is scrolled across, for the scroller.
    public var scrollState: GridScrollState?
    /// Points at the top covered by the window's toolbar and formula bar. The grid starts below
    /// them; above, the rows scrolled past go on under frosted glass, the column headers on it.
    /// This view's coordinates start where the grid does (its bounds begin at `-topInset`).
    public var topInset: CGFloat = 0 {
        didSet {
            guard topInset != oldValue else { return }
            setBoundsOrigin(NSPoint(x: 0, y: -topInset))
            needsDisplay = true
        }
    }
    /// Where the grid is laid out: the view below `topInset`, in this view's coordinates.
    public var gridBounds: CGRect {
        CGRect(x: 0, y: 0, width: bounds.width, height: max(0, bounds.height - topInset))
    }
    private var gridSize: CGSize { gridBounds.size }
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
        view.onPick = { [weak self] index in
            self?.editor?.acceptAutocomplete(index)
            self?.cellEditor.sync(force: true)
        }
        view.autoresizingMask = [.width, .height]
        popupGlass.contentView = view
        return view
    }()

    /// The Liquid Glass the popup sits on.
    private lazy var popupGlass: NSGlassEffectView = {
        let glass = NSGlassEffectView(frame: .zero)
        glass.cornerRadius = 10
        glass.isHidden = true
        addSubview(glass)
        return glass
    }()

    public override func setFrameSize(_ newSize: NSSize) {
        super.setFrameSize(newSize)
        setBoundsOrigin(NSPoint(x: 0, y: -topInset))
    }

    /// Tells the scroller where the grid is, after this display pass (SwiftUI state must not
    /// change while AppKit draws).
    private func publishScroll(_ layout: GridRenderer.Layout) {
        guard let state = scrollState else { return }
        let extent = layout.geometry.scrollExtent()
        let frozen = layout.geometry.frozenSize()
        let scale = max(CGFloat(layout.scale), 0.01)
        let across = bounds.width - layout.headerWidth - CGFloat(frozen.width) - trailingInset
        let down = gridSize.height - layout.headerHeight - CGFloat(frozen.height) - bottomInset
        let values = (x: scroll.x, y: scroll.y, extentX: CGFloat(extent.x), extentY: CGFloat(extent.y),
                      visibleX: max(0, across / scale), visibleY: max(0, down / scale), cellsTop: layout.headerHeight + CGFloat(frozen.height))
        guard state.x != values.x || state.y != values.y || state.extentX != values.extentX || state.extentY != values.extentY
            || state.visibleX != values.visibleX || state.visibleY != values.visibleY || state.cellsTop != values.cellsTop
        else { return }
        Task { @MainActor in
            state.update(x: values.x, y: values.y, extentX: values.extentX, extentY: values.extentY,
                         visibleX: values.visibleX, visibleY: values.visibleY, cellsTop: values.cellsTop)
        }
    }

    /// Blurs the rows under the window's chrome.
    private static let blurContext = CIContext()

    /// Above the cells, frosted: the rows before the first one shown, as if the sheet went on up
    /// under the window's chrome (not with frozen rows, nor in Page Layout view), blurred and
    /// washed white; the column headers on top, translucent.
    private func drawUnderChrome(_ ctx: CGContext, renderer: GridRenderer, layout: GridRenderer.Layout, selection: SelectionState) throws {
        let area = CGRect(x: 0, y: -topInset, width: bounds.width, height: topInset + layout.headerHeight)
        ctx.setFillColor(Palette.sheet)
        ctx.fill(area)
        if layout.geometry.frozen().row == 0, layout.geometry.pages().isEmpty, let rows = try rowsAbove(renderer: renderer, layout: layout, area: area) {
            // A flipped context draws images upside down.
            ctx.saveGState()
            ctx.translateBy(x: 0, y: area.maxY)
            ctx.scaleBy(x: 1, y: -1)
            ctx.draw(rows, in: CGRect(x: 0, y: 0, width: area.width, height: area.height))
            ctx.restoreGState()
        }
        ctx.setFillColor(Palette.sheet.copy(alpha: 0.3) ?? Palette.sheet)
        ctx.fill(area)
        renderer.drawColumnHeaders(in: ctx, layout: layout, width: bounds.width, selection: selection, background: Palette.header.copy(alpha: 0.55) ?? Palette.header)
    }

    /// The rows scrolled past, drawn for `area` and blurred.
    private func rowsAbove(renderer: GridRenderer, layout: GridRenderer.Layout, area: CGRect) throws -> CGImage? {
        let pixels = window?.backingScaleFactor ?? 2
        let width = Int((area.width * pixels).rounded(.up)), height = Int((area.height * pixels).rounded(.up))
        guard width > 0, height > 0, width < 20_000, height < 20_000,
              let bitmap = CGContext(data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: 0,
                                     space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.premultipliedFirst.rawValue)
        else { return nil }
        // Draw as this view does: flipped, in points, the top of `area` at the top.
        bitmap.scaleBy(x: pixels, y: pixels)
        bitmap.translateBy(x: 0, y: area.height)
        bitmap.scaleBy(x: 1, y: -1)
        bitmap.translateBy(x: 0, y: -area.minY)
        let scale = CGFloat(layout.scale)
        let above = CGPoint(x: scroll.x, y: max(0, scroll.y - area.height / max(scale, 0.01)))
        var strip = renderer
        strip.columnHeaders = false
        strip.headerWidth = layout.headerWidth
        // Sheet units `above.y` start where `scroll.y` would be minus the area's height.
        bitmap.translateBy(x: 0, y: (above.y - scroll.y) * scale)
        try strip.draw(in: bitmap, size: CGSize(width: area.width, height: area.height + layout.headerHeight * 2), scroll: above)
        guard let sharp = bitmap.makeImage() else { return nil }
        let image = CIImage(cgImage: sharp)
        let blurred = image.clampedToExtent().applyingGaussianBlur(sigma: 2.5 * pixels).cropped(to: image.extent)
        return Self.blurContext.createCGImage(blurred, from: image.extent)
    }

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
            renderer.selectedObject = selectedObject?.id
            renderer.formulaLocale = editor?.locale ?? "en"
            let layout: GridRenderer.Layout
            if topInset > 0 {
                // The cells and row headers below the header band, then the frosted top.
                renderer.columnHeaders = false
                let planned = try renderer.layout(size: gridSize, scroll: scroll)
                ctx.saveGState()
                ctx.clip(to: CGRect(x: 0, y: planned.headerHeight, width: bounds.width, height: max(0, gridSize.height - planned.headerHeight)))
                layout = try renderer.draw(in: ctx, size: gridSize, scroll: scroll)
                ctx.restoreGState()
                try drawUnderChrome(ctx, renderer: renderer, layout: layout, selection: document.engine.selection())
            } else {
                layout = try renderer.draw(in: ctx, size: gridSize, scroll: scroll)
            }
            layoutCache = layout
            publishScroll(layout)
            drawReferenceOutlines(ctx, layout: layout)
            drawDragPreview(ctx, layout: layout)
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
        let cells = CGRect(x: layout.headerWidth, y: layout.headerHeight, width: bounds.width - layout.headerWidth, height: gridSize.height - layout.headerHeight)
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

    /// A fill or move in progress: the target outlined; a resize: the guide line.
    private func drawDragPreview(_ ctx: CGContext, layout: GridRenderer.Layout) {
        ctx.saveGState()
        defer { ctx.restoreGState() }
        ctx.setStrokeColor(NSColor.secondaryLabelColor.cgColor)
        if let preview = objectPreview {
            ctx.setLineWidth(1.5)
            ctx.setLineDash(phase: 0, lengths: [4, 3])
            ctx.stroke(preview)
        }
        if let target = fillPreview ?? movePreview {
            ctx.setLineWidth(1.5)
            ctx.setLineDash(phase: 0, lengths: [4, 3])
            ctx.stroke(cgRect(layout.geometry.rangeRect(range: target)).insetBy(dx: 0.5, dy: 0.5))
        }
        if let guide = resizeGuide {
            ctx.setLineWidth(1)
            ctx.setLineDash(phase: 0, lengths: [3, 2])
            ctx.beginPath()
            if guide.columns {
                ctx.move(to: CGPoint(x: guide.at, y: 0))
                ctx.addLine(to: CGPoint(x: guide.at, y: gridSize.height))
            } else {
                ctx.move(to: CGPoint(x: 0, y: guide.at))
                ctx.addLine(to: CGPoint(x: bounds.width, y: guide.at))
            }
            ctx.strokePath()
        }
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

    /// Starts editing for a command (`UiRequest.editCell`): with `text` (an engine template such
    /// as `=SUM(`), or from the cell's content.
    public func beginEditing(text: String?) {
        guard let editor else { return }
        editor.begin(typing: text)
        if editor.isEditing {
            showEditor()
        }
    }

    private func showEditor() {
        guard let editor, let document else { return }
        // The editor writes in the cell's font, at the zoom.
        if let format = try? document.engine.selectionFormat(), let layout = layoutCache {
            let size = CGFloat(format.active.fontSize ?? 11) * 96 / 72 * CGFloat(layout.scale)
            cellEditor.baseFont = NSFont(name: format.active.fontName ?? "", size: size) ?? .systemFont(ofSize: size)
            cellEditor.valign = format.active.valign ?? .bottom
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
        guard let editor, editor.isEditing, let document, let layout = try? GridRenderer(engine: document.engine, measurer: measurer).layout(size: gridSize, scroll: scroll) else {
            cellEditor.isHidden = true
            popupGlass.isHidden = true
            return
        }
        let r = cgRect(layout.geometry.cellRect(cell: editor.cell))
        cellEditor.layoutManager?.ensureLayout(for: cellEditor.textContainer ?? NSTextContainer())
        let used = cellEditor.layoutManager?.usedRect(for: cellEditor.textContainer ?? NSTextContainer()).size ?? .zero
        let width = min(max(r.width, used.width + 12), max(r.width, bounds.width - r.minX))
        let height = max(r.height, used.height + 4)
        cellEditor.frame = NSRect(x: r.minX, y: r.minY, width: width, height: height)
        // The text sits where the cell draws it (bottom by default), so the caret is not at the top.
        let z = CGFloat(layout.scale)
        let top = 1 * z, bottom = max(top, height - used.height - 1.5 * z)
        let lineTop: CGFloat
        switch cellEditor.valign {
        case .top: lineTop = top
        case .center, .justify, .distributed: lineTop = (top + bottom) / 2
        case .bottom: lineTop = bottom
        }
        cellEditor.textContainerInset = NSSize(width: 2, height: lineTop)
        cellEditor.isHidden = r.maxX < layout.headerWidth || r.minY > gridSize.height
        let size = popup.fittingSize()
        popupGlass.isHidden = size == .zero || cellEditor.isHidden
        popupGlass.frame = NSRect(x: r.minX, y: cellEditor.frame.maxY + 4, width: size.width, height: size.height)
        popup.frame = popupGlass.bounds
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

    /// Where `cell` is drawn now, in this view's coordinates (the control channel clicks there).
    public func cellFrame(_ cell: CellAddress) -> CGRect? {
        guard let document, let layout = try? GridRenderer(engine: document.engine, measurer: measurer).layout(size: gridSize, scroll: scroll) else {
            return nil
        }
        return cgRect(layout.geometry.cellRect(cell: cell))
    }

    /// The edit ended (confirmed or cancelled).
    public func editorEnded() {
        cellEditor.isHidden = true
        popupGlass.isHidden = true
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
    private enum DragKind: Equatable {
        case none, cells, rows, columns, pointing
        /// The fill handle: from `source`, to where the pointer is.
        case fill(source: CellRange)
        /// Resizing column / row `index`, which started `original` sheet units big, from screen `start`.
        case resizeColumn(index: UInt32, start: CGFloat, original: CGFloat)
        case resizeRow(index: UInt32, start: CGFloat, original: CGFloat)
        /// Moving the selection by its border: grabbed at `grab`.
        case move(grab: CellAddress)
        /// Moving or resizing a floating object, from screen point `start`.
        case moveObject(start: CGPoint)
        case resizeObject(start: CGPoint)
    }

    /// The live outcome of a drag, drawn over the grid until the mouse goes up.
    private var fillPreview: CellRange?
    private var movePreview: CellRange?
    private var resizeGuide: (columns: Bool, at: CGFloat, size: CGFloat)?
    /// The selected floating object, and where a drag would put it.
    private var selectedObject: SheetObject?
    private var objectPreview: CGRect?

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
        // Above the grid is the window's chrome (the frost shows rows scrolled past).
        guard p.y >= 0 else { return }
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
        let inColumnHeader = p.y < layout.headerHeight && p.x >= layout.headerWidth
        let inRowHeader = p.x < layout.headerWidth && p.y >= layout.headerHeight
        // Header edges: drag to resize, double-click to AutoFit (the selected whole columns or rows).
        if inColumnHeader, let edge = geometry.colEdge(x: Float(p.x)) {
            if event.clickCount == 2 {
                document.run("home.autofitColumnWidth", ["cols": (try? document.engine.resizeColumns(col: edge.index)) ?? ""])
                drag = .none
            } else {
                let width = cgRect(geometry.cellRect(cell: CellAddress(row: 0, col: edge.index))).width / CGFloat(layout.scale)
                drag = .resizeColumn(index: edge.index, start: p.x, original: width)
            }
            needsDisplay = true
            return
        }
        if inRowHeader, let edge = geometry.rowEdge(y: Float(p.y)) {
            if event.clickCount == 2 {
                document.run("home.autofitRowHeight", ["rows": (try? document.engine.resizeRows(row: edge.index)) ?? ""])
                drag = .none
            } else {
                let height = cgRect(geometry.cellRect(cell: CellAddress(row: edge.index, col: 0))).height / CGFloat(layout.scale)
                drag = .resizeRow(index: edge.index, start: p.y, original: height)
            }
            needsDisplay = true
            return
        }
        if p.x >= layout.headerWidth, p.y >= layout.headerHeight, case let .object(_, id, rect) = geometry.hit(x: Float(p.x), y: Float(p.y)),
           let object = try? document.engine.objects().first(where: { $0.id == id }) {
            // A floating object: select it; drag to move, or from its corner to resize.
            let r = cgRect(rect)
            let corner = hypot(p.x - r.maxX, p.y - r.maxY) < 10
            drag = corner && selectedObject?.id == id ? .resizeObject(start: p) : .moveObject(start: p)
            selectedObject = object
            objectPreview = nil
            needsDisplay = true
            return
        }
        selectedObject = nil
        if p.x >= layout.headerWidth, p.y >= layout.headerHeight, let current = try? document.engine.selection().ranges.last {
            switch geometry.hit(x: Float(p.x), y: Float(p.y)) {
            case .fillHandle:
                if event.clickCount == 2 {
                    if let target = try? document.engine.fillDownTarget() {
                        document.run("edit.autoFill", ["source": rangeName(range: current), "target": rangeName(range: target)])
                    }
                    drag = .none
                } else {
                    drag = .fill(source: current)
                    fillPreview = current
                }
                needsDisplay = true
                return
            case .selectionBorder:
                drag = .move(grab: CellAddress(row: geometry.rowAt(y: Float(p.y)), col: geometry.colAt(x: Float(p.x))))
                movePreview = current
                needsDisplay = true
                return
            default:
                break
            }
        }
        if event.clickCount == 2, p.x >= layout.headerWidth, p.y >= layout.headerHeight {
            beginEditing()
            return
        }
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
            case let .filterButton(cell):
                // The header's ▾: select it (sorting acts on its region) and open its menu.
                drag = .none
                select(CellRange(first: cell, last: cell))
                showFilterMenu(for: cell, at: cgRect(geometry.filterButton(cell: cell)))
            case let .cell(cell):
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
        case let .fill(source):
            fillPreview = fillTarget(source: source, pointer: cell)
        case let .move(grab):
            if let current = try? document?.engine.selection().ranges.last {
                movePreview = moved(current, by: Int(cell.row) - Int(grab.row), Int(cell.col) - Int(grab.col))
            }
        case let .moveObject(start):
            if let object = selectedObject {
                objectPreview = cgRect(layout.geometry.objectRect(anchor: object.anchor)).offsetBy(dx: p.x - start.x, dy: p.y - start.y)
            }
        case let .resizeObject(start):
            if let object = selectedObject {
                let r = cgRect(layout.geometry.objectRect(anchor: object.anchor))
                objectPreview = CGRect(x: r.minX, y: r.minY, width: max(20, r.width + p.x - start.x), height: max(20, r.height + p.y - start.y))
            }
        case let .resizeColumn(_, start, original):
            let size = max(0, original + (p.x - start) / CGFloat(layout.scale))
            resizeGuide = (true, p.x, size)
            document?.notice = resizeLabel(columns: true, size: Float(size))
        case let .resizeRow(_, start, original):
            let size = max(0, original + (p.y - start) / CGFloat(layout.scale))
            resizeGuide = (false, p.y, size)
            document?.notice = resizeLabel(columns: false, size: Float(size))
        case .none: break
        }
        needsDisplay = true
    }

    public override func mouseUp(with event: NSEvent) {
        defer {
            drag = .none
            fillPreview = nil
            movePreview = nil
            resizeGuide = nil
            objectPreview = nil
            needsDisplay = true
        }
        guard let document else { return }
        switch drag {
        case let .fill(source):
            if let target = fillPreview, target != source {
                // ⌘ swaps copying and extending a series, as in Excel.
                let mode = event.modifierFlags.contains(.command) ? "alternate" : "series"
                document.run("edit.autoFill", ["source": rangeName(range: source), "target": rangeName(range: target), "mode": mode])
                document.run("selection.set", ["range": rangeName(range: target)])
            }
        case .move:
            if let current = try? document.engine.selection().ranges.last, let to = movePreview, to != current {
                document.grouped {
                    document.run("edit.cut", ["range": rangeName(range: current)])
                    document.run("edit.paste", ["at": cellName(cell: to.first)])
                }
            }
        case let .moveObject(start):
            let p = convert(event.locationInWindow, from: nil)
            if let object = selectedObject, let scale = layoutCache?.scale, p != start,
               let to = try? document.engine.movedAnchor(anchor: object.anchor, dx: Float(p.x - start.x) / scale, dy: Float(p.y - start.y) / scale) {
                document.run("object.move", ["kind": object.kind, "id": object.id, "at": cellName(cell: to.cell), "dx": to.dx, "dy": to.dy])
                selectedObject = try? document.engine.objects().first { $0.id == object.id }
            }
        case let .resizeObject(start):
            let p = convert(event.locationInWindow, from: nil)
            if let object = selectedObject, let scale = layoutCache?.scale, p != start {
                let width = max(20, object.anchor.width + Float(p.x - start.x) / scale), height = max(20, object.anchor.height + Float(p.y - start.y) / scale)
                document.run("object.move", ["kind": object.kind, "id": object.id, "width": width, "height": height])
                selectedObject = try? document.engine.objects().first { $0.id == object.id }
            }
        case let .resizeColumn(index, _, _):
            if let guide = resizeGuide {
                document.run("home.columnWidth", ["cols": (try? document.engine.resizeColumns(col: index)) ?? "", "width": guide.size.rounded()])
            }
        case let .resizeRow(index, _, _):
            if let guide = resizeGuide {
                document.run("home.rowHeight", ["rows": (try? document.engine.resizeRows(row: index)) ?? "", "height": guide.size.rounded()])
            }
        default:
            break
        }
    }

    /// `range` moved by rows and columns, kept on the sheet.
    private func moved(_ range: CellRange, by dr: Int, _ dc: Int) -> CellRange {
        let height = Int(range.last.row) - Int(range.first.row), width = Int(range.last.col) - Int(range.first.col)
        let row = max(0, min(Int(Self.lastRow) - height, Int(range.first.row) + dr))
        let col = max(0, min(Int(Self.lastCol) - width, Int(range.first.col) + dc))
        return CellRange(first: CellAddress(row: UInt32(row), col: UInt32(col)), last: CellAddress(row: UInt32(row + height), col: UInt32(col + width)))
    }

    // MARK: - Cursors

    private var tracking: NSTrackingArea?

    public override func updateTrackingAreas() {
        super.updateTrackingAreas()
        if let tracking { removeTrackingArea(tracking) }
        let area = NSTrackingArea(rect: bounds, options: [.mouseMoved, .activeInKeyWindow, .inVisibleRect], owner: self)
        addTrackingArea(area)
        tracking = area
    }

    public override func mouseMoved(with event: NSEvent) {
        guard let layout = layoutCache else { return }
        let p = convert(event.locationInWindow, from: nil)
        let geometry = layout.geometry
        if p.y < layout.headerHeight, p.x >= layout.headerWidth, geometry.colEdge(x: Float(p.x)) != nil {
            NSCursor.columnResize.set()
        } else if p.x < layout.headerWidth, p.y >= layout.headerHeight, geometry.rowEdge(y: Float(p.y)) != nil {
            NSCursor.rowResize.set()
        } else if p.x >= layout.headerWidth, p.y >= layout.headerHeight, editor?.isEditing != true {
            switch geometry.hit(x: Float(p.x), y: Float(p.y)) {
            case .fillHandle: NSCursor.crosshair.set()
            case .selectionBorder: NSCursor.openHand.set()
            default: NSCursor.arrow.set()
            }
        } else {
            NSCursor.arrow.set()
        }
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
        let page = max(1, Int((gridSize.height - (layoutCache?.headerHeight ?? 0)) / max(1, 16 * CGFloat(layoutCache?.scale ?? 0.8) / 0.8)) - 1)
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
        if event.specialKey == .backspace || event.specialKey == .delete, !command, let object = selectedObject {
            document.run("object.delete", ["kind": object.kind, "id": object.id])
            selectedObject = nil
            needsDisplay = true
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
        case .deleteForward? where selectedObject != nil, .delete? where selectedObject != nil:
            if let object = selectedObject {
                document.run("object.delete", ["kind": object.kind, "id": object.id])
                selectedObject = nil
            }
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
        guard let document, var layout = layoutCache, let active = try? document.engine.selection().active else { return }
        let width = bounds.width - (bounds.width > trailingInset * 2 ? trailingInset : 0)
        let height = gridSize.height - (gridSize.height > bottomInset * 2 ? bottomInset : 0)
        if width < bounds.width || height < gridSize.height,
            let uncovered = try? GridRenderer(engine: document.engine, measurer: measurer)
                .layout(size: CGSize(width: width, height: height), scroll: scroll)
        {
            layout = uncovered
        }
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

// MARK: - Context menus

extension GridNSView {
    /// What a right-click is on.
    private enum MenuTarget {
        case cells, rows, columns
    }

    /// The context menu for a right-click: the cells, or a row or column header. A click outside
    /// the selection selects what it is on first, as in Excel.
    public override func menu(for event: NSEvent) -> NSMenu? {
        guard let document, let layout = layoutCacheForMenu() else { return nil }
        guard editor?.commitIfEditing() != false else { return nil }
        let p = convert(event.locationInWindow, from: nil)
        let geometry = layout.geometry
        let target: MenuTarget
        let cell = CellAddress(row: geometry.rowAt(y: Float(max(p.y, layout.headerHeight + 1))), col: geometry.colAt(x: Float(max(p.x, layout.headerWidth + 1))))
        let ranges = (try? document.engine.selection().ranges) ?? []
        if p.y < layout.headerHeight, p.x >= layout.headerWidth {
            target = .columns
            if !ranges.contains(where: { $0.first.row == 0 && $0.last.row >= 1_048_575 && cell.col >= $0.first.col && cell.col <= $0.last.col }) {
                document.run("selection.set", ["range": rangeName(range: CellRange(first: CellAddress(row: 0, col: cell.col), last: CellAddress(row: 1_048_575, col: cell.col)))])
            }
        } else if p.x < layout.headerWidth, p.y >= layout.headerHeight {
            target = .rows
            if !ranges.contains(where: { $0.first.col == 0 && $0.last.col >= 16_383 && cell.row >= $0.first.row && cell.row <= $0.last.row }) {
                document.run("selection.set", ["range": rangeName(range: CellRange(first: CellAddress(row: cell.row, col: 0), last: CellAddress(row: cell.row, col: 16_383)))])
            }
        } else if p.x >= layout.headerWidth, p.y >= layout.headerHeight {
            target = .cells
            if !ranges.contains(where: { cell.row >= $0.first.row && cell.row <= $0.last.row && cell.col >= $0.first.col && cell.col <= $0.last.col }) {
                document.run("selection.set", ["range": cellName(cell: cell)])
            }
        } else {
            return nil
        }
        needsDisplay = true
        let menu = NSMenu()
        let enabled = Dictionary(((try? document.engine.commands()) ?? []).map { ($0.id, $0.enabled) }, uniquingKeysWith: { a, _ in a })
        func item(_ title: String, _ id: String, _ params: [String: Any] = [:]) {
            let entry = NSMenuItem(title: title, action: #selector(runMenuCommand(_:)), keyEquivalent: "")
            entry.target = self
            entry.representedObject = MenuCommand(id: id, params: params)
            entry.isEnabled = enabled[id] ?? true
            menu.addItem(entry)
        }
        func standard(_ title: String, _ action: Selector) {
            menu.addItem(NSMenuItem(title: title, action: action, keyEquivalent: ""))
        }
        standard("Cut", #selector(cut(_:)))
        standard("Copy", #selector(copy(_:)))
        standard("Paste", #selector(paste(_:)))
        item("Paste Special…", "edit.pasteSpecial")
        menu.addItem(.separator())
        switch target {
        case .cells:
            item("Insert…", "home.insertCells")
            item("Delete…", "home.deleteCells")
            item("Clear Contents", "edit.clearContents")
            menu.addItem(.separator())
            item("Filter by Selected Cell's Value", "data.filterBy")
            item("Sort A to Z", "data.sortAscending")
            item("Sort Z to A", "data.sortDescending")
            menu.addItem(.separator())
            item("New Comment", "review.newComment")
            item("New Note", "review.newNote")
            menu.addItem(.separator())
            let format = NSMenuItem(title: "Format Cells…", action: #selector(showInspector(_:)), keyEquivalent: "")
            format.target = self
            menu.addItem(format)
            item("Define Name…", "formulas.defineName")
            item("Link…", "insert.link")
        case .rows:
            item("Insert", "home.insertRows")
            item("Delete", "home.deleteRows")
            item("Clear Contents", "edit.clearContents")
            menu.addItem(.separator())
            item("Row Height…", "home.rowHeight")
            item("AutoFit Row Height", "home.autofitRowHeight")
            item("Hide", "home.hideRows")
            item("Unhide", "home.unhideRows")
            item("Group", "data.group")
        case .columns:
            item("Insert", "home.insertColumns")
            item("Delete", "home.deleteColumns")
            item("Clear Contents", "edit.clearContents")
            menu.addItem(.separator())
            item("Column Width…", "home.columnWidth")
            item("AutoFit Column Width", "home.autofitColumnWidth")
            item("Hide", "home.hideColumns")
            item("Unhide", "home.unhideColumns")
            item("Sort A to Z", "data.sortAscending")
            item("Group", "data.group")
        }
        return menu
    }

    private func layoutCacheForMenu() -> GridRenderer.Layout? {
        guard let document else { return nil }
        return try? GridRenderer(engine: document.engine, measurer: CoreTextMeasurer()).layout(size: gridSize, scroll: scroll)
    }

    @objc func runMenuCommand(_ sender: NSMenuItem) {
        guard let command = sender.representedObject as? MenuCommand else { return }
        document?.run(command.id, command.params)
        needsDisplay = true
    }

    @objc private func showInspector(_ sender: Any?) {
        document?.showInspector = true
    }
}

/// A context-menu item's command.
final class MenuCommand: NSObject {
    let id: String
    let params: [String: Any]

    init(id: String, params: [String: Any]) {
        self.id = id
        self.params = params
    }
}

extension GridNSView: NSMenuItemValidation {
    public func validateMenuItem(_ item: NSMenuItem) -> Bool {
        guard let document else { return false }
        if let command = item.representedObject as? MenuCommand {
            return (try? document.engine.commands().first { $0.id == command.id }?.enabled) ?? true
        }
        switch item.action {
        case #selector(paste(_:)):
            return NSPasteboard.general.string(forType: .string) != nil || (try? document.engine.commands().first { $0.id == "edit.paste" }?.enabled) == true
        default:
            return true
        }
    }
}

/// The grid in SwiftUI.
public struct GridView: NSViewRepresentable {
    @ObservedObject var document: SpreadsheetDocument
    let editor: EditController
    var topInset: CGFloat = 0
    var bottomInset: CGFloat = 0
    var trailingInset: CGFloat = 0
    var scrollState: GridScrollState?
    var onFrame: ((Double) -> Void)?

    public init(document: SpreadsheetDocument, editor: EditController, topInset: CGFloat = 0, bottomInset: CGFloat = 0, trailingInset: CGFloat = 0,
                scrollState: GridScrollState? = nil, onFrame: ((Double) -> Void)? = nil) {
        self.document = document
        self.editor = editor
        self.topInset = topInset
        self.bottomInset = bottomInset
        self.trailingInset = trailingInset
        self.scrollState = scrollState
        self.onFrame = onFrame
    }

    public func makeNSView(context: Context) -> GridNSView {
        let view = GridNSView(frame: .zero)
        view.document = document
        view.editor = editor
        view.onFrame = onFrame
        view.bottomInset = bottomInset
        view.trailingInset = trailingInset
        view.scrollState = scrollState
        view.topInset = topInset
        return view
    }

    public func updateNSView(_ view: GridNSView, context: Context) {
        _ = document.generation
        view.document = document
        view.onFrame = onFrame
        view.bottomInset = bottomInset
        view.trailingInset = trailingInset
        view.scrollState = scrollState
        view.topInset = topInset
        view.needsDisplay = true
    }
}

