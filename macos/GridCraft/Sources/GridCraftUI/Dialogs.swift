import AppKit
import GridCraftEngine
import SwiftUI
import UniformTypeIdentifiers

/// A dialog a command asked for that isn't a plain form: drawn here, acting through commands.
struct DialogRequest: Identifiable {
    let id = UUID()
    let name: String
    let params: [String: Any]

    /// The dialogs the Mac draws itself (forms go to `FormSheet`).
    static let handled: Set<String> = [
        "find", "goTo", "sort", "nameManager", "insertFunction", "goalSeek", "manageRules", "statistics", "accessibility",
        "errorChecking", "evaluateFormula", "spelling", "macros", "about", "agents",
    ]
}

struct CustomDialog: View {
    let request: DialogRequest
    @ObservedObject var document: SpreadsheetDocument
    let editor: EditController
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        Group {
            switch request.name {
            case "find": FindReplaceDialog(document: document, replace: request.params["replace"] as? Bool == true)
            case "goTo": GoToDialog(document: document)
            case "sort": SortDialog(document: document)
            case "nameManager": NameManagerDialog(document: document)
            case "insertFunction": InsertFunctionDialog(document: document, editor: editor)
            case "goalSeek": GoalSeekDialog(document: document)
            case "manageRules": RulesDialog(document: document)
            case "about", "agents": AboutDialog(agents: request.name == "agents")
            default: ResultDialog(document: document, name: request.name)
            }
        }
        .padding(20)
    }
}

/// The buttons along a dialog's bottom.
private struct DialogButtons: View {
    var ok = "OK"
    var cancel = "Cancel"
    var okDisabled = false
    let onOK: () -> Void
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        HStack {
            Spacer()
            Button(cancel, role: .cancel) { dismiss() }
                .keyboardShortcut(.cancelAction)
            Button(ok, action: onOK)
                .keyboardShortcut(.defaultAction)
                .disabled(okDisabled)
        }
    }
}

// MARK: - Find and Replace

struct FindReplaceDialog: View {
    @ObservedObject var document: SpreadsheetDocument
    @State var replace: Bool
    @State private var what = ""
    @State private var with = ""
    @State private var matchCase = false
    @State private var wholeCell = false
    @State private var workbook = false
    @State private var found: [[String: Any]] = []
    @State private var status = ""
    @Environment(\.dismiss) private var dismiss

    init(document: SpreadsheetDocument, replace: Bool) {
        self.document = document
        _replace = State(initialValue: replace)
    }

    private var base: [String: Any] {
        ["what": what, "matchCase": matchCase, "wholeCell": wholeCell, "within": workbook ? "workbook" : "sheet"]
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Picker("", selection: $replace) {
                Text("Find").tag(false)
                Text("Replace").tag(true)
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            Form {
                TextField("Find what:", text: $what)
                if replace {
                    TextField("Replace with:", text: $with)
                }
                Toggle("Match case", isOn: $matchCase)
                Toggle("Match entire cell contents", isOn: $wholeCell)
                Toggle("Within workbook", isOn: $workbook)
            }
            if !found.isEmpty {
                List(Array(found.enumerated()), id: \.offset) { _, item in
                    Button {
                        if let sheet = item["sheet"] as? String { document.run("sheet.activate", ["sheet": sheet]) }
                        if let cell = item["cell"] as? String { document.run("selection.set", ["cell": cell]) }
                    } label: {
                        HStack {
                            Text(item["sheet"] as? String ?? "").foregroundStyle(.secondary)
                            Text(item["cell"] as? String ?? "").monospaced()
                            Text(String(describing: item["value"] ?? "")).lineLimit(1)
                        }
                    }
                    .buttonStyle(.plain)
                }
                .frame(height: 150)
            }
            Text(status).font(.caption).foregroundStyle(.secondary)
            HStack {
                if replace {
                    Button("Replace All") { replaceAll() }
                    Button("Replace") {
                        var p = base
                        p["with"] = with
                        p["all"] = false
                        document.run("edit.replace", p)
                        document.run("edit.find", base)
                    }
                }
                Spacer()
                Button("Find All") { findAll() }
                Button("Find Next") { document.run("edit.find", base) }
                    .keyboardShortcut(.defaultAction)
                Button("Close") { dismiss() }
                    .keyboardShortcut(.cancelAction)
            }
            .disabled(what.isEmpty)
        }
        .frame(width: 480)
    }

    private func findAll() {
        var p = base
        p["all"] = true
        guard let result = document.run("edit.find", p) else { return }
        let object = JSON.decode(result)
        found = object["results"] as? [[String: Any]] ?? []
        status = "\(found.count) cell(s) found"
    }

