import ServiceManagement
import SwiftUI

struct MainView: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        NavigationSplitView {
            List(Pane.allCases, selection: Binding(get: { model.pane }, set: { if let p = $0 { model.pane = p } })) { pane in
                Label(pane.rawValue, systemImage: pane.symbol)
            }
            .accessibilityLabel("Sections")
            .safeAreaInset(edge: .top) {
                HStack(spacing: 8) {
                    Image(nsImage: NSApp.applicationIconImage).resizable().frame(width: 32, height: 32).accessibilityHidden(true)
                    VStack(alignment: .leading, spacing: 0) {
                        Text("Headroom").font(.headline)
                        Text("Keeps the space you chose").font(.caption).foregroundStyle(.secondary)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading).padding(.horizontal, 16).padding(.vertical, 8)
                .accessibilityElement(children: .combine)
            }
            .safeAreaInset(edge: .bottom) {
                Text(version).font(.caption).foregroundStyle(.secondary).monospacedDigit()
                    .frame(maxWidth: .infinity, alignment: .leading).padding(16)
            }
            .navigationSplitViewColumnWidth(min: 180, ideal: 200)
        } detail: {
            switch model.pane {
            case .overview: OverviewView()
            case .insights: InsightsView()
            case .history: HistoryView()
            case .settings: SettingsView().frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top).navigationTitle("Settings")
            case .about: ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    Text("How Headroom decides").font(.title.weight(.bold))
                    About(recentDays: model.settings?.recentDays ?? 14)
                    Text("\(version) · MIT License").font(.caption).foregroundStyle(.secondary)
                }
                .padding(24).frame(maxWidth: 640, alignment: .leading)
            }
                .navigationTitle("About Headroom")
            }
        }
        .frame(minWidth: 800, minHeight: 480)
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Headroom")
        .onAppear {
            model.loadHistory()
            model.loadSettings()
            model.loadInsights()
        }
        .alert(model.alert ?? "", isPresented: Binding(
            get: { model.alert != nil }, set: { if !$0 { model.alert = nil } }
        )) {
            Button("OK", role: .cancel) {}
        }
    }

    private var version: String {
        (Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String).map { "Version \($0)" } ?? "Development build"
    }
}

/// Every delete, newest first, on the Overview's own row. A dry-run row says what would have gone.
struct HistoryView: View {
    @EnvironmentObject private var model: AppModel
    @State private var asking: Candidate?

    var body: some View {
        let rows = model.actions.sorted { $0.when > $1.when }
        Group {
            if rows.isEmpty {
                EmptyState(symbol: "clock.arrow.circlepath", title: "Nothing deleted yet",
                           detail: "When Headroom frees space, each folder shows up here, and you can rebuild it with one click.",
                           action: model.status.budgetBytes == 0 ? ("Set a Budget…", { model.pane = .settings }) : nil)
            } else {
                List {
                    HistoryHeader(actions: model.actions, status: model.status).monospacedDigit().padding(.vertical, 8) // M7: the parts add up
                    ForEach(rows) { a in
                    let busy = model.undoing.contains(a.id)
                    ItemRow(item: a, note: status(a, busy),
                            buttons: a.dryRun || a.undone || busy || !a.freesHost ? [] : [("\(a.rebuildVerb)…", { asking = a })])
                    }
                }
                .listStyle(.inset(alternatesRowBackgrounds: false))
                .accessibilityLabel("Deleted folders")
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

    private func status(_ a: Candidate, _ busy: Bool) -> String {
        let when = a.date.map(ago) ?? "at an unknown time"
        if busy { return "Rebuilding…" }
        if !a.freesHost && !a.dryRun { return "Deleted \(when). Docker rebuilds this cache on its next build." }
        if a.dryRun { return "Dry run: would have been deleted \(when). Nothing was deleted." }
        return a.undone ? "Deleted \(when), and rebuilt since." : "Deleted \(when)."
    }

    private func rebuildMessage(_ a: Candidate) -> String {
        var text = a.rebuild.map { "Headroom runs “\($0)” in \(a.place)." }
            ?? "Headroom runs its install or build command again in \(a.place)."
        text += " It can take a few minutes and may download from the internet."
        if let loss = a.dataLoss { text += " It can't bring back everything: \(loss)." }
        return text
    }
}

/// History's summary in the Overview's style: one large figure, the parts that add up to it below.
private struct HistoryHeader: View {
    let actions: [Candidate]
    let status: Status

    var body: some View {
        let done = actions.filter { !$0.dryRun }
        let sum = { (rows: [Candidate]) in rows.reduce(Int64(0)) { $0 + $1.bytes } }
        let freed = sum(done.filter { !$0.undone && $0.freesHost }), rebuilt = sum(done.filter(\.undone))
        let docker = sum(done.filter { !$0.undone && !$0.freesHost })
        VStack(alignment: .leading, spacing: 8) {
            Text(done.isEmpty ? "Nothing deleted yet" : "\(bytes(freed)) freed").font(.largeTitle.weight(.bold)).monospacedDigit()
            Text(done.isEmpty ? "\(bytes(sum(actions.filter(\.dryRun)))) would have gone in the dry run."
                 : ["Still free on this Mac.", rebuilt > 0 ? "\(bytes(rebuilt)) was rebuilt since." : nil,
                    docker > 0 ? "\(bytes(docker)) was inside Docker's disk." : nil].compactMap { $0 }.joined(separator: " "))
                .foregroundStyle(.secondary)
            if let next = status.next, let f = status.freeBytes, let b = status.budgetBytes, f < b, !status.dryRun {
                Label("Deleting \(next.name) now, \(bytes(next.bytes)). It's listed here when it's done.", systemImage: "gauge.badge.minus")
                    .foregroundStyle(Color.headroom)
            }
        }
        .padding(.vertical, 16)
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
                    .pickerStyle(.menu)
                } header: {
                    Text("Free space")
                } footer: {
                    Footnote(binding.wrappedValue.budgetBytes == 0
                        ? "Headroom deletes nothing until you set a budget."
                        : "When free space drops below your budget, Headroom deletes the files you haven't used in the longest time, as long as it can rebuild them. It never touches a project you opened within the days above.")
                }
                Section {
                    Toggle("Only show what Headroom would delete", isOn: binding.dryRun)
                } header: {
                    Text("Dry run")
                } footer: {
                    Footnote(binding.wrappedValue.dryRun
                        ? "Nothing is deleted while this is on. It's on for your first 7 days, and what it would delete is listed in History."
                        : "Every deletion is listed in History, where Rebuild brings it back.")
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
                    Button("Open Settings…") { openFullDiskAccessSettings() }
                } label: {
                    Text("Full Disk Access")
                    Text(model.status.needsFullDiskAccess ? "Off: some folders can't be measured" : "No folders are blocked")
                }
            } header: {
                Text("General")
            }
        }
        .formStyle(.grouped)
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Settings")
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
