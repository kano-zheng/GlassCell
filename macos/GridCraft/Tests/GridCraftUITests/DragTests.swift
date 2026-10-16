import AppKit
import GridCraftEngine
@testable import GridCraftUI
import Testing

@MainActor
@Suite struct DragTests {
    @MainActor private struct Rig {
        let document: SpreadsheetDocument
        let window: NSWindow
        let grid: GridNSView
        let undo = UndoManager()
        var layout: GridRenderer.Layout

        init() throws {
            document = SpreadsheetDocument()
            window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 800, height: 500), styleMask: [.titled], backing: .buffered, defer: false)
            grid = GridNSView(frame: NSRect(x: 0, y: 0, width: 800, height: 500))
            window.contentView = grid
            grid.document = document
            grid.editor = EditController(document: document)
            undo.groupsByEvent = false
            document.undoManager = undo
            layout = try GridRenderer(engine: document.engine, measurer: CoreTextMeasurer()).layout(size: grid.bounds.size, scroll: .zero)
            redraw()
        }

        func redraw() {
            if let bitmap = grid.bitmapImageRepForCachingDisplay(in: grid.bounds) {
                grid.cacheDisplay(in: grid.bounds, to: bitmap)
            }
        }

        func rect(_ row: UInt32, _ col: UInt32) -> CGRect {
            cgRect(layout.geometry.cellRect(cell: CellAddress(row: row, col: col)))
        }

        /// A mouse event at a point in the (flipped) grid.
        func event(_ type: NSEvent.EventType, _ p: CGPoint, clicks: Int = 1) -> NSEvent? {
            let inWindow = grid.convert(p, to: nil)
            return NSEvent.mouseEvent(with: type, location: inWindow, modifierFlags: [], timestamp: 0, windowNumber: window.windowNumber,
                                      context: nil, eventNumber: 0, clickCount: clicks, pressure: 1)
        }

        func drag(from a: CGPoint, to b: CGPoint) {
            undo.beginUndoGrouping()
            if let down = event(.leftMouseDown, a) { grid.mouseDown(with: down) }
            if let moved = event(.leftMouseDragged, b) { grid.mouseDragged(with: moved) }
            if let up = event(.leftMouseUp, b) { grid.mouseUp(with: up) }
            undo.endUndoGrouping()
            redraw()
        }

        func text(_ cell: String) -> String {
            JSON.decode((try? document.engine.execute(id: "cell.get", params: JSON.encode(["cell": cell]))) ?? "{}")["text"] as? String ?? ""
        }
    }

    @Test func dragTheFillHandleToExtendASeries() throws {
        let rig = try Rig()
        rig.document.run("cell.set", ["cell": "A1", "input": "1"])
        rig.document.run("cell.set", ["cell": "A2", "input": "2"])
        rig.document.run("selection.set", ["range": "A1:A2"])
        rig.redraw()
        let corner = CGPoint(x: rig.rect(1, 0).maxX, y: rig.rect(1, 0).maxY)
        rig.drag(from: corner, to: CGPoint(x: rig.rect(4, 0).midX, y: rig.rect(4, 0).midY))
        #expect(rig.text("A5") == "5")
    }

    @Test func dragAColumnEdgeToResizeIt() throws {
        let rig = try Rig()
        let before = rig.rect(0, 1).width
        let edge = CGPoint(x: rig.rect(0, 1).maxX, y: rig.layout.headerHeight / 2)
        rig.drag(from: edge, to: CGPoint(x: edge.x + 40, y: edge.y))
        let after = try GridRenderer(engine: rig.document.engine, measurer: CoreTextMeasurer()).layout(size: rig.grid.bounds.size, scroll: .zero)
        #expect(abs(cgRect(after.geometry.cellRect(cell: CellAddress(row: 0, col: 1))).width - (before + 40)) < 1.5)
    }

    @Test func dragTheSelectionBorderToMoveTheCellsInOneUndoStep() throws {
        let rig = try Rig()
        rig.document.run("cell.set", ["cell": "A1", "input": "moved"])
        rig.document.run("selection.set", ["range": "A1"])
        rig.redraw()
        let a1 = rig.rect(0, 0)
        rig.drag(from: CGPoint(x: a1.midX, y: a1.minY + 1), to: CGPoint(x: rig.rect(2, 2).midX, y: rig.rect(2, 2).minY + 1))
        #expect(rig.text("C3") == "moved")
        #expect(rig.text("A1") == "")
        rig.undo.undo()
        #expect(rig.text("A1") == "moved")
        #expect(rig.text("C3") == "")
    }
}
