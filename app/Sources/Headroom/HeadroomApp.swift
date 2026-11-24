import AppKit
import SwiftUI

struct HeadroomApp: App {
    @StateObject private var model = AppModel()

    var body: some Scene {
        MenuBarExtra {
            MenuView().environmentObject(model)
        } label: {
            MenuBarIcon(status: model.status, paused: model.paused)
                .background(LaunchHook(model: model))
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
                        .opacity(budget > 0 && at > geo.size.width - 210 ? 0 : 1)
                    if budget > 0 { // the label's tick sits exactly under the marker, flipped near the right end
                        let flip = at > 130 // always left of the marker when it fits, the same in the menu and the window
                        Label("\(bytes(budget)) budget", systemImage: "line.diagonal").labelStyle(BudgetLabelStyle(tickTrailing: flip))
                            .fixedSize()
                            .frame(maxWidth: .infinity, alignment: flip ? .trailing : .leading)
                            .offset(x: flip ? at - geo.size.width + 1 : at - 1)
                    }
                }
            }
            .frame(height: 16)
        }
        .font(.subheadline)
        .animation(reduceMotion ? .easeInOut(duration: 0.2) : .spring(response: 0.4, dampingFraction: 1),
                   value: free)
        // To VoiceOver the chart is a level indicator: how full the disk is, with the details spoken.
        .accessibilityRepresentation {
            ProgressView(value: Double(capacity - free), total: Double(max(capacity, 1))) { Text("Disk") }
                .accessibilityValue("\(bytes(free)) free of \(bytes(capacity)), budget \(bytes(budget))"
                    + (canBeFreed.map { ", \(bytes($0)) can be freed" } ?? ""))
        }
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
                ctx.stroke(p, with: .color(Color.primary.opacity(0.8)), lineWidth: 1.5)
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
    var tickTrailing = false
    func makeBody(configuration: Configuration) -> some View {
        HStack(spacing: 4) {
            if !tickTrailing { Rectangle().fill(.primary).frame(width: 2, height: 10) }
            configuration.title.foregroundStyle(.secondary).monospacedDigit()
            if tickTrailing { Rectangle().fill(.primary).frame(width: 2, height: 10) }
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

            if model.ticking > 0 || model.lastCheck != nil {
                LiveLine(checking: model.ticking > 0, last: model.lastCheck)
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
                Button("Open Headroom") { open(.overview) }.keyboardShortcut("o")
                Spacer()
                // A plain button that opens a real NSMenu: VoiceOver gets a press action, people a menu.
                Button { showMoreMenu(mood) } label: { Image(systemName: "ellipsis.circle") }
                    .buttonStyle(.borderless)
                    .help("More")
                    .accessibilityLabel("More")
            }
        }
        .padding(16)
        .frame(width: 320)
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Headroom")
        .background { // ⌘, and ⌘Q work while the panel is open, even with the More menu closed
            Button("") { openSettings() }.keyboardShortcut(",").opacity(0).accessibilityHidden(true)
            Button("") { NSApp.terminate(nil) }.keyboardShortcut("q").opacity(0).accessibilityHidden(true)
        }
        .onAppear { model.refresh() }
    }

    private func showMoreMenu(_ mood: Mood) {
        let menu = NSMenu()
        let pause = ActionItem(model.paused ? "Resume Headroom" : "Pause Headroom") { model.paused.toggle() }
        pause.isEnabled = !(mood == .noBudget || mood == .failed)
        menu.autoenablesItems = false
        menu.addItem(pause)
        menu.addItem(ActionItem("Settings…", key: ",") { openSettings() })
        menu.addItem(.separator())
        menu.addItem(ActionItem("Quit Headroom", key: "q") { NSApp.terminate(nil) })
        menu.popUp(positioning: nil, at: NSEvent.mouseLocation, in: nil)
    }

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

/// "Checking your disk…" while a tick runs, then what it did and when.
struct LiveLine: View {
    let checking: Bool
    let last: LastCheck?

    var body: some View {
        HStack(spacing: 8) {
            if checking {
                ProgressView().controlSize(.small).frame(width: 20)
                Text("Measuring your folders…")
            } else if let last {
                Image(systemName: last.failed ? "exclamationmark.circle" : last.acted ? "checkmark.circle.fill" : "checkmark.circle")
                    .foregroundStyle(last.acted ? Color.headroom : .secondary)
                    .frame(width: 20)
                // M10: "2 minutes ago", never a bare "00:30" that reads like a duration.
                let when = ago(last.at).replacingOccurrences(of: " ", with: "\u{00A0}") // never "1 / second ago"
                Text("\(last.text) · \(when)").lineLimit(2)
            }
        }
        .font(.callout)
        .foregroundStyle(.secondary)
        .monospacedDigit()
        .accessibilityElement(children: .combine)
    }
}

extension LaunchHook {
    /// Developer-only (HEADROOM_DEMO): every 1.5 s free space swings by 20 GB and one History row
    /// flips between freed and rebuilt, so Instruments can measure the chart and row animations.
    static func demo(_ model: AppModel) {
        var up = false
        Timer.scheduledTimer(withTimeInterval: 1.5, repeats: true) { _ in
            guard var status = Core.fixture?["status"] as? JSON, var history = Core.fixture?["history"] as? JSON,
                  var rows = history["actions"] as? [JSON], let free = status["free_bytes"] as? Int64 else { return }
            up.toggle()
            status["free_bytes"] = free + (up ? 20 : -20) * 1_000_000_000
            if !rows.isEmpty { rows[0]["undone"] = up }
            history["actions"] = rows
            Core.fixture?["status"] = status
            Core.fixture?["history"] = history
            model.refresh()
            model.loadHistory()
        }
    }
}

/// Developer-only: HEADROOM_OPEN=main opens the main window at launch, and HEADROOM_OPEN=menu shows
/// the menu's content in an ordinary window, so UI tests reach them without the menu bar item
/// (which macOS may hide when the menu bar is full).
struct LaunchHook: View {
    let model: AppModel
    @Environment(\.openWindow) private var openWindow

    var body: some View {
        Color.clear.frame(width: 0, height: 0).onAppear {
            switch ProcessInfo.processInfo.environment["HEADROOM_OPEN"] {
            case "main":
                openWindow(id: "main")
            case "settings":
                openSettings()
            case "menu":
                let panel = NSWindow(contentViewController: NSHostingController(
                    rootView: MenuView().environmentObject(model)))
                panel.title = "Headroom Menu"
                panel.makeKeyAndOrderFront(nil)
            default:
                return
            }
            NSApp.activate(ignoringOtherApps: true)
            if ProcessInfo.processInfo.environment["HEADROOM_DEMO"] != nil { Self.demo(model) }
        }
    }
}

/// An NSMenuItem that runs a closure, for menus built in code.
final class ActionItem: NSMenuItem {
    private let run: () -> Void

    init(_ title: String, key: String = "", run: @escaping () -> Void) {
        self.run = run
        super.init(title: title, action: #selector(fire), keyEquivalent: key)
        target = self
    }

    @available(*, unavailable) required init(coder: NSCoder) { fatalError() }

    @objc private func fire() { run() }
}
