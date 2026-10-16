import GridCraftEngine
@testable import GridCraftUI
import SwiftUI
import Testing

@MainActor
@Suite struct MenuTests {
    private let catalog: [CommandInfo] = {
        let engine = Engine()
        _ = try? engine.execute(id: "file.new", params: "")
        return (try? engine.commands()) ?? []
    }()

    @Test func everyRibbonCommandHasAMacPlace() {
        let unplaced = catalog.filter { !$0.menu.isEmpty && !MacPlacement.system.contains($0.id) && MacPlacement.place($0) == nil }
        #expect(unplaced.map(\.id) == [])
    }

    @Test func shortcutsAreUniqueAndLeaveTheSystemsAlone() {
        var seen: [String: String] = [:]
        let system: Set<String> = ["⌘n", "⌘o", "⌘s", "⌘w", "⌘z", "⇧⌘z", "⌘c", "⌘x", "⌘v", "⌘a", "⌘q", "⌘h", "⌘m", "⌘,", "⇧⌘p", "⌥⌘i"]
        for info in catalog where MacPlacement.place(info) != nil {
            guard let s = MacPlacement.shortcut(for: info) else { continue }
            var key = ""
            if s.modifiers.contains(.control) { key += "⌃" }
            if s.modifiers.contains(.option) { key += "⌥" }
            if s.modifiers.contains(.shift) { key += "⇧" }
            if s.modifiers.contains(.command) { key += "⌘" }
            key += String(s.key.character).lowercased()
            #expect(!system.contains(key), "\(info.id) takes the system's \(key)")
            if let other = seen[key] {
                Issue.record("\(info.id) and \(other) share \(key)")
            }
            seen[key] = info.id
        }
        #expect(seen.count > 20)
    }

    @Test func catalogShortcutsParse() {
        let bold = catalog.first { $0.id == "home.bold" }
        #expect(bold.flatMap(MacPlacement.shortcut(for:)) == KeyboardShortcut("b", modifiers: .command))
        let filter = catalog.first { $0.id == "data.filter" }
        #expect(filter.flatMap(MacPlacement.shortcut(for:)) == KeyboardShortcut("l", modifiers: [.control, .shift]))
        #expect(MacPlacement.sections(.format, in: catalog).contains { $0.title == "Font" })
    }
}
