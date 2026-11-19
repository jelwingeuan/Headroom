import AppKit
import SwiftUI

/// Developer-only screenshot mode (HEADROOM_RENDER=<folder>): draws every screen in every
/// appearance from fixed sample data, so each design round is judged on identical pictures.
enum Screens {
    static let GB: Int64 = 1_000_000_000
    static let now = Int64(Date().timeIntervalSince1970)

    static func row(_ id: String, _ path: String, _ kind: String, _ gb: Double, daysAgo: Int64,
                    dryRun: Bool = false, undone: Bool = false, rebuild: String? = nil) -> JSON {
        var r: JSON = ["id": id, "path": path, "kind": kind, "bytes": Int64(gb * 1e9),
                       "at": now - daysAgo * 86_400, "dry_run": dryRun, "undone": undone,
                       "reason": "not opened for 94 days"]
        if let rebuild { r["rebuild"] = rebuild }
        return r
    }

    static let done = [
        row("1", "/Users/me/code/shop/node_modules", "node_modules", 1.2, daysAgo: 0, rebuild: "npm ci"),
        row("2", "/Users/me/code/engine/target", "target", 4.8, daysAgo: 1, rebuild: "cargo build"),
        row("3", "/Users/me/Library/Developer/Xcode/DerivedData/Atlas-bxjqk", "DerivedData", 2.1, daysAgo: 3,
            undone: true, rebuild: "xcodebuild"),
        row("4", "/Users/me/code/ml/.venv", ".venv", 0.9, daysAgo: 6, rebuild: "pip install -r requirements.txt"),
    ]
    static let wouldHave = [
        row("d1", "/Users/me/code/blog/node_modules", "node_modules", 0.6, daysAgo: 0, dryRun: true),
        row("d2", "/Users/me/code/game/target", "target", 6.3, daysAgo: 1, dryRun: true),
        row("d3", "/Users/me/.ollama/models/llama3.2", "model", 2.0, daysAgo: 2, dryRun: true),
    ]
    static func settings(_ budgetGB: Int64, dryRun: Bool) -> JSON {
        ["budget_bytes": budgetGB * GB, "recent_days": 14, "dry_run": dryRun]
    }

    /// name -> (status reply, history rows, settings reply); each world is self-consistent.
    static let worlds: [(String, JSON, [JSON], JSON)] = [
        ("first-run", ["free_bytes": 18 * GB, "budget_bytes": 0, "dry_run": true, "dry_run_days_left": 7,
                       "permission_errors": 3], [], settings(0, dryRun: true)),
        ("dry-run", ["free_bytes": 52 * GB, "budget_bytes": 60 * GB, "dry_run": true, "dry_run_days_left": 4,
                     "next": wouldHave[1], "reclaimable_bytes": 38 * GB, "protected_bytes": 21 * GB],
         wouldHave, settings(60, dryRun: true)),
        ("plenty", ["free_bytes": 112 * GB, "budget_bytes": 60 * GB, "dry_run": false,
                    "reclaimable_bytes": 24 * GB, "protected_bytes": 19 * GB],
         done + wouldHave, settings(60, dryRun: false)),
        ("acting", ["free_bytes": 57 * GB, "budget_bytes": 60 * GB, "dry_run": false, "next": done[1],
                    "snapshot_holding": true, "reclaimable_bytes": 9 * GB], done, settings(60, dryRun: false)),
        ("error", ["error": "Headroom's core gave no reply. Quit and reopen Headroom."], [], settings(60, dryRun: false)),
    ]

    static let looks: [(String, NSAppearance.Name)] = [
        ("light", .aqua), ("dark", .darkAqua),
        ("light-contrast", .accessibilityHighContrastAqua), ("dark-contrast", .accessibilityHighContrastDarkAqua),
    ]

    static func render(to dir: URL) {
        let app = NSApplication.shared
        app.setActivationPolicy(.accessory)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        for (world, status, rows, settings) in worlds {
            let fixture: JSON = ["status": status, "history": ["actions": rows], "settings_get": settings]
            for (look, name) in looks {
                let appearance = NSAppearance(named: name)!
                let menu = AppModel(fixture: fixture, pane: .history)
                shoot(MenuView().environmentObject(menu).background(.background),
                      size: nil, appearance, dir.appendingPathComponent("menu-\(world)-\(look).png"))
                shoot(MenuBarIcon(status: menu.status, paused: false).font(.system(size: 14)).padding(4),
                      size: nil, appearance, dir.appendingPathComponent("icon-\(world)-\(look).png"))
                guard world == "plenty" || world == "first-run" else { continue }
                shoot(SettingsView().environmentObject(menu), size: nil, appearance,
                      dir.appendingPathComponent("settings-\(world)-\(look).png"))
                for pane in Pane.allCases {
                    let model = AppModel(fixture: fixture, pane: pane)
                    let title = (pane.rawValue, pane == .history ? HistoryView.subtitle(model.actions)
                        : WouldHaveDoneView.subtitle(model.actions, dryRun: model.status.dryRun))
                    shoot(MainView().environmentObject(model), size: CGSize(width: 720, height: 480), appearance, title: title,
                          dir.appendingPathComponent("window-\(pane.id.rawValue.lowercased().replacingOccurrences(of: " ", with: "-"))-\(world)-\(look).png"))
                }
            }
        }
    }

    /// Hosts the view in an offscreen window so AppKit-backed controls (List, Form, Toggle) draw too.
    private static func shoot<V: View>(_ view: V, size: CGSize?, _ appearance: NSAppearance, title: (String, String)? = nil,
                                       _ url: URL) {
        let controller = NSHostingController(rootView: view)
        let fit = size ?? controller.view.fittingSize
        let titled = title != nil
        let style: NSWindow.StyleMask = titled ? [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView]
            : [.borderless]
        let window = KeyWindow(contentRect: CGRect(origin: CGPoint(x: -10_000, y: 0), size: fit),
                               styleMask: style, backing: .buffered, defer: false)
        window.appearance = appearance
        window.contentViewController = controller // lets SwiftUI set the title, subtitle and toolbar
        window.setContentSize(fit)
        if let title { // offscreen, SwiftUI doesn't push navigationTitle into the window: do it here
            window.title = title.0
            window.subtitle = title.1
        }
        window.makeKeyAndOrderFront(nil) // an active window, so selection and switches show their real color
        RunLoop.main.run(until: Date().addingTimeInterval(0.5))
        // A titled window is drawn with its title bar: capture the frame view, not just the content.
        let host = titled ? (window.contentView?.superview ?? controller.view) : controller.view
        host.layoutSubtreeIfNeeded()
        guard let rep = host.bitmapImageRepForCachingDisplay(in: host.bounds) else { return }
        host.cacheDisplay(in: host.bounds, to: rep)
        try? rep.representation(using: .png, properties: [:])?.write(to: url)
        window.orderOut(nil)
    }
}

private final class KeyWindow: NSWindow {
    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { true }
}
