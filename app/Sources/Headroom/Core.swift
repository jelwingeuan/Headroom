import CHeadroom
import Foundation

typealias JSON = [String: Any]

/// The only door into the Rust core from the UI: one JSON request in, one JSON reply out.
/// Every reply is read defensively; a missing field is treated as unknown, never as a crash.
enum Core {
    /// headroom_call is not assumed to be thread-safe, so every call goes through this queue.
    static let queue = DispatchQueue(label: "io.github.jelwingeuan.headroom.core", qos: .utility)

    /// Blocking; call it on `queue`.
    static func call(_ request: JSON) -> JSON {
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
    var error: String?

    init(_ j: JSON) {
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

    init(_ j: JSON) {
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

    var shortPath: String { (path as NSString).abbreviatingWithTildeInPath }
    /// The folder that holds it, e.g. "~/code/app" for "~/code/app/node_modules".
    var place: String { (shortPath as NSString).deletingLastPathComponent }
    /// "app" for ~/code/app/node_modules; "Foo-abc" for DerivedData/Foo-abc.
    var name: String {
        let last = (path as NSString).lastPathComponent
        return last == kind ? (place as NSString).lastPathComponent : last
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
