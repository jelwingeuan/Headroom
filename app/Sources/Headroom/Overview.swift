import AppKit
import SwiftUI

/// The Overview's list, worked out from `breakdown`: freeable folders by section (projects as
/// cards), then everything protected. Pure, so the render-mode self-check can test it.
struct Overview {
    /// A project card (with `rows`) or a single folder (with `item`).
    struct Node: Identifiable {
        let id: String
        var item: Candidate?
        var title = "", place = ""
        var rows: [Candidate] = []
        var bytes: Int64 { item?.bytes ?? rows.reduce(0) { $0 + $1.bytes } }
    }

    struct Group: Identifiable {
        let id: String, title: String
        var nodes: [Node]
        var freeing: String?
        var bytes: Int64 { nodes.flatMap { $0.item.map { [$0] } ?? $0.rows }.filter { $0.path != freeing }.reduce(0) { $0 + $1.bytes } }
    }

    var groups: [Group] = []
    var freeable: [Candidate] = []
    var kept: [Candidate] = []
    /// Ticked when the list first shows: everything freeable except kinds that say why not.
    var defaults: Set<String> = []
    /// The folder autopilot is deleting right now: shown, never tickable, never counted twice.
    var freeing: String?

    init(_ breakdown: JSON, freeing: String? = nil) {
        self.freeing = freeing
        let all = breakdown.objects("entries").map(Candidate.init)
        freeable = all.filter(\.canFree)
        kept = all.filter { !$0.canFree }
        defaults = Set(freeable.filter { $0.info.unticked == nil && $0.path != freeing }.map(\.path))
        for section in Kind.Section.allCases {
            let rows = freeable.filter { $0.section == section }
            guard !rows.isEmpty else { continue }
            var nodes: [Node]
            if section == .projects {
                nodes = Dictionary(grouping: rows) { $0.project ?? ($0.path as NSString).deletingLastPathComponent }.map { project, rows in
                    let short = (project as NSString).abbreviatingWithTildeInPath
                    return Node(id: project, title: (project as NSString).lastPathComponent,
                                place: (short as NSString).deletingLastPathComponent, rows: rows.sorted { $0.bytes > $1.bytes })
                }
            } else {
                nodes = rows.map { Node(id: $0.path, item: $0) }
            }
            groups.append(Group(id: section.title, title: section.title, nodes: nodes.sorted { ($0.bytes, $0.id) > ($1.bytes, $1.id) }, freeing: freeing))
        }
    }
}

struct OverviewView: View {
    @EnvironmentObject private var model: AppModel
    /// nil until the person ticks something: the defaults stand until then.
    @State private var ticked: Set<String>?
    @State private var selection = Set<String>()
    @State private var showKept = Screens.rendering // renders show why each is kept
    @State private var confirming = false

