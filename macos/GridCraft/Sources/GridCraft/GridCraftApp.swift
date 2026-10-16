import GridCraftUI
import SwiftUI

@main
struct GridCraftApp: App {
    var body: some Scene {
        DocumentGroup(newDocument: { SpreadsheetDocument() }) { file in
            WorkbookView(document: file.document)
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
