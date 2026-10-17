import AppKit
import GridCraftEngine
import SwiftUI

/// The inspector beside the grid: what Excel's Format Cells dialog and the Home tab's groups
/// edit, for the selection (Cell), and the sheet's view and page settings (Sheet). Values come
/// from the engine's `selection_format`, where `nil` means the selected cells differ ("Mixed");
/// every change is an engine command.
struct InspectorView: View {
    enum Pane: String, CaseIterable {
        case cell = "Cell", sheet = "Sheet"
    }

    @ObservedObject var document: SpreadsheetDocument
    let editor: EditController
    @State private var pane = Pane.cell

    var body: some View {
        VStack(spacing: 0) {
            Picker("Pane", selection: $pane) {
                ForEach(Pane.allCases, id: \.self) { Text($0.rawValue).tag($0) }
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            .padding(10)
            Divider()
            switch pane {
            case .cell: CellInspector(document: document, editor: editor)
            case .sheet: SheetInspector(document: document, editor: editor)
            }
        }
    }
}

/// Runs inspector changes: an edit in progress is confirmed first.
@MainActor
private func apply(_ document: SpreadsheetDocument, _ editor: EditController, _ id: String, _ params: [String: Any] = [:]) {
    guard editor.commitIfEditing() else { return }
    document.run(id, params)
}

/// Applies part of a style to the selection (`home.formatCells` merges it in).
@MainActor
private func style(_ document: SpreadsheetDocument, _ editor: EditController, _ partial: [String: Any]) {
    apply(document, editor, "home.formatCells", ["style": partial])
}

/// A three-state toggle: on, off, or mixed (shown as a dash).
private struct MixedToggle: View {
    let title: String
    let value: Bool?
    let set: @MainActor @Sendable (Bool) -> Void

    var body: some View {
        // A closure, not `set` itself: converting the isolated function crashed Swift 6.3's IRGen.
        Toggle(isOn: Binding(get: { value == true }, set: { set($0) })) { Text(title) }
            .toggleStyle(.checkbox)
            .overlay(alignment: .leading) {
                if value == nil {
                    Text("–").font(.caption.bold()).offset(x: 4)
                }
            }
    }
}

struct CellInspector: View {
    @ObservedObject var document: SpreadsheetDocument
    let editor: EditController
    @State private var code = ""

    private var format: SelectionFormat? {
        _ = document.generation
        return try? document.engine.selectionFormat()
    }

    private static let families = NSFontManager.shared.availableFontFamilies
    private static let horizontal: [(HAlign, String, String)] = [
        (.general, "General", "General"), (.left, "Left", "Left"), (.center, "Center", "Center"), (.right, "Right", "Right"),
        (.fill, "Fill", "Fill"), (.justify, "Justify", "Justify"), (.centerAcross, "Center Across Selection", "CenterAcross"),
        (.distributed, "Distributed", "Distributed"),
    ]
    private static let vertical: [(VAlign, String, String)] = [
        (.top, "Top", "Top"), (.center, "Center", "Center"), (.bottom, "Bottom", "Bottom"), (.justify, "Justify", "Justify"),
        (.distributed, "Distributed", "Distributed"),
    ]

