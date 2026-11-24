import AppKit
import SwiftUI

/// Developer-only screenshot mode (HEADROOM_RENDER=<folder>): draws every screen in every
/// appearance from fixed sample data, so each design round is judged on identical pictures.
enum Screens {
    static let GB: Int64 = 1_000_000_000
    static let now = Int64(Date().timeIntervalSince1970)
    /// Set while rendering: charts start in place, so a still shows them whole.
    static var rendering = false

    static func row(_ id: String, _ path: String, _ kind: String, _ gb: Double, daysAgo: Double,
                    dryRun: Bool = false, undone: Bool = false, rebuild: String? = nil) -> JSON {
        var r: JSON = ["id": id, "path": path, "kind": kind, "bytes": Int64(gb * 1e9),
                       "at": now - Int64(daysAgo * 86_400), "dry_run": dryRun, "undone": undone,
                       "reason": "git ignores it; last used \(30 + Int(daysAgo)) days ago"]
        if let rebuild { r["rebuild"] = rebuild }
        return r
    }

    static let done = [
        row("1", "/Users/me/code/shop/node_modules", "node_modules", 1.2, daysAgo: 0.2, rebuild: "npm ci"),
        row("2", "/Users/me/code/engine/target", "target", 4.8, daysAgo: 1, rebuild: "cargo build"),
        row("3", "/Users/me/Library/Developer/Xcode/DerivedData/Atlas-bxjqk", "DerivedData", 2.1, daysAgo: 3,
            undone: true, rebuild: "xcodebuild").merging(["rebuild_secs": 184]) { $1 },
        row("4", "/Users/me/code/ml/.venv", ".venv", 0.9, daysAgo: 6, rebuild: "pip install -r requirements.txt"),
    ]
    static let wouldHave = [
        row("d1", "/Users/me/code/blog/node_modules", "node_modules", 0.6, daysAgo: 0, dryRun: true),
        row("d2", "/Users/me/code/game/target", "target", 6.3, daysAgo: 1, dryRun: true),
        row("d3", "/Users/me/.ollama/models/llama3.2", "model", 2.0, daysAgo: 2, dryRun: true),
    ]
    /// Docker's space stays inside its VM disk: a real action that frees nothing on this disk.
    static let docker = row("5", "docker://build-cache", "docker_build_cache", 3.0, daysAgo: 2)

    static func gigs(_ gb: Double) -> Int64 { Int64((gb * 1e9).rounded()) }

    static func entry(_ path: String, _ kind: String, _ gb: Double, _ reason: String, free: Bool = false, project: String? = nil) -> JSON {
        var e: JSON = ["path": path, "kind": kind, "bytes": gigs(gb), "free": free, "reason": reason, "frees_host": !kind.hasPrefix("docker")]
        if let project { e["project"] = project }
        return e
    }

    /// What every world's last scan kept: 20 GB listed, plus 1 GB of node_modules too small to list.
    static let kept = [
        entry("/Users/me/code/api/target", "target", 9.8, "used 3 days ago (recent: the last 14 days)"),
        entry("/Users/me/.claude/projects", "ai_history", 3.2, "AI conversation history: never deleted"),
        entry("/Users/me/Library/Developer/CoreSimulator/Devices/5C1E9A2B-77D0-4F7B-9E61-2C1A0B7F3D44", "simulator", 2.6,
              "the newest iPhone 16 Pro simulator of its type is always kept"),
        entry("/Users/me/code/app/.claude/worktrees/fix-login", "worktree", 2.4,
              "in use: node is working in /Users/me/code/app/.claude/worktrees/fix-login"),
        entry("/Users/me/code/store/node_modules", "node_modules", 1.1, "used 2 days ago (recent: the last 14 days)"),
        entry("/Users/me/Library/Application Support/Claude/Cache", "electron_cache", 0.5, "Claude is running"),
        entry("/Users/me/code/notes/build", "build", 0.4, "git tracks files inside it"),
    ]
    /// status's split of the same 21 GB, typed in separately so the self-check compares two sources.
    static let keptByReason: JSON = ["recent": gigs(10.9), "in_use": gigs(2.9), "history": gigs(3.2), "other": gigs(3.0)]