    var body: some View {
        let s = model.status
        let acting = !s.dryRun && (s.freeBytes ?? 0) < (s.budgetBytes ?? 0)
        let list = Overview(model.breakdown, freeing: acting ? s.next?.path : nil)
        let on = ticked ?? list.defaults
        let chosen = list.freeable.filter { on.contains($0.path) && $0.path != list.freeing }
        let total = chosen.reduce(Int64(0)) { $0 + $1.bytes }
        List(selection: $selection) {
            Section { Header(list: list, status: s) }
            ForEach(list.groups) { g in
                Section {
                    ForEach(g.nodes) { n in
                        if let item = n.item {
                            ItemRow(item: item, tick: item.path == list.freeing ? nil : tick(item.path, list), note: note(item, on, list))
                                .tag(item.path)
                        } else {
                            ProjectCard(node: n, ticks: n.rows.map { $0.path == list.freeing ? nil : tick($0.path, list) },
                                        notes: n.rows.map { note($0, on, list) }).tag(n.id)
                        }
                    }
                } header: {
                    SectionHeader(title: g.title, bytes: g.bytes)
                }
            }
            if !list.kept.isEmpty {
                Section {
                    DisclosureGroup(isExpanded: $showKept.animation(.headroom)) {
                        ForEach(list.kept) { ItemRow(item: $0, note: $0.reason.map { "Kept: " + $0.replacingOccurrences(of: NSHomeDirectory(), with: "~").replacingOccurrences(of: ": ", with: " — ") }).tag($0.path) }
                    } label: {
                        Text("\(list.kept.count) folders Headroom won't delete, and why").foregroundStyle(.secondary)
                    }
                } header: {
                    SectionHeader(title: "Protected", bytes: list.kept.reduce(0) { $0 + $1.bytes })
                }
            }
        }
        .listStyle(.inset(alternatesRowBackgrounds: false))
        .animation(.headroom, value: list.freeable.map(\.path)) // deleted rows collapse, nothing overshoots
        .safeAreaInset(edge: .bottom, spacing: 0) {
            if !list.freeable.isEmpty { BottomBar(count: chosen.count, total: total, all: list.freeable.filter { $0.freesHost && $0.path != list.freeing }.reduce(0) { $0 + $1.bytes }, dryRun: s.dryRun, working: model.working) { confirming = true } }
        }
        .background { // Space ticks the selected rows; ⌘A ticks everything; ⌘⌫ asks to delete.
            Button("") { toggle(selection, list) }.keyboardShortcut(.space, modifiers: []).hidden()
            Button("") { withAnimation(.headroom) { ticked = Set(list.freeable.map(\.path)) } }.keyboardShortcut("a").hidden()
            Button("") { if !chosen.isEmpty { confirming = true } }.keyboardShortcut(.delete, modifiers: .command).hidden()
        }
        .navigationTitle("Overview")
        .toolbar {
            Button { model.rescan() } label: { Label("Rescan", systemImage: "arrow.clockwise") }
                .help("Measure every folder again now").disabled(model.working)
        }
        .sheet(isPresented: $confirming) {
            ConfirmSheet(rows: chosen, dryRun: s.dryRun, history: model.actions) { model.reclaim(chosen.map(\.path)) }
        }
        .onAppear { model.loadInsights() }
    }

    private func note(_ item: Candidate, _ on: Set<String>, _ list: Overview) -> String? {
        item.path == list.freeing ? "Headroom is freeing this now." : on.contains(item.path) ? nil : item.info.unticked
    }

    private func tick(_ path: String, _ list: Overview) -> Binding<Bool> {
        Binding(get: { (ticked ?? list.defaults).contains(path) }, set: { on in
            var set = ticked ?? list.defaults
            if on { set.insert(path) } else { set.remove(path) }
            withAnimation(.headroom) { ticked = set }
        })
    }

    /// Space on a project ticks all its folders unless all are ticked already.
    private func toggle(_ ids: Set<String>, _ list: Overview) {
        var set = ticked ?? list.defaults
        let rows = list.groups.flatMap(\.nodes).flatMap { ids.contains($0.id) ? ($0.item.map { [$0] } ?? $0.rows) : [] }
            .filter(\.canFree)
        let paths = rows.map(\.path)
        if paths.allSatisfy(set.contains) { set.subtract(paths) } else { set.formUnion(paths) }
        withAnimation(.headroom) { ticked = set }
    }
}

/// Title, one sentence, and the disk against the budget.
private struct Header: View {
    @EnvironmentObject private var model: AppModel
    let list: Overview
    let status: Status

