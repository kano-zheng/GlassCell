import AppKit
import GridCraftEngine
import SwiftUI

/// The window's customizable toolbar: the common commands of Excel's Home and Insert tabs, as
/// Mac toolbar items (SF Symbols through the system API). Each runs an engine command; toggles
/// show the active cell's state, which is what they flip.
struct WorkbookToolbar: CustomizableToolbarContent {
    @ObservedObject var document: SpreadsheetDocument
    let editor: EditController
    let undoManager: UndoManager?
    let fileURL: URL?

    private var format: SelectionFormat? {
        _ = document.generation
        return try? document.engine.selectionFormat()
    }

    private var zoom: UInt16 {
        _ = document.generation
        return (try? document.engine.sheet().zoom) ?? 100
    }

    /// Runs a command from the toolbar, confirming an edit in progress first.
    private func run(_ id: String, _ params: [String: Any] = [:]) {
        guard editor.commitIfEditing() else { return }
        document.run(id, params)
    }

    private func toggle(_ id: String, _ label: String, _ symbol: String, isOn: Bool?) -> some View {
        Toggle(isOn: Binding(get: { isOn == true }, set: { run(id, ["on": $0]) })) {
            Label(label, systemImage: symbol)
        }
        .toggleStyle(.button)
        .help(label)
    }

    /// An alignment button: on when the active cell has that alignment.
    private func align(_ id: String, _ label: String, _ symbol: String, _ value: HAlign) -> some View {
        Toggle(isOn: Binding(get: { format?.active.halign == value }, set: { _ in run(id) })) {
            Label(label, systemImage: symbol)
        }
        .toggleStyle(.button)
        .help(label)
    }

    /// A thin line between two groups of the long capsule.
    private func divider(_ id: String) -> some CustomizableToolbarContent {
        ToolbarItem(id: id) {
            // A toolbar lays a Divider out horizontally.
            Rectangle()
                .fill(.separator)
                .frame(width: 1, height: 18)
                .padding(.horizontal, 4)
        }
        .customizationBehavior(.disabled)
    }