    /// What the dry-run world's scan could free: 36.5 GB listed (plus 1.5 GB unlisted).
    static let canFree = [
        entry("/Users/me/.cache/huggingface/hub/models--mistralai--Mistral-7B-v0.1", "hf_model", 14.5,
              "Hugging Face model mistralai/Mistral-7B-v0.1, downloaded from the Hub; last used 71 days ago", free: true),
        entry("/Users/me/Library/Developer/Xcode/DerivedData/Atlas-bxjqk", "DerivedData", 7.4,
              "Xcode build data for /Users/me/code/atlas/Atlas.xcodeproj; last used 38 days ago", free: true, project: "/Users/me/code/atlas"),
        entry("/Users/me/code/game/target", "target", 6.3,
              "Rust project (Cargo.toml); git ignores it; last used 41 days ago; rebuild: cargo build", free: true, project: "/Users/me/code/game"),
        entry("/Users/me/code/game/web/node_modules", "node_modules", 0.8,
              "Node project (package.json); git ignores it; last used 41 days ago; rebuild: npm ci", free: true, project: "/Users/me/code/game"),
        entry("/Users/me/Library/Developer/Xcode/iOS DeviceSupport/iPhone15,2 17.5 (21F79)", "device_support", 5.1,
              "symbols copied from an older device OS", free: true),
        entry("/Users/me/.ollama/models/manifests/registry.ollama.ai/library/llama3.2", "ollama_model", 2.0,
              "Ollama registry model llama3.2; last used 63 days ago", free: true),
        entry("/Users/me/code/blog/node_modules", "node_modules", 0.6,
              "Node project (package.json); git ignores it; last used 92 days ago; rebuild: npm ci", free: true, project: "/Users/me/code/blog"),
        entry("/Users/me/code/site/.next", ".next", 0.4,
              "Node project (package.json); git ignores it; last used 55 days ago; rebuild: npm run build", free: true, project: "/Users/me/code/site"),
        entry("/Users/me/Library/Developer/CoreSimulator/Devices/9A0B7C21-4D3E-4F5A-8B6C-7D8E9F0A1B2C", "simulator", 1.9,
              "iPhone 14 simulator; last used 64 days ago; not the newest of its type", free: true),
        entry("/Users/me/.local/share/claude/versions/2.0.14", "claude_code_old", 0.2,
              "an old Claude Code version: not the one ~/.local/bin/claude runs, not the newest", free: true),
    ]

    /// Docker: listed for the Overview, outside every total (its space is inside Docker's disk).
    static let dockerEntry = entry("docker://build-cache", "docker_build_cache", 3.4, "Docker build cache; last used 30 days ago", free: true)

    /// A `breakdown` reply: these entries plus `kept`, and per-kind totals that also count the
    /// node_modules too small to be listed.
    static func scan(_ free: [JSON], unlisted: Double) -> JSON {
        var kinds: JSON = ["node_modules": ["free_bytes": gigs(unlisted), "protected_bytes": Int64(0)] as JSON]
        for e in free + kept where e.bool("frees_host") == true {
            let key = e.bool("free")! ? "free_bytes" : "protected_bytes"
            var k = kinds.object(e.string("kind")!) ?? ["free_bytes": Int64(0), "protected_bytes": Int64(0)]
            k[key] = k.int(key)! + e.int("bytes")!
            kinds[e.string("kind")!] = k
        }
        return ["entries": (free + kept + [dockerEntry]).sorted { $0.int("bytes")! > $1.int("bytes")! }, "kinds": kinds,
                "totals_at": now - 7200]
    }

    /// A `samples` reply: hourly free space for `days` days up to now. The disk fills by `drift` GB
    /// a day with a daily build rhythm, each delete in `rows` that stuck gives its bytes back at
    /// its time, `jump` is a cleanup of the user's own (days ago, GB), and the Mac slept for 12
    /// hours about 8 days ago.
    static func samples(_ days: Int, free: Int64, budget: Int64, drift: Double, rows: [JSON] = [],
                        jump: (Double, Double)? = nil) -> JSON {
        let stayed = rows.filter { $0.bool("dry_run") != true && $0.bool("undone") != true && !$0.string("kind")!.hasPrefix("docker") }
        let hours = (0..<days * 24).reversed().filter { !(200..<212).contains($0) }
        return ["samples": hours.map { h -> JSON in
            let at = now - Int64(h) * 3600
            var gb = Double(free) / 1e9 + drift * Double(h) / 24 + 1.5 * sin(Double(h) * .pi / 12)
            gb -= stayed.filter { $0.int("at")! > at }.reduce(0) { $0 + Double($1.int("bytes")!) / 1e9 }
            if let (ago, size) = jump, Double(h) > ago * 24 { gb -= size }
            return ["at": at, "free": Int64(gb * 1e9), "budget": budget]
        }]
    }