    private func replaceAll() {
        var p = base
        p["with"] = with
        p["all"] = true
        guard let result = document.run("edit.replace", p) else { return }
        status = "All done. \(JSON.decode(result)["replaced"] ?? 0) replacements."
    }
}

// MARK: - Go To

struct GoToDialog: View {
    @ObservedObject var document: SpreadsheetDocument
    @State private var reference = ""
    @Environment(\.dismiss) private var dismiss

    private var names: [String] {
        guard let result = try? document.engine.execute(id: "formulas.nameManager", params: "") else { return [] }
        let list = (try? JSONSerialization.jsonObject(with: Data(result.utf8))) as? [[String: Any]] ?? []
        return list.compactMap { $0["name"] as? String }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Go To").font(.headline)
            List(names, id: \.self, selection: Binding(get: { reference }, set: { reference = $0 ?? "" })) { Text($0).tag($0) }
                .frame(height: 140)
            TextField("Reference:", text: $reference)
            DialogButtons(okDisabled: reference.isEmpty) {
                if document.run("edit.goTo", ["reference": reference]) != nil { dismiss() }
            }
        }
        .frame(width: 360)
    }
}

// MARK: - Sort

struct SortDialog: View {
    struct Level: Identifiable {
        let id = UUID()
        var column = "A"
        var descending = false
    }

    @ObservedObject var document: SpreadsheetDocument
    @State private var header = true
    @State private var levels = [Level()]
    @State private var failure: String?
    @Environment(\.dismiss) private var dismiss

    /// The selection's columns, to sort by.
    private var columns: [String] {
        guard let range = try? document.engine.selection().ranges.last else { return ["A"] }
        return (range.first.col...min(range.last.col, range.first.col + 200)).map { columnLabel(col: $0) }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Sort").font(.headline)
            Toggle("My data has headers", isOn: $header)
            ForEach($levels) { $level in
                HStack {
                    Text(level.id == levels.first?.id ? "Sort by" : "Then by").frame(width: 60, alignment: .leading)
                    Picker("Column", selection: $level.column) {
                        ForEach(columns, id: \.self) { Text("Column \($0)").tag($0) }
                    }
                    .labelsHidden()
                    Picker("Order", selection: $level.descending) {
                        Text("A to Z").tag(false)
                        Text("Z to A").tag(true)
                    }
                    .labelsHidden()
                    Button("Remove", systemImage: "minus.circle") { levels.removeAll { $0.id == level.id } }
                        .labelStyle(.iconOnly)
                        .disabled(levels.count == 1)
                }
            }
            Button("Add Level", systemImage: "plus") { levels.append(Level(column: columns.first ?? "A")) }
            if let failure { Text(failure).foregroundStyle(.red).font(.caption) }
            DialogButtons {
                let keys = levels.map { ["column": $0.column, "order": $0.descending ? "desc" : "asc"] }
                do {
                    try document.perform("data.sort", json: JSON.encode(["header": header, "keys": keys]))
                    dismiss()
                } catch {
                    failure = error.localizedDescription
                }
            }
        }
        .frame(width: 440)
        .onAppear { if let first = columns.first { levels[0].column = first } }
    }
}

// MARK: - Name Manager

struct NameManagerDialog: View {
    @ObservedObject var document: SpreadsheetDocument
    @State private var selected: String?
    @Environment(\.dismiss) private var dismiss

    private var names: [[String: Any]] {
        _ = document.generation
        guard let result = try? document.engine.execute(id: "formulas.nameManager", params: "") else { return [] }
        return (try? JSONSerialization.jsonObject(with: Data(result.utf8))) as? [[String: Any]] ?? []
    }

    private func key(_ n: [String: Any]) -> String { "\(n["name"] ?? "")|\(n["scope"] ?? "")" }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Name Manager").font(.headline)
            Table(names.map { NameRow(fields: $0, id: key($0)) }, selection: $selected) {
                TableColumn("Name") { Text($0.fields["name"] as? String ?? "") }
                TableColumn("Refers To") { Text($0.fields["refersTo"] as? String ?? "").monospaced() }
                TableColumn("Scope") { Text($0.fields["scope"] as? String ?? "") }
                TableColumn("Comment") { Text($0.fields["comment"] as? String ?? "") }
            }
            .frame(height: 220)
            HStack {
                Button("New…") { document.requests.append(.dialog(name: "defineName", params: "{}")); dismiss() }
                Button("Edit…") {
                    guard let n = names.first(where: { key($0) == selected }) else { return }
                    let params: [String: Any] = ["oldName": n["name"] ?? "", "oldScope": n["scope"] ?? "", "name": n["name"] ?? "", "scope": n["scope"] ?? "",
                                                 "refersTo": n["refersTo"] ?? "", "comment": n["comment"] ?? ""]
                    document.requests.append(.dialog(name: "defineName", params: JSON.encode(params)))
                    dismiss()
                }
                .disabled(selected == nil)
                Button("Delete") {
                    guard let n = names.first(where: { key($0) == selected }) else { return }
                    document.run("formulas.deleteName", ["name": n["name"] ?? "", "scope": n["scope"] ?? ""])
                    selected = nil
                }
                .disabled(selected == nil)
                Spacer()
                Button("Close") { dismiss() }.keyboardShortcut(.cancelAction)
            }
        }
        .frame(width: 620)
    }

    struct NameRow: Identifiable {
        let fields: [String: Any]
        let id: String
    }
}

