import AppKit
import SwiftUI

struct HeadroomApp: App {
    @StateObject private var model = AppModel()

    var body: some Scene {
        MenuBarExtra {
            MenuView().environmentObject(model)
        } label: {
            MenuBarIcon(status: model.status, paused: model.paused)
        }
        .menuBarExtraStyle(.window)

        Window("Headroom", id: "main") {
            MainView().environmentObject(model)
        }
        .defaultSize(width: 720, height: 480)

        SwiftUI.Settings {
            SettingsView().environmentObject(model)
        }
    }
}

/// Where Headroom stands, in one word. Every surface (icon, chart, verdict) reads from this.
enum Mood {
    case unknown, noBudget, paused, plenty, dryRun, acting, stuck, failed

    init(_ s: Status, paused: Bool) {
        if s.error != nil { self = .failed; return }
        guard let free = s.freeBytes, let budget = s.budgetBytes else { self = .unknown; return }
        if budget == 0 { self = .noBudget; return }
        if paused { self = .paused; return }
        if free >= budget { self = .plenty; return }
        if s.dryRun { self = .dryRun; return }
        self = s.next == nil ? .stuck : .acting
    }
}

/// A template symbol whose shape says the state, so it reads at 16 px in any menu bar color.
struct MenuBarIcon: View {
    let status: Status
    let paused: Bool

    var body: some View {
        let mood = Mood(status, paused: paused)
        Image(systemName: symbol(mood))
            .accessibilityLabel("Headroom")
            .accessibilityValue(label(mood))
    }

    private func symbol(_ m: Mood) -> String {
        switch m {
        case .unknown, .noBudget: "gauge.with.dots.needle.bottom.0percent"
        case .paused: "pause.circle"
        case .plenty: "gauge.with.dots.needle.67percent"
        case .dryRun: "gauge.with.dots.needle.33percent"
        case .acting: "arrow.triangle.2.circlepath.circle"
        case .stuck, .failed: "exclamationmark.triangle"
        }
    }

    private func label(_ m: Mood) -> String {
        switch m {
        case .unknown: "measuring free space"
        case .noBudget: "no budget set"
        case .paused: "paused"
        case .plenty: "plenty of free space"
        case .dryRun: "below budget, dry run"
        case .acting: "freeing space"
        case .stuck: "below budget, nothing safe left to free"
        case .failed: "needs attention"
        }
    }
}

/// The one chart: free space against the budget line, with what could still come back.
/// The scale runs from 0 to the larger of the budget and free space, plus a margin, so the
/// budget line sits where it truly is.
struct SpaceBar: View {
    let free: Int64
    let budget: Int64
    let canComeBack: Int64?
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        let top = Double(max(free + (canComeBack ?? 0), budget)) * 1.1
        let f = Double(free) / top, b = Double(budget) / top
        let back = Double(canComeBack ?? 0) / top
        VStack(alignment: .leading, spacing: 4) {
            GeometryReader { geo in
                let w = geo.size.width
                ZStack(alignment: .leading) {
                    Capsule().fill(.quaternary)
                    Rectangle().fill(.tertiary)
                        .frame(width: w * min(f + back, 1))
                    Rectangle().fill(free >= budget ? Color.primary.opacity(0.55) : Color.orange)
                        .frame(width: w * f)
                }
                .clipShape(Capsule())
                .overlay(alignment: .leading) {
                    Rectangle().fill(.primary).frame(width: 2, height: 16).offset(x: w * b - 1)
                }
            }
            .frame(height: 8)
            .padding(.vertical, 4)
            GeometryReader { geo in
                Text("\(bytes(budget)) budget")
                    .font(.caption).foregroundStyle(.secondary).monospacedDigit()
                    .fixedSize()
                    .position(x: min(max(geo.size.width * b, 40), geo.size.width - 40), y: 6)
            }
            .frame(height: 12)
        }
        .animation(reduceMotion ? .easeInOut(duration: 0.2) : .spring(response: 0.4, dampingFraction: 1),
                   value: free)
        .accessibilityElement()
        .accessibilityLabel("Free space")
        .accessibilityValue("\(bytes(free)) free, budget \(bytes(budget))"
            + (canComeBack.map { ", \(bytes($0)) can come back" } ?? ""))
    }
}