    static func reclaimed(_ r: JSON, event: String = "reclaim") -> JSON {
        ["event": event, "path": r["path"]!, "kind": r["kind"]!, "bytes": r["bytes"]!, "reason": r["reason"]!]
    }

    static let failed: JSON = ["error": "Headroom's core gave no reply. Quit and reopen Headroom."]

    /// The Insights ops per world. first-run has no tick reply: its first check is still running.
    static let insights: [String: JSON] = [
        "first-run": ["samples": ["samples": [JSON]()], "breakdown": ["entries": NSNull(), "kinds": NSNull(), "totals_at": NSNull()]],
        "dry-run": ["samples": samples(3, free: 52 * GB, budget: 60 * GB, drift: 3), "breakdown": scan(canFree, unlisted: 0),
                    "tick": ["events": [reclaimed(wouldHave[1], event: "would_reclaim")]]],
        "plenty": ["samples": samples(30, free: 112 * GB, budget: 60 * GB, drift: 1.6, rows: done, jump: (0.1, 55)),
                   "breakdown": scan(Array(canFree.dropFirst()), unlisted: 0), "tick": ["events": [JSON]()]],
        "acting": ["samples": samples(30, free: 57 * GB, budget: 60 * GB, drift: 0.7, rows: done),
                   "breakdown": scan([
                       entry("/Users/me/code/engine/target", "target", 4.8,
                             "Rust project (Cargo.toml); git ignores it; last used 33 days ago; rebuild: cargo build", free: true, project: "/Users/me/code/engine"),
                       entry("/Users/me/Library/Developer/Xcode/iOS DeviceSupport/iPhone15,2 17.5 (21F79)", "device_support", 2.1,
                             "symbols copied from an older device OS", free: true),
                   ] + canFree.suffix(3), unlisted: 0),
                   "tick": ["events": [reclaimed(done[0])]]],
        "error": ["samples": failed, "breakdown": failed, "tick": failed],
    ]

    static func settings(_ budgetGB: Int64, dryRun: Bool) -> JSON {
        ["budget_bytes": budgetGB * GB, "recent_days": 14, "dry_run": dryRun]
    }

    /// name -> (status reply, history rows, settings reply); each world is self-consistent.
    static let worlds: [(String, JSON, [JSON], JSON)] = [
        ("first-run", ["capacity_bytes": 245 * GB, "free_bytes": 18 * GB, "budget_bytes": 0, "dry_run": true, "dry_run_days_left": 7,
                       "permission_errors": 3], [], settings(0, dryRun: true)),
        ("dry-run", ["capacity_bytes": 245 * GB, "free_bytes": 52 * GB, "budget_bytes": 60 * GB, "dry_run": true, "dry_run_days_left": 4,
                     "next": wouldHave[1], "reclaimable_bytes": gigs(39.2), "protected_bytes": 20 * GB,
                     "protected_by_reason": keptByReason, "totals_at": now - 7200],
         wouldHave, settings(60, dryRun: true)),
        ("plenty", ["capacity_bytes": 245 * GB, "free_bytes": 112 * GB, "budget_bytes": 60 * GB, "dry_run": false,
                    "reclaimable_bytes": gigs(24.7), "protected_bytes": 20 * GB,
                    "protected_by_reason": keptByReason, "totals_at": now - 7200],
         done + [docker] + wouldHave, settings(60, dryRun: false)),
        ("acting", ["capacity_bytes": 245 * GB, "free_bytes": 57 * GB, "budget_bytes": 60 * GB, "dry_run": false, "next": done[1],
                    "snapshot_holding": true, "reclaimable_bytes": gigs(9.4), "protected_bytes": 20 * GB,
                    "protected_by_reason": keptByReason, "totals_at": now - 7200], done, settings(60, dryRun: false)),
        ("error", ["error": "Headroom's core gave no reply. Quit and reopen Headroom."], [], settings(60, dryRun: false)),
    ]

    static let looks: [(String, NSAppearance.Name)] = [
        ("light", .aqua), ("dark", .darkAqua),
        ("light-contrast", .accessibilityHighContrastAqua), ("dark-contrast", .accessibilityHighContrastDarkAqua),
    ]

