import CoreSpotlight
import Foundation
import GridCraftEngine
@testable import GridCraftUI
import Testing

@MainActor
@Suite struct SharingTests {
    @Test func theSharedFileIsTheWorkbookAsItIsNow() async throws {
        let document = SpreadsheetDocument(sample: "sales")
        document.run("cell.set", ["cell": "A1", "input": "unsaved edit"])
        let shared = SharedWorkbook(document: document, fileURL: URL(fileURLWithPath: "/tmp/Q3 Sales.csv"))
        #expect(shared.name == "Q3 Sales.xlsx")
        #expect(SharedWorkbook(document: document, fileURL: nil).name == "Workbook.xlsx")
        let url = try await shared.write()
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        #expect(url.lastPathComponent == "Q3 Sales.xlsx")
        let reopened = Engine()
        let data = try Data(contentsOf: url)
        _ = try reopened.execute(id: "file.open", params: JSON.encode(["name": "x.xlsx", "base64": data.base64EncodedString()]))
        let a1 = JSON.decode(try reopened.execute(id: "cell.get", params: #"{"cell": "A1"}"#))
        #expect(a1["text"] as? String == "unsaved edit")
    }

    @Test func spotlightFindsAWorkbookByItsContent() throws {
        let document = SpreadsheetDocument(sample: "sales")
        let file = URL(fileURLWithPath: "/tmp/Dashboard.xlsx")
        let item = try #require(WorkbookSpotlight.item(for: document, fileURL: file))
        #expect(item.uniqueIdentifier == file.path)
        #expect(item.attributeSet.title == "Dashboard")
        #expect(item.attributeSet.keywords?.contains("Sales") == true)
        #expect(item.attributeSet.textContent?.contains("North") == true)
        #expect(item.attributeSet.contentURL == file)

        let activity = NSUserActivity(activityType: CSSearchableItemActionType)
        activity.userInfo = [CSSearchableItemActivityIdentifier: file.path]
        #expect(WorkbookSpotlight.fileURL(from: activity) == file)
        #expect(WorkbookSpotlight.fileURL(from: NSUserActivity(activityType: "other")) == nil)
    }

    /// Indexes for real (the test runner's own index) and searches by a cell's text.
    @Test func anIndexedWorkbookIsFoundByItsCellText() async throws {
        let document = SpreadsheetDocument()
        let marker = "gridcraftprobe\(UInt32.random(in: 100_000...999_999))"
        document.run("cell.set", ["cell": "C3", "input": marker])
        let file = URL(fileURLWithPath: "/tmp/\(marker).xlsx")
        let item = try #require(WorkbookSpotlight.item(for: document, fileURL: file))
        try await CSSearchableIndex.default().indexSearchableItems([item])
        var found: [String] = []
        for _ in 0..<50 where found.isEmpty {
            try? await Task.sleep(for: .milliseconds(200))
            for try await result in CSSearchQuery(queryString: "textContent == \"*\(marker)*\"", queryContext: nil).results {
                found.append(result.item.uniqueIdentifier)
            }
        }
        try await CSSearchableIndex.default().deleteSearchableItems(withIdentifiers: [file.path])
        #expect(found == [file.path])
    }
}
