import CHeadroom
import Foundation

typealias JSON = [String: Any]

/// The only door into the Rust core from the UI: one JSON request in, one JSON reply out.
/// Every reply is read defensively; a missing field is treated as unknown, never as a crash.
enum Core {
    /// headroom_call is not assumed to be thread-safe, so every call goes through this queue.
    static let queue = DispatchQueue(label: "io.github.jelwingeuan.headroom.core", qos: .utility)

    /// Screenshot mode only (see Screens.swift): canned replies by op, so every render is identical.
    static var fixture: JSON?

    /// Blocking; call it on `queue`.
    static func call(_ request: JSON) -> JSON {
        if let fixture { return fixture[request["op"] as? String ?? ""] as? JSON ?? ["error": "no fixture"] }
        guard let data = try? JSONSerialization.data(withJSONObject: request),
              let text = String(data: data, encoding: .utf8)
        else { return ["error": "Headroom couldn't encode a request. Please report this bug."] }
        guard let reply = headroom_call(text) else {
            return ["error": "Headroom's core gave no reply. Quit and reopen Headroom."]
        }
        defer { headroom_free(reply) }
        let value = try? JSONSerialization.jsonObject(with: Data(String(cString: reply).utf8))
        if let object = value as? JSON { return object }
        if let array = value as? [Any] { return ["items": array] }
        return ["error": "Headroom's core sent a reply it couldn't read. Quit and reopen Headroom."]
    }
}

extension Dictionary where Key == String, Value == Any {
    func int(_ key: String) -> Int64? { (self[key] as? NSNumber)?.int64Value }
    func bool(_ key: String) -> Bool? { (self[key] as? NSNumber)?.boolValue }
    func string(_ key: String) -> String? { self[key] as? String }
    func object(_ key: String) -> JSON? { self[key] as? JSON }
    func objects(_ key: String) -> [JSON] { self[key] as? [JSON] ?? [] }
    var error: String? { string("error") }
}

struct Status: Equatable {
    var freeBytes: Int64?
    var budgetBytes: Int64?
    var dryRun = false
    var dryRunDaysLeft: Int64?
    var next: Candidate?
    var needsFullDiskAccess = false
    var snapshotHolding = false
    /// From the last scan (at most a day old); nil until one has run.
    var canComeBack: Int64?
    /// Size of the disk that holds the home folder: the chart's fixed scale.
    var capacityBytes: Int64?
    var protectedBytes: Int64?
    var error: String?

    init(_ j: JSON) {
        canComeBack = j.int("reclaimable_bytes")
        capacityBytes = j.int("capacity_bytes") ?? (try? URL(fileURLWithPath: NSHomeDirectory())
            .resourceValues(forKeys: [.volumeTotalCapacityKey]).volumeTotalCapacity).map(Int64.init)
        protectedBytes = j.int("protected_bytes")
        freeBytes = j.int("free_bytes")
        budgetBytes = j.int("budget_bytes")
        dryRun = j.bool("dry_run") ?? false
        dryRunDaysLeft = j.int("dry_run_days_left")
        next = j.object("next").map(Candidate.init)
        let denied = j.int("permission_errors") ?? Int64(j.objects("permission_errors").count)
        needsFullDiskAccess = denied > 0 || j.bool("needs_full_disk_access") == true
        snapshotHolding = j.bool("snapshot_holding") ?? false
        error = j.error
    }
}

/// Something Headroom deleted, would delete, or plans to delete next.
struct Candidate: Equatable, Identifiable {
    var id: String
    var path: String
    var kind: String
    var bytes: Int64
    var date: Date?
    var dryRun: Bool
    var undone: Bool
    var reason: String?
    var rebuild: String?
    /// What a rebuild can't bring back (a simulator's apps and test data), if anything.
    var dataLoss: String?

    init(_ j: JSON) {
        dataLoss = j.string("data_loss")
        path = j.string("path") ?? ""
        id = j.string("id") ?? j.int("id").map(String.init) ?? path
        kind = j.string("kind") ?? "folder"
        bytes = j.int("bytes") ?? 0
        date = j.int("at").map { Date(timeIntervalSince1970: TimeInterval($0)) }
        dryRun = j.bool("dry_run") ?? false
        undone = j.bool("undone") ?? false
        reason = j.string("reason")
        rebuild = j.string("rebuild")
    }

    /// For sorting: rows without a time sort as oldest.
    var when: Date { date ?? .distantPast }

    var shortPath: String { (path as NSString).abbreviatingWithTildeInPath }
    /// The folder that holds it, e.g. "~/code/app" for "~/code/app/node_modules".
    var place: String { (shortPath as NSString).deletingLastPathComponent }
    /// "app" for ~/code/app/node_modules; "Foo-abc" for DerivedData/Foo-abc.
    var name: String {
        let last = (path as NSString).lastPathComponent
        if last == kind { return (place as NSString).lastPathComponent }
        // Xcode names DerivedData folders "<Project>-<28-letter hash>"; people know the project.
        if place.hasSuffix("DerivedData"), let dash = last.lastIndex(of: "-") { return String(last[..<dash]) }
        return last
    }

    /// The kind in plain words, for people who don't know the tool's folder names.
    var what: String {
        switch kind {
        case "node_modules": "Node packages"
        case "target", "rust_target": "Rust build files"
        case ".next": "Next.js build files"
        case ".venv", "venv": "Python environment"
        case "DerivedData", "derived_data": "Xcode build files"
        case "simulator": "iOS simulator"
        case "model", "ollama", "hf": "AI model"
        case "app_cache", "electron_cache": "app cache"
        case "worktree": "agent worktree"
        case "docker_images", "docker_build_cache": "Docker images"
        default: kind.hasPrefix(".") ? String(kind.dropFirst()) + " files" : kind
        }
    }

    /// Undo puts it back by rebuilding: the verb people should see on the button.
    var rebuildVerb: String {
        switch kind {
        case "model", "ollama", "hf": "Download Again"
        case "simulator": "Recreate"
        default: "Rebuild"
        }
    }
}

struct Settings: Equatable {
    /// 0 means no budget chosen yet: the core leaves the disk alone.
    var budgetBytes: Int64
    var recentDays: Int
    /// The dry run in effect. Sending it back unchanged keeps the automatic 7-day dry run.
    var dryRun: Bool

    init?(_ j: JSON) {
        guard j.error == nil else { return nil }
        budgetBytes = j.int("budget_bytes") ?? 0
        recentDays = Int(j.int("recent_days") ?? 14)
        dryRun = j.bool("dry_run") ?? true
    }

    /// Whole GB for the form; bytes stay exact until the user edits this.
    var budgetGB: Int {
        get { Int((budgetBytes + 500_000_000) / 1_000_000_000) }
        set { budgetBytes = Int64(newValue) * 1_000_000_000 }
    }

    var request: JSON {
        ["op": "settings_set", "budget_bytes": budgetBytes, "recent_days": recentDays, "dry_run": dryRun]
    }
}

func bytes(_ n: Int64) -> String { ByteCountFormatter.string(fromByteCount: n, countStyle: .file) }
