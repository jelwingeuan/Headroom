import ServiceManagement
import SwiftUI

struct MainView: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        NavigationSplitView {
            List(Pane.allCases, selection: $model.pane) { pane in
                Label(pane.rawValue, systemImage: pane.symbol)
            }
            .navigationSplitViewColumnWidth(min: 170, ideal: 190)
        } detail: {
            switch model.pane ?? .history {
            case .history: HistoryView()
            case .wouldHaveDone: WouldHaveDoneView()
            case .settings: SettingsView()
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

    var body: some View {
        let done = model.actions.filter { !$0.dryRun }
        Group {
            if done.isEmpty {
                EmptyState(symbol: "clock.arrow.circlepath", title: "Nothing deleted yet",
                           detail: "Everything Headroom deletes shows up here, with Undo to rebuild it.")
            } else {
                List(done) { action in
                    ActionRow(action: action) {
                        if action.undone {
                            Text("Undone").foregroundStyle(.secondary)
                        } else if model.undoing.contains(action.id) {
                            ProgressView().controlSize(.small)
                            Text("Rebuilding…").foregroundStyle(.secondary)
                        } else {
                            Button("Undo") { model.undo(action) }
                                .help(action.rebuild.map { "Rebuild it by running: \($0)" }
                                    ?? "Rebuild it by running its install or build command again")
                        }
                    }
                }
            }
        }
        .navigationTitle("History")
    }
}

struct WouldHaveDoneView: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        let planned = model.actions.filter(\.dryRun)
        Group {
            if planned.isEmpty {
                EmptyState(symbol: "list.bullet.clipboard", title: "Nothing to report yet",
                           detail: "While dry run is on, what Headroom would have deleted shows up here.")
            } else {
                List {
                    Section {
                        ForEach(planned) { ActionRow(action: $0) { EmptyView() } }
                    } header: {
                        Text("Would have freed \(bytes(planned.reduce(0) { $0 + $1.bytes })). "
                            + (model.status.dryRun ? "Nothing has been deleted." : "Dry run is over."))
                    }
                }
            }
        }
        .navigationTitle("Would Have Done")
    }
}

struct ActionRow<Trailing: View>: View {
    let action: Candidate
    @ViewBuilder let trailing: Trailing

    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: "folder").foregroundStyle(.secondary)
            VStack(alignment: .leading, spacing: 2) {
                Text(action.name)
                Text("\(action.kind) · \(action.place)")
                    .font(.caption).foregroundStyle(.secondary)
                    .lineLimit(1).truncationMode(.middle)
            }
            Spacer()
            VStack(alignment: .trailing, spacing: 2) {
                Text(bytes(action.bytes)).monospacedDigit()
                if let date = action.date {
                    Text(date, format: .relative(presentation: .named))
                        .font(.caption).foregroundStyle(.secondary)
                }
            }
            trailing
        }
        .padding(.vertical, 2)
        .help(action.reason ?? "")
    }
}

struct EmptyState: View {
    let symbol: String, title: String, detail: String

    var body: some View {
        VStack(spacing: 8) {
            Image(systemName: symbol).font(.largeTitle).foregroundStyle(.tertiary)
            Text(title).font(.headline)
            Text(detail).foregroundStyle(.secondary).multilineTextAlignment(.center)
        }
        .padding(40)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

struct SettingsView: View {
    @EnvironmentObject private var model: AppModel
    @State private var draft: Settings?

    var body: some View {
        Form {
            if let binding = Binding($draft) {
                Section {
                    LabeledContent("Keep at least") {
                        HStack(spacing: 4) {
                            TextField("Budget", value: binding.budgetGB, format: .number)
                                .labelsHidden().frame(width: 64).multilineTextAlignment(.trailing)
                            Stepper("Budget", value: binding.budgetGB, in: 0...100_000, step: 5).labelsHidden()
                            Text("GB free")
                        }
                    }
                    Stepper(value: binding.recentDays, in: 1...365) {
                        Text("Leave projects alone for \(binding.wrappedValue.recentDays) days after you open them")
                    }
                } header: {
                    Text("Budget")
                }
                Section {
                    Toggle("Dry run: only show what Headroom would delete", isOn: binding.dryRun)
                } header: {
                    Text("Safety")
                }
            } else {
                Text("Loading settings…").foregroundStyle(.secondary)
            }
            Section {
                Toggle("Launch at login", isOn: Binding(
                    get: { model.launchAtLogin }, set: { model.setLaunchAtLogin($0) }
                ))
                if model.loginNeedsApproval {
                    HStack {
                        Text("Approve Headroom in Login Items to finish.").foregroundStyle(.secondary)
                        Spacer()
                        Button("Open Login Items") { SMAppService.openSystemSettingsLoginItems() }
                    }
                }
                LabeledContent("Full Disk Access") {
                    Button("Open Privacy Settings") { openFullDiskAccessSettings() }
                }
            } header: {
                Text("General")
            }
        }
        .formStyle(.grouped)
        .navigationTitle("Settings")
        .onAppear { draft = model.settings }
        .onReceive(model.$settings) { draft = $0 }
        .onChange(of: draft) { if let new = $0 { model.save(new) } }
    }
}