struct MenuView: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.openWindow) private var openWindow

    var body: some View {
        let s = model.status
        let mood = Mood(s, paused: model.paused)
        VStack(alignment: .leading, spacing: 12) {
            VStack(alignment: .leading, spacing: 4) {
                if let free = s.freeBytes {
                    Text("\(bytes(free)) free").font(.title2.weight(.semibold)).monospacedDigit()
                } else if mood == .unknown {
                    Text("Measuring free space…").font(.title2.weight(.semibold)).foregroundStyle(.secondary)
                }
                Text(summary(s, mood)).font(.callout).foregroundStyle(.secondary).monospacedDigit()
                    .fixedSize(horizontal: false, vertical: true)
            }

            if let free = s.freeBytes, let budget = s.budgetBytes, budget > 0 {
                SpaceBar(free: free, budget: budget, canComeBack: s.canComeBack)
            }

            verdict(s, mood)

            if s.dryRun && !planned.isEmpty && mood != .noBudget {
                WouldDoList(rows: Array(planned.prefix(3)), total: planned.count) { open(.wouldHaveDone) }
            }

            if s.snapshotHolding {
                Notice(symbol: "clock.badge.exclamationmark",
                       text: "A Time Machine snapshot is holding some freed space. macOS releases it within a day.")
            }
            if s.needsFullDiskAccess {
                Notice(symbol: "lock.shield",
                       text: "Headroom can't see some folders yet.",
                       action: ("Allow Full Disk Access…", openFullDiskAccessSettings))
            }

            Divider()
            VStack(spacing: 0) {
                MenuRow("Open Headroom", shortcut: "o") { open(.history) }
                MenuRow("Settings…", shortcut: ",") { openSettings() }
                MenuRow(model.paused ? "Resume Autopilot" : "Pause Autopilot",
                        disabled: mood == .noBudget || mood == .failed) { model.paused.toggle() }
                Divider().padding(.vertical, 4)
                MenuRow("Quit Headroom", shortcut: "q") { NSApp.terminate(nil) }
            }
        }
        .padding(16)
        .frame(width: 320)
        .onAppear {
            model.refresh()
            model.loadHistory()
        }
    }

    private var planned: [Candidate] { model.actions.filter(\.dryRun) }

    /// The status line: free space, what can come back, and what Headroom will do.
    private func summary(_ s: Status, _ mood: Mood) -> String {
        var parts: [String] = []
        if let back = s.canComeBack, back > 0 { parts.append("\(bytes(back)) can come back") }
        switch mood {
        case .plenty: parts.append("nothing to do")
        case .dryRun: parts.append("dry run, nothing deleted")
        case .acting: parts.append("freeing space")
        case .paused: parts.append("paused")
        default: break
        }
        return parts.joined(separator: " · ")
    }

    @ViewBuilder private func verdict(_ s: Status, _ mood: Mood) -> some View {
        let gap = (s.budgetBytes ?? 0) - (s.freeBytes ?? 0)
        switch mood {
        case .failed:
            Notice(symbol: "exclamationmark.triangle",
                   text: "Headroom can't check your disk right now. Nothing will be deleted until it can.",
                   action: ("Try Again", { model.refresh() }))
        case .noBudget:
            Notice(symbol: "gauge.with.dots.needle.bottom.0percent",
                   text: "Choose how much space to keep free. Headroom won't delete anything until you do.",
                   action: ("Set a Budget…", openSettings))
        case .paused:
            Notice(symbol: "pause.circle", text: "Paused. Nothing will be deleted until you resume.")
        case .plenty:
            Notice(symbol: "checkmark.circle", text: "\(bytes(-gap)) above your budget. Nothing to do.")
        case .dryRun:
            let days = s.dryRunDaysLeft ?? 0
            Notice(symbol: "eye", text: "\(bytes(gap)) below your budget. This is a dry run, so nothing is deleted"
                + (days > 0 ? " for \(days) more \(days == 1 ? "day" : "days")." : "."))
        case .acting:
            if let next = s.next {
                Notice(symbol: "arrow.triangle.2.circlepath",
                       text: "\(bytes(gap)) below your budget. Next: \(next.what) in \(next.name), \(bytes(next.bytes)).")
            }
        case .stuck:
            Notice(symbol: "exclamationmark.triangle",
                   text: "\(bytes(gap)) below your budget, and nothing left is safe to delete. Free some space yourself.")
        case .unknown:
            EmptyView()
        }
    }

    private func open(_ pane: Pane) {
        model.pane = pane
        openWindow(id: "main")
        NSApp.activate(ignoringOtherApps: true)
    }
}

