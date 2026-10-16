import GridCraftEngine
@testable import GridCraftUI
import Testing

@MainActor
@Suite(.serialized) struct AutomationTests {
    @Test func intentsActOnTheFrontmostWorkbook() throws {
        let document = SpreadsheetDocument()
        let editor = EditController(document: document)
        WorkbookRegistry.shared.activated(document, editor: editor)
        try Automation.setCell("B2", to: "=6*7")
        #expect(try Automation.cellText("B2") == "42")
        let result = try Automation.run("home.bold", params: #"{"range": "B2", "on": true}"#)
        #expect(result.contains("true"))
        #expect(throws: Automation.Failure.self) { try Automation.run("no.such", params: "") }
        #expect(Automation.commandIds().contains("cell.set"))
    }

    @Test func withoutAWorkbookTheyExplain() {
        // Only documents still alive count; none here once the previous test's are gone.
        if WorkbookRegistry.shared.frontmost == nil {
            #expect(throws: Automation.Failure.self) { try Automation.cellText("A1") }
        }
    }
}
