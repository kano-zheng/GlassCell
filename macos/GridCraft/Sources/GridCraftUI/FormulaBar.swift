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
        GlassEffectContainer(spacing: 8) {
            HStack(spacing: 8) {
                TextField("Name Box", text: $nameBox)
                    .textFieldStyle(.plain)
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
                    .padding(.horizontal, 12)
                    .frame(width: 120, height: Self.height)
                    .glassEffect(.regular, in: .capsule)
                HStack(spacing: 6) {
                    Image(systemName: "function")
                        .foregroundStyle(.secondary)
                    FormulaTextField(document: document, editor: editor)
                        .frame(minHeight: 20, maxHeight: 20)
                }
                .padding(.horizontal, 12)
                .frame(height: Self.height)
                .glassEffect(.regular, in: .capsule)
            }
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 6)
    }

    /// The capsules' height, a little under the toolbar's controls.
    private static let height: CGFloat = 30
}

/// The formula bar's text: a `CellTextView` sharing the cell edit.
struct FormulaTextField: NSViewRepresentable {
    @ObservedObject var document: SpreadsheetDocument
    let editor: EditController

    func makeNSView(context: Context) -> NSScrollView {
        let scroll = NSScrollView()
        scroll.hasVerticalScroller = false
        scroll.hasHorizontalScroller = false
        scroll.borderType = .noBorder
        scroll.drawsBackground = false
        let text = CellTextView(frame: .zero, formulaBar: true)
        // The capsule's glass shows through.
        text.drawsBackground = false
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
