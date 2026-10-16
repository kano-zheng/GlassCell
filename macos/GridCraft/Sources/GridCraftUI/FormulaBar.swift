import AppKit
import GridCraftEngine
import SwiftUI

/// Above the grid: the Name Box (where the selection is; type a reference or a name to go there,
/// or a new name to name the selection) and the formula bar (the selected cell's content, and
/// the same edit as the cell editor while editing).
public struct FormulaBar: View {
    @ObservedObject var document: SpreadsheetDocument
    let editor: EditController
    @State private var nameBox = ""
    @FocusState private var nameBoxFocused: Bool

    public init(document: SpreadsheetDocument, editor: EditController) {
        self.document = document
        self.editor = editor
    }

    private var shownName: String {
        _ = document.generation
        return (try? document.engine.nameBoxText(dragging: false)) ?? ""
    }

    public var body: some View {
        HStack(spacing: 6) {
            TextField("Name Box", text: $nameBox)
                .textFieldStyle(.roundedBorder)
                .frame(width: 110)
                .focused($nameBoxFocused)
                .onSubmit {
                    let target = nameBox.trimmingCharacters(in: .whitespaces)
                    if !target.isEmpty, target != shownName {
                        document.run("edit.nameBox", ["text": target])
                    }
                    nameBoxFocused = false
                    nameBox = shownName
                    editor.grid?.editorEnded()
                }
                .onChange(of: shownName) { _, name in
                    if !nameBoxFocused { nameBox = name }
                }
                .onAppear { nameBox = shownName }
            Image(systemName: "function")
                .foregroundStyle(.secondary)
            FormulaTextField(document: document, editor: editor)
                .frame(minHeight: 22, maxHeight: 22)
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 5)
    }
}

/// The formula bar's text: a `CellTextView` sharing the cell edit.
struct FormulaTextField: NSViewRepresentable {
    @ObservedObject var document: SpreadsheetDocument
    let editor: EditController

    func makeNSView(context: Context) -> NSScrollView {
        let scroll = NSScrollView()
        scroll.hasVerticalScroller = false
        scroll.hasHorizontalScroller = false
        scroll.borderType = .bezelBorder
        let text = CellTextView(frame: .zero, formulaBar: true)
        text.controller = editor
        text.baseFont = .monospacedSystemFont(ofSize: 12, weight: .regular)
        text.textContainer?.widthTracksTextView = true
        text.isHorizontallyResizable = false
        text.isVerticallyResizable = true
        text.minSize = NSSize(width: 0, height: 20)
        text.frame = NSRect(origin: .zero, size: NSSize(width: 400, height: 20))
        text.autoresizingMask = [.width, .height]
        scroll.documentView = text
        editor.formulaBar = text
        text.showIdleText()
        return scroll
    }

    func updateNSView(_ scroll: NSScrollView, context: Context) {
        _ = document.generation
        guard let text = scroll.documentView as? CellTextView else { return }
        if editor.isEditing {
            text.sync()
        } else {
            text.showIdleText()
        }
    }
}