// MARK: - Insert Function

struct InsertFunctionDialog: View {
    @ObservedObject var document: SpreadsheetDocument
    let editor: EditController
    @State private var search = ""
    @State private var selected: String?
    @Environment(\.dismiss) private var dismiss

    private var names: [String] {
        let all = ((try? document.engine.functionNames(locale: editor.locale)) ?? []).sorted()
        return search.isEmpty ? all : all.filter { $0.localizedCaseInsensitiveContains(search) }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Insert Function").font(.headline)
            TextField("Search for a function", text: $search)
            List(names, id: \.self, selection: $selected) { Text($0).monospaced() }
                .frame(height: 220)
            if let name = selected, let hint = entryFunctionHint(text: "=\(name)(", caret: UInt32(name.utf16.count + 2), locale: editor.locale) {
                Text(hint.signature ?? name).monospaced()
                Text(hint.description ?? "").font(.caption).foregroundStyle(.secondary)
            }
            DialogButtons(okDisabled: selected == nil) {
                guard let name = selected, editor.commitIfEditing() else { return }
                document.run("formulas.insertFunction", ["name": name])
                dismiss()
            }
        }
        .frame(width: 440)
    }
}

// MARK: - Goal Seek

struct GoalSeekDialog: View {
    @ObservedObject var document: SpreadsheetDocument
    @State private var set = ""
    @State private var to = "0"
    @State private var changing = ""
    @State private var outcome: String?
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Goal Seek").font(.headline)
            Form {
                TextField("Set cell:", text: $set)
                TextField("To value:", text: $to)
                TextField("By changing cell:", text: $changing)
            }
            if let outcome { Text(outcome).font(.callout) }
            DialogButtons(okDisabled: set.isEmpty || changing.isEmpty || Double(to) == nil) {
                do {
                    let result = JSON.decode(try document.perform("data.goalSeek", json: JSON.encode(["set": set, "to": Double(to) ?? 0, "changing": changing])))
                    let converged = result["converged"] as? Bool == true
                    outcome = converged ? "Goal Seek found a solution: \(result["value"] ?? "")." : "Goal Seek may not have found a solution."
                } catch {
                    outcome = error.localizedDescription
                }
            }
        }
        .frame(width: 360)
        .onAppear { set = (try? document.engine.nameBoxText(dragging: false)) ?? "" }
    }
}

// MARK: - Conditional Formatting Rules Manager

struct RulesDialog: View {
    @ObservedObject var document: SpreadsheetDocument
    @State private var selected: Int?
    @Environment(\.dismiss) private var dismiss

    private var rules: [[String: Any]] {
        _ = document.generation
        guard let result = try? document.engine.execute(id: "home.manageRules", params: "") else { return [] }
        return JSON.decode(result)["rules"] as? [[String: Any]] ?? []
    }

    private func describe(_ rule: [String: Any]) -> String {
        let kind = (rule["rule"] as? [String: Any])?.keys.first ?? "\(rule["rule"] ?? "")"
        // `CellIs` → "Cell Is", `DataBar` → "Data Bar".
        return kind.replacingOccurrences(of: "([a-z])([A-Z])", with: "$1 $2", options: .regularExpression)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Conditional Formatting Rules").font(.headline)
            List(Array(rules.enumerated()), id: \.offset, selection: $selected) { index, rule in
                HStack {
                    Text(describe(rule))
                    Spacer()
                    Text((rule["ranges"] as? [String] ?? []).joined(separator: ", ")).monospaced().foregroundStyle(.secondary)
                }
                .tag(index)
            }
            .frame(height: 200)
            HStack {
                Button("Delete Rule") { if let i = selected { document.run("home.manageRules", ["delete": i]); selected = nil } }
                Button("Move Up", systemImage: "chevron.up") { if let i = selected, i > 0 { document.run("home.manageRules", ["moveUp": i]); selected = i - 1 } }
                Button("Move Down", systemImage: "chevron.down") { if let i = selected, i + 1 < rules.count { document.run("home.manageRules", ["moveDown": i]); selected = i + 1 } }
                Spacer()
                Button("Close") { dismiss() }.keyboardShortcut(.cancelAction)
            }
            .disabled(rules.isEmpty)
        }
        .frame(width: 520)
    }
}

