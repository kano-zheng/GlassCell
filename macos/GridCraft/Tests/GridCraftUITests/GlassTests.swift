import AppKit
import GridCraftEngine
@testable import GridCraftUI
import Testing

@MainActor
@Suite struct GlassTests {
    @Test func theStatusBarSumsTheSelection() throws {
        let document = SpreadsheetDocument(sample: "sales")
        document.run("selection.set", ["range": "C5"])
        #expect(try document.engine.statusItems().isEmpty)
        document.run("selection.set", ["range": "C5:D6"])
        let items = try document.engine.statusItems()
        #expect(items.map(\.label) == ["Sum", "Count", "Average"])
        #expect(items.first?.value == "$176,670")
    }

    @Test func theFilterMenuSendsWhatIsChecked() throws {
        let document = SpreadsheetDocument(sample: "sales")
        let menu = try #require(try document.engine.filterMenu(cell: CellAddress(row: 3, col: 1)))
        let all = menu.values.map(\.text)
        // Everything checked clears the column's filter; nothing checked filters nothing.
        #expect(FilterMenuView.filterParams(menu, showing: all)?["clear"] as? Bool == true)
        #expect(FilterMenuView.filterParams(menu, showing: []) == nil)
        let params = try #require(FilterMenuView.filterParams(menu, showing: ["North", "West"]))
        #expect(params["values"] as? [String] == ["North", "West"] && params["blanks"] as? Bool == false)
        // Applied, it hides the other regions' rows and the menu shows them unchecked.
        document.run("data.filterBy", params)
        #expect(document.problem == nil)
        let after = try #require(try document.engine.filterMenu(cell: CellAddress(row: 3, col: 1)))
        #expect(after.values.first { $0.text == "East" }?.shown == false)
        #expect(after.values.first { $0.text == "West" }?.shown == true)
    }

    @Test func theGridStartsBelowTheFrostedTop() throws {
        let document = SpreadsheetDocument(sample: "sales")
        let grid = GridNSView(frame: CGRect(x: 0, y: 0, width: 800, height: 600))
        grid.document = document
        grid.topInset = 90
        #expect(grid.bounds.minY == -90)
        #expect(grid.gridBounds == CGRect(x: 0, y: 0, width: 800, height: 510))
        grid.setFrameSize(NSSize(width: 900, height: 700))
        #expect(grid.bounds.minY == -90 && grid.gridBounds.height == 610)
        // Rows scrolled past draw (blurred) under the chrome; the cells start below it.
        grid.scroll(to: CGPoint(x: 0, y: 200))
        let rep = try #require(grid.bitmapImageRepForCachingDisplay(in: grid.bounds))
        grid.cacheDisplay(in: grid.bounds, to: rep)
        let a10 = try #require(grid.cellFrame(CellAddress(row: 9, col: 0)))
        #expect(grid.gridBounds.minY <= a10.minY)
    }

    @Test func aRevealedCellStaysLeftOfTheInspector() throws {
        let document = SpreadsheetDocument(sample: "sales")
        let grid = GridNSView(frame: CGRect(x: 0, y: 0, width: 900, height: 600))
        grid.document = document
        grid.trailingInset = 310
        let rep = try #require(grid.bitmapImageRepForCachingDisplay(in: grid.bounds))
        grid.cacheDisplay(in: grid.bounds, to: rep)
        document.run("selection.set", ["range": "N25"])
        grid.reveal()
        let cell = try #require(grid.cellFrame(CellAddress(row: 24, col: 13)))
        #expect(grid.scroll.x > 0)
        #expect(cell.maxX <= grid.bounds.width - 310)
    }

    @Test func theGridTellsTheScrollersWhereItIs() async throws {
        let document = SpreadsheetDocument(sample: "sales")
        let grid = GridNSView(frame: CGRect(x: 0, y: 0, width: 600, height: 400))
        let state = GridScrollState()
        grid.document = document
        grid.scrollState = state
        let rep = try #require(grid.bitmapImageRepForCachingDisplay(in: grid.bounds))
        // The first pass opens the sheet where its file left it; then scroll.
        grid.cacheDisplay(in: grid.bounds, to: rep)
        grid.scroll(to: CGPoint(x: 120, y: 300))
        grid.cacheDisplay(in: grid.bounds, to: rep)
        // It reports after the display pass.
        for _ in 0..<10 where state.y != 300 { await Task.yield() }
        #expect(state.x == 120 && state.y == 300)
        #expect(state.visibleX > 0 && state.visibleY > 0)
        // The scroll bars reach column Z and row 100 at least, or where the view is.
        #expect(state.extentX >= 120 && state.extentY >= 300)
        #expect(state.cellsTop > 0)
    }
}
