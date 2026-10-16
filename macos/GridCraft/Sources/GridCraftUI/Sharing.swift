import AppKit
import CoreSpotlight
import CoreTransferable
import Foundation
import GridCraftEngine
import SwiftUI
import UniformTypeIdentifiers

/// The workbook as a file to share (Mail, Messages, AirDrop…): an `.xlsx` of what the window
/// shows now, saved or not, written when the share actually happens.
public struct SharedWorkbook: Transferable, Sendable {
    let document: SpreadsheetDocument
    /// The file name the recipient sees (`Sales.xlsx`).
    public let name: String

    public init(document: SpreadsheetDocument, fileURL: URL?) {
        self.document = document
        let base = fileURL?.deletingPathExtension().lastPathComponent ?? "Workbook"
        name = base + ".xlsx"
    }

    public static var transferRepresentation: some TransferRepresentation {
        FileRepresentation(exportedContentType: .xlsx) { shared in
            SentTransferredFile(try await shared.write())
        }
        .suggestedFileName { $0.name }
    }

    /// Writes the workbook into a fresh temporary folder under its name.
    func write() async throws -> URL {
        let data = try await MainActor.run { try document.snapshot(contentType: .xlsx) }
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent("GridCraft Share \(UUID().uuidString)")
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let url = folder.appendingPathComponent(name)
        try data.write(to: url)
        return url
    }
}

/// Spotlight finds the workbooks opened in GridCraft by their sheets, tables, names and cell
/// text; choosing one opens it here. Items live in the app's own index on this Mac.
@MainActor
public enum WorkbookSpotlight {
    static let domain = "workbooks"
    /// Characters of cell text indexed per workbook.
    static let textLimit: UInt32 = 100_000

    /// The searchable item for `document` saved at `fileURL`.
    static func item(for document: SpreadsheetDocument, fileURL: URL) -> CSSearchableItem? {
        guard let summary = try? document.engine.searchSummary(maxChars: textLimit) else { return nil }
        let type = UTType(filenameExtension: fileURL.pathExtension) ?? .xlsx
        let attributes = CSSearchableItemAttributeSet(contentType: type)
        attributes.title = fileURL.deletingPathExtension().lastPathComponent
        attributes.displayName = fileURL.lastPathComponent
        attributes.contentDescription = summary.sheets.joined(separator: ", ")
        attributes.keywords = Array(Set(summary.sheets + summary.tables + summary.names)).sorted()
        attributes.textContent = summary.text
        attributes.contentURL = fileURL
        attributes.contentModificationDate = (try? fileURL.resourceValues(forKeys: [.contentModificationDateKey]))?.contentModificationDate
        return CSSearchableItem(uniqueIdentifier: fileURL.path, domainIdentifier: domain, attributeSet: attributes)
    }

    /// (Re)indexes a workbook that has a file.
    public static func index(_ document: SpreadsheetDocument, fileURL: URL?) {
        guard let fileURL, let item = item(for: document, fileURL: fileURL) else { return }
        CSSearchableIndex.default().indexSearchableItems([item]) { error in
            if let error {
                FileHandle.standardError.write(Data("GridCraft Spotlight: \(error.localizedDescription)\n".utf8))
            }
        }
    }

    /// The file a Spotlight result stands for, if `activity` is one of ours.
    public static func fileURL(from activity: NSUserActivity) -> URL? {
        guard activity.activityType == CSSearchableItemActionType,
              let path = activity.userInfo?[CSSearchableItemActivityIdentifier] as? String else { return nil }
        return URL(fileURLWithPath: path)
    }

    /// Opens the workbook a Spotlight result stands for; one that's gone leaves the index.
    @discardableResult
    public static func open(_ activity: NSUserActivity) -> Bool {
        guard let url = fileURL(from: activity) else { return false }
        guard FileManager.default.fileExists(atPath: url.path) else {
            CSSearchableIndex.default().deleteSearchableItems(withIdentifiers: [url.path]) { _ in }
            return true
        }
        NSDocumentController.shared.openDocument(withContentsOf: url, display: true) { _, _, _ in }
        return true
    }
}
