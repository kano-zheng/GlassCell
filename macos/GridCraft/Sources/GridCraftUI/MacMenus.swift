import AppKit
import GridCraftEngine
import SwiftUI

/// Where the engine's commands go in the Mac menu bar. The engine's catalog keeps Excel's ribbon
/// paths (`["Home", "Font"]`); this table places each tab and group in a Mac menu and section.
public enum MacPlacement {
    public enum Menu: String, CaseIterable, Sendable {
        case file = "File", edit = "Edit", view = "View", insert = "Insert", format = "Format"
        case formulas = "Formulas", data = "Data", review = "Review", tools = "Tools", window = "Window"
    }

    /// (menu, section title) by "Tab/Group", then by "Tab".
    static let byGroup: [String: (Menu, String)] = [
        "File/Export": (.file, "Export"),
        "Page Layout/Page Setup": (.file, "Page Setup"),
        "Page Layout/Scale to Fit": (.file, "Page Setup"),
        "Page Layout/Sheet Options": (.view, "Sheet"),
        "Page Layout/Themes": (.format, "Themes"),
        "Page Layout/Arrange": (.format, "Arrange"),
        "Home/Clipboard": (.edit, "Clipboard"),
        "Home/Editing": (.edit, "Editing"),
        "Home/Font": (.format, "Font"),
        "Home/Alignment": (.format, "Alignment"),
        "Home/Number": (.format, "Number"),
        "Home/Styles": (.format, "Styles"),
        "Home/Cells": (.format, "Cells"),
        "Home/Add-ins": (.tools, "Add-ins"),
        "Table/Table Style Options": (.data, "Table"),
        "Table/Table Styles": (.data, "Table"),
        "Table/Properties": (.data, "Table"),
        "Table/Tools": (.data, "Table"),
        "Table Design/Properties": (.data, "Table"),
        "Table Design/Tools": (.data, "Table"),
        "Chart Design/": (.format, "Chart"),
        "Chart Design/Data": (.format, "Chart"),
        "Chart Design/Chart Layouts": (.format, "Chart"),
        "Chart Design/Chart Styles": (.format, "Chart"),
        "Chart Design/Type": (.format, "Chart"),
        "Chart Design/Location": (.format, "Chart"),
        "Format/Current Selection": (.format, "Chart"),
        "Format/Shape Styles": (.format, "Shape"),
        "Draw/Tools": (.insert, "Draw"),
        "Draw/Convert": (.insert, "Draw"),
        "Automate/Scripting Tools": (.tools, "Scripts"),
        "View/Macros": (.tools, "Macros"),
        "Data/Analysis": (.tools, "Analysis"),
        "Data/Forecast": (.tools, "What-If"),
    ]

    static let byTab: [String: Menu] = [
        "File": .file, "Edit": .edit, "Window": .window, "Insert": .insert, "Data": .data, "Formulas": .formulas,
        "Review": .review, "View": .view, "PivotTable Analyze": .data, "PivotTable Design": .data, "Chart Design": .format,
    ]

    /// Commands the standard Mac menus already have (New, Open, Save, Undo, Cut, Copy, Paste,
    /// Select All, Delete), or that a key press in the grid does.
    static let system: Set<String> = [
        "file.new", "file.open", "file.save", "file.saveAs", "file.close", "edit.undo", "edit.redo", "edit.copy", "edit.cut",
        "edit.paste", "edit.selectAll", "edit.clearContents", "window.activate",
    ]

    /// Mac shortcuts that differ from the catalog's (`nil`: none; the system has the key).
    static let shortcutOverrides: [String: String?] = [
        "edit.redo": nil,
        "file.close": nil,
    ]

    /// The menu and section for a catalog entry, or `nil` when it isn't in a menu.
    public static func place(_ info: CommandInfo) -> (Menu, String)? {
        guard !system.contains(info.id), let tab = info.menu.first else { return nil }
        let group = info.menu.count > 1 ? info.menu[1] : ""
        if let placed = byGroup["\(tab)/\(group)"] {
            return placed
        }
        if tab == "Home", group == "Cells" {
            return (.format, "Cells")
        }
        guard let menu = byTab[tab] else { return nil }
        return (menu, group)
    }

