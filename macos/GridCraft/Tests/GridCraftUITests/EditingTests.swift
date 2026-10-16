import AppKit
import GridCraftEngine
@testable import GridCraftUI
import Testing

@MainActor
@Suite struct EditingTests {
    private func text(_ document: SpreadsheetDocument, _ cell: String) -> String {
        JSON.decode((try? document.engine.execute(id: "cell.get", params: JSON.encode(["cell": cell]))) ?? "{}")["text"] as? String ?? ""
    }

    private func formula(_ document: SpreadsheetDocument, _ cell: String) -> String? {
        JSON.decode((try? document.engine.execute(id: "cell.get", params: JSON.encode(["cell": cell]))) ?? "{}")["formula"] as? String
    }

    @Test func typingConfirmsWithEnterAndMovesDown() {
        let document = SpreadsheetDocument()
        let editor = EditController(document: document)
        editor.begin(typing: "")
        editor.textChanged("42", caret: 2)
        #expect(editor.commit(.down))
        #expect(text(document, "A1") == "42")
        #expect((try? document.engine.selection().active) == CellAddress(row: 1, col: 0))
        #expect(!editor.isEditing)
    }

    @Test func f2EditsTheCurrentContent() {
        let document = SpreadsheetDocument()
        document.run("cell.set", ["cell": "A1", "input": "=1+2"])
        let editor = EditController(document: document)
        editor.begin()
        #expect(editor.text == "=1+2")
        #expect(!editor.enterMode)
        editor.cancel()
        #expect(formula(document, "A1") == "=1+2")
    }

    @Test func pointModeInsertsReferencesByKeyboardAndClick() {
        let document = SpreadsheetDocument()
        document.run("selection.set", ["range": "C3"])
        let editor = EditController(document: document)
        editor.begin(typing: "")
        editor.textChanged("=SUM(", caret: 5)
        #expect(editor.canPoint)
        editor.pointMove(dr: -1, dc: 0, extend: false)
        #expect(editor.text == "=SUM(C2")
        editor.pointMove(dr: -1, dc: 0, extend: true)
        #expect(editor.text == "=SUM(C1:C2")
        editor.pointClick(CellAddress(row: 0, col: 0), extendingTo: CellAddress(row: 1, col: 1))
        #expect(editor.text == "=SUM(A1:B2")
        #expect(editor.references.count == 1)
        editor.cycleAnchor()
        #expect(editor.text == "=SUM($A$1:$B$2")
        #expect(editor.commit())
        #expect(formula(document, "C3") == "=SUM($A$1:$B$2)")
    }

    @Test func aStopRuleKeepsTheEditOpen() {
        let document = SpreadsheetDocument()
        document.run("data.validation", ["range": "A1", "type": "whole", "operator": "greater", "formula1": "0"])
        let editor = EditController(document: document)
        editor.begin(typing: "")
        editor.textChanged("-3", caret: 2)
        #expect(!editor.commit(.down))
        #expect(editor.isEditing)
        #expect(document.problem != nil)
        #expect(text(document, "A1") == "")
    }

    @Test func hintsAndAutocompleteFollowTheCaret() {
        let document = SpreadsheetDocument()
        let editor = EditController(document: document)
        editor.begin(typing: "")
        editor.textChanged("=SU", caret: 3)
        #expect(editor.autocomplete.contains("SUM"))
        #expect(editor.acceptAutocomplete(editor.autocomplete.firstIndex(of: "SUM")))
        #expect(editor.text == "=SUM(")
        #expect(editor.hint?.name == "SUM")
    }

    @Test func aKeyPressInTheGridStartsAnEditThatEnterConfirms() throws {
        let document = SpreadsheetDocument()
        let editor = EditController(document: document)
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 600, height: 400), styleMask: [.titled], backing: .buffered, defer: true)
        let grid = GridNSView(frame: NSRect(x: 0, y: 0, width: 600, height: 400))
        window.contentView = grid
        grid.document = document
        grid.editor = editor
        let bitmap = try #require(grid.bitmapImageRepForCachingDisplay(in: grid.bounds))
        grid.cacheDisplay(in: grid.bounds, to: bitmap)
        let key = try #require(NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: [], timestamp: 0, windowNumber: window.windowNumber,
                                                context: nil, characters: "7", charactersIgnoringModifiers: "7", isARepeat: false, keyCode: 26))
        grid.keyDown(with: key)
        #expect(editor.isEditing)
        #expect(editor.text == "7")
        let editorView = try #require(window.firstResponder as? CellTextView)
        editorView.doCommand(by: #selector(NSResponder.insertNewline(_:)))
        #expect(!editor.isEditing)
        #expect(text(document, "A1") == "7")
        #expect((try? document.engine.selection().active) == CellAddress(row: 1, col: 0))
    }
}

/// `GRIDCRAFT_SNAPSHOTS=dir swift test --filter EditingSnapshot` writes what an edit looks like.
@MainActor
@Suite struct EditingSnapshot {
    @Test func writeAnEditInProgress() throws {
        guard let dir = ProcessInfo.processInfo.environment["GRIDCRAFT_SNAPSHOTS"] else { return }
        let document = SpreadsheetDocument(sample: "sales")
        let editor = EditController(document: document)
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 900, height: 420), styleMask: [.titled], backing: .buffered, defer: false)
        let grid = GridNSView(frame: NSRect(x: 0, y: 0, width: 900, height: 420))
        window.contentView = grid
        grid.document = document
        grid.editor = editor
        document.run("selection.set", ["range": "D14"])
        let bitmap = try #require(grid.bitmapImageRepForCachingDisplay(in: grid.bounds))
        grid.cacheDisplay(in: grid.bounds, to: bitmap)
        grid.beginEditing(with: NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: [], timestamp: 0, windowNumber: window.windowNumber,
                                                 context: nil, characters: "=", charactersIgnoringModifiers: "=", isARepeat: false, keyCode: 24))
        let view = try #require(window.firstResponder as? CellTextView)
        view.insertText("SUM(", replacementRange: view.selectedRange())
        editor.pointClick(CellAddress(row: 4, col: 2), extendingTo: CellAddress(row: 9, col: 2))
        view.sync(force: true)
        view.insertText(",AVERAGE(", replacementRange: view.selectedRange())
        editor.pointClick(CellAddress(row: 4, col: 3), extendingTo: CellAddress(row: 9, col: 4))
        view.sync(force: true)
        grid.editorChanged()
        let shot = try #require(grid.bitmapImageRepForCachingDisplay(in: grid.bounds))
        grid.cacheDisplay(in: grid.bounds, to: shot)
        try #require(shot.representation(using: .png, properties: [:])).write(to: URL(fileURLWithPath: dir).appendingPathComponent("editing.png"))
    }
}