// MARK: - Results

/// Dialogs that report: workbook statistics, accessibility, error checking, evaluate formula,
/// spelling, macros. The engine's query result, laid out as an outline.
struct ResultDialog: View {
    @ObservedObject var document: SpreadsheetDocument
    let name: String
    @Environment(\.dismiss) private var dismiss

    static let queries: [String: (title: String, command: String)] = [
        "statistics": ("Workbook Statistics", "review.workbookStatistics"),
        "accessibility": ("Accessibility", "review.checkAccessibility"),
        "errorChecking": ("Error Checking", "formulas.errorChecking"),
        "evaluateFormula": ("Evaluate Formula", "formulas.evaluateFormula"),
        "spelling": ("Spelling", "review.spelling"),
        "macros": ("Macros", "view.macros"),
    ]

    private var result: Any? {
        guard let query = Self.queries[name], let text = try? document.engine.execute(id: query.command, params: "") else { return nil }
        return try? JSONSerialization.jsonObject(with: Data(text.utf8))
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(Self.queries[name]?.title ?? name).font(.headline)
            ScrollView {
                OutlineText(value: result ?? "Nothing to report.")
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            .frame(height: 260)
            HStack {
                Spacer()
                Button("Close") { dismiss() }.keyboardShortcut(.defaultAction)
            }
        }
        .frame(width: 480)
    }
}

/// A JSON value as indented "key: value" lines.
private struct OutlineText: View {
    let value: Any

    var body: some View {
        Text(lines(value, indent: 0).joined(separator: "\n"))
            .font(.callout.monospaced())
            .textSelection(.enabled)
    }

    private func lines(_ v: Any, indent: Int) -> [String] {
        let pad = String(repeating: "  ", count: indent)
        switch v {
        case let object as [String: Any]:
            return object.keys.sorted().flatMap { key -> [String] in
                let child = object[key] ?? ""
                if child is [String: Any] || child is [Any] {
                    return ["\(pad)\(key):"] + lines(child, indent: indent + 1)
                }
                return ["\(pad)\(key): \(child)"]
            }
        case let array as [Any]:
            return array.isEmpty ? ["\(pad)(none)"] : array.flatMap { lines($0, indent: indent) }
        default:
            return ["\(pad)\(v)"]
        }
    }
}

// MARK: - About

struct AboutDialog: View {
    let agents: Bool
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(agents ? "Agent Control" : "GlassCell").font(.title2.bold())
            if agents {
                Text("Every menu item, button and gesture is an engine command, so GlassCell can be driven by scripts and agents (the control channel and MCP; see docs/control-protocol.md and docs/mcp.md).")
            } else {
                Text("A native Mac spreadsheet on GridCraft's open-source Rust engine.")
                Text("An independent project, not made or endorsed by the GridCraft or ArtCraft teams.")
                    .foregroundStyle(.secondary)
                Text("Version \(Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "–")").foregroundStyle(.secondary)
            }
            HStack {
                Spacer()
                Button("OK") { dismiss() }.keyboardShortcut(.defaultAction)
            }
        }
        .frame(width: 400)
    }
}

// MARK: - Insert Picture

/// Insert ▸ Picture: the system's open panel, then `insert.picture` with the file's bytes.
@MainActor
func insertPicture(_ document: SpreadsheetDocument, params: [String: Any]) {
    let panel = NSOpenPanel()
    panel.allowedContentTypes = [.png, .jpeg]
    panel.allowsMultipleSelection = false
    guard panel.runModal() == .OK, let url = panel.url, let data = try? Data(contentsOf: url) else { return }
    guard data.count <= 16 * 1024 * 1024 else {
        document.problem = "Choose a PNG or JPEG no larger than 16 MB."
        return
    }
    var p = params
    p["base64"] = data.base64EncodedString()
    p["mime"] = url.pathExtension.lowercased() == "png" ? "image/png" : "image/jpeg"
    p["alt"] = url.lastPathComponent
    if p["at"] == nil, let active = try? document.engine.selection().active {
        p["at"] = cellName(cell: active)
    }
    document.run("insert.picture", p)
}
