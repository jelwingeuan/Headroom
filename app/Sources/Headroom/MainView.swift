import ServiceManagement
import SwiftUI

struct MainView: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        NavigationSplitView {
            List(Pane.allCases, selection: $model.pane) { pane in
                Label(pane.rawValue, systemImage: pane.symbol)
            }
            .navigationSplitViewColumnWidth(min: 160, ideal: 180)
        } detail: {
            switch model.pane ?? .history {
            case .history: HistoryView()
            case .wouldHaveDone: WouldHaveDoneView()
            }
        }
        .frame(minWidth: 620, minHeight: 400)
        .onAppear {
            model.loadHistory()
            model.loadSettings()
        }
        .onChange(of: model.pane) { _ in model.loadHistory() }
        .alert(model.alert ?? "", isPresented: Binding(
            get: { model.alert != nil }, set: { if !$0 { model.alert = nil } }
        )) {
            Button("OK", role: .cancel) {}
        }
    }
}

struct HistoryView: View {
    @EnvironmentObject private var model: AppModel
    @State private var asking: Candidate?
    @State private var order = [KeyPathComparator(\Candidate.when, order: .reverse)]

    var body: some View {
        let done = model.actions.filter { !$0.dryRun }
        Group {
            if done.isEmpty {
                EmptyState(symbol: "clock.arrow.circlepath", title: "Nothing deleted yet",
                           detail: "When Headroom frees space, each folder shows up here, and you can rebuild it with one click.",
                           action: model.status.budgetBytes == 0 ? ("Set a Budget…", openSettings) : nil)
            } else {
                Table(done.sorted(using: order), sortOrder: $order) {
                    TableColumn("Folder", value: \.name) { FolderCell(action: $0) }.width(min: 160, ideal: 210)
                    TableColumn("Size", value: \.bytes) { SizeCell(bytes: $0.bytes) }
                        .width(min: 70, ideal: 80, max: 100)
                    TableColumn("Date Freed", value: \.when) { WhenCell(date: $0.date) }
                        .width(min: 80, ideal: 100, max: 130)
                    TableColumn("Status") { trailing($0) }.width(min: 100, ideal: 110, max: 130)
                }
                .tableStyle(.inset(alternatesRowBackgrounds: true))
            }
        }
        .navigationTitle("History")
        .navigationSubtitle(Self.subtitle(model.actions))
        .confirmationDialog(asking.map { "\($0.rebuildVerb) \($0.name)?" } ?? "",
                            isPresented: Binding(get: { asking != nil }, set: { if !$0 { asking = nil } }),
                            presenting: asking) { action in
            Button(action.rebuildVerb) { model.undo(action) }
            Button("Cancel", role: .cancel) {}
        } message: { action in
            Text(rebuildMessage(action))
        }
    }

    @ViewBuilder private func trailing(_ action: Candidate) -> some View {
        if action.undone {
            Label("Rebuilt", systemImage: "checkmark").foregroundStyle(.secondary)
        } else if model.undoing.contains(action.id) {
            Text("Rebuilding…").foregroundStyle(.secondary)
        } else {
            Button("\(action.rebuildVerb)…") { asking = action }
                .accessibilityLabel("\(action.rebuildVerb) \(action.name)")
        }
    }

    static func subtitle(_ actions: [Candidate]) -> String {
        let done = actions.filter { !$0.dryRun }, kept = done.filter { !$0.undone }
        guard !done.isEmpty else { return "" }
        let rebuilt = done.count - kept.count
        return "\(bytes(kept.reduce(0) { $0 + $1.bytes })) freed now"
            + (rebuilt > 0 ? " · \(rebuilt) of \(done.count) rebuilt" : "")
    }

    private func rebuildMessage(_ a: Candidate) -> String {
        var text = a.rebuild.map { "Headroom runs “\($0)” in \(a.place)." }
            ?? "Headroom runs its install or build command again in \(a.place)."
        text += " It can take a few minutes and may download from the internet."
        if let loss = a.dataLoss { text += " It can't bring back everything: \(loss)." }
        return text
    }
}

struct WouldHaveDoneView: View {
    @EnvironmentObject private var model: AppModel
    @State private var order: [KeyPathComparator<Candidate>] = [] // the dry run's own order: first to go on top

    var body: some View {
        let planned = model.actions.filter(\.dryRun)
        let noBudget = model.status.budgetBytes == 0
        Group {
            if planned.isEmpty {
                EmptyState(symbol: "eye", title: "Nothing to show yet",
                           detail: noBudget
                               ? "Set a budget first. For the 7 days after that, this list shows what Headroom would delete."
                               : "For its first 7 days, this list shows what Headroom would delete.",
                           action: noBudget ? ("Set a Budget…", openSettings) : nil)
            } else {
                Table(planned.sorted(using: order), sortOrder: $order) {
                    TableColumn("Folder", value: \.name) { FolderCell(action: $0) }.width(min: 160, ideal: 210)
                    TableColumn("Size", value: \.bytes) { SizeCell(bytes: $0.bytes) }
                        .width(min: 70, ideal: 80, max: 100)
                    TableColumn("Date Found", value: \.when) { WhenCell(date: $0.date) }
                        .width(min: 80, ideal: 100, max: 130)
                }
                .tableStyle(.inset(alternatesRowBackgrounds: true))
            }
        }
        .navigationTitle("Dry Run")
        .navigationSubtitle(Self.subtitle(model.actions, dryRun: model.status.dryRun))
    }

