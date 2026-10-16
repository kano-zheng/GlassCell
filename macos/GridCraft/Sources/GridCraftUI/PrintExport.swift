import AppKit
import GridCraftEngine
import PDFKit
import UniformTypeIdentifiers

/// Commands the Mac does with system panels: printing goes through `NSPrintOperation` over the
/// engine's PDF (with the system's preview); exports ask where with `NSSavePanel`.
@MainActor
enum MacActions {
    /// Catalog commands that run here instead of straight through the engine.
    static let handled: Set<String> = ["file.print", "file.printPreview", "file.exportPdf", "file.exportCsv", "file.exportHtml"]

    static func run(_ id: String, document: SpreadsheetDocument) {
        switch id {
        case "file.print", "file.printPreview": printWorkbook(document)
        case "file.exportPdf": export(document, command: "file.exportPdf", type: .pdf, ext: "pdf")
        case "file.exportCsv": export(document, command: "file.exportCsv", type: .commaSeparatedText, ext: "csv")
        case "file.exportHtml": export(document, command: "file.exportHtml", type: .html, ext: "html")
        default: break
        }
    }

    /// The active sheet as the engine prints it (its Page Setup), in the system's print panel.
    static func printWorkbook(_ document: SpreadsheetDocument) {
        do {
            let result = JSON.decode(try document.perform("file.print", json: ""))
            guard let base64 = result["base64"] as? String, let data = Data(base64Encoded: base64), let pdf = PDFDocument(data: data) else {
                document.problem = "There is nothing to print."
                return
            }
            let info = NSPrintInfo.shared.copy() as? NSPrintInfo ?? NSPrintInfo()
            info.topMargin = 0
            info.bottomMargin = 0
            info.leftMargin = 0
            info.rightMargin = 0
            guard let operation = pdf.printOperation(for: info, scalingMode: .pageScaleToFit, autoRotate: true) else {
                document.problem = "The page couldn't be prepared for printing."
                return
            }
            operation.showsPrintPanel = true
            operation.showsProgressPanel = true
            if let window = NSApp.keyWindow {
                operation.runModal(for: window, delegate: nil, didRun: nil, contextInfo: nil)
            } else {
                operation.run()
            }
        } catch {
            document.problem = error.localizedDescription
        }
    }

    /// Asks where, then has the engine write the file there.
    static func export(_ document: SpreadsheetDocument, command: String, type: UTType, ext: String) {
        let panel = NSSavePanel()
        panel.allowedContentTypes = [type]
        let title = (try? document.engine.sheet().name) ?? "Sheet"
        panel.nameFieldStringValue = "\(title).\(ext)"
        let finish: (NSApplication.ModalResponse) -> Void = { response in
            guard response == .OK, let url = panel.url else { return }
            do {
                try document.perform(command, json: JSON.encode(["path": url.path]))
            } catch {
                document.problem = error.localizedDescription
            }
        }
        if let window = NSApp.keyWindow {
            panel.beginSheetModal(for: window, completionHandler: finish)
        } else {
            finish(panel.runModal())
        }
    }
}
