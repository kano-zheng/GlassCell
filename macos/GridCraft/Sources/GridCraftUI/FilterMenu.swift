import AppKit
import GridCraftEngine
import SwiftUI

/// A header's filter dropdown (its ▾ button): sort the column, or choose the values it shows.
/// It opens in a popover, which macOS draws in Liquid Glass.
struct FilterMenuView: View {
    @ObservedObject var document: SpreadsheetDocument
    let menu: FilterMenu
    let close: () -> Void
    @State private var shown: [String: Bool]
    @State private var search = ""
    @FocusState private var searchFocused: Bool

    init(document: SpreadsheetDocument, menu: FilterMenu, close: @escaping () -> Void) {
        self.document = document
        self.menu = menu
        self.close = close
        _shown = State(initialValue: Dictionary(menu.values.map { ($0.text, $0.shown) }, uniquingKeysWith: { a, _ in a }))
    }

    /// The values the search matches (all of them without a search).
    private var matches: [String] {
        let needle = search.trimmingCharacters(in: .whitespaces).lowercased()
        return menu.values.map(\.text).filter { needle.isEmpty || $0.lowercased().contains(needle) }
    }

    /// What Apply shows: the checked values the search matches.
    private var applied: [String] {
        matches.filter { shown[$0] == true }
    }

    var body: some View {
        let matches = matches
        VStack(alignment: .leading, spacing: 10) {
            VStack(alignment: .leading, spacing: 2) {
                menuButton("Sort Ascending", "arrow.up") { run("data.sortAscending", ["column": menu.column]) }
                menuButton("Sort Descending", "arrow.down") { run("data.sortDescending", ["column": menu.column]) }
            }
            Divider()
            Text("Filter").font(.headline)
            TextField("Search", text: $search)
                .textFieldStyle(.roundedBorder)
                .focused($searchFocused)
                .onSubmit { if !applied.isEmpty { apply() } }
            Toggle(search.isEmpty ? "(Select All)" : "(Select All Search Results)", isOn: Binding(
                get: { matches.allSatisfy { shown[$0] == true } },
                set: { on in for value in matches { shown[value] = on } }
            ))
            .toggleStyle(.checkbox)
            ScrollView {
                VStack(alignment: .leading, spacing: 4) {
                    ForEach(matches, id: \.self) { value in
                        Toggle(value, isOn: Binding(get: { shown[value] == true }, set: { shown[value] = $0 }))
                            .toggleStyle(.checkbox)
                            .lineLimit(1)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.leading, 18)
            }
            .frame(height: min(CGFloat(matches.count) * 22 + 4, 220))
            Divider()
            HStack {
                Button("Clear Filter") { run("data.filterBy", ["column": menu.column, "clear": true, "range": menu.range]) }
                Spacer()
                Button("Apply", action: apply)
                    .buttonStyle(.glassProminent)
                    .keyboardShortcut(.defaultAction)
                    // Nothing checked would hide every row.
                    .disabled(applied.isEmpty)
            }
        }
        .padding(14)
        .frame(width: 260)
        .onAppear { searchFocused = true }
    }

    private func menuButton(_ title: LocalizedStringKey, _ symbol: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Label(title, systemImage: symbol)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, 6)
                .frame(height: 24)
                .contentShape(.rect)
        }
        .buttonStyle(.plain)
    }

    private func apply() {
        if let params = Self.filterParams(menu, showing: applied) { run("data.filterBy", params) }
    }

    /// `data.filterBy`'s params to show `values` of the menu's column: every value clears the
    /// column's filter; none is no filter at all (it would hide every row).
    static func filterParams(_ menu: FilterMenu, showing values: [String]) -> [String: Any]? {
        guard !values.isEmpty else { return nil }
        if Set(values).isSuperset(of: menu.values.map(\.text)) {
            return ["column": menu.column, "clear": true, "range": menu.range]
        }
        return ["column": menu.column, "values": values.filter { $0 != "(Blanks)" }, "blanks": values.contains("(Blanks)"), "range": menu.range]
    }

    private func run(_ id: String, _ params: [String: Any]) {
        close()
        document.run(id, params)
    }
}

extension GridNSView {
    /// Opens the filter dropdown of header `cell` under its button at `rect` (view coordinates).
    func showFilterMenu(for cell: CellAddress, at rect: CGRect) {
        guard let document, let menu = try? document.engine.filterMenu(cell: cell) else { return }
        let popover = NSPopover()
        popover.behavior = .transient
        let close = { [weak popover] in popover?.performClose(nil) ?? () }
        popover.contentViewController = NSHostingController(rootView: FilterMenuView(document: document, menu: menu, close: close))
        popover.show(relativeTo: rect, of: self, preferredEdge: .maxY)
    }
}
