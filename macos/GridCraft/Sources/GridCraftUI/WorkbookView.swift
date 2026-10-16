import AppKit
import GridCraftEngine
import SwiftUI

/// A workbook window: the grid, with the sheet tabs below and the zoom in the toolbar.
public struct WorkbookView: View {
    @ObservedObject var document: SpreadsheetDocument
    @Environment(\.undoManager) private var undoManager
    @State private var frameTime: Double = 0
    @State private var editor: EditController

    public init(document: SpreadsheetDocument) {
        self.document = document
        _editor = State(initialValue: EditController(document: document))
    }

    private var sheet: SheetInfo? {
        _ = document.generation
        return try? document.engine.sheet()
    }

    /// What a command asked for: dialogs (the Mac's come in MAC6; Format Cells is the
    /// inspector), messages, editing with a template, links.
    private func handle(_ request: UiRequest) {
        switch request {
        case let .dialog(name, _):
            if name == "formatCells" {
                document.showInspector = true
            } else {
                document.problem = "The “\(name)” dialog isn't in GridCraft for Mac yet."
            }
        case let .message(text):
            document.problem = text
        case let .editCell(text):
            editor.grid?.beginEditing(text: text)
        case let .openUrl(url):
            if let link = URL(string: url) {
                NSWorkspace.shared.open(link)
            }
        }
    }

    public var body: some View {
        VStack(spacing: 0) {
            FormulaBar(document: document, editor: editor)
            Divider()
            GridView(document: document, editor: editor) { ms in
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
                if let notice = document.notice {
                    Text(notice)
                        .foregroundStyle(.secondary)
                        .font(.caption)
                        .lineLimit(1)
                        .task(id: notice) {
                            try? await Task.sleep(for: .seconds(6))
                            document.notice = nil
                        }
                }
                Text(String(format: "%.1f ms", frameTime))
                    .monospacedDigit()
                    .foregroundStyle(.secondary)
                    .font(.caption)
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 4)
        }
        .inspector(isPresented: $document.showInspector) {
            InspectorView(document: document, editor: editor)
                .inspectorColumnWidth(min: 260, ideal: 300, max: 420)
        }
        .toolbar(id: "gridcraft.workbook") {
            WorkbookToolbar(document: document, editor: editor, undoManager: undoManager)
        }
        .focusedSceneValue(\.workbook, WorkbookFocus(document: document, editor: editor, generation: document.generation))
        .sheet(isPresented: $document.showPalette) {
            CommandPalette(document: document, editor: editor)
        }
        .onChange(of: document.requests) { _, requests in
            guard !requests.isEmpty else { return }
            document.requests = []
            for request in requests {
                handle(request)
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