    var body: some View {
        let sum = { (rows: [Candidate]) in rows.reduce(Int64(0)) { $0 + $1.bytes } }
        let host = list.freeable.filter { $0.freesHost && $0.path != list.freeing }
        let here = sum(host), docker = sum(list.freeable.filter { !$0.freesHost }), kept = sum(list.kept)
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 12) {
                Text(status.totalsAt == nil ? "Measuring your folders…" : list.freeable.isEmpty ? "Nothing can go right now" : "\(bytes(here)) can go")
                    .font(.largeTitle.weight(.bold)).monospacedDigit()
                if status.totalsAt == nil || model.working { ProgressView().controlSize(.small) }
            }
            Text(subline(host.count, docker, kept, sum(host.filter { $0.info.unticked != nil }))).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
            // The window says what the menu says about the budget, in the same words.
            if let f = status.freeBytes, let b = status.budgetBytes, b > 0, f < b {
                if let next = status.next, !status.dryRun {
                    Label("\(bytes(b - f)) below your budget. Freeing \(bytes(next.bytes)) from \(next.name) now.", systemImage: "gauge.badge.minus")
                        .foregroundStyle(Color.headroom)
                } else if status.dryRun {
                    let days = status.dryRunDaysLeft ?? 0
                    Label("\(bytes(b - f)) below your budget. Dry run: nothing is deleted" + (days > 0 ? " for \(days) more \(days == 1 ? "day" : "days")." : "."),
                          systemImage: "gauge.with.dots.needle.33percent")
                }
            } else if let f = status.freeBytes, let b = status.budgetBytes, b > 0 {
                Label("\(bytes(f - b)) above your \(bytes(b)) budget, so Headroom has nothing to do.", systemImage: "gauge.with.dots.needle.67percent")
            }
            if status.budgetBytes == 0 {
                Text("Headroom deletes nothing until you set a budget.")
                Button("Set a Budget…") { model.pane = .settings }.buttonStyle(.borderedProminent).tint(.headroom)
            }
            if status.needsFullDiskAccess {
                HStack {
                    Label("Headroom can't see some folders yet, so this list may be short.", systemImage: "lock.shield")
                    Button("Full Disk Access Settings…", action: openFullDiskAccessSettings)
                }
                .foregroundStyle(.secondary)
            }
            if let f = status.freeBytes, let b = status.budgetBytes {
                SpaceBar(capacity: status.capacityBytes ?? f, free: f, budget: b, canBeFreed: status.canComeBack,
                         freeing: !status.dryRun && f < b ? status.next?.bytes ?? 0 : 0).padding(.top, 8)
            }
        }
        .padding(.top, 16)
        .accessibilityElement(children: .contain)
    }

    private func subline(_ count: Int, _ docker: Int64, _ kept: Int64, _ unticked: Int64) -> String {
        if status.totalsAt == nil {
            return "Headroom's first check lists every folder it could free, and every one it keeps. It takes a minute or two."
        }
        var parts = [status.freeBytes.map { f in "\(bytes(f)) free." },
                     count > 0 ? "\(count) \(count == 1 ? "folder" : "folders")\(unticked > 0 ? ", \(bytes(unticked)) of it left unticked" : "")"
                        + "\(docker > 0 ? ", plus \(bytes(docker)) inside Docker" : "")." : nil,
                     kept > 0 ? "\(bytes(kept)) is protected." : nil]
        if let at = status.totalsAt { parts.append("Folders measured \(ago(at)).") }
        return parts.compactMap { $0 }.joined(separator: " ")
    }
}

private struct SectionHeader: View {
    let title: String, bytes: Int64
    var body: some View {
        HStack {
            Text(title)
            Spacer()
            Text(Headroom.bytes(bytes)).monospacedDigit()
        }
        .font(.headline).foregroundStyle(.primary).padding(.top, 8)
    }
}

/// A project: one tri-state checkbox for all its folders, then its folders.
private struct ProjectCard: View {
    let node: Overview.Node
    let ticks: [Binding<Bool>?]
    let notes: [String?]

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 8) {
                if ticks.contains(where: { $0 != nil }) {
                    Toggle(sources: ticks.compactMap { $0 }, isOn: \.self) { EmptyView() }.toggleStyle(.checkbox).labelsHidden()
                        .accessibilityLabel("All of \(node.title)")
                } else { // its only folder is being freed right now
                    Color.clear.frame(width: 16) // the folder row below carries the spinner
                }
                Text(node.title).font(.headline)
                Text(node.place).foregroundStyle(.secondary).lineLimit(1).truncationMode(.head)
                Spacer()
                Text(bytes(node.bytes)).monospacedDigit().font(.headline)
            }
            .frame(minHeight: 32)
            ForEach(node.rows.indices, id: \.self) { i in
                Divider().padding(.leading, 24)
                ItemRow(item: node.rows[i], tick: ticks[i], note: notes[i], inCard: true)
                    .padding(.leading, 24)
            }
        }
        .padding(.vertical, 4)
    }
}

/// The one folder row: checkbox, name in mono, where it lives, tags, size, Show in Finder.
/// Overview, Protected and History all draw folders with it.
struct ItemRow: View {
    let item: Candidate
    var tick: Binding<Bool>?
    /// One sentence under the tags: why it's unticked, why it's kept, or what happened to it.
    var note: String?
    var inCard = false
    var buttons: [(String, () -> Void)] = []

