import AppKit
import SwiftUI
import ServiceManagement
import UserNotifications

enum Pane: String, CaseIterable, Identifiable {
    case overview = "Overview", insights = "Insights", history = "History", settings = "Settings", about = "About"
    var id: Self { self }
    var symbol: String {
        switch self {
        case .overview: "internaldrive"
        case .insights: "chart.bar.xaxis"
        case .history: "clock.arrow.circlepath"
        case .settings: "gearshape"
        case .about: "info.circle"
        }
    }
}

/// UI state. Lives on the main thread; all core work hops to `Core.queue` and back.
final class AppModel: ObservableObject {
    @Published private(set) var status = Status([:])
    @Published private(set) var actions: [Candidate] = []
    @Published private(set) var settings: Settings?
    @Published private(set) var undoing: Set<String> = []
    /// The `samples` and `breakdown` replies, for Insights.
    @Published private(set) var samples: JSON = [:]
    @Published private(set) var breakdown: JSON = [:]
    /// Ticks in flight, and what the last one did: the menu's live line.
    @Published private(set) var ticking = 0
    @Published private(set) var lastCheck: LastCheck?
    /// A rescan or a delete from the Overview is running.
    @Published private(set) var working = false
    @Published var pane: Pane = .overview
    @Published var alert: String?
    @Published var paused = UserDefaults.standard.bool(forKey: "paused") {
        didSet { UserDefaults.standard.set(paused, forKey: "paused") }
    }

    private var timers: [DispatchSourceTimer] = []

    /// Screenshot mode: load everything synchronously from Core.fixture, no timers.
    init(fixture: JSON, pane: Pane) {
        Core.fixture = fixture
        status = Status(Core.call(["op": "status"]))
        let h = Core.call(["op": "history"])
        actions = (h.objects("actions") + h.objects("items")).map(Candidate.init)
        settings = Settings(Core.call(["op": "settings_get"]))
        samples = Core.call(["op": "samples"])
        breakdown = Core.call(["op": "breakdown"])
        if let tick = fixture["tick"] as? JSON { lastCheck = LastCheck(tick) } else { ticking = 1 } // no reply yet: in flight
        self.pane = pane
    }

    init() {
        if UserDefaults.standard.object(forKey: "lastDigest") == nil {
            UserDefaults.standard.set(Date().timeIntervalSince1970, forKey: "lastDigest")
        }
        timers = [
            every(60) { [weak self] in self?.refresh() },
            every(600) { [weak self] in self?.tick() },
        ]
    }

    // MARK: Core calls

    private func send(_ request: JSON, then: @escaping (JSON) -> Void) {
        Core.queue.async {
            let reply = Core.call(request)
            DispatchQueue.main.async { then(reply) }
        }
    }

    func refresh() {
        send(["op": "status"]) { [weak self] reply in
            let new = Status(reply)
            if self?.status != new { self?.status = new } // no redraw when nothing changed
        }
    }

    func loadHistory() {
        send(["op": "history"]) { [weak self] reply in
            if let error = reply.error { self?.alert = error; return }
            let items = reply.objects("actions") + reply.objects("items")
            withAnimation(.headroom) { self?.actions = items.map(Candidate.init) } // Undo springs its row back
        }
    }

    func loadInsights() {
        send(["op": "samples"]) { [weak self] in self?.samples = $0 }
        send(["op": "breakdown"]) { [weak self] in self?.breakdown = $0 }
    }

    /// Overview's Rescan: the tick's scan, now.
    func rescan() {
        working = true
        send(["op": "rescan"]) { [weak self] reply in
            self?.working = false
            if let error = reply.error { self?.alert = "Couldn't rescan: \(error)"; return }
            withAnimation(.headroom) { self?.breakdown = reply }
            self?.refresh()
        }
    }

    /// Overview's Delete: the core rechecks every path under every rule before it deletes.
    func reclaim(_ paths: [String]) {
        working = true
        send(["op": "reclaim", "paths": paths]) { [weak self] reply in
            self?.working = false
            if let error = reply.error { self?.alert = "Couldn't delete: \(error)"; return }
            self?.lastCheck = LastCheck(reply)
            withAnimation(.headroom) {
                if let b = reply.object("breakdown") { self?.breakdown = b }
                self?.refresh()
            }
            self?.loadHistory()
        }
    }

    func undo(_ action: Candidate) {
        undoing.insert(action.id)
        send(["op": "undo", "id": action.id]) { [weak self] reply in
            self?.undoing.remove(action.id)
            if let error = reply.error { self?.alert = "Couldn't undo \(action.name): \(error)" }
            self?.loadHistory()
            self?.refresh()
        }
    }

    func loadSettings() {
        send(["op": "settings_get"]) { [weak self] reply in
            if let error = reply.error { self?.alert = error }
            self?.settings = Settings(reply)
        }
    }

