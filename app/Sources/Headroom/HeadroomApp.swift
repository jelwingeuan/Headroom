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
        .defaultSize(width: 800, height: 500)

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
        case .plenty: "gauge.with.dots.needle.67percent"
        case .dryRun: "gauge.with.dots.needle.33percent"
        case .acting: "gauge.badge.minus"
        case .stuck, .failed: "exclamationmark.triangle"
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

/// The one chart, drawn the way Finder draws a disk: used space fills from the left, free space
/// is the empty track on the right. The scale is the whole disk, so it never changes between
/// states, and the budget tick sits where free space would equal the budget.
struct SpaceBar: View {
    let capacity: Int64
    let free: Int64
    let budget: Int64
    let canBeFreed: Int64?
    /// Bytes Headroom is freeing right now (only while acting); drawn in Headroom's teal.
    var freeing: Int64 = 0
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        let cap = Double(max(capacity, free + 1))
        let now = Double(min(freeing, canBeFreed ?? 0))
        let back = Double(canBeFreed ?? 0) - now
        let used = cap - Double(free) - now - back
        VStack(alignment: .leading, spacing: 6) {
            GeometryReader { geo in
                let w = geo.size.width, x = { (b: Double) in max(w * b / cap - 1, 2) }
                HStack(spacing: 2) {
                    Rectangle().fill(Color.primary.opacity(0.55)).frame(width: x(max(used, 0)))
                    // Never a sliver: the job running now keeps at least 8 pt, edged so it reads on gray.
                    if now > 0 {
                        Rectangle().fill(Color.headroom).frame(width: max(x(now), 8))
                            .overlay(Rectangle().strokeBorder(Color.primary.opacity(0.6), lineWidth: 1))
                    }
                    if back > 0 { HatchedBar().frame(width: x(back)) }
                    Rectangle().fill(.quaternary)
                }
                .clipShape(Capsule())
                .overlay(Capsule().strokeBorder(Color.secondary.opacity(0.5), lineWidth: 1)) // the free track shows
                .overlay(alignment: .leading) {
                    // The budget line: label color with a ring of the background, legible on any segment.
                    if budget > 0 {
                        Rectangle().fill(.primary).frame(width: 2, height: 18)
                            .background(Rectangle().fill(.background).frame(width: 4, height: 20))
                            .offset(x: w * (1 - Double(budget) / cap) - 1)
                    }
                }
            }
            .frame(height: 10)
            .padding(.vertical, 4)
            HStack(spacing: 12) {
                Swatch(style: .fill(Color.primary.opacity(0.55)), text: "Used")
                if now > 0 { Swatch(style: .fill(.headroom), text: "Freeing") }
                if back > 0 { Swatch(style: .hatched, text: "Can be freed") }
                Swatch(style: .track, text: "Free")
            }
            GeometryReader { geo in // the budget label hangs under its marker; the disk size sits at the end
                let at = geo.size.width * (1 - Double(budget) / cap)
                ZStack(alignment: .topLeading) {
                    Text("\(bytes(capacity)) disk").foregroundStyle(.secondary).monospacedDigit()
                        .frame(maxWidth: .infinity, alignment: .trailing)
                        .opacity(budget > 0 && at > geo.size.width - 150 ? 0 : 1)
                    if budget > 0 {
                        Label("\(bytes(budget)) budget", systemImage: "line.diagonal").labelStyle(BudgetLabelStyle())
                            .fixedSize()
                            .position(x: min(max(at, 50), geo.size.width - 50), y: 8)
                    }
                }
            }
            .frame(height: 16)
        }
        .font(.subheadline)
        .animation(reduceMotion ? .easeInOut(duration: 0.2) : .spring(response: 0.4, dampingFraction: 1),
                   value: free)
        .accessibilityElement()
        .accessibilityLabel("Disk")
        .accessibilityValue("\(bytes(free)) free of \(bytes(capacity)), budget \(bytes(budget))"
            + (canBeFreed.map { ", \(bytes($0)) can be freed" } ?? ""))
    }
}

/// Diagonal stripes: "can be freed" reads apart from used space by pattern, not by shade.
struct HatchedBar: View {
    var body: some View {
        Canvas { ctx, size in
            ctx.fill(Path(CGRect(origin: .zero, size: size)), with: .color(Color.primary.opacity(0.12)))
            var x: CGFloat = -size.height
            while x < size.width {
                var p = Path()
                p.move(to: CGPoint(x: x, y: size.height))
                p.addLine(to: CGPoint(x: x + size.height, y: 0))
                ctx.stroke(p, with: .color(Color.primary.opacity(0.55)), lineWidth: 1.5)
                x += 4
            }
        }
    }
}

