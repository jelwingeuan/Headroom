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

    /// One glyph per state, shared by the menu bar icon and the menu's verdict line.
    var symbol: String {
        switch self {
        case .unknown, .noBudget: "gauge.with.dots.needle.bottom.0percent"
        case .paused: "pause.circle"
        case .plenty: "checkmark.circle"
        case .dryRun: "eye.circle"
        case .acting: "arrow.triangle.2.circlepath.circle"
        case .stuck, .failed: "exclamationmark.circle"
        }
    }

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
        Image(systemName: mood.symbol)
            .accessibilityLabel("Headroom")
            .accessibilityValue(label(mood))
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
    /// Bytes Headroom is freeing right now (only while acting); drawn in Headroom's teal.
    var freeing: Int64 = 0
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        // The track ends at the larger of the budget and what free space could reach: no padding.
        let top = Double(max(free + (canComeBack ?? 0), budget))
        let f = Double(free) / top, b = Double(budget) / top
        let now = Double(min(freeing, canComeBack ?? 0)) / top
        let back = Double(canComeBack ?? 0) / top - now
        // Free is always the same color; teal appears only on what Headroom is freeing right now.
        let freeColor = Color.primary.opacity(0.7)
        let backColor = Color.secondary.opacity(0.45)
        VStack(alignment: .leading, spacing: 6) {
            GeometryReader { geo in
                let w = geo.size.width
                // Segments sit apart by a 2 pt gap, so they read as separate without relying on hue.
                HStack(spacing: 2) {
                    Rectangle().fill(freeColor).frame(width: max(w * f - 1, 2))
                    if now > 0 { Rectangle().fill(Color.headroom).frame(width: max(w * now - 1, 2)) }
                    if back > 0 { Rectangle().fill(backColor).frame(width: max(w * back - 1, 2)) }
                    Rectangle().fill(.quaternary)
                }
                .clipShape(Capsule())
                .overlay(alignment: .leading) {
                    // The budget line: label color with a ring of the background, legible on any segment.
                    Rectangle().fill(.primary).frame(width: 2, height: 18)
                        .background(Rectangle().fill(.background).frame(width: 4, height: 20))
                        .offset(x: w * b - 1)
                }
            }
            .frame(height: 10)
            .padding(.vertical, 4)
            let swatches = HStack(spacing: 12) {
                Swatch(color: freeColor, text: "Free")
                if now > 0 { Swatch(color: .headroom, text: "Freeing now") }
                if back > 0 { Swatch(color: backColor, text: "Can be freed") }
            }
            let budgetLabel = Label("\(bytes(budget)) budget", systemImage: "line.diagonal").labelStyle(BudgetLabelStyle())
            ViewThatFits(in: .horizontal) {
                HStack(spacing: 12) { swatches; Spacer(minLength: 0); budgetLabel }
                VStack(alignment: .leading, spacing: 4) { swatches; budgetLabel }
            }
            .font(.subheadline)
        }
        .animation(reduceMotion ? .easeInOut(duration: 0.2) : .spring(response: 0.4, dampingFraction: 1),
                   value: free)
        .accessibilityElement()
        .accessibilityLabel("Free space chart")
        .accessibilityValue("\(bytes(free)) free, budget \(bytes(budget))"
            + (canComeBack.map { ", \(bytes($0)) more can be freed" } ?? ""))
    }
}

struct Swatch: View {
    let color: Color
    let text: String

    var body: some View {
        HStack(spacing: 4) {
            RoundedRectangle(cornerRadius: 2).fill(color).frame(width: 10, height: 10)
            Text(text).foregroundStyle(.secondary).fixedSize()
        }
    }
}