    var body: some CustomizableToolbarContent {
        // The document's name, a capsule by itself (the system title is removed).
        ToolbarItem(id: "document", placement: .navigation) {
            // In a container, so the toolbar keeps it a view (a bare Menu shows its icon only).
            HStack(spacing: 0) {
                DocumentTitleMenu(fileURL: fileURL)
            }
        }
        .customizationBehavior(.disabled)
        // Neighbouring items share one piece of glass: the editing commands make one long
        // capsule; spacers set the view items apart.
        ToolbarItem(id: "undo") {
            Button("Undo", systemImage: "arrow.uturn.backward") { undoManager?.undo() }
        }
        ToolbarItem(id: "redo") {
            Button("Redo", systemImage: "arrow.uturn.forward") { undoManager?.redo() }
        }
        divider("fontGroup")
        ToolbarItem(id: "formatPainter") {
            Button("Format Painter", systemImage: "paintbrush.pointed") { run("edit.formatPainter") }
                .help("Format Painter")
        }
        ToolbarItem(id: "font") {
            FontMenu(current: format?.active.fontName) { run("home.fontName", ["name": $0]) }
        }
        ToolbarItem(id: "fontSize") {
            let current = format?.active.fontSize.map { Double($0) } ?? 11
            let sizes = Array(Set([8, 9, 10, 11, 12, 14, 16, 18, 20, 24, 28, 36, 48, 72].map(Double.init) + [current])).sorted()
            PopUpPicker(items: sizes.map { $0.formatted() }, selected: current.formatted(), width: 52) { title in
                if let size = Double(title) { run("home.fontSize", ["size": size]) }
            }
            .help("Font Size")
        }
        divider("fontStyleGroup")
        ToolbarItem(id: "bold") {
            toggle("home.bold", "Bold", "bold", isOn: format?.active.bold)
        }
        ToolbarItem(id: "italic") {
            toggle("home.italic", "Italic", "italic", isOn: format?.active.italic)
        }
        ToolbarItem(id: "underline") {
            toggle("home.underline", "Underline", "underline", isOn: format?.active.underline.map { $0 != .none })
        }
        ToolbarItem(id: "fontColor") {
            ColorMenu(title: "Font Color", symbol: "character", automatic: "Automatic") { hex in run("home.fontColor", ["color": hex ?? "auto"]) }
        }
        ToolbarItem(id: "fill") {
            ColorMenu(title: "Fill Color", symbol: "drop.halffull", automatic: "No Fill") { hex in run("home.fillColor", ["color": hex ?? "none"]) }
        }
        divider("alignmentGroup")
        ToolbarItem(id: "alignLeft") {
            align("home.alignLeft", "Align Left", "text.alignleft", .left)
        }
        ToolbarItem(id: "alignCenter") {
            align("home.alignCenter", "Center", "text.aligncenter", .center)
        }
        ToolbarItem(id: "alignRight") {
            align("home.alignRight", "Align Right", "text.alignright", .right)
        }
        divider("cellGroup")
        ToolbarItem(id: "wrap") {
            toggle("home.wrapText", "Wrap Text", "text.word.spacing", isOn: format?.active.wrap)
        }
        ToolbarItem(id: "merge") {
            Button("Merge & Center", systemImage: "arrow.left.and.right.square") { run("home.mergeCenter") }
                .help("Merge & Center")
        }
        ToolbarItem(id: "borders") {
            Menu {
                ForEach(Self.borderPresets, id: \.preset) { item in
                    Button(item.label) { run("home.borders", ["preset": item.preset]) }
                }
            } label: {
                Label("Borders", systemImage: "square.dashed")
            }
            .help("Borders")
            .menuIndicator(.hidden)
        }
        ToolbarItem(id: "numberFormat") {
            Menu {
                ForEach(["General", "Number", "Currency", "Accounting", "Short Date", "Long Date", "Time", "Percentage", "Fraction", "Scientific", "Text"], id: \.self) { name in
                    Button(name) { run("home.numberFormat", ["format": name]) }
                }
                Divider()
                Button("Increase Decimal") { run("home.increaseDecimal") }
                Button("Decrease Decimal") { run("home.decreaseDecimal") }
                Button("Percent Style") { run("home.percent") }
                Button("Comma Style") { run("home.comma") }
                Divider()
                Button("More Number Formats…") { document.showInspector = true }
            } label: {
                Label(format?.active.numberKind ?? "Number Format", systemImage: "number")
            }
            .help("Number Format")
            .menuIndicator(.hidden)
        }
        divider("dataGroup")
        ToolbarItem(id: "sortFilter") {
            Menu {
                Button("Sort A to Z") { run("data.sortAscending") }
                Button("Sort Z to A") { run("data.sortDescending") }
                Button("Custom Sort…") { run("data.sort") }
                Divider()
                Button("Filter") { run("data.filter") }
                Button("Clear Filter") { run("data.clearFilter") }
                Button("Reapply") { run("data.reapply") }
            } label: {
                Label("Sort & Filter", systemImage: "line.3.horizontal.decrease")
            }
            .help("Sort & Filter")
            .menuIndicator(.hidden)
        }
        ToolbarItem(id: "insert") {
            Menu {
                Button("Chart") { run("insert.chart") }
                Button("Table") { run("insert.table") }
                Button("PivotTable") { run("insert.pivotTable") }
                Divider()
                Button("Picture…") { run("insert.picture") }
                Menu("Shape") {
                    ForEach(["rectangle", "roundedRectangle", "ellipse", "triangle", "line", "arrow", "textBox"], id: \.self) { kind in
                        Button(kind) { run("insert.shape", ["kind": kind]) }
                    }
                }
                Divider()
                Button("Link…") { run("insert.link") }
                Button("Comment") { run("review.newComment") }
            } label: {
                Label("Insert", systemImage: "plus.square.on.square")
            }
            .help("Insert")
            .menuIndicator(.hidden)
        }
        // The functions: a capsule of their own.
        ToolbarSpacer(.fixed)
        ToolbarItem(id: "autoSum") {
            Button("AutoSum", systemImage: "sum") { run("formulas.autoSum") }
                .help("AutoSum")
        }
        ToolbarItem(id: "autoSumMenu") {
            Menu {
                ForEach(["SUM", "AVERAGE", "COUNT", "MAX", "MIN"], id: \.self) { function in
                    Button(function.capitalized) { run("formulas.autoSum", ["function": function]) }
                }
            } label: {
                Label("AutoSum Functions", systemImage: "chevron.down")
            }
            .menuIndicator(.hidden)
            .help("AutoSum Functions")
        }
        ToolbarSpacer(.flexible)
        ToolbarItem(id: "zoom") {
            let levels = Array(Set([50, 75, 100, 125, 150, 200, Int(zoom)])).sorted()
            PopUpPicker(items: levels.map { "\($0)%" }, selected: "\(zoom)%", width: 70) { title in
                if let percent = Int(title.dropLast()) { run("view.zoom", ["percent": percent]) }
            }
            .help("Zoom")
        }
        ToolbarItem(id: "share") {
            let shared = SharedWorkbook(document: document, fileURL: fileURL)
            ShareLink(item: shared, preview: SharePreview(shared.name)) {
                Label("Share", systemImage: "square.and.arrow.up")
            }
            .help("Share")
        }
        ToolbarSpacer(.fixed)
        ToolbarItem(id: "inspector") {
            Button("Inspector", systemImage: "sidebar.trailing") { document.showInspector.toggle() }
                .help("Show or hide the inspector")
        }
    }

