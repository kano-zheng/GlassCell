import Foundation
import GridCraftEngine
@testable import GridCraftUI
import PDFKit
import Testing

@MainActor
@Suite struct PrintTests {
    @Test func theEnginesPrintPDFOpensInPDFKit() throws {
        let document = SpreadsheetDocument(sample: "sales")
        let result = JSON.decode(try document.perform("file.print", json: ""))
        let base64 = try #require(result["base64"] as? String)
        let data = try #require(Data(base64Encoded: base64))
        let pdf = try #require(PDFDocument(data: data))
        #expect(pdf.pageCount >= 1)
    }

    @Test func exportsWriteFiles() throws {
        let document = SpreadsheetDocument(sample: "sales")
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        for (command, name) in [("file.exportCsv", "out.csv"), ("file.exportPdf", "out.pdf")] {
            let url = dir.appendingPathComponent(name)
            try document.perform(command, json: JSON.encode(["path": url.path]))
            #expect(((try? Data(contentsOf: url))?.count ?? 0) > 10, "\(command)")
        }
    }
}
