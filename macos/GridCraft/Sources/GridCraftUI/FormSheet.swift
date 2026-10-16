import GridCraftEngine
import SwiftUI

/// One of the engine's form dialogs as a sheet: its fields, then Cancel and OK. OK runs the
/// form's command; when the engine refuses, the sheet stays open with the reason.
struct FormSheet: View {
    let form: FormSession
    @ObservedObject var document: SpreadsheetDocument
    @Environment(\.dismiss) private var dismiss
    /// Bumped after every change: the fields and values are re-read from the form.
    @State private var revision = 0
    @State private var failure: String?

    var body: some View {
        let _ = revision
        VStack(alignment: .leading, spacing: 0) {
            Text(LocalizedStringKey(form.title()))
                .font(.headline)
                .padding([.top, .horizontal], 20)
            Form {
                ForEach(Array(form.fields().enumerated()), id: \.offset) { _, field in
                    row(field)
                }
            }
            .formStyle(.grouped)
            if let failure {
                Label(failure, systemImage: "exclamationmark.triangle")
                    .foregroundStyle(.red)
                    .padding(.horizontal, 20)
            }
            HStack {
                Spacer()
                Button("Cancel", role: .cancel) { dismiss() }
                    .keyboardShortcut(.cancelAction)
                Button("OK") { confirm() }
                    .keyboardShortcut(.defaultAction)
            }
            .padding(20)
        }
        .frame(minWidth: 420, idealWidth: 460, minHeight: 200)
    }

    @ViewBuilder
    private func row(_ field: FormField) -> some View {
        switch field {
        case let .text(key, label):
            TextField(LocalizedStringKey(label), text: text(key))
        case let .password(key, label):
            SecureField(LocalizedStringKey(label), text: text(key))
        case let .number(key, label):
            TextField(LocalizedStringKey(label), text: Binding(get: { form.text(key: key) }, set: { try? form.setNumber(key: key, value: $0); revision += 1 }))
        case let .choice(key, label, options):
            Picker(LocalizedStringKey(label), selection: text(key)) {
                if !options.contains(where: { $0.value == form.text(key: key) }) {
                    Text("—").tag(form.text(key: key))
                }
                ForEach(options, id: \.value) { Text(LocalizedStringKey($0.label)).tag($0.value) }
            }
        case let .check(key, label):
            Toggle(LocalizedStringKey(label), isOn: Binding(get: { form.flag(key: key) }, set: { try? form.setFlag(key: key, value: $0); revision += 1 }))
        case let .note(text):
            Text(text)
                .font(.callout)
                .foregroundStyle(.secondary)
                .textSelection(.enabled)
        }
    }

    private func text(_ key: String) -> Binding<String> {
        Binding(get: { form.text(key: key) }, set: { try? form.setText(key: key, value: $0); revision += 1 })
    }

    private func confirm() {
        do {
            try document.perform(form.command(), json: try form.params())
            dismiss()
        } catch {
            failure = error.localizedDescription
        }
    }
}

/// A sheet-presentable form.
struct PresentedForm: Identifiable {
    let id = UUID()
    let session: FormSession
}