    static func fixture(_ world: String) -> JSON? {
        guard let (_, status, rows, settings) = worlds.first(where: { $0.0 == world }) else { return nil }
        var f = (["status": status, "history": ["actions": rows], "settings_get": settings,
                  "undo": ["ok": true], "settings_set": settings] as JSON).merging(insights[world] ?? [:]) { $1 }
        f["rescan"] = f["breakdown"]
        // The fixtures live in /Users/me; on screen they read as this Mac's home, so ~/… shows.
        let text = String(data: try! JSONSerialization.data(withJSONObject: f), encoding: .utf8)!
            .replacingOccurrences(of: "\\/Users\\/me", with: NSHomeDirectory().replacingOccurrences(of: "/", with: "\\/"))
        f = try! JSONSerialization.jsonObject(with: Data(text.utf8)) as! JSON
        f["reclaim"] = ["events": [JSON](), "breakdown": f["breakdown"] ?? [:]]
        return f
    }

    static func render(to dir: URL) {
        let app = NSApplication.shared
        app.setActivationPolicy(.accessory)
        app.applicationIconImage = NSImage(contentsOfFile: "app/Resources/AppIcon.icns") // no bundle when rendering
        rendering = true
        let failures = selfCheck()
        failures.forEach { print("SELF-CHECK FAIL \($0)") }
        print("SELF-CHECK \(failures.isEmpty ? "passed" : "failed: \(failures.count)")")
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        for (world, _, _, _) in worlds {
            let fixture = Self.fixture(world)!
            for (look, name) in looks {
                let appearance = NSAppearance(named: name)!
                let menu = AppModel(fixture: fixture, pane: .history)
                // The whole Insights pane, scrolled out flat, at the detail column's width.
                shoot(InsightsContent().environmentObject(menu).padding(20).frame(width: 620)
                    .background(Color(nsColor: .windowBackgroundColor)),
                      size: nil, appearance, dir.appendingPathComponent("insights-\(world)-\(look).png"))
                shoot(MenuView().environmentObject(menu).background(.background),
                      size: nil, appearance, dir.appendingPathComponent("menu-\(world)-\(look).png"))
                shoot(MenuBarIcon(status: menu.status, paused: false).font(.system(size: 14)).padding(4),
                      size: nil, appearance, dir.appendingPathComponent("icon-\(world)-\(look).png"))
                guard world != "error" else { continue }
                shoot(SettingsView().environmentObject(menu), size: nil, appearance,
                      dir.appendingPathComponent("settings-\(world)-\(look).png"))
                // The whole Overview list, Protected open, and the sheet its button opens.
                let overview = AppModel(fixture: fixture, pane: .overview)
                shoot(OverviewView().environmentObject(overview), size: CGSize(width: 760, height: 1900), appearance,
                      dir.appendingPathComponent("overview-full-\(world)-\(look).png"))
                let picks = Overview(overview.breakdown).freeable.filter { $0.info.unticked == nil }
                if !picks.isEmpty {
                    shoot(ConfirmSheet(rows: picks, dryRun: overview.status.dryRun, history: overview.actions) {}
                        .background(Color(nsColor: .windowBackgroundColor)), size: nil, appearance, dir.appendingPathComponent("sheet-\(world)-\(look).png"))
                }
                for pane in Pane.allCases {
                    let model = AppModel(fixture: fixture, pane: pane)
                    let title = (pane.rawValue, "")
                    shoot(MainView().environmentObject(model), size: CGSize(width: 960, height: 640), appearance, title: title,
                          dir.appendingPathComponent("window-\(pane.rawValue.lowercased())-\(world)-\(look).png"))
                }
            }
        }
        if !failures.isEmpty { exit(1) }
    }