    static let borderPresets: [(label: String, preset: String)] = [
        ("Bottom Border", "bottom"), ("Top Border", "top"), ("Left Border", "left"), ("Right Border", "right"),
        ("No Border", "none"), ("All Borders", "all"), ("Outside Borders", "outside"), ("Thick Outside Borders", "thickOutside"),
        ("Bottom Double Border", "doubleBottom"), ("Thick Bottom Border", "thickBottom"), ("Top and Bottom Border", "topBottom"),
    ]
}

/// The document's name in the toolbar, with the document menu the system title had: rename,
/// move, duplicate, versions, and Show in Finder.
struct DocumentTitleMenu: View {
    let fileURL: URL?

    var body: some View {
        Menu {
            Button("Rename…") { send(#selector(NSDocument.rename(_:))) }
            Button("Move To…") { send(#selector(NSDocument.move(_:))) }
            Button("Duplicate") { send(#selector(NSDocument.duplicate(_:))) }
            Divider()
            Button("Browse All Versions…") { send(#selector(NSDocument.browseVersions(_:))) }
            if let fileURL {
                Divider()
                Button("Show in Finder") { NSWorkspace.shared.activateFileViewerSelecting([fileURL]) }
            }
        } label: {
            HStack(spacing: 6) {
                Image(systemName: "tablecells")
                Text(fileURL?.lastPathComponent ?? String(localized: "Untitled"))
                    .fontWeight(.semibold)
                    .lineLimit(1)
            }
        }
        .menuStyle(.borderlessButton)
        .fixedSize()
        .padding(.horizontal, 10)
        .help(fileURL?.path ?? "")
    }

    /// The window's document handles these (it is in the responder chain).
    private func send(_ action: Selector) {
        NSApp.sendAction(action, to: nil, from: nil)
    }
}

/// The font family picker: the cell's font first (also when the Mac lacks it), then every
/// family the Mac has.
struct FontMenu: View {
    let current: String?
    let pick: (String) -> Void

    private static let families = NSFontManager.shared.availableFontFamilies

    var body: some View {
        let name = current ?? "–"
        PopUpPicker(items: Self.families.contains(name) ? Self.families : [name] + Self.families, selected: name, width: 140) { family in
            if family != "–" { pick(family) }
        }
        .help("Font")
    }
}

/// A colour menu: Automatic / No Fill and a palette of our own swatches.
struct ColorMenu: View {
    let title: String
    let symbol: String
    let automatic: String
    /// `nil`: Automatic (font) or No Fill.
    let pick: (String?) -> Void

    static let palette: [(name: String, hex: String)] = [
        ("Black", "#000000"), ("Dark Gray", "#595959"), ("Gray", "#A6A6A6"), ("White", "#FFFFFF"),
        ("Red", "#C0392B"), ("Orange", "#E67E22"), ("Yellow", "#F4D03F"), ("Green", "#27AE60"),
        ("Teal", "#16A085"), ("Blue", "#2E6FD8"), ("Navy", "#1F3A5F"), ("Purple", "#7A4FC9"),
        ("Light Red", "#F5B7B1"), ("Light Yellow", "#FCF3CF"), ("Light Green", "#D5F5E3"), ("Light Blue", "#D6EAF8"),
    ]

    var body: some View {
        Menu {
            Button(automatic) { pick(nil) }
            Divider()
            ForEach(Self.palette, id: \.hex) { color in
                Button {
                    pick(color.hex)
                } label: {
                    Label { Text(color.name) } icon: { Image(nsImage: Self.swatch(color.hex)) }
                }
            }
        } label: {
            Label(title, systemImage: symbol)
        }
        .help(title)
        .menuIndicator(.hidden)
    }

    /// A small square of `hex` for the menu.
    static func swatch(_ hex: String) -> NSImage {
        let value = Int(hex.dropFirst(), radix: 16) ?? 0
        let color = NSColor(srgbRed: CGFloat((value >> 16) & 0xFF) / 255, green: CGFloat((value >> 8) & 0xFF) / 255, blue: CGFloat(value & 0xFF) / 255, alpha: 1)
        return NSImage(size: NSSize(width: 14, height: 14), flipped: false) { rect in
            color.setFill()
            NSBezierPath(roundedRect: rect.insetBy(dx: 0.5, dy: 0.5), xRadius: 3, yRadius: 3).fill()
            NSColor.separatorColor.setStroke()
            NSBezierPath(roundedRect: rect.insetBy(dx: 0.5, dy: 0.5), xRadius: 3, yRadius: 3).stroke()
            return true
        }
    }
}
