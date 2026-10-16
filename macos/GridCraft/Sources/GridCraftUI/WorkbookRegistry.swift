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

    /// `window` came to the front by other means than a click (the control channel opened it).
    public func activated(window: NSWindow?) {
        guard let grid = window?.contentView?.firstDescendant(of: GridNSView.self), let document = grid.document, let editor = grid.editor else { return }
        activated(document, editor: editor)
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

    /// Every open workbook, the frontmost last.
    public var documents: [SpreadsheetDocument] {
        entries.removeAll { $0.document == nil }
        return entries.compactMap(\.document)
    }

    /// The window showing `document`, if it has one.
    public func window(for document: SpreadsheetDocument) -> NSWindow? {
        NSApplication.shared.windows.first { $0.contentView?.firstDescendant(of: GridNSView.self)?.document === document }
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
