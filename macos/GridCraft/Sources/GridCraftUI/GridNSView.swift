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
            layoutCache = try GridRenderer(engine: document.engine, measurer: measurer).draw(in: ctx, size: bounds.size, scroll: scroll)
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

    // MARK: - Scrolling and zoom

    public func scroll(to point: CGPoint) {
        scroll = CGPoint(x: max(0, point.x), y: max(0, point.y))
        try? document?.engine.storeScroll(scroll: GridCraftUI.point(scroll.x, scroll.y))
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

    public override func mouseDown(with event: NSEvent) {
        window?.makeFirstResponder(self)
        guard let document, let layout = layoutCache else { return }
        let p = convert(event.locationInWindow, from: nil)
        guard p.x >= layout.headerWidth, p.y >= layout.headerHeight else { return }
        switch layout.geometry.hit(x: Float(p.x), y: Float(p.y)) {
        case let .cell(cell), let .filterButton(cell):
            let extend = event.modifierFlags.contains(.shift)
            if extend, let active = try? document.engine.selection().active {
                document.run("selection.set", ["range": rangeName(range: CellRange(first: active, last: cell)), "active": cellName(cell: active)])
            } else {
                document.run("selection.set", ["range": cellName(cell: cell)])
            }
        default:
            break
        }
        needsDisplay = true
    }

    public override func keyDown(with event: NSEvent) {
        let (dr, dc): (Int, Int)
        switch event.specialKey {
        case .upArrow?: (dr, dc) = (-1, 0)
        case .downArrow?, .carriageReturn?, .enter?: (dr, dc) = (1, 0)
        case .leftArrow?: (dr, dc) = (0, -1)
        case .rightArrow?, .tab?: (dr, dc) = (0, 1)
        default:
            super.keyDown(with: event)
            return
        }
        guard let document else { return }
        let flags = event.modifierFlags
        document.run("selection.move", ["dr": dr, "dc": dc, "extend": flags.contains(.shift), "jump": flags.contains(.command)])
        reveal()
    }

    /// Scrolls the active cell into view.
    public func reveal() {
        guard let document, let layout = layoutCache, let active = try? document.engine.selection().active else { return }
        let target = layout.geometry.reveal(cell: active)
        scroll(to: CGPoint(x: CGFloat(target.x), y: CGFloat(target.y)))
    }
}

/// The grid in SwiftUI.
public struct GridView: NSViewRepresentable {
    @ObservedObject var document: SpreadsheetDocument
    var onFrame: ((Double) -> Void)?

    public init(document: SpreadsheetDocument, onFrame: ((Double) -> Void)? = nil) {
        self.document = document
        self.onFrame = onFrame
    }

    public func makeNSView(context: Context) -> GridNSView {
        let view = GridNSView(frame: .zero)
        view.document = document
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
