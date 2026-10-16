import AppKit
import GridCraftEngine
@testable import GridCraftUI
import SwiftUI
import Testing

@MainActor
@Suite struct FormTests {
    @Test func aFormRunsItsCommand() throws {
        let document = SpreadsheetDocument()
        let form = try #require(try document.engine.openForm(name: "rowHeight", params: "{}"))
        try form.setNumber(key: "height", value: "40")
        try document.perform(form.command(), json: try form.params())
        let again = try #require(try document.engine.openForm(name: "rowHeight", params: "{}"))
        #expect(again.text(key: "height") == "40.0" || again.text(key: "height") == "40")
    }

    /// `GRIDCRAFT_SNAPSHOTS=dir swift test --filter FormTests` writes the forms as images.
    @Test func writeFormImages() throws {
        guard let dir = ProcessInfo.processInfo.environment["GRIDCRAFT_SNAPSHOTS"] else { return }
        let document = SpreadsheetDocument(sample: "sales")
        for name in ["rowHeight", "pasteSpecial", "dataValidation"] {
            let form = try #require(try document.engine.openForm(name: name, params: "{}"))
            if name == "dataValidation" { try form.setText(key: "type", value: "whole") }
            let host = NSHostingView(rootView: FormSheet(form: form, document: document))
            host.frame = NSRect(x: 0, y: 0, width: 460, height: name == "rowHeight" ? 220 : 560)
            let window = NSWindow(contentRect: host.frame, styleMask: [.titled], backing: .buffered, defer: false)
            window.contentView = host
            host.layoutSubtreeIfNeeded()
            let bitmap = try #require(host.bitmapImageRepForCachingDisplay(in: host.bounds))
            host.cacheDisplay(in: host.bounds, to: bitmap)
            try #require(bitmap.representation(using: .png, properties: [:])).write(to: URL(fileURLWithPath: dir).appendingPathComponent("form-\(name).png"))
        }
    }

    @Test func writeDialogImages() throws {
        guard let dir = ProcessInfo.processInfo.environment["GRIDCRAFT_SNAPSHOTS"] else { return }
        let document = SpreadsheetDocument(sample: "sales")
        document.run("formulas.defineName", ["name": "Totals", "refersTo": "=Sales!$G$5:$G$10"])
        document.run("selection.set", ["range": "B4:G10"])
        let editor = EditController(document: document)
        for name in ["sort", "nameManager", "insertFunction", "statistics", "find"] {
            let host = NSHostingView(rootView: CustomDialog(request: DialogRequest(name: name, params: [:]), document: document, editor: editor))
            host.frame = NSRect(x: 0, y: 0, width: 680, height: 460)
            let window = NSWindow(contentRect: host.frame, styleMask: [.titled], backing: .buffered, defer: false)
            window.contentView = host
            host.layoutSubtreeIfNeeded()
            let bitmap = try #require(host.bitmapImageRepForCachingDisplay(in: host.bounds))
            host.cacheDisplay(in: host.bounds, to: bitmap)
            try #require(bitmap.representation(using: .png, properties: [:])).write(to: URL(fileURLWithPath: dir).appendingPathComponent("dialog-\(name).png"))
        }
    }
}
