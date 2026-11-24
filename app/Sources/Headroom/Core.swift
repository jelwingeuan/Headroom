import CHeadroom
import Foundation
import SwiftUI

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
    /// protectedBytes split by why it's kept: recent, in_use, history, other.
    var protectedByReason: [String: Int64] = [:]
    /// When the scan behind these totals ran.
    var totalsAt: Date?
    var error: String?

    init(_ j: JSON) {
        protectedByReason = (j.object("protected_by_reason") ?? [:]).compactMapValues { ($0 as? NSNumber)?.int64Value }
        totalsAt = j.int("totals_at").map { Date(timeIntervalSince1970: TimeInterval($0)) }
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
    /// False for Docker, whose space stays inside its VM disk.
    var freesHost: Bool
    /// How long Undo's rebuild took, when the core reports it.
    var rebuildSecs: Int64?
    /// breakdown entries: true when autopilot could still reclaim it, false when it's kept.
    var canFree: Bool
    /// The project it belongs to (git top level, else the marker folder), from breakdown.
    var project: String?

    init(_ j: JSON) {
        project = j.string("project")
        dataLoss = j.string("data_loss")
        path = j.string("path") ?? ""
        id = j.string("id") ?? j.int("id").map(String.init) ?? path
        kind = j.string("kind") ?? "folder"
        freesHost = j.bool("frees_host") ?? !kind.hasPrefix("docker_")
        rebuildSecs = j.int("rebuild_secs")
        canFree = j.bool("free") ?? false
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
        if last.hasPrefix("models--") { return last.dropFirst(8).replacingOccurrences(of: "--", with: "/") }
        return last
    }

    var info: Kind { Kind(kind) }
    /// The kind in plain words, for people who don't know the tool's folder names.
    var what: String { info.label }

    /// The scan's reason split into its clauses, minus the project marker ("Rust project
    /// (Cargo.toml)") the card already shows: "git ignores it", "last used 41 days ago"…
    var tags: [String] {
        let parts = (reason ?? "").replacingOccurrences(of: NSHomeDirectory(), with: "~").components(separatedBy: "; ").filter { !$0.isEmpty }
        return parts.first?.contains(" project (") == true ? Array(parts.dropFirst()) : parts
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

/// One table for every core kind: its plain name, its Insights group and Overview section, its
/// color, and why it starts unticked when it does.
struct Kind {
    enum Section: Int, CaseIterable, Identifiable {
        case projects, aiTools, xcode, docker, models
        var id: Self { self }
        var title: String {
            ["Build output in projects", "AI-tool leftovers", "Xcode and simulators", "Docker", "AI models"][rawValue]
        }
    }

    var label: String, group: String, section = Section.projects, color = Color.gray
    /// Set: the Overview shows it unticked, and says this in one sentence.
    var unticked: String?

    init(_ kind: String) {
        switch kind {
        case "node_modules": (label, group, color) = ("Node packages", "Node packages", .green)
        case "target", "rust_target": (label, group, color) = ("Rust build files", "Rust builds", .orange)
        case ".next": (label, group) = ("Next.js build files", "Other")
        case ".venv", "venv": (label, group) = ("Python environment", "Other")
        case "worktree": (label, group, color) = ("Agent worktree", "Worktrees", .indigo)
        case "DerivedData", "derived_data": (label, group, section, color) = ("Xcode build files", "Xcode", .xcode, .blue)
        case "device_support": (label, group, section, color) = ("Device symbols", "Xcode", .xcode, .blue)
        case "xcode_archive": (label, group, section, color) = ("Xcode archive", "Xcode", .xcode, .blue)
        case "simulator":
            (label, group, section, color) = ("iOS simulator", "Xcode", .xcode, .blue)
            unticked = "Left unticked: its apps and test data can't come back, and Undo gives a blank device."
        case "model", "ollama", "hf", "ollama_model", "hf_model", "lmstudio_model":
            (label, group, section, color) = ("AI model", "AI models", .models, .purple)
        case "claude_code_old": (label, group, section, color) = ("Old Claude Code version", "AI-tool leftovers", .aiTools, .pink)
        case "codex_temp": (label, group, section, color) = ("Codex temporary files", "AI-tool leftovers", .aiTools, .pink)
        case "app_cache", "electron_cache": (label, group, section, color) = ("App cache", "AI-tool leftovers", .aiTools, .pink)
        case "ai_history": (label, group, section, color) = ("AI conversation history", "AI-tool leftovers", .aiTools, .pink)
        case "docker_images", "docker_build_cache":
            (label, group, section, color) = (kind == "docker_images" ? "Docker images" : "Docker build cache", "Docker", .docker, .cyan)
            unticked = "Left unticked: this frees space inside Docker's own disk, not on this Mac's."
        default: (label, group) = (kind.hasPrefix(".") ? String(kind.dropFirst()) + " files" : kind, "Other")
        }
    }
}

/// "2 hours ago", in English like every other word in the app, whatever the Mac's region.
func ago(_ d: Date) -> String { d.formatted(.relative(presentation: .named).locale(Locale(identifier: "en"))) }

func bytes(_ n: Int64) -> String { ByteCountFormatter.string(fromByteCount: n, countStyle: .file) }