struct Swatch: View {
    enum Style { case fill(Color), hatched, track }
    let style: Style
    let text: String

    var body: some View {
        HStack(spacing: 4) {
            Group {
                switch style {
                case .fill(let c): RoundedRectangle(cornerRadius: 2).fill(c)
                case .hatched: HatchedBar().clipShape(RoundedRectangle(cornerRadius: 2))
                case .track: RoundedRectangle(cornerRadius: 2).fill(.quaternary)
                    .overlay(RoundedRectangle(cornerRadius: 2).strokeBorder(Color.secondary.opacity(0.6)))
                }
            }
            .frame(width: 10, height: 10)
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

            if let free = s.freeBytes, let budget = s.budgetBytes {
                SpaceBar(capacity: s.capacityBytes ?? free, free: free, budget: budget, canBeFreed: s.canComeBack,
                         freeing: mood == .acting ? s.next?.bytes ?? 0 : 0)
            }

            verdict(s, mood)

            if s.dryRun && !planned.isEmpty && mood != .noBudget {
                WouldDoList(rows: Array(planned.prefix(3)), total: planned.count) { open(.wouldHaveDone) }
            }

            if s.snapshotHolding {
                Notice(symbol: "clock.badge.exclamationmark",
                       text: "A Time Machine snapshot holds some freed space for up to a day.")
            }
            if s.needsFullDiskAccess {
                Notice(symbol: "lock.shield",
                       text: "Headroom can't see some folders yet.",
                       action: ("Full Disk Access Settings…", openFullDiskAccessSettings))
            }

            Divider()
            // Real controls, not imitation menu items: a window-style extra gets a footer bar.
            HStack(spacing: 8) {
                Button("Open Headroom") { open(.history) }.keyboardShortcut("o")
                Spacer()
                Menu {
                    Button(model.paused ? "Resume Headroom" : "Pause Headroom") { model.paused.toggle() }
                        .disabled(mood == .noBudget || mood == .failed)
                    Button("Settings…") { openSettings() }.keyboardShortcut(",")
                    Divider()
                    Button("Quit Headroom") { NSApp.terminate(nil) }.keyboardShortcut("q")
                } label: {
                    Image(systemName: "ellipsis.circle")
                }
                .menuStyle(.borderlessButton)
                .menuIndicator(.hidden)
                .fixedSize()
                .help("More")
                .accessibilityLabel("More")
            }
        }
        .padding(16)
        .frame(width: 320)
        .background { // ⌘, and ⌘Q work while the panel is open, even with the More menu closed
            Button("") { openSettings() }.keyboardShortcut(",").opacity(0).accessibilityHidden(true)
            Button("") { NSApp.terminate(nil) }.keyboardShortcut("q").opacity(0).accessibilityHidden(true)
        }
        .onAppear {
            model.refresh()
            model.loadHistory()
        }
    }

    private var planned: [Candidate] { model.actions.filter(\.dryRun) }

    /// The status line: free space, what can come back, and what Headroom will do.
    private func summary(_ s: Status, _ mood: Mood) -> String {
        var parts: [String] = []
        let now = mood == .acting ? s.next?.bytes ?? 0 : 0
        if now > 0 { parts.append("freeing \(bytes(now)) now") }
        if let back = s.canComeBack, back - now > 0 {
            parts.append("\(bytes(back - now)) \(now > 0 ? "more " : "")can be freed")
        }
        switch mood {
        case .paused: parts.append("paused")
        default: break
        }
        let line = parts.joined(separator: " · ")
        return line.prefix(1).uppercased() + line.dropFirst()
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
                   text: "Set a budget: the free space Headroom keeps for you. Until then, it deletes nothing.",
                   action: ("Set a Budget…", openSettings), prominent: true)
        case .paused:
            Notice(symbol: mood.symbol, text: "Paused. Nothing will be deleted until you resume.")
        case .plenty:
            Notice(symbol: mood.symbol, text: "\(bytes(-gap)) above your budget, so there's nothing to do.")
        case .dryRun:
            let days = s.dryRunDaysLeft ?? 0
            Notice(symbol: mood.symbol, text: "\(bytes(gap)) below your budget. This is a dry run, so nothing is deleted"
                + (days > 0 ? " for \(days) more \(days == 1 ? "day" : "days")." : "."))
        case .acting:
            if let next = s.next {
                Notice(symbol: mood.symbol,
                       text: "\(bytes(gap)) below your budget. Freeing \(bytes(next.bytes)) of \(next.what) in \(next.name).",
                       tint: .headroom)
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
    var tint: Color = .secondary

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Group {
                Image(systemName: symbol)
            }
            .foregroundStyle(tint)
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
