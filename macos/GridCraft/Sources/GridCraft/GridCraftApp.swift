import AppKit
import GridCraftUI
import SwiftUI

@main
struct GridCraftApp: App {
    @NSApplicationDelegateAdaptor private var delegate: AppDelegate

    init() {
        // Agents and scripts drive the app over 127.0.0.1 when launched with `--control <port>`.
        ControlServer.startIfRequested()
    }

    var body: some Scene {
        DocumentGroup(newDocument: { SpreadsheetDocument() }) { file in
            WorkbookView(document: file.document, fileURL: file.fileURL)
        }
        .defaultSize(width: 1200, height: 800)
        .commands {
            SampleCommands()
            WorkbookCommands()
        }
    }
}

/// File ▸ New from Sample.
struct SampleCommands: Commands {
    @Environment(\.newDocument) private var newDocument

    var body: some Commands {
        CommandGroup(after: .newItem) {
            Menu("New from Sample") {
                ForEach([("Sales", "sales"), ("Budget", "budget"), ("Grades", "grades")], id: \.1) { label, sample in
                    Button(label) {
                        newDocument { SpreadsheetDocument(sample: sample) }
                    }
                }
            }
        }
    }
}

/// Spotlight results open their workbook, also when no window is open.
final class AppDelegate: NSObject, NSApplicationDelegate {
    func application(_ application: NSApplication, continue userActivity: NSUserActivity, restorationHandler: @escaping ([any NSUserActivityRestoring]) -> Void) -> Bool {
        MainActor.assumeIsolated { WorkbookSpotlight.open(userActivity) }
    }
}
