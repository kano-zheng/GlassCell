import AppKit
import GridCraftEngine
import SwiftUI

/// ⇧⌘P: every command of the catalog by name, run with Return. A Liquid Glass panel floating
/// over the top of the window, like Spotlight: type to filter, ↑↓ to choose, Esc or a click
/// outside to close.
struct CommandPalette: View {
    @ObservedObject var document: SpreadsheetDocument
    let editor: EditController
    @State private var query = ""
    @State private var selection: String?
    @FocusState private var searchFocused: Bool

    private var matches: [CommandInfo] {
        let all = ((try? document.engine.commands()) ?? []).filter { !$0.menu.isEmpty && !MacPlacement.system.contains($0.id) }
        let words = query.lowercased().split(separator: " ")
        guard !words.isEmpty else { return all }
        return all.filter { info in
            let haystack = (info.label + " " + info.menu.joined(separator: " ") + " " + info.id).lowercased()
            return words.allSatisfy { haystack.contains($0) }
        }
    }

    var body: some View {
        let matches = matches
        // Return runs the chosen command, or the first one that can run.
        let chosen = selection ?? matches.first(where: \.enabled)?.id
        VStack(spacing: 0) {
            HStack(spacing: 10) {
                Image(systemName: "magnifyingglass")
                    .foregroundStyle(.secondary)
                TextField("Search commands", text: $query)
                    .textFieldStyle(.plain)
                    .focused($searchFocused)
                    .onSubmit { run(chosen, in: matches) }
                    .onKeyPress(.return) {
                        guard !composing else { return .ignored }
                        run(chosen, in: matches)
                        return .handled
                    }
                    .onKeyPress(.downArrow) { composing ? .ignored : step(1, in: matches) }
                    .onKeyPress(.upArrow) { composing ? .ignored : step(-1, in: matches) }
                    .onChange(of: query) { selection = nil }
            }
            .font(.title2)
            .padding(.horizontal, 18)
            .frame(height: 52)
            if !matches.isEmpty {
                Divider()
                ScrollViewReader { proxy in
                    ScrollView {
                        LazyVStack(spacing: 0) {
                            ForEach(matches, id: \.id) { info in
                                row(info, chosen: info.id == chosen)
                                    .id(info.id)
                                    .onTapGesture { run(info.id, in: matches) }
                            }
                        }
                        .padding(8)
                    }
                    .frame(height: min(CGFloat(matches.count) * Self.rowHeight + 16, 360))
                    .onChange(of: selection) { _, id in
                        if let id { proxy.scrollTo(id) }
                    }
                }
            }
        }
        .frame(width: 600)
        .glassEffect(.regular, in: .rect(cornerRadius: 26))
        .onAppear { searchFocused = true }
        .onExitCommand { close() }
        .onKeyPress(.escape) {
            close()
            return .handled
        }
    }

    private static let rowHeight: CGFloat = 32

    /// A command; the chosen one is highlighted in the accent colour, as in Spotlight.
    private func row(_ info: CommandInfo, chosen: Bool) -> some View {
        HStack {
            Text(LocalizedStringKey(info.label))
            Spacer()
            Text(info.menu.joined(separator: " ▸ "))
                .foregroundStyle(chosen ? AnyShapeStyle(.white.opacity(0.8)) : AnyShapeStyle(.secondary))
                .font(.caption)
            if let shortcut = MacPlacement.shortcut(for: info) {
                Text(describe(shortcut))
                    .foregroundStyle(chosen ? AnyShapeStyle(.white.opacity(0.8)) : AnyShapeStyle(.secondary))
                    .font(.caption.monospaced())
            }
        }
        .padding(.horizontal, 10)
        .frame(height: Self.rowHeight)
        .foregroundStyle(chosen ? AnyShapeStyle(.white) : AnyShapeStyle(.primary))
        .background(chosen ? AnyShapeStyle(.tint) : AnyShapeStyle(.clear), in: .rect(cornerRadius: 10))
        .contentShape(.rect)
        .opacity(info.enabled ? 1 : 0.4)
    }

    /// An input method is composing in the search field (its Return and arrows are its own).
    private var composing: Bool {
        (editor.grid?.window?.firstResponder as? NSTextView)?.hasMarkedText() == true
    }

    /// ↑ / ↓ from the search field move the chosen command.
    private func step(_ by: Int, in matches: [CommandInfo]) -> KeyPress.Result {
        guard !matches.isEmpty else { return .handled }
        let chosen = selection ?? matches.first(where: \.enabled)?.id
        let current = chosen.flatMap { id in matches.firstIndex { $0.id == id } } ?? (by > 0 ? -1 : matches.count)
        let next = min(max(current + by, 0), matches.count - 1)
        selection = matches[next].id
        return .handled
    }

    private func close() {
        document.showPalette = false
        if let grid = editor.grid { grid.window?.makeFirstResponder(grid) }
    }

    private func run(_ id: String?, in matches: [CommandInfo]) {
        guard let id, matches.first(where: { $0.id == id })?.enabled == true else { return }
        close()
        guard editor.commitIfEditing() else { return }
        if MacActions.handled.contains(id) {
            MacActions.run(id, document: document)
        } else {
            document.run(id)
        }
    }

    private func describe(_ s: KeyboardShortcut) -> String {
        var out = ""
        if s.modifiers.contains(.control) { out += "⌃" }
        if s.modifiers.contains(.option) { out += "⌥" }
        if s.modifiers.contains(.shift) { out += "⇧" }
        if s.modifiers.contains(.command) { out += "⌘" }
        return out + String(s.key.character).uppercased()
    }
}
