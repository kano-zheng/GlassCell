import GridCraftEngine
import SwiftUI

/// A workbook window: the grid, with the sheet tabs below and the zoom in the toolbar.
public struct WorkbookView: View {
    @ObservedObject var document: SpreadsheetDocument
    @Environment(\.undoManager) private var undoManager
    @State private var frameTime: Double = 0

    public init(document: SpreadsheetDocument) {
        self.document = document
    }

    private var sheet: SheetInfo? {
        _ = document.generation
        return try? document.engine.sheet()
    }

    public var body: some View {
        VStack(spacing: 0) {
            GridView(document: document) { ms in
                frameTime = ms
            }
            Divider()
            HStack(spacing: 4) {
                if let sheet {
                    ForEach(Array(sheet.sheetNames.enumerated()), id: \.offset) { index, name in
                        Button(name) {
                            document.run("sheet.activate", ["sheet": index])
                        }
                        .buttonStyle(.accessoryBar)
                        .fontWeight(index == Int(sheet.index) ? .semibold : .regular)
                    }
                }
                Spacer()
                Text(String(format: "%.1f ms", frameTime))
                    .monospacedDigit()
                    .foregroundStyle(.secondary)
                    .font(.caption)
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 4)
        }
        .toolbar {
            ToolbarItemGroup {
                Button("Zoom Out", systemImage: "minus.magnifyingglass") {
                    document.run("view.zoom", ["percent": max(10, Int(sheet?.zoom ?? 100) - 10)])
                }
                Text("\(sheet?.zoom ?? 100)%")
                    .monospacedDigit()
                Button("Zoom In", systemImage: "plus.magnifyingglass") {
                    document.run("view.zoom", ["percent": min(400, Int(sheet?.zoom ?? 100) + 10)])
                }
            }
        }
        .onAppear { document.undoManager = undoManager }
        .onChange(of: undoManager.map(ObjectIdentifier.init)) { document.undoManager = undoManager }
        .alert("GridCraft", isPresented: Binding(get: { document.problem != nil }, set: { if !$0 { document.problem = nil } })) {
            Button("OK") { document.problem = nil }
        } message: {
            Text(document.problem ?? "")
        }
    }
}