    /// Insights' figures against the core's own numbers: per world, the breakdown's kinds add up to
    /// status's totals and the reasons to its protected bytes; question 4's week, worked out from
    /// what the real core's history op returns, matches the real core's digest of the same rows;
    /// and the menu's live line reads as designed.
    static func selfCheck() -> [String] {
        var failures: [String] = []
        func expect(_ ok: Bool, _ what: @autoclosure () -> String) { if !ok { failures.append(what()) } }
        for (world, status, rows, _) in worlds {
            let f = fixture(world)!, s = Status(status)
            let i = Insights(status: s, actions: rows.map(Candidate.init), samples: f.object("samples")!, breakdown: f.object("breakdown")!)
            let free = i.kinds.reduce(0) { $0 + $1.free }, kept = i.kinds.reduce(0) { $0 + $1.kept }
            let byReason = i.reasons.reduce(0) { $0 + $1.kept }
            expect(free == s.canComeBack ?? 0, "\(world): kinds' can-be-freed \(free) ≠ status reclaimable_bytes \(s.canComeBack ?? 0)")
            expect(kept == s.protectedBytes ?? 0, "\(world): kinds' protected \(kept) ≠ status protected_bytes \(s.protectedBytes ?? 0)")
            expect(byReason == s.protectedBytes ?? 0, "\(world): reasons \(byReason) ≠ status protected_bytes \(s.protectedBytes ?? 0)")
            print("SELF-CHECK \(world): can be freed \(free) = \(s.canComeBack ?? 0), protected \(kept) = by reason \(byReason) = \(s.protectedBytes ?? 0)")
        }

        // The real core, on a throwaway HEADROOM_HOME holding the plenty world's real actions.
        let home = FileManager.default.temporaryDirectory.appendingPathComponent("headroom-self-check-\(getpid())")
        let old = ProcessInfo.processInfo.environment["HEADROOM_HOME"], saved = Core.fixture
        defer {
            try? FileManager.default.removeItem(at: home)
            if let old { setenv("HEADROOM_HOME", old, 1) } else { unsetenv("HEADROOM_HOME") }
            Core.fixture = saved
        }
        try? FileManager.default.createDirectory(at: home, withIntermediateDirectories: true)
        let lines = (done + [docker]).map { r -> String in
            var r = r
            r["why"] = r["reason"]
            r["origin"] = "autopilot"
            r["frees_host"] = !r.string("kind")!.hasPrefix("docker_") // as the core records it
            return String(data: try! JSONSerialization.data(withJSONObject: r), encoding: .utf8)!
        }
        try? (lines.joined(separator: "\n") + "\n").write(to: home.appendingPathComponent("history.jsonl"), atomically: true, encoding: .utf8)
        setenv("HEADROOM_HOME", home.path, 1)
        Core.fixture = nil
        let history = Core.call(["op": "history"]), digest = Core.call(["op": "digest"])
        let week = Insights(status: Status([:]), actions: history.objects("actions").map(Candidate.init), samples: [:], breakdown: [:]).week
        // digest counts a folder as freed even if it was rebuilt later; Insights shows only what stayed freed.
        expect(digest.int("freed_bytes") == week.freed + week.rebuiltBytes,
               "week freed \(week.freed) + rebuilt \(week.rebuiltBytes) ≠ digest freed_bytes \(digest.int("freed_bytes") ?? -1) \(digest.error ?? "")")
        expect(digest.int("actions") == Int64(week.actions), "week actions \(week.actions) ≠ digest actions \(digest.int("actions") ?? -1)")
        print("SELF-CHECK week from the real core: freed \(week.freed) + rebuilt since \(week.rebuiltBytes) = digest freed_bytes "
            + "\(digest.int("freed_bytes") ?? -1); actions \(week.actions) = digest actions \(digest.int("actions") ?? -1)")
        if week.rebuilt > 0 && week.rebuildSecs == nil {
            print("SELF-CHECK note: the core's history op doesn't return rebuild_secs, so rebuild time shows as not recorded")
        }

        let line = { (events: [JSON]) in LastCheck(["events": events]).text }
        expect(line([reclaimed(done[1])]) == "Freed 4.8 GB from engine", "live line: \(line([reclaimed(done[1])]))")
        let busy: JSON = ["event": "skip", "path": "/Users/me/code/shop/node_modules", "kind": "node_modules",
                          "reason": "in use: node is working in /Users/me/code/shop"]
        expect(line([busy]) == "Waiting: shop is in use", "live line: \(line([busy]))")
        expect(line([["event": "tick"]]) == "Nothing to do", "live line: \(line([["event": "tick"]]))")
        return failures
    }

    /// Hosts the view in an offscreen window so AppKit-backed controls (List, Form, Toggle) draw too.
    private static func shoot<V: View>(_ view: V, size: CGSize?, _ appearance: NSAppearance, title: (String, String)? = nil,
                                       _ url: URL) {
        // The app's words are English; its dates and numbers follow them in renders too.
        let controller = NSHostingController(rootView: view.environment(\.locale, Locale(identifier: "en_US")))
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
