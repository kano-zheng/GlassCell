import GridCraftEngine
import SwiftUI

/// The bottom of a workbook window, floating over the grid in Liquid Glass: the sheet tabs with
/// a button that adds one, a passing notice, and the selection's Sum / Count / Average.
struct WorkbookStatusBar: View {
    @ObservedObject var document: SpreadsheetDocument
    @Namespace private var tabs

    private var sheet: SheetInfo? {
        _ = document.generation
        return try? document.engine.sheet()
    }

    private var stats: [StatusItem] {
        _ = document.generation
        return (try? document.engine.statusItems()) ?? []
    }

    var body: some View {
        GlassEffectContainer(spacing: 8) {
            HStack(alignment: .center, spacing: 8) {
                if let sheet {
                    sheetTabs(sheet)
                    Button {
                        document.run("home.insertSheet", ["before": sheet.sheetNames.count])
                    } label: {
                        Image(systemName: "plus")
                            .frame(width: 28, height: 28)
                            .contentShape(.circle)
                    }
                    .buttonStyle(.plain)
                    .glassEffect(.regular.interactive(), in: .circle)
                    .help("Insert Sheet")
                }
                Spacer(minLength: 8)
                if let notice = document.notice {
                    Text(notice)
                        .lineLimit(1)
                        .padding(.horizontal, 12)
                        .frame(height: 28)
                        .glassEffect(.regular.tint(.accentColor.opacity(0.25)), in: .capsule)
                        .glassEffectTransition(.materialize)
                        .task(id: notice) {
                            try? await Task.sleep(for: .seconds(6))
                            document.notice = nil
                        }
                }
                let stats = stats
                if !stats.isEmpty {
                    HStack(spacing: 14) {
                        ForEach(stats, id: \.label) { item in
                            HStack(spacing: 4) {
                                Text(LocalizedStringKey(item.label)).foregroundStyle(.secondary)
                                Text(item.value).monospacedDigit()
                            }
                        }
                    }
                    .padding(.horizontal, 14)
                    .frame(height: 28)
                    .glassEffect(.regular, in: .capsule)
                    .glassEffectTransition(.materialize)
                    .textSelection(.enabled)
                }
            }
            .font(.callout)
        }
        .padding(.horizontal, 12)
        .padding(.bottom, 10)
        .animation(.smooth(duration: 0.25), value: document.notice)
        .animation(.smooth(duration: 0.25), value: stats.isEmpty)
    }

    /// The tabs share one capsule of glass; the active one is marked by a tinted pill that slides
    /// between them.
    private func sheetTabs(_ sheet: SheetInfo) -> some View {
        ViewThatFits(in: .horizontal) {
            tabRow(sheet)
            ScrollView(.horizontal) { tabRow(sheet) }
                .scrollIndicators(.never)
        }
        .glassEffect(.regular.interactive(), in: .capsule)
    }

    private func tabRow(_ sheet: SheetInfo) -> some View {
        HStack(spacing: 2) {
            ForEach(Array(sheet.sheetNames.enumerated()), id: \.offset) { index, name in
                let active = index == Int(sheet.index)
                Button {
                    withAnimation(.smooth(duration: 0.25)) {
                        _ = document.run("sheet.activate", ["sheet": index])
                    }
                } label: {
                    Text(name)
                        .fontWeight(active ? .semibold : .regular)
                        .lineLimit(1)
                        .padding(.horizontal, 12)
                        .frame(height: 24)
                        .background {
                            if active {
                                Capsule()
                                    .fill(.tint.opacity(0.18))
                                    .matchedGeometryEffect(id: "active", in: tabs)
                            }
                        }
                        .contentShape(.capsule)
                }
                .buttonStyle(.plain)
                .contextMenu {
                    Button("Insert Sheet") { document.run("home.insertSheet", ["before": index]) }
                    Button("Delete Sheet") { document.run("home.deleteSheet", ["sheet": index]) }
                        .disabled(sheet.sheetNames.count < 2)
                }
            }
        }
        .padding(2)
    }
}
