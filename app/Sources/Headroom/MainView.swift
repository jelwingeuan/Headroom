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
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        let done = model.actions.filter { !$0.dryRun }
        Group {
            if done.isEmpty {
                EmptyState(symbol: "clock.arrow.circlepath", title: "Nothing deleted yet",
                           detail: "When Headroom frees space, each folder shows up here, and you can rebuild it with one click.",
                           action: model.status.budgetBytes == 0 ? ("Set a Budget…", openSettings) : nil)
            } else {
                List {
                    Section {
                        ForEach(done) { action in
                            ActionRow(action: action) { trailing(action) }
                        }
                    } header: {
                        let freed = done.filter { !$0.undone }.reduce(0) { $0 + $1.bytes }
                        Text("Headroom freed \(bytes(freed)) from \(done.filter { !$0.undone }.count) folders")
                            .monospacedDigit()
                    }
                }
                .animation(reduceMotion ? .easeInOut(duration: 0.2) : .spring(response: 0.35, dampingFraction: 1),
                           value: done.map(\.undone))
            }
        }
        .navigationTitle("History")
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
            Label("Rebuilt", systemImage: "checkmark").labelStyle(.titleAndIcon).foregroundStyle(.secondary)
        } else if model.undoing.contains(action.id) {
            ProgressView().controlSize(.small)
            Text("Rebuilding…").foregroundStyle(.secondary)
        } else {
            Button("\(action.rebuildVerb)…") { asking = action }
                .accessibilityLabel("\(action.rebuildVerb) \(action.name)")
        }
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

    var body: some View {
        let planned = model.actions.filter(\.dryRun)
        let noBudget = model.status.budgetBytes == 0
        Group {
            if planned.isEmpty {
                EmptyState(symbol: "eye", title: "Nothing to show yet",
                           detail: noBudget
                               ? "Choose how much space to keep free first. For its first 7 days Headroom then only shows what it would delete, here."
                               : "For its first 7 days Headroom only shows what it would delete. Its picks appear here.",
                           action: noBudget ? ("Set a Budget…", openSettings) : nil)
            } else {
                List {
                    Section {
                        ForEach(planned) { ActionRow(action: $0) { EmptyView() } }
                    } header: {
                        Text("Would have freed \(bytes(planned.reduce(0) { $0 + $1.bytes })). Nothing was deleted.")
                            .monospacedDigit()
                    } footer: {
                        if !model.status.dryRun {
                            Footnote("The dry run is over. Headroom now frees space for real, and everything it deletes appears in History.")
                        }
                    }
                }
            }
        }
        .navigationTitle("Dry Run")
    }
}

struct ActionRow<Trailing: View>: View {
    let action: Candidate
    @ViewBuilder let trailing: Trailing

    var body: some View {
        HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 2) {
                Text(action.name)
                Text("\(action.what) · \(action.place)")
                    .font(.subheadline).foregroundStyle(.secondary)
                    .lineLimit(1).truncationMode(.middle)
            }
            Spacer(minLength: 16)
            VStack(alignment: .trailing, spacing: 2) {
                Text(bytes(action.bytes)).monospacedDigit()
                if let date = action.date {
                    Text(date, format: .relative(presentation: .named))
                        .font(.subheadline).foregroundStyle(.secondary)
                }
            }
            HStack(spacing: 4) { trailing }.frame(minWidth: 96, alignment: .trailing)
        }
        .padding(.vertical, 4)
        .accessibilityElement(children: .contain)
        .help(action.reason ?? "")
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
                    LabeledContent("Keep free") {
                        HStack(spacing: 8) {
                            TextField("Gigabytes", value: Binding(
                                get: { binding.wrappedValue.budgetGB == 0 ? nil : binding.wrappedValue.budgetGB },
                                set: { binding.wrappedValue.budgetGB = $0 ?? 0 }
                            ), format: .number, prompt: Text("60"))
                                .labelsHidden().frame(width: 64).multilineTextAlignment(.trailing)
                                .textFieldStyle(.roundedBorder)
                                .accessibilityLabel("Space to keep free, in gigabytes")
                            Text("GB")
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
                        ? "Headroom won't delete anything until you choose an amount."
                        : "When free space drops below this, Headroom deletes the files you haven't used for longest, as long as it can rebuild them.")
                }
                Section {
                    Toggle("Only show what Headroom would delete", isOn: binding.dryRun)
                } header: {
                    Text("Dry Run")
                } footer: {
                    Footnote(binding.wrappedValue.dryRun
                        ? "Nothing is deleted while this is on. It's on for your first 7 days; its picks appear under Dry Run in the Headroom window."
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
                LabeledContent("Full Disk Access") {
                    Button(model.status.needsFullDiskAccess ? "Allow…" : "Review…") { openFullDiskAccessSettings() }
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
