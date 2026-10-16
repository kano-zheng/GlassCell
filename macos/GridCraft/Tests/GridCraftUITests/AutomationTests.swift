import AppKit
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

@MainActor
@Suite struct PreviewTests {
    @Test func aWorkbookFileRendersAsAPreview() throws {
        let source = SpreadsheetDocument(sample: "sales")
        let data = try source.snapshot(contentType: .xlsx)
        let image = try WorkbookPreview.render(data: data, name: "Sales.xlsx", size: CGSize(width: 800, height: 500), scale: 1)
        #expect(image.width == 800 && image.height == 500)
        if let dir = ProcessInfo.processInfo.environment["GRIDCRAFT_SNAPSHOTS"] {
            let png = NSBitmapImageRep(cgImage: image).representation(using: .png, properties: [:])
            try png?.write(to: URL(fileURLWithPath: dir).appendingPathComponent("quicklook.png"))
        }
        // Anything else opens leniently or fails, but never crashes.
        _ = try? WorkbookPreview.render(data: Data("not a workbook".utf8), name: "x.xlsx")
        _ = try? WorkbookPreview.render(data: Data([0x50, 0x4B, 0x03, 0x04, 0xFF]), name: "broken.xlsx")
    }
}