    var body: some View {
        HStack(spacing: 8) {
            if let tick {
                Toggle("", isOn: tick).toggleStyle(.checkbox).labelsHidden()
            } else if item.canFree && item.date == nil { // being freed right now
                ProgressView().controlSize(.small).frame(width: 16).accessibilityHidden(true)
            } else if !item.canFree && item.date == nil {
                Image(systemName: "lock").foregroundStyle(.secondary).frame(width: 16).help("Protected: can't be ticked")
            }
            VStack(alignment: .leading, spacing: 4) {
                HStack(spacing: 6) {
                    Text(inCard ? item.inProject : item.name).font(item.kind == "simulator" ? .body : .body.monospaced()) // mono only for real folder names
                        .foregroundStyle(item.dryRun ? .secondary : .primary) // a dry-run row: nothing was deleted
                    if !inCard {
                        Text(item.place).foregroundStyle(.secondary).lineLimit(1).truncationMode(.head)
                    }
                }
                HStack(spacing: 6) {
                    Tag(text: item.what, dot: item.info.color)
                    if item.canFree || item.date != nil {
                        Text(item.tags.filter { $0 != item.what }.prefix(3).joined(separator: " · ")).foregroundStyle(.secondary).lineLimit(1)
                    }
                }
                .font(.subheadline)
                if let note {
                    if item.canFree && tick != nil { // unticked by default: a caution, said once
                        Label { Text(note) } icon: { item.dataLoss != nil || item.kind == "simulator" // a warning only where data can't come back
                            ? Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(.orange) : Image(systemName: "info.circle").foregroundStyle(.secondary) }
                            .font(.subheadline).fixedSize(horizontal: false, vertical: true)
                    } else {
                        Text(note).font(.subheadline).fixedSize(horizontal: false, vertical: true)
                            .foregroundStyle(item.canFree && tick == nil && item.date == nil ? Color.headroom : .primary) // freeing now
                    }
                }
            }
            Spacer(minLength: 8)
            Text(bytes(item.bytes)).font(.body.monospacedDigit())
            if item.date != nil { // History: one column for its button, so sizes line up
                HStack { ForEach(buttons, id: \.0) { b in Button(b.0, action: b.1).controlSize(.small) } }.frame(width: 96, alignment: .trailing)
            }
            Button(action: reveal) { Image(systemName: "magnifyingglass") }
                .buttonStyle(.borderless).help("Show in Finder")
                .opacity(item.path.hasPrefix("/") ? 1 : 0).disabled(!item.path.hasPrefix("/")) // Docker's folders live inside its disk
        }
        .frame(minHeight: 44)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("\(item.name), \(item.what), \(bytes(item.bytes))")
        .accessibilityValue([tick.map { $0.wrappedValue ? "ticked" : "not ticked" }, note ?? item.reason].compactMap { $0 }.joined(separator: ". "))
        .accessibilityAddTraits(tick == nil ? [] : .isButton)
        .accessibilityAction { tick?.wrappedValue.toggle() }
        .accessibilityAction(named: "Show in Finder", reveal)
        .modifier(Actions(buttons: buttons))
    }

    private func reveal() { NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: item.path)]) }
}

/// The row's buttons as VoiceOver actions too.
private struct Actions: ViewModifier {
    let buttons: [(String, () -> Void)]
    func body(content: Content) -> some View {
        buttons.reduce(AnyView(content)) { v, b in AnyView(v.accessibilityAction(named: b.0, b.1)) }
    }
}

private struct Tag: View {
    let text: String
    var dot: Color?
    var body: some View {
        HStack(spacing: 4) {
            if let dot { Circle().fill(dot).frame(width: 6, height: 6) }
            Text(text).lineLimit(1)
        }
        .padding(.horizontal, 6).padding(.vertical, 1)
        .background(.quaternary.opacity(0.6), in: Capsule())
    }
}

/// "10 selected · 6.7 GB" and the one button. The total counts as rows are ticked.
private struct BottomBar: View {
    let count: Int, total: Int64, all: Int64, dryRun: Bool, working: Bool
    let go: () -> Void

