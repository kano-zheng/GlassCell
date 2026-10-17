import AppIntents
import GridCraftUI

/// Runs any of GridCraft's engine commands on the frontmost workbook: every menu item and
/// button is one (`home.bold`, `data.sortAscending`, `cell.set`…).
struct RunCommandIntent: AppIntent {
    static let title: LocalizedStringResource = "Run GlassCell Command"
    static let description = IntentDescription("Runs a GlassCell command on the frontmost workbook and returns its result as JSON.")
    static let openAppWhenRun = true

    @Parameter(title: "Command", optionsProvider: CommandOptions())
    var command: String

    @Parameter(title: "Parameters (JSON)", default: "")
    var parameters: String

    static var parameterSummary: some ParameterSummary {
        Summary("Run \(\.$command) with \(\.$parameters)")
    }

    @MainActor
    func perform() async throws -> some IntentResult & ReturnsValue<String> {
        .result(value: try Automation.run(command, params: parameters))
    }

    struct CommandOptions: DynamicOptionsProvider {
        @MainActor
        func results() async throws -> [String] {
            Automation.commandIds()
        }
    }
}

/// Reads a cell of the frontmost workbook as it is displayed.
struct GetCellIntent: AppIntent {
    static let title: LocalizedStringResource = "Get Cell Value"
    static let description = IntentDescription("Returns a cell's displayed value from the frontmost GlassCell workbook.")
    static let openAppWhenRun = true

    @Parameter(title: "Cell", default: "A1")
    var cell: String

    static var parameterSummary: some ParameterSummary {
        Summary("Get the value of \(\.$cell)")
    }

    @MainActor
    func perform() async throws -> some IntentResult & ReturnsValue<String> {
        .result(value: try Automation.cellText(cell))
    }
}

/// Types a value or formula into a cell of the frontmost workbook.
struct SetCellIntent: AppIntent {
    static let title: LocalizedStringResource = "Set Cell Value"
    static let description = IntentDescription("Enters text, a number or a formula into a cell of the frontmost GlassCell workbook.")
    static let openAppWhenRun = true

    @Parameter(title: "Cell", default: "A1")
    var cell: String

    @Parameter(title: "Value")
    var value: String

    static var parameterSummary: some ParameterSummary {
        Summary("Set \(\.$cell) to \(\.$value)")
    }

    @MainActor
    func perform() async throws -> some IntentResult {
        try Automation.setCell(cell, to: value)
        return .result()
    }
}

struct GridCraftShortcuts: AppShortcutsProvider {
    static var appShortcuts: [AppShortcut] {
        AppShortcut(intent: GetCellIntent(), phrases: ["Read a cell in \(.applicationName)"], shortTitle: "Get Cell Value", systemImageName: "tablecells")
        AppShortcut(intent: SetCellIntent(), phrases: ["Set a cell in \(.applicationName)"], shortTitle: "Set Cell Value", systemImageName: "square.and.pencil")
        AppShortcut(intent: RunCommandIntent(), phrases: ["Run a command in \(.applicationName)"], shortTitle: "Run Command", systemImageName: "command")
    }
}
