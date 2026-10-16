import GridCraftEngine
import SwiftUI

/// ⇧⌘P: every command of the catalog by name, run with Return.
struct CommandPalette: View {
    @ObservedObject var document: SpreadsheetDocument
    let editor: EditController
    @Environment(\.dismiss) private var dismiss
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
        VStack(spacing: 0) {
            TextField("Search commands", text: $query)
                .textFieldStyle(.plain)
                .font(.title3)
                .padding(12)
                .focused($searchFocused)
                .onSubmit { run(selection ?? matches.first(where: \.enabled)?.id) }
            Divider()
            List(matches, id: \.id, selection: $selection) { info in
                HStack {
                    Text(info.label)
                    Spacer()
                    Text(info.menu.joined(separator: " ▸ "))
                        .foregroundStyle(.secondary)
                        .font(.caption)
                    if let shortcut = MacPlacement.shortcut(for: info) {
                        Text(describe(shortcut))
                            .foregroundStyle(.secondary)
                            .font(.caption.monospaced())
                    }
                }
                .opacity(info.enabled ? 1 : 0.4)
                .tag(info.id)
            }
            .listStyle(.plain)
            .onKeyPress(.return) {
                run(selection)
                return .handled
            }
        }
        .frame(width: 560, height: 420)
        .onAppear { searchFocused = true }
        .onExitCommand { dismiss() }
    }

    private func run(_ id: String?) {
        guard let id, matches.first(where: { $0.id == id })?.enabled == true else { return }
        dismiss()
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
