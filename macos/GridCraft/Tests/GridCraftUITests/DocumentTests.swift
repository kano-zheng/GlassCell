import Foundation
import GridCraftEngine
@testable import GridCraftUI
import Testing

@MainActor
@Suite struct DocumentTests {
    private func value(_ document: SpreadsheetDocument, _ cell: String) -> String {
        JSON.decode((try? document.engine.execute(id: "cell.get", params: JSON.encode(["cell": cell]))) ?? "{}")["text"] as? String ?? ""
    }

    @Test func undoableCommandsStepTheWindowsUndoManagerThroughTheEngine() {
        let document = SpreadsheetDocument()
        let undo = UndoManager()
        undo.groupsByEvent = false
        document.undoManager = undo
        undo.beginUndoGrouping()
        document.run("cell.set", ["cell": "A1", "input": "one"])
        undo.endUndoGrouping()
        document.run("selection.set", ["range": "B2"])
        #expect(undo.canUndo)
        #expect(undo.undoActionName == "Enter Cell")
        undo.undo()
        #expect(value(document, "A1") == "")
        #expect(undo.canRedo)
        undo.redo()
        #expect(value(document, "A1") == "one")
        #expect(undo.canUndo)
    }

    @Test func aFailingCommandIsReportedNotThrown() {
        let document = SpreadsheetDocument()
        #expect(document.run("no.such.command") == nil)
        #expect(document.problem != nil)
    }
}

@MainActor
@Suite struct UndoRobustnessTests {
    /// An undo manager without event grouping threw "must begin a group" (and took the app down)
    /// when a command registered its step outside a group.
    @Test func registeringOutsideAGroupDoesNotThrow() {
        let document = SpreadsheetDocument()
        let undo = UndoManager()
        undo.groupsByEvent = false
        document.undoManager = undo
        document.run("cell.set", ["cell": "A1", "input": "x"])
        #expect(undo.canUndo)
        undo.undo()
        #expect(undo.canRedo)
    }
}