    /// The catalog's shortcut as a Mac key equivalent (`Cmd` is ⌘, `Ctrl` is ⌃, `Alt` is ⌥).
    public static func shortcut(for info: CommandInfo) -> KeyboardShortcut? {
        let spec: String?
        if let override = shortcutOverrides[info.id] {
            spec = override
        } else {
            spec = info.shortcut
        }
        guard let spec else { return nil }
        var parts = spec.split(separator: "+", omittingEmptySubsequences: false).map(String.init)
        // "Cmd+Shift++" style: an empty last part is the "+" key.
        if spec.hasSuffix("++") {
            parts.removeLast(2)
            parts.append("+")
        }
        guard let key = parts.popLast() else { return nil }
        var modifiers: EventModifiers = []
        for part in parts {
            switch part {
            case "Cmd": modifiers.insert(.command)
            case "Ctrl": modifiers.insert(.control)
            case "Alt": modifiers.insert(.option)
            case "Shift": modifiers.insert(.shift)
            default: return nil
            }
        }
        // Unmodified keys stay with the grid (Escape, Delete…), except the function keys.
        guard !modifiers.isEmpty || (key.count > 1 && key.first == "F" && Int(key.dropFirst()) != nil) else { return nil }
        let equivalent: KeyEquivalent
        switch key {
        case "PageDown": equivalent = .pageDown
        case "PageUp": equivalent = .pageUp
        case "Enter": equivalent = .return
        case "Space": equivalent = .space
        case "Tab": equivalent = .tab
        case "Left": equivalent = .leftArrow
        case "Right": equivalent = .rightArrow
        case "Up": equivalent = .upArrow
        case "Down": equivalent = .downArrow
        default:
            if key.count > 1, key.first == "F", let n = Int(key.dropFirst()), (1...19).contains(n),
               let scalar = UnicodeScalar(NSF1FunctionKey + n - 1) {
                equivalent = KeyEquivalent(Character(scalar))
            } else if key.count == 1, let c = key.lowercased().first {
                equivalent = KeyEquivalent(c)
            } else {
                return nil
            }
        }
        return KeyboardShortcut(equivalent, modifiers: modifiers)
    }

    /// The menu sections for `menu`, in catalog order.
    public static func sections(_ menu: Menu, in catalog: [CommandInfo]) -> [(title: String, commands: [CommandInfo])] {
        var order: [String] = []
        var groups: [String: [CommandInfo]] = [:]
        for info in catalog {
            guard let (m, section) = place(info), m == menu else { continue }
            if groups[section] == nil { order.append(section) }
            groups[section, default: []].append(info)
        }
        return order.map { ($0, groups[$0] ?? []) }
    }
}

/// The workbook in the key window, for the menu bar.
public struct WorkbookFocus {
    public let document: SpreadsheetDocument
    public let editor: EditController
    /// The document's generation when this was set: a new value makes the menus re-read the
    /// enabled states.
    public let generation: Int
}

extension FocusedValues {
    @Entry public var workbook: WorkbookFocus?
}

/// The menu bar: the engine's catalog placed by `MacPlacement`, each item enabled as the engine says.
public struct WorkbookCommands: Commands {
    @FocusedValue(\.workbook) private var focus

    public init() {}

    public var body: some Commands {
        // The engine's Print and Page Setup replace the standard ones.
        CommandGroup(replacing: .printItem) { MenuItems(menu: .file, focus: focus) }
        CommandGroup(after: .pasteboard) { MenuItems(menu: .edit, focus: focus) }
        CommandGroup(after: .toolbar) { MenuItems(menu: .view, focus: focus) }
        CommandMenu("Insert") { MenuItems(menu: .insert, focus: focus) }
        CommandMenu("Format") { MenuItems(menu: .format, focus: focus) }
        CommandMenu("Formulas") { MenuItems(menu: .formulas, focus: focus) }
        CommandMenu("Data") { MenuItems(menu: .data, focus: focus) }
        CommandMenu("Review") { MenuItems(menu: .review, focus: focus) }
        CommandMenu("Tools") {
            Button("Command Palette…") { focus?.document.showPalette = true }
                .keyboardShortcut("p", modifiers: [.command, .shift])
                .disabled(focus == nil)
            Divider()
            MenuItems(menu: .tools, focus: focus)
        }
        CommandGroup(before: .windowArrangement) { MenuItems(menu: .window, focus: focus) }
    }
}

/// One menu's sections.
struct MenuItems: View {
    let menu: MacPlacement.Menu
    let focus: WorkbookFocus?

    private var catalog: [CommandInfo] {
        // Re-read on every change to the workbook: enabled states follow it.
        _ = focus?.document.generation
        if let document = focus?.document, let commands = try? document.engine.commands() {
            return commands
        }
        return Self.offline
    }

    /// The catalog without a workbook (everything disabled).
    private static let offline: [CommandInfo] = (try? Engine().commands()) ?? []

    var body: some View {
        let sections = MacPlacement.sections(menu, in: catalog)
        ForEach(sections, id: \.title) { section in
            Section(section.title) {
                ForEach(section.commands, id: \.id) { info in
                    Button(info.label) {
                        guard let focus, focus.editor.commitIfEditing() else { return }
                        focus.document.run(info.id)
                    }
                    .keyboardShortcut(MacPlacement.shortcut(for: info))
                    .disabled(focus == nil || !info.enabled)
                }
            }
        }
    }
}
