import AppKit
import GridCraftEngine
import SwiftUI

/// A workbook window: the formula bar, and the grid with the status bar floating over its bottom.
public struct WorkbookView: View {
    @ObservedObject var document: SpreadsheetDocument
    /// Where the workbook is saved (`nil` until it is): sharing names it, Spotlight indexes it.
    let fileURL: URL?
    @Environment(\.undoManager) private var undoManager
    /// How much of the grid's bottom the status bar covers.
    @State private var statusBarHeight: CGFloat = 0
    @State private var editor: EditController

    public init(document: SpreadsheetDocument, fileURL: URL? = nil) {
        self.document = document
        self.fileURL = fileURL
        _editor = State(initialValue: EditController(document: document))
    }

    private var sheet: SheetInfo? {
        _ = document.generation
        return try? document.engine.sheet()
    }

    public var body: some View {
        VStack(spacing: 0) {
            FormulaBar(document: document, editor: editor)
            Divider()
            // The grid runs to the window's bottom edge, under the floating status bar.
            GridView(document: document, editor: editor, bottomInset: statusBarHeight)
                .overlay(alignment: .bottom) {
                    WorkbookStatusBar(document: document)
                        .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { statusBarHeight = $0 }
                }
        }
        .overlay(alignment: .top) {
            if document.showPalette {
                ZStack(alignment: .top) {
                    // A click beside the palette closes it.
                    Color.clear
                        .contentShape(.rect)
                        .onTapGesture { document.showPalette = false }
                    CommandPalette(document: document, editor: editor)
                        .padding(.top, 40)
                }
                .transition(.opacity.combined(with: .scale(scale: 0.96, anchor: .top)))
            }
        }
        .animation(.smooth(duration: 0.2), value: document.showPalette)
        .inspector(isPresented: $document.showInspector) {
            InspectorView(document: document, editor: editor)
                .inspectorColumnWidth(min: 260, ideal: 300, max: 420)
        }
        .toolbar(id: "gridcraft.workbook") {
            WorkbookToolbar(document: document, editor: editor, undoManager: undoManager, fileURL: fileURL)
        }
        .focusedSceneValue(\.workbook, WorkbookFocus(document: document, editor: editor, generation: document.generation))
        .sheet(item: $document.dialog) { request in
            CustomDialog(request: request, document: document, editor: editor)
        }
        .sheet(item: $document.form) { presented in
            FormSheet(form: presented.session, document: document)
        }
        .onChange(of: document.requests) { _, requests in
            guard !requests.isEmpty else { return }
            document.requests = []
            for request in requests {
                document.handle(request, editor: editor)
            }
        }
        .onAppear {
            document.undoManager = undoManager
            WorkbookRegistry.shared.activated(document, editor: editor)
        }
        .onReceive(NotificationCenter.default.publisher(for: NSWindow.didBecomeKeyNotification)) { note in
            // This window came to the front: intents and the control channel act on its workbook.
            if (note.object as? NSWindow)?.contentView?.firstDescendant(of: GridNSView.self)?.document === document {
                WorkbookRegistry.shared.activated(document, editor: editor)
            }
        }
        .onChange(of: undoManager.map(ObjectIdentifier.init)) { document.undoManager = undoManager }
        // Spotlight indexes the workbook when it opens or gets a new file, and as it closes.
        .task(id: fileURL) { WorkbookSpotlight.index(document, fileURL: fileURL) }
        .onDisappear { WorkbookSpotlight.index(document, fileURL: fileURL) }
        .alert("GridCraft", isPresented: Binding(get: { document.problem != nil }, set: { if !$0 { document.problem = nil } })) {
            Button("OK") { document.problem = nil }
        } message: {
            Text(document.problem ?? "")
        }
    }
}

extension SpreadsheetDocument {
    /// What a command asked for: dialogs (Format Cells is the inspector), messages, editing
    /// with a template, links.
    @MainActor func handle(_ request: UiRequest, editor: EditController) {
        switch request {
        case let .dialog(name, params):
            if name == "formatCells" {
                showInspector = true
            } else if let session = try? engine.openForm(name: name, params: params) {
                form = PresentedForm(session: session)
            } else if DialogRequest.handled.contains(name) {
                dialog = DialogRequest(name: name, params: JSON.decode(params))
            } else if name == "commandSearch" {
                showPalette = true
            } else if name == "insertPicture" {
                let values = JSON.decode(params)
                Task { @MainActor in insertPicture(self, params: values) }
            } else {
                problem = "The “\(name)” dialog isn't in GridCraft for Mac yet."
            }
        case let .message(text):
            problem = text
        case let .editCell(text):
            editor.grid?.beginEditing(text: text)
        case let .openUrl(url):
            if let link = URL(string: url) {
                NSWorkspace.shared.open(link)
            }
        }
    }
}
