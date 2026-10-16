import AppKit
import GridCraftEngine
import GridCraftUI
import QuickLookUI

/// Quick Look for workbooks: the active sheet's top-left corner, drawn by the same renderer as
/// the app's grid.
final class PreviewProvider: QLPreviewProvider, QLPreviewingController {
    static let size = CGSize(width: 1000, height: 640)

    func providePreview(for request: QLFilePreviewRequest) async throws -> QLPreviewReply {
        let data = try Data(contentsOf: request.fileURL)
        let name = request.fileURL.lastPathComponent
        let image = try await MainActor.run { try WorkbookPreview.render(data: data, name: name, size: Self.size) }
        let size = Self.size
        return QLPreviewReply(contextSize: size, isBitmap: true) { context, _ in
            context.draw(image, in: CGRect(origin: .zero, size: size))
        }
    }
}