    var body: some View {
        let shared = format?.selection
        Form {
            Section("Font") {
                let name = shared?.fontName ?? ""
                Picker("Font", selection: Binding(get: { name }, set: { style(document, editor, ["font": ["name": $0]]) })) {
                    if name.isEmpty { Text("Mixed").tag("") }
                    if !name.isEmpty, !Self.families.contains(name) { Text(name).tag(name) }
                    ForEach(Self.families, id: \.self) { Text($0).tag($0) }
                }
                LabeledContent("Size") {
                    TextField("Size", value: Binding(get: { shared?.fontSize.map(Double.init) }, set: { if let size = $0 { apply(document, editor, "home.fontSize", ["size": size]) } }),
                              format: .number, prompt: Text("Mixed"))
                        .labelsHidden()
                        .multilineTextAlignment(.trailing)
                        .frame(width: 70)
                }
                HStack {
                    MixedToggle(title: "Bold", value: shared?.bold) { apply(document, editor, "home.bold", ["on": $0]) }
                    MixedToggle(title: "Italic", value: shared?.italic) { apply(document, editor, "home.italic", ["on": $0]) }
                    MixedToggle(title: "Strikethrough", value: shared?.strike) { apply(document, editor, "home.strikethrough", ["on": $0]) }
                }
                Picker("Underline", selection: Binding(get: { shared?.underline }, set: { u in
                    apply(document, editor, "home.underline", ["on": u != Underline.none, "style": Self.underlineName(u ?? .none)])
                })) {
                    if shared?.underline == nil { Text("Mixed").tag(Underline?.none) }
                    Text("None").tag(Underline?.some(.none))
                    Text("Single").tag(Underline?.some(.single))
                    Text("Double").tag(Underline?.some(.double))
                    Text("Single Accounting").tag(Underline?.some(.singleAccounting))
                    Text("Double Accounting").tag(Underline?.some(.doubleAccounting))
                }
                LabeledContent("Color") {
                    ColorMenu(title: "Font Color", symbol: "character", automatic: "Automatic") { apply(document, editor, "home.fontColor", ["color": $0 ?? "auto"]) }
                }
            }
            Section("Alignment") {
                Picker("Horizontal", selection: Binding(get: { shared?.halign }, set: { h in
                    if let name = Self.horizontal.first(where: { $0.0 == h })?.2 { style(document, editor, ["align": ["h": name]]) }
                })) {
                    if shared?.halign == nil { Text("Mixed").tag(HAlign?.none) }
                    ForEach(Self.horizontal, id: \.2) { Text($0.1).tag(HAlign?.some($0.0)) }
                }
                Picker("Vertical", selection: Binding(get: { shared?.valign }, set: { v in
                    if let name = Self.vertical.first(where: { $0.0 == v })?.2 { style(document, editor, ["align": ["v": name]]) }
                })) {
                    if shared?.valign == nil { Text("Mixed").tag(VAlign?.none) }
                    ForEach(Self.vertical, id: \.2) { Text($0.1).tag(VAlign?.some($0.0)) }
                }
                LabeledContent("Indent") {
                    Stepper(value: Binding(get: { Int(shared?.indent ?? 0) }, set: { style(document, editor, ["align": ["indent": max(0, min(15, $0))]]) }), in: 0...15) {
                        Text(shared?.indent.map { "\($0)" } ?? "Mixed").monospacedDigit()
                    }
                }
                LabeledContent("Rotation") {
                    Stepper(value: Binding(get: { Int(shared?.rotation ?? 0) }, set: { apply(document, editor, "home.orientation", ["angle": max(-90, min(90, $0))]) }), in: -90...90, step: 15) {
                        Text(shared?.rotation.map { "\($0)°" } ?? "Mixed").monospacedDigit()
                    }
                }
                HStack {
                    MixedToggle(title: "Wrap Text", value: shared?.wrap) { apply(document, editor, "home.wrapText", ["on": $0]) }
                    MixedToggle(title: "Shrink to Fit", value: shared?.shrink) { apply(document, editor, "home.shrinkToFit", ["on": $0]) }
                }
                HStack {
                    Button("Merge & Center") { apply(document, editor, "home.mergeCenter") }
                    Button("Unmerge") { apply(document, editor, "home.unmergeCells") }
                }
            }
            Section("Number") {
                Picker("Category", selection: Binding(get: { shared?.numberKind ?? "" }, set: { kind in
                    if let name = Self.categoryFormat[kind] { apply(document, editor, "home.numberFormat", ["format": name]) }
                })) {
                    if shared?.numberKind == nil { Text("Mixed").tag("") }
                    ForEach(Self.categories, id: \.self) { Text($0).tag($0) }
                    if let kind = shared?.numberKind, !Self.categories.contains(kind) { Text(kind).tag(kind) }
                }
                TextField("Format Code", text: $code, prompt: Text(shared?.numberFormat ?? "Mixed"))
                    .onSubmit {
                        if !code.isEmpty { apply(document, editor, "home.numberFormat", ["code": code]) }
                    }
                    .onAppear { code = shared?.numberFormat ?? "" }
                    .onChange(of: shared?.numberFormat) { _, new in code = new ?? "" }
                HStack {
                    Button("Increase Decimal") { apply(document, editor, "home.increaseDecimal") }
                    Button("Decrease Decimal") { apply(document, editor, "home.decreaseDecimal") }
                }
            }
            Section("Border") {
                LazyVGrid(columns: Array(repeating: GridItem(.flexible()), count: 2), alignment: .leading) {
                    ForEach(WorkbookToolbar.borderPresets, id: \.preset) { item in
                        Button(item.label) { apply(document, editor, "home.borders", ["preset": item.preset]) }
                            .buttonStyle(.link)
                    }
                }
                if let borders = format?.borders {
                    LabeledContent("Outline") {
                        Text([("Top", borders.top), ("Bottom", borders.bottom), ("Left", borders.left), ("Right", borders.right)]
                            .map { "\($0.0): \(Self.describe($0.1))" }.joined(separator: "  "))
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }
            }
            Section("Fill") {
                LabeledContent("Color") {
                    ColorMenu(title: "Fill Color", symbol: "drop.halffull", automatic: "No Fill") { apply(document, editor, "home.fillColor", ["color": $0 ?? "none"]) }
                }
            }
            Section("Protection") {
                MixedToggle(title: "Locked", value: shared?.locked) { style(document, editor, ["protection": ["locked": $0]]) }
                MixedToggle(title: "Hidden", value: shared?.hidden) { style(document, editor, ["protection": ["hidden": $0]]) }
                Text("Locking and hiding take effect when the sheet is protected (Review ▸ Protect Sheet).")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .formStyle(.grouped)
        // The panel's glass shows through.
        .scrollContentBackground(.hidden)
    }

    static let categories = ["General", "Number", "Currency", "Accounting", "Date", "Time", "Percentage", "Fraction", "Scientific", "Text"]
    /// Category → the `home.numberFormat` preset that sets it.
    static let categoryFormat = ["General": "General", "Number": "Number", "Currency": "Currency", "Accounting": "Accounting", "Date": "Short Date",
                                 "Time": "Time", "Percentage": "Percentage", "Fraction": "Fraction", "Scientific": "Scientific", "Text": "Text"]

    static func underlineName(_ u: Underline) -> String {
        switch u {
        case .none, .single: "single"
        case .double: "double"
        case .singleAccounting: "singleAccounting"
        case .doubleAccounting: "doubleAccounting"
        }
    }

    static func describe(_ line: BorderLine?) -> String {
        guard let line else { return "Mixed" }
        return line.style == .none ? "None" : "\(line.style)".capitalized
    }
}

struct SheetInspector: View {
    @ObservedObject var document: SpreadsheetDocument
    let editor: EditController

    private var sheet: SheetInfo? {
        _ = document.generation
        return try? document.engine.sheet()
    }

    var body: some View {
        Form {
            if let sheet {
                Section("View") {
                    Toggle("Gridlines", isOn: Binding(get: { sheet.showGridlines }, set: { apply(document, editor, "view.gridlines", ["on": $0]) }))
                    Toggle("Headings", isOn: Binding(get: { sheet.showHeadings }, set: { apply(document, editor, "view.headings", ["on": $0]) }))
                    Toggle("Right to Left", isOn: Binding(get: { sheet.rightToLeft }, set: { apply(document, editor, "view.rightToLeft", ["on": $0]) }))
                    LabeledContent("Zoom") {
                        Slider(value: Binding(get: { Double(sheet.zoom) }, set: { apply(document, editor, "view.zoom", ["percent": Int($0.rounded())]) }), in: 10...400, step: 5) {
                            Text("Zoom")
                        } minimumValueLabel: {
                            Text("10%")
                        } maximumValueLabel: {
                            Text("\(sheet.zoom)%").monospacedDigit()
                        }
                        .labelsHidden()
                    }
                }
                Section("Freeze Panes") {
                    if sheet.frozen.row > 0 || sheet.frozen.col > 0 {
                        LabeledContent("Frozen", value: "\(sheet.frozen.row) rows, \(sheet.frozen.col) columns")
                        Button("Unfreeze Panes") { apply(document, editor, "view.unfreezePanes") }
                    } else {
                        Button("Freeze at Selection") { apply(document, editor, "view.freezePanes") }
                        Button("Freeze Top Row") { apply(document, editor, "view.freezeTopRow") }
                        Button("Freeze First Column") { apply(document, editor, "view.freezeFirstColumn") }
                    }
                }
                Section("Page Setup") {
                    Picker("Orientation", selection: Binding(get: { sheet.landscape ? "landscape" : "portrait" }, set: { apply(document, editor, "pageLayout.orientation", ["orientation": $0]) })) {
                        Text("Portrait").tag("portrait")
                        Text("Landscape").tag("landscape")
                    }
                    Toggle("Print Gridlines", isOn: Binding(get: { sheet.printGridlines }, set: { apply(document, editor, "pageLayout.printGridlines", ["on": $0]) }))
                    Toggle("Print Headings", isOn: Binding(get: { sheet.printHeadings }, set: { apply(document, editor, "pageLayout.printHeadings", ["on": $0]) }))
                }
                Section("Tab") {
                    LabeledContent("Tab Color") {
                        ColorMenu(title: "Tab Color", symbol: "paintpalette", automatic: "No Color") { apply(document, editor, "sheet.tabColor", ["color": $0 ?? "none"]) }
                    }
                }
            }
        }
        .formStyle(.grouped)
        // The panel's glass shows through.
        .scrollContentBackground(.hidden)
    }
}
