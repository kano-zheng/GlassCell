import Foundation
import GridCraftEngine

/// What Shortcuts, Siri and agents can do with the frontmost workbook: run any engine command,
/// read and write cells. Everything goes through `execute`, like the UI.
@MainActor
public enum Automation {
    public enum Failure: LocalizedError {
        case noWorkbook
        case engine(String)

        public var errorDescription: String? {
            switch self {
            case .noWorkbook: "Open a workbook in GlassCell first."
            case let .engine(message): message
            }
        }
    }

    private static func workbook() throws -> SpreadsheetDocument {
        guard let document = WorkbookRegistry.shared.frontmost else { throw Failure.noWorkbook }
        return document
    }

    /// Runs command `id` with JSON `params` (empty for none) and returns its JSON result. An edit
    /// in progress in the window is confirmed first.
    public static func run(_ id: String, params: String) throws -> String {
        let document = try workbook()
        guard WorkbookRegistry.shared.frontmostEditor?.commitIfEditing() != false else {
            throw Failure.engine("The cell being edited couldn't be confirmed.")
        }
        do {
            return try document.perform(id, json: params.trimmingCharacters(in: .whitespacesAndNewlines))
        } catch {
            throw Failure.engine(error.localizedDescription)
        }
    }

    /// A cell's displayed text (`B3`, or `Sheet2!B3`).
    public static func cellText(_ cell: String) throws -> String {
        let result = JSON.decode(try run("cell.get", params: JSON.encode(["cell": cell])))
        return result["text"] as? String ?? ""
    }

    /// Types `input` into a cell: text, a number, or a formula starting with `=`.
    public static func setCell(_ cell: String, to input: String) throws {
        _ = try run("cell.set", params: JSON.encode(["cell": cell, "input": input]))
    }

    /// The catalog's command ids, for pickers.
    public static func commandIds() -> [String] {
        let engine = WorkbookRegistry.shared.frontmost?.engine ?? Engine()
        return ((try? engine.commands()) ?? []).map(\.id).sorted()
    }
}