    static func subtitle(_ actions: [Candidate], dryRun: Bool) -> String {
        let planned = actions.filter(\.dryRun)
        guard !planned.isEmpty else { return "" }
        let total = bytes(planned.reduce(0) { $0 + $1.bytes })
        return dryRun ? "Would free \(total). Nothing has been deleted."
            : "Ended. It would have freed \(total); Headroom now frees space for real."
    }
}

/// Name on top, what it is and where it lives below.
struct FolderCell: View {
    let action: Candidate

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(action.name)
            HStack(spacing: 0) { // the kind never truncates; the path gives way, in the middle
                Text(action.what).layoutPriority(1)
                Text(" · \(action.place)").truncationMode(.middle)
            }
            .font(.subheadline).foregroundStyle(.secondary).lineLimit(1)
        }
        .padding(.vertical, 4)
        .help(action.reason ?? "")
    }
}

struct SizeCell: View {
    let bytes: Int64

    var body: some View {
        Text(Headroom.bytes(bytes)).monospacedDigit().frame(maxWidth: .infinity, alignment: .trailing)
    }
}

struct WhenCell: View {
    let date: Date?

    var body: some View {
        if let date { Text(date, format: .relative(presentation: .named)).foregroundStyle(.secondary) }
    }
}

struct EmptyState: View {
    let symbol: String, title: String, detail: String
    let action: (String, () -> Void)?

    var body: some View {
        VStack(spacing: 8) {
            Image(systemName: symbol).font(.largeTitle).foregroundStyle(.secondary)
            Text(title).font(.title3.weight(.semibold))
            Text(detail).foregroundStyle(.secondary).multilineTextAlignment(.center).frame(maxWidth: 360)
            if let action { Button(action.0, action: action.1).padding(.top, 8) }
        }
        .padding(40)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(.background)
    }
}

struct SettingsView: View {
    @EnvironmentObject private var model: AppModel
    @State private var draft: Settings?

    var body: some View {
        Form {
            if let binding = Binding($draft) {
                Section {
                    LabeledContent("Budget") {
                        HStack(spacing: 8) {
                            TextField("Gigabytes", value: Binding(
                                get: { binding.wrappedValue.budgetGB == 0 ? nil : binding.wrappedValue.budgetGB },
                                set: { binding.wrappedValue.budgetGB = $0 ?? 0 }
                            ), format: .number, prompt: Text("e.g. 60"))
                                .labelsHidden().frame(width: 64).multilineTextAlignment(.trailing)
                                .textFieldStyle(.roundedBorder)
                                .accessibilityLabel("Space to keep free, in gigabytes")
                            Text("GB free")
                        }
                    }
                    Picker("Leave projects alone for", selection: binding.recentDays) {
                        ForEach([7, 14, 30, 60, 90], id: \.self) { Text("\($0) days").tag($0) }
                        if ![7, 14, 30, 60, 90].contains(binding.wrappedValue.recentDays) {
                            Text("\(binding.wrappedValue.recentDays) days").tag(binding.wrappedValue.recentDays)
                        }
                    }
                } header: {
                    Text("Free Space")
                } footer: {
                    Footnote(binding.wrappedValue.budgetBytes == 0
                        ? "Headroom deletes nothing until you set a budget."
                        : "When free space drops below your budget, Headroom deletes the files you haven't used in the longest time, as long as it can rebuild them. It never touches a project you opened within the days above.")
                }
                Section {
                    Toggle("Only show what Headroom would delete", isOn: binding.dryRun)
                } header: {
                    Text("Dry Run")
                } footer: {
                    Footnote(binding.wrappedValue.dryRun
                        ? "Nothing is deleted while this is on. It's on for your first 7 days, and what it would delete appears under Dry Run in the Headroom window."
                        : "Off: Headroom frees space for real, and every deletion appears in History.")
                }
            } else {
                ProgressView("Loading settings…")
            }
            Section {
                Toggle("Open at login", isOn: Binding(
                    get: { model.launchAtLogin }, set: { model.setLaunchAtLogin($0) }
                ))
                if model.loginNeedsApproval {
                    LabeledContent("Approve Headroom in Login Items to finish.") {
                        Button("Open Login Items…") { SMAppService.openSystemSettingsLoginItems() }
                    }
                }
                LabeledContent {
                    Button("Open Privacy Settings…") { openFullDiskAccessSettings() }
                } label: {
                    Text("Full Disk Access")
                    Text(model.status.needsFullDiskAccess ? "Off: some folders can't be measured" : "No folders blocked")
                }
            } header: {
                Text("General")
            }
        }
        .formStyle(.grouped)
        .frame(width: 460)
        .fixedSize(horizontal: false, vertical: true)
        .onAppear {
            model.loadSettings()
            draft = model.settings
        }
        .onReceive(model.$settings) { draft = $0 }
        .onChange(of: draft) { if let new = $0 { model.save(new) } }
    }
}

/// A form footer that wraps instead of truncating and lines up with the section above it.
struct Footnote: View {
    let text: String
    init(_ text: String) { self.text = text }

    var body: some View {
        Text(text).font(.callout).foregroundStyle(.secondary).multilineTextAlignment(.leading)
            .fixedSize(horizontal: false, vertical: true)
            .frame(maxWidth: .infinity, alignment: .leading)
    }
}