    func save(_ new: Settings) {
        guard new != settings else { return }
        settings = new
        send(new.request) { [weak self] reply in
            if let error = reply.error {
                self?.alert = "Couldn't save settings: \(error)"
                self?.loadSettings()
            }
            self?.refresh()
        }
    }

    private func tick() {
        maybeSendDigest()
        guard !paused else { return }
        ticking += 1
        send(["op": "tick"]) { [weak self] reply in
            self?.ticking -= 1
            self?.lastCheck = LastCheck(reply)
            self?.refresh()
            self?.loadInsights()
        }
    }

    // MARK: Weekly digest

    private func maybeSendDigest() {
        let last = UserDefaults.standard.double(forKey: "lastDigest")
        guard Date().timeIntervalSince1970 - last >= 7 * 86_400,
              Bundle.main.bundleIdentifier != nil // notifications need the .app bundle
        else { return }
        UserDefaults.standard.set(Date().timeIntervalSince1970, forKey: "lastDigest")
        send(["op": "digest"]) { reply in
            guard reply.error == nil,
                  let body = reply.string("body") ?? reply.string("text") ?? reply.string("summary")
            else { return }
            let center = UNUserNotificationCenter.current()
            center.requestAuthorization(options: [.alert]) { granted, _ in
                guard granted else { return }
                let content = UNMutableNotificationContent()
                content.title = reply.string("title") ?? "Your week with Headroom"
                content.body = body
                center.add(UNNotificationRequest(identifier: "digest", content: content, trigger: nil))
            }
        }
    }

    // MARK: Login item

    var launchAtLogin: Bool { SMAppService.mainApp.status == .enabled }
    var loginNeedsApproval: Bool { SMAppService.mainApp.status == .requiresApproval }

    func setLaunchAtLogin(_ on: Bool) {
        do {
            if on { try SMAppService.mainApp.register() } else { try SMAppService.mainApp.unregister() }
        } catch {
            alert = "Couldn't change Launch at Login: \(error.localizedDescription) "
                + "Move Headroom to the Applications folder, open it from there and try again."
        }
        objectWillChange.send()
    }

    // MARK: Helpers

    private func every(_ seconds: Int, _ work: @escaping () -> Void) -> DispatchSourceTimer {
        let timer = DispatchSource.makeTimerSource(queue: .main)
        // Generous leeway lets macOS batch these wake-ups with others, keeping idle cost near zero.
        timer.schedule(deadline: .now(), repeating: .seconds(seconds), leeway: .seconds(seconds / 6))
        timer.setEventHandler(handler: work)
        timer.resume()
        return timer
    }
}

extension Animation {
    /// Headroom's one motion: critically damped, no overshoot. With Reduce Motion, a crossfade.
    static var headroom: Animation {
        NSWorkspace.shared.accessibilityDisplayShouldReduceMotion ? .easeInOut(duration: 0.2) : .spring(response: 0.4, dampingFraction: 1)
    }
}

func openFullDiskAccessSettings() {
    NSWorkspace.shared.open(URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles")!)
}

/// Question 5, "What is it doing right now?": one line from a tick's events and when it ran.
struct LastCheck {
    let text: String
    /// Headroom deleted or shared something: the line gets Headroom's teal.
    let acted: Bool
    let failed: Bool
    let at: Date

    init(_ reply: JSON, at: Date = Date()) {
        self.at = at
        let events = reply.objects("events")
        let all = { (name: String) in events.filter { $0.string("event") == name }.map(Candidate.init) }
        let more = { (rows: [Candidate]) in rows.count > 1 ? " and \(rows.count - 1) more" : "" }
        let freed = all("reclaim"), skipped = all("skip"), would = all("would_reclaim")
        let shared = events.filter { $0.string("event") == "shrink" }.reduce(Int64(0)) { $0 + ($1.int("bytes") ?? 0) }
        acted = !freed.isEmpty || shared > 0
        let error = reply.error ?? events.first(where: { $0.string("event") == "error" })?.string("error")
        failed = error != nil
        if failed {
            text = "Last check failed" // the menu says why, once, in its warning above
        } else if let first = freed.first {
            text = "Freed \(bytes(freed.reduce(0) { $0 + $1.bytes })) from \(first.name)\(more(freed))"
        } else if shared > 0 {
            text = "Freed \(bytes(shared)) by sharing identical files"
        } else if let first = skipped.first {
            text = first.reason?.hasPrefix("in use") == true ? "Waiting: \(first.name) is in use"
                : "Skipped \(first.name): \(first.reason ?? "it changed")"
        } else if let first = would.first {
            text = "Dry run: would free \(bytes(would.reduce(0) { $0 + $1.bytes })) from \(first.name)\(more(would))"
        } else if events.contains(where: { $0.string("event") == "shortfall" }) {
            text = "Below budget, and nothing left is safe to free"
        } else {
            text = "Nothing to do"
        }
    }
}