/// Opens the Settings scene (⌘,); macOS 13 has no SwiftUI call for it from a menu bar extra.
func openSettings() {
    NSApp.activate(ignoringOtherApps: true)
    NSApp.sendAction(Selector(("showSettingsWindow:")), to: nil, from: nil)
}

/// A line of status with its symbol. Text stays primary; only the symbol carries color.
struct Notice: View {
    let symbol: String
    let text: String
    var action: (String, () -> Void)?

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Label {
                Text(text).fixedSize(horizontal: false, vertical: true)
            } icon: {
                Image(systemName: symbol).foregroundStyle(.secondary)
            }
            if let action {
                Button(action.0, action: action.1).padding(.leading, 28)
            }
        }
    }
}

/// The dry run's picks, so the week shows what autopilot would really do.
struct WouldDoList: View {
    let rows: [Candidate]
    let total: Int
    let seeAll: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("After the dry run, Headroom would free").font(.caption).foregroundStyle(.secondary)
            ForEach(rows) { r in
                HStack {
                    Text(r.name).lineLimit(1)
                    Text(r.what).foregroundStyle(.secondary).lineLimit(1)
                    Spacer()
                    Text(bytes(r.bytes)).monospacedDigit()
                }
                .font(.callout)
                .accessibilityElement(children: .combine)
            }
            if total > rows.count {
                Button("Show All \(total)…", action: seeAll).buttonStyle(.link).font(.callout)
            }
        }
    }
}

/// A command row that looks and behaves like a menu item.
struct MenuRow: View {
    let title: String
    let shortcut: KeyEquivalent?
    let disabled: Bool
    let action: () -> Void
    @State private var hover = false

    init(_ title: String, shortcut: KeyEquivalent? = nil, disabled: Bool = false, action: @escaping () -> Void) {
        self.title = title
        self.shortcut = shortcut
        self.disabled = disabled
        self.action = action
    }

    var body: some View {
        Button(action: action) {
            HStack {
                Text(title)
                Spacer()
                if let shortcut {
                    Text("⌘\(String(shortcut.character).uppercased())").foregroundStyle(.secondary)
                }
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 4)
            .contentShape(Rectangle())
            .background(hover && !disabled ? Color.accentColor.opacity(0.85) : .clear,
                        in: RoundedRectangle(cornerRadius: 4))
            .foregroundStyle(hover && !disabled ? Color.white : Color.primary)
        }
        .buttonStyle(.plain)
        .disabled(disabled)
        .opacity(disabled ? 0.4 : 1)
        .onHover { hover = $0 }
        .commandShortcut(shortcut)
        .padding(.horizontal, -8)
    }
}

extension View {
    @ViewBuilder func commandShortcut(_ key: KeyEquivalent?) -> some View {
        if let key { keyboardShortcut(key, modifiers: .command) } else { self }
    }
}
