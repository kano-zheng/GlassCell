import Foundation
import GridCraftEngine
import SwiftUI
import UniformTypeIdentifiers

extension UTType {
    /// Excel workbook (.xlsx).
    public static let xlsx = UTType(importedAs: "org.openxmlformats.spreadsheetml.sheet")
}

/// A workbook window's document: one engine session holding the workbook. The UI changes it only
/// through `Engine.execute`; `changed()` tells the views to redraw.
public final class SpreadsheetDocument: ReferenceFileDocument, @unchecked Sendable {
    public static var readableContentTypes: [UTType] { [.xlsx, .commaSeparatedText, .tabSeparatedText] }
    public static var writableContentTypes: [UTType] { [.xlsx] }

    public let engine: Engine
    /// Bumped after every command, so SwiftUI redraws what shows the workbook.
    @Published public private(set) var generation = 0
    /// An error to show in an alert.
    @Published public var problem: String?
    /// A passing remark for the status bar (a Warning or Information validation message).
    @Published public var notice: String?

    /// A new workbook: empty, or one of the engine's samples (`sales`, `budget`, `grades`).
    public init(sample: String? = nil) {
        engine = Engine()
        do {
            let params = sample.map { JSON.encode(["sample": $0]) } ?? ""
            _ = try engine.execute(id: "file.new", params: params)
        } catch {
            problem = error.localizedDescription
        }
    }

    public required init(configuration: ReadConfiguration) throws {
        guard let data = configuration.file.regularFileContents else {
            throw CocoaError(.fileReadCorruptFile)
        }
        engine = Engine()
        let name = configuration.file.filename ?? "Book.xlsx"
        _ = try engine.execute(id: "file.open", params: JSON.encode(["name": name, "base64": data.base64EncodedString()]))
    }

    public func snapshot(contentType: UTType) throws -> Data {
        let result = JSON.decode(try engine.execute(id: "file.saveBytes", params: JSON.encode(["format": "xlsx"])))
        guard let base64 = result["base64"] as? String, let data = Data(base64Encoded: base64) else {
            throw CocoaError(.fileWriteUnknown)
        }
        return data
    }

    public func fileWrapper(snapshot: Data, configuration: WriteConfiguration) throws -> FileWrapper {
        FileWrapper(regularFileWithContents: snapshot)
    }

    /// The window's undo manager. Every undoable engine command registers one step on it, which
    /// runs the engine's own undo; that also marks the document edited, so it autosaves.
    @MainActor public weak var undoManager: UndoManager? {
        didSet { undoManager?.levelsOfUndo = 100 }
    }

    /// Runs a command; errors are kept in `problem` for the window to show.
    @MainActor @discardableResult
    public func run(_ id: String, _ params: [String: Any] = [:]) -> String? {
        let before = try? engine.history().revision
        defer { generation += 1 }
        do {
            let result = try engine.execute(id: id, params: params.isEmpty ? "" : JSON.encode(params))
            if let history = try? engine.history(), history.revision != before {
                registerUndo(history.undo.first ?? "")
            }
            return result
        } catch {
            problem = error.localizedDescription
            return nil
        }
    }

    /// Runs several commands as one step for Edit ▸ Undo (moving cells is a cut and a paste).
    @MainActor public func grouped(_ body: () -> Void) {
        undoManager?.beginUndoGrouping()
        body()
        undoManager?.endUndoGrouping()
    }

    @MainActor private func registerUndo(_ label: String) {
        register(label) { document in document.step("edit.undo") }
    }

    /// Registers one undo step. An undo manager that doesn't group by event throws (and the app
    /// goes down) when nothing opened a group: open one then.
    @MainActor private func register(_ label: String, _ handler: @escaping @MainActor (SpreadsheetDocument) -> Void) {
        guard let undoManager else { return }
        let ownGroup = !undoManager.groupsByEvent && undoManager.groupingLevel == 0
        if ownGroup { undoManager.beginUndoGrouping() }
        undoManager.registerUndo(withTarget: self) { document in handler(document) }
        undoManager.setActionName(label)
        if ownGroup { undoManager.endUndoGrouping() }
    }

    /// Undo or redo one engine step, registering the opposite step.
    @MainActor private func step(_ id: String) {
        defer { generation += 1 }
        do {
            _ = try engine.execute(id: id, params: "")
        } catch {
            problem = error.localizedDescription
            return
        }
        guard let history = try? engine.history() else { return }
        let opposite = id == "edit.undo" ? "edit.redo" : "edit.undo"
        register((id == "edit.undo" ? history.redo.first : history.undo.first) ?? "") { document in document.step(opposite) }
    }
}
