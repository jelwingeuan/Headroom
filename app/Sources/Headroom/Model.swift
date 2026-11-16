import AppKit
import ServiceManagement
import UserNotifications

enum Pane: String, CaseIterable, Identifiable {
    case history = "History"
    case wouldHaveDone = "Would Have Done"
    case settings = "Settings"
    var id: Self { self }
    var symbol: String {
        switch self {
        case .history: "clock.arrow.circlepath"
        case .wouldHaveDone: "list.bullet.clipboard"
        case .settings: "gearshape"
        }
    }
}

/// UI state. Lives on the main thread; all core work hops to `Core.queue` and back.
final class AppModel: ObservableObject {
    @Published private(set) var status = Status([:])
    @Published private(set) var actions: [Candidate] = []
    @Published private(set) var settings: Settings?
    @Published private(set) var undoing: Set<String> = []
    @Published var pane: Pane? = .history
    @Published var alert: String?
    @Published var paused = UserDefaults.standard.bool(forKey: "paused") {
        didSet { UserDefaults.standard.set(paused, forKey: "paused") }
    }

    private var timers: [DispatchSourceTimer] = []

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
            self?.actions = items.map(Candidate.init)
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
        send(["op": "tick"]) { [weak self] _ in self?.refresh() }
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

func openFullDiskAccessSettings() {
    NSWorkspace.shared.open(URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles")!)
}
