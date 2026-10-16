import GridCraftEngine
import Testing

@Suite struct EngineTests {
    private func sales() throws -> Engine {
        let engine = Engine()
        _ = try engine.execute(id: "file.new", params: #"{"sample": "sales"}"#)
        return engine
    }

    @Test func commandsRunAndErrorsAreThrown() throws {
        let engine = try sales()
        _ = try engine.execute(id: "cell.set", params: #"{"cell": "A1", "input": "=1+2"}"#)
        let cell = try engine.execute(id: "cell.get", params: #"{"cell": "A1"}"#)
        #expect(cell.contains(#""text":"3""#))
        #expect(throws: FfiError.self) { try engine.execute(id: "no.such", params: "") }
        #expect(try engine.commands().contains { $0.id == "home.bold" && $0.enabled })
    }

    @Test func theViewportLaysOutTheSampleWithCoreText() throws {
        let engine = try sales()
        let query = ViewportQuery(sheet: nil, rows: LineSpan(first: 0, last: 40), cols: LineSpan(first: 0, last: 12), scale: 1, formulaLocale: "en")
        let viewport = try engine.viewport(request: query, measurer: CoreTextMeasurer())
        let title = viewport.cells.first { $0.text?.text == "Quarterly Sales Dashboard" }
        #expect(title?.cell == CellAddress(row: 1, col: 1))
        #expect(!viewport.sparklines.isEmpty)
        #expect(!viewport.filterButtons.isEmpty)
    }

    @Test func geometryHitTestsTheSelection() throws {
        let engine = try sales()
        _ = try engine.execute(id: "selection.set", params: #"{"range": "B4:C5"}"#)
        let geometry = try engine.geometry(origin: GridPoint(x: 0, y: 0), size: GridSize(width: 900, height: 600), scroll: GridPoint(x: 0, y: 0), scale: 1)
        let rect = geometry.rangeRect(range: CellRange(first: CellAddress(row: 3, col: 1), last: CellAddress(row: 4, col: 2)))
        #expect(geometry.hit(x: rect.x + rect.width, y: rect.y + rect.height) == .fillHandle)
        #expect(try engine.selectionFormat().cells == 4)
        #expect(try engine.nameBoxText(dragging: false) == "B4")
    }

    @Test func coreTextMeasuresWiderTextWider() {
        let measurer = CoreTextMeasurer()
        let face = FontFace(name: "Helvetica", bold: false, italic: false)
        let one = measurer.textWidth(text: "0", face: face, size: 12)
        #expect(one > 0)
        #expect(measurer.textWidth(text: "0000", face: face, size: 12) > one * 3.5)
        #expect(measurer.textWidth(text: "0", face: FontFace(name: "No Such Font", bold: true, italic: false), size: 12) > 0)
        #expect(measurer.textWidth(text: "0", face: face, size: .nan) == 0)
    }
}