    var body: some View {
        HStack {
            Text("\(count) selected · \(bytes(total))\(total < all ? " of \(bytes(all))" : "")").monospacedDigit()
                .contentTransition(.numericText()).animation(.headroom, value: total)
            if working { ProgressView().controlSize(.small).padding(.leading, 4) }
            Spacer()
            Button(dryRun ? "Record \(bytes(total)) in History…" : "Delete \(bytes(total))…", action: go)
                .buttonStyle(.borderedProminent).tint(.headroom).controlSize(.large)
                .keyboardShortcut(.defaultAction)
                .disabled(count == 0 || working)
        }
        .padding(.horizontal, 20).padding(.vertical, 12)
        .background(.bar)
        .overlay(alignment: .top) { Divider() }
    }
}

/// Exactly what goes, that History's Rebuild brings it back, and how long that takes.
struct ConfirmSheet: View {
    let rows: [Candidate], dryRun: Bool, history: [Candidate]
    let go: () -> Void
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        let total = rows.reduce(Int64(0)) { $0 + $1.bytes }
        let loss = rows.filter { $0.kind == "simulator" }
        VStack(alignment: .leading, spacing: 12) {
            Text("\(dryRun ? "Record" : "Delete") \(rows.count) \(rows.count == 1 ? "folder" : "folders"), \(bytes(total))?")
                .font(.title2.weight(.semibold))
            Text(dryRun ? "It's the dry run week, so nothing is deleted. History lists these as what Headroom would delete."
                 : "Headroom checks each folder again just before deleting it. Each one is listed in History, where Rebuild runs its install or build again\(rows.contains { $0.info.section == .models } ? " and Download Again fetches a model" : ""). \(rebuildTime).")
                .fixedSize(horizontal: false, vertical: true)
            VStack(spacing: 6) { // every row it acts on, or the largest seven and what's left, never a silent cut
                let shown = Array(rows.prefix(rows.count > 8 ? 7 : 8))
                ForEach(shown) { r in
                    HStack {
                        Text(r.project.map { "\(($0 as NSString).lastPathComponent) › \(r.inProject)" } ?? r.name).font(.body.monospaced()).lineLimit(1)
                        Text(r.what).foregroundStyle(.secondary).lineLimit(1)
                        Spacer()
                        Text(bytes(r.bytes)).monospacedDigit()
                    }
                }
                if shown.count < rows.count {
                    let rest = rows.dropFirst(shown.count)
                    HStack {
                        Text("and \(rest.count) more folders").foregroundStyle(.secondary)
                        Spacer()
                        Text(bytes(rest.reduce(0) { $0 + $1.bytes })).monospacedDigit().foregroundStyle(.secondary)
                    }
                }
            }
            if !dryRun && !loss.isEmpty {
                Label("\(loss.count) iOS \(loss.count == 1 ? "simulator comes" : "simulators come") back blank: their apps and test data can't be rebuilt.",
                      systemImage: "exclamationmark.triangle")
            }
            HStack {
                Spacer()
                Button("Cancel", role: .cancel) { dismiss() }.keyboardShortcut(.cancelAction)
                Button(dryRun ? "Record \(bytes(total))" : "Delete \(bytes(total))", role: .destructive) { go(); dismiss() }
                    .keyboardShortcut(.defaultAction)
            }
        }
        .padding(20)
        .frame(width: 480)
    }

    /// From History's timed rebuilds of these folders, when there are any.
    private var rebuildTime: String {
        let timed = rows.compactMap { r in history.first { $0.path == r.path && $0.rebuildSecs != nil } }
        guard let first = timed.first else { return "A rebuild usually takes a few minutes and may download packages" }
        if timed.count == 1 {
            let d = Duration.seconds(first.rebuildSecs ?? 0).formatted(.units(allowed: [.hours, .minutes], width: .wide).locale(Locale(identifier: "en")))
            return "Last time, rebuilding \(first.name) took \(d); the others usually take a few minutes each"
        }
        let d = Duration.seconds(timed.compactMap(\.rebuildSecs).reduce(0, +)).formatted(.units(allowed: [.hours, .minutes], width: .wide, maximumUnitCount: 2).locale(Locale(identifier: "en")))
        return "Last time, rebuilding \(timed.count) of these took \(d) in all" + (timed.count < rows.count ? "; the others usually take a few minutes each" : "")
    }
}