/// "| 60 GB budget": a short vertical tick like the one on the chart, then the label.
struct BudgetLabelStyle: LabelStyle {
    func makeBody(configuration: Configuration) -> some View {
        HStack(spacing: 4) {
            Rectangle().fill(.primary).frame(width: 2, height: 10)
            configuration.title.foregroundStyle(.secondary).monospacedDigit()
        }
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
                } else {
                    Text(mood == .failed ? "Free space unknown" : "Measuring free space…")
                        .font(.title2.weight(.semibold))
                }
                let line = summary(s, mood)
                if !line.isEmpty {
                    Text(line).font(.callout).monospacedDigit().fixedSize(horizontal: false, vertical: true)
                }
            }

            if let free = s.freeBytes, let budget = s.budgetBytes, budget > 0 {
                SpaceBar(free: free, budget: budget, canComeBack: s.canComeBack,
                         freeing: mood == .acting ? s.next?.bytes ?? 0 : 0)
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
                MenuRow(model.paused ? "Resume Headroom" : "Pause Headroom",
                        disabled: mood == .noBudget || mood == .failed) { model.paused.toggle() }
                Divider().padding(.vertical, 4)
                MenuRow("Quit Headroom", shortcut: "q") { NSApp.terminate(nil) }
            }
        }
        .padding(16)
        .frame(width: 320)
        .tint(.headroom)
        .onAppear {
            model.refresh()
            model.loadHistory()
        }
    }

    private var planned: [Candidate] { model.actions.filter(\.dryRun) }

    /// The status line: free space, what can come back, and what Headroom will do.
    private func summary(_ s: Status, _ mood: Mood) -> String {
        var parts: [String] = []
        if let back = s.canComeBack, back > 0 { parts.append("\(bytes(back)) can be freed") }
        switch mood {
        case .plenty: parts.append("nothing to do")
        case .dryRun: parts.append("nothing will be deleted yet")
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
            Notice(symbol: mood.symbol,
                   text: "Headroom couldn't read your disk. Nothing will be deleted until it can. If Try Again doesn't help, quit and reopen Headroom.",
                   action: ("Try Again", { model.refresh() }), prominent: true)
        case .noBudget:
            Notice(symbol: mood.symbol,
                   text: "Choose how much space to keep free. Headroom won't delete anything until you do.",
                   action: ("Set a Budget…", openSettings), prominent: true)
        case .paused:
            Notice(symbol: mood.symbol, text: "Paused. Nothing will be deleted until you resume.")
        case .plenty:
            Notice(symbol: mood.symbol, text: "\(bytes(-gap)) above your budget. Nothing to do.")
        case .dryRun:
            let days = s.dryRunDaysLeft ?? 0
            Notice(symbol: mood.symbol, text: "\(bytes(gap)) below your budget. This is a dry run, so nothing is deleted"
                + (days > 0 ? " for \(days) more \(days == 1 ? "day" : "days")." : "."))
        case .acting:
            if let next = s.next {
                Notice(symbol: mood.symbol,
                       text: "\(bytes(gap)) below your budget. Freeing \(bytes(next.bytes)) of \(next.what) in \(next.name).",
                       busy: true)
            }
        case .stuck:
            Notice(symbol: mood.symbol,
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

extension Color {
    /// Headroom's teal (#0F6E5C), lifted in dark mode so it keeps its contrast. Used only where
    /// it means "Headroom is acting or acted".
    static let headroom = Color(nsColor: NSColor(name: nil) { look in
        look.bestMatch(from: [.darkAqua, .accessibilityHighContrastDarkAqua]) == nil
            ? NSColor(srgbRed: 0.059, green: 0.431, blue: 0.361, alpha: 1)
            : NSColor(srgbRed: 0.247, green: 0.702, blue: 0.604, alpha: 1)
    })
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
    var prominent = false
    var busy = false

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Group {
                if busy { ProgressView().controlSize(.small) } else { Image(systemName: symbol) }
            }
            .foregroundStyle(.secondary)
            .frame(width: 20)
            VStack(alignment: .leading, spacing: 8) {
                Text(text).fixedSize(horizontal: false, vertical: true)
                if let action {
                    if prominent {
                        Button(action.0, action: action.1).buttonStyle(.borderedProminent)
                    } else {
                        Button(action.0, action: action.1)
                    }
                }
            }
        }
        .accessibilityElement(children: .combine)
    }
}

/// The dry run's picks, so the week shows what autopilot would really do.
struct WouldDoList: View {
    let rows: [Candidate]
    let total: Int
    let seeAll: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("First to go when the dry run ends").font(.subheadline).foregroundStyle(.secondary)
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
