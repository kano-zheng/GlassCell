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
}
