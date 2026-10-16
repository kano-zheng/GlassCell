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
    /// Set when the workbook couldn't be created.
    @Published public var problem: String?

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

    /// Runs a command; errors are kept in `problem` for the window to show.
    @discardableResult
    public func run(_ id: String, _ params: [String: Any] = [:]) -> String? {
        do {
            let result = try engine.execute(id: id, params: params.isEmpty ? "" : JSON.encode(params))
            generation += 1
            return result
        } catch {
            problem = error.localizedDescription
            generation += 1
            return nil
        }
    }
}
