import AppKit
import Foundation

/// The workbooks open in windows, the frontmost last: App Intents and the control channel act
/// on these (SwiftUI's documents aren't reachable through `NSDocumentController`).
@MainActor
public final class WorkbookRegistry {
    public static let shared = WorkbookRegistry()

    private struct Entry {
        weak var document: SpreadsheetDocument?
        weak var editor: EditController?
    }

    private var entries: [Entry] = []

    /// A window showing `document` came to the front (or opened).
    public func activated(_ document: SpreadsheetDocument, editor: EditController) {
        entries.removeAll { $0.document == nil || $0.document === document }
        entries.append(Entry(document: document, editor: editor))
    }

    /// The frontmost workbook, if any window is open.
    public var frontmost: SpreadsheetDocument? {
        entries.removeAll { $0.document == nil }
        return entries.last?.document
    }

    /// The frontmost workbook's edit, to confirm before a command runs.
    public var frontmostEditor: EditController? {
        entries.last?.editor
    }

    public var count: Int {
        entries.removeAll { $0.document == nil }
        return entries.count
    }
}

extension NSView {
    /// The first view of type `T` in this view's subtree.
    func firstDescendant<T: NSView>(of type: T.Type) -> T? {
        if let match = self as? T { return match }
        for sub in subviews {
            if let match = sub.firstDescendant(of: type) { return match }
        }
        return nil
    }
}
