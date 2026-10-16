import AppKit
import GridCraftEngine
@testable import GridCraftUI
import Testing

@MainActor
@Suite struct RendererTests {
    private func context(_ size: CGSize) throws -> CGContext {
        let ctx = try #require(CGContext(data: nil, width: Int(size.width), height: Int(size.height), bitsPerComponent: 8, bytesPerRow: 0,
                                         space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
        ctx.translateBy(x: 0, y: size.height)
        ctx.scaleBy(x: 1, y: -1)
        return ctx
    }

    @Test func drawsTheSampleAndHitTestsItsCells() throws {
        let document = SpreadsheetDocument(sample: "sales")
        #expect(document.problem == nil)
        let renderer = GridRenderer(engine: document.engine, measurer: CoreTextMeasurer())
        let size = CGSize(width: 800, height: 500)
        let layout = try renderer.draw(in: try context(size), size: size, scroll: .zero)
        #expect(layout.headerWidth > 0 && layout.headerHeight > 0)
        let b2 = cgRect(layout.geometry.cellRect(cell: CellAddress(row: 1, col: 1)))
        #expect(layout.geometry.hit(x: Float(b2.midX), y: Float(b2.midY)) == .cell(cell: CellAddress(row: 1, col: 1)))
    }

    @Test func aWorkbookRoundTripsThroughTheDocument() throws {
        let document = SpreadsheetDocument()
        document.run("cell.set", ["cell": "A1", "input": "hello"])
        let data = try document.snapshot(contentType: .xlsx)
        #expect(data.count > 100)
        #expect(data.prefix(2) == Data("PK".utf8), "an XLSX is a zip")
    }

    @Test func drawingWithoutAWorkbookFailsCleanly() throws {
        let engine = Engine()
        let renderer = GridRenderer(engine: engine, measurer: CoreTextMeasurer())
        #expect(throws: (any Error).self) { try renderer.draw(in: try context(CGSize(width: 100, height: 100)), size: CGSize(width: 100, height: 100), scroll: .zero) }
    }

    /// SwiftUI state must not change while AppKit draws (it threw "needs update constraints"
    /// mid-display and the app went down at launch): the frame report comes after the pass.
    @Test func drawingReportsTheFrameTimeOnlyAfterTheDisplayPass() async throws {
        let view = GridNSView(frame: NSRect(x: 0, y: 0, width: 600, height: 400))
        view.document = SpreadsheetDocument(sample: "sales")
        var reported: [Double] = []
        view.onFrame = { reported.append($0) }
        let bitmap = try #require(view.bitmapImageRepForCachingDisplay(in: view.bounds))
        view.cacheDisplay(in: view.bounds, to: bitmap)
        #expect(reported.isEmpty, "not during draw")
        #expect(view.lastFrameMilliseconds > 0)
        await Task.yield()
        try await Task.sleep(for: .milliseconds(50))
        #expect(reported.count == 1)
    }
}
