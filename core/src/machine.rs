//! The machine-wide places Headroom knows: Xcode, local AI models, Docker, and what AI coding
//! tools leave behind. Each source proves its own rule (docs/THREAT-MODEL.md table).

use crate::json::Json;
use crate::rebuild::Plan;
use crate::scan::{par_map, Cand, How};
use crate::state::{self, Ctx, DAY};
use crate::{cold, git, obj, proc};
use std::path::{Path, PathBuf};
use std::process::Command;

type Source = fn(&Ctx, bool) -> Vec<Cand>;

const SOURCES: &[(&[&str], Source)] = &[
    (&["DerivedData"], derived_data),
    (&["device_support", "xcode_archive"], xcode_misc),
    (&["simulator"], simulators),
    (&["claude_code_old", "codex_temp", "electron_cache", "ai_history"], ai_leftovers),
    (&["ollama_model"], ollama),
    (&["hf_model"], huggingface),
    (&["lmstudio_model"], lmstudio),
    (&["docker_images", "docker_build_cache"], docker),
];

/// Every source (or only the one that finds `kind`, read fresh, for the recheck before delete).
pub fn discover(ctx: &Ctx, kind: Option<&str>) -> Vec<Cand> {
    let srcs: Vec<Source> =
        SOURCES.iter().filter(|(k, _)| kind.map_or(true, |w| k.contains(&w))).map(|(_, f)| *f).collect();
    let mut out: Vec<Cand> = par_map(srcs, |f| f(ctx, kind.is_some())).into_iter().flatten().collect();
    for c in &mut out {
        c.machine = true;
    }
    out
}

fn children(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> =
        std::fs::read_dir(dir).map(|rd| rd.flatten().map(|e| e.path()).collect()).unwrap_or_default();
    v.sort();
    v
}

fn is_dir(p: &Path) -> bool {
    std::fs::symlink_metadata(p).is_ok_and(|m| m.is_dir())
}

fn mtime(p: &Path) -> u64 {
    std::fs::symlink_metadata(p).map_or(0, |m| cold::mtime(&m))
}

fn atime(p: &Path) -> u64 {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(p).map_or(0, |m| m.atime().max(0) as u64)
}

fn leftover(path: PathBuf, kind: &str, why: &str, note: &str) -> Cand {
    let mut c = Cand::new(path, kind, How::Tree);
    if std::fs::symlink_metadata(&c.path).is_ok_and(|m| m.is_file()) {
        c.how = How::Files(vec![c.path.clone()]);
    }
    c.recent_rule = false;
    c.active = Some(mtime(&c.path));
    c.why = why.into();
    c.plan = Some(Plan::note(note));
    c
}

fn running(procs: &[String], names: &[&str]) -> bool {
    procs.iter().any(|p| names.contains(&proc::program(p)))
}

// ---------------------------------------------------------------- Xcode

/// One string value out of a plist (XML read directly; binary through plutil).
pub fn plist_string(file: &Path, key: &str) -> Option<String> {
    let bytes = std::fs::read(file).ok()?;
    if bytes.starts_with(b"bplist") {
        let mut c = Command::new("/usr/bin/plutil");
        c.args(["-extract", key, "raw", "-o", "-"]).arg(file);
        return proc::run(c, 10).filter(proc::Out::ok).map(|o| o.stdout.trim().to_string());
    }
    let text = String::from_utf8_lossy(&bytes);
    let after = text.split(&format!("<key>{key}</key>")).nth(1)?;
    let v = after.trim_start().strip_prefix("<string>")?.split("</string>").next()?;
    Some(
        v.replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&apos;", "'")
            .replace("&amp;", "&"),
    )
}

fn derived_data(ctx: &Ctx, _fresh: bool) -> Vec<Cand> {
    let dd = ctx.home.join("Library/Developer/Xcode/DerivedData");
    let entries = children(&dd);
    if entries.is_empty() {
        return vec![];
    }
    let building =
        proc::processes().map_or(true, |p| running(&p, &["xcodebuild", "XCBBuildService", "SWBBuildService"]));
    let mut out = vec![];
    for p in entries {
        let name = p.file_name().unwrap_or_default().to_string_lossy().into_owned();
        // ModuleCache.noindex, SymbolCache.noindex…: shared by every build, not one project's.
        let Some((scheme, _hash)) = name.rsplit_once('-') else { continue };
        if !is_dir(&p) || name.ends_with(".noindex") {
            continue;
        }
        let mut c = Cand::new(p.clone(), "DerivedData", How::Tree);
        match plist_string(&p.join("info.plist"), "WorkspacePath").map(PathBuf::from) {
            // A package's own .swiftpm workspace is optional: the package folder is enough.
            Some(ws) if ws.exists() || (project_dir(&ws).is_dir() && ws.to_string_lossy().contains("/.swiftpm/")) => {
                // Real path (Xcode may record /tmp for /private/tmp), so it matches the scan's.
                let dir = project_dir(&ws);
                let dir = dir.canonicalize().unwrap_or(dir);
                c.repo = git::toplevel(&dir);
                c.project = Some(c.repo.clone().unwrap_or_else(|| dir.clone()));
                c.why = format!("Xcode build data for {}", ws.display());
                // The folder is named after the workspace, not the scheme: the scheme of the last
                // build is in Xcode's build log index.
                let scheme = last_scheme(&p).unwrap_or_else(|| scheme.to_string());
                c.plan = Some(xcode_plan(&p, &ws, &dir, &scheme));
            }
            Some(ws) if project_dir(&ws).exists() => {
                c.blocked = Some(format!("its workspace {} moved: Headroom can't rebuild it", ws.display()));
            }
            Some(ws) => {
                c.recent_rule = false;
                c.active = Some(mtime(&p));
                c.why = format!("Xcode build data for {}, whose folder no longer exists", ws.display());
                c.plan = Some(Plan::note("its project is gone; nothing to rebuild"));
            }
            None => c.blocked = Some("no info.plist: can't tell which project made it".into()),
        }
        if building {
            c.blocked = Some("Xcode is building right now (or ps couldn't tell)".into());
        }
        out.push(c);
    }
    out
}

/// The folder an Xcode workspace path belongs to (a package's own .swiftpm workspace counts
/// as the package).
fn project_dir(ws: &Path) -> PathBuf {
    let s = ws.to_string_lossy();
    if let Some((pkg, _)) = s.split_once("/.swiftpm/") {
        return PathBuf::from(pkg);
    }
    match ws.extension().and_then(|e| e.to_str()) {
        Some("xcodeproj" | "xcworkspace" | "playground") => ws.parent().unwrap_or(ws).into(),
        _ => ws.into(),
    }
}

/// The scheme of the newest build recorded in DerivedData's Logs/Build/LogStoreManifest.plist.
fn last_scheme(dd: &Path) -> Option<String> {
    let mut c = Command::new("/usr/bin/plutil");
    c.args(["-convert", "json", "-o", "-"]).arg(dd.join("Logs/Build/LogStoreManifest.plist"));
    let j = proc::run(c, 10).filter(proc::Out::ok).and_then(|o| Json::parse(&o.stdout).ok())?;
    let Some(Json::Obj(logs)) = j.get("logs") else { return None };
    let stopped = |l: &Json| match l.get("timeStoppedRecording") {
        Some(Json::Num(x)) => *x,
        Some(Json::Int(n)) => *n as f64,
        _ => 0.0,
    };
    logs.iter()
        .map(|(_, l)| l)
        .filter(|l| !l.s("schemeIdentifier-schemeName").is_empty())
        .max_by(|a, b| stopped(a).total_cmp(&stopped(b)))
        .map(|l| l.s("schemeIdentifier-schemeName").to_string())
}

fn xcode_plan(dd: &Path, ws: &Path, dir: &Path, scheme: &str) -> Plan {
    let products: Vec<String> = children(&dd.join("Build/Products"))
        .iter()
        .map(|p| p.file_name().unwrap_or_default().to_string_lossy().into_owned())
        .collect();
    let dest = if products.iter().any(|n| n == "Debug" || n == "Release") {
        Some("platform=macOS")
    } else if products.iter().any(|n| n.ends_with("-iphonesimulator")) {
        Some("generic/platform=iOS Simulator")
    } else if products.iter().any(|n| n.ends_with("-iphoneos")) {
        Some("generic/platform=iOS")
    } else {
        None
    };
    let mut step = vec!["xcodebuild".to_string()];
    let w = ws.to_string_lossy().into_owned();
    match ws.extension().and_then(|e| e.to_str()) {
        Some("xcworkspace") if !w.contains("/.swiftpm/") => step.extend(["-workspace".into(), w]),
        Some("xcodeproj") => step.extend(["-project".into(), w]),
        _ => {}
    }
    step.extend(["-scheme".into(), scheme.into()]);
    if let Some(d) = dest {
        step.extend(["-destination".into(), d.into()]);
    }
    step.extend(["-quiet".into(), "build".into()]);
    Plan { cwd: dir.into(), steps: vec![step], ..Plan::default() }
}

const ARCHIVE_MONTHS: u64 = 6;

fn xcode_misc(ctx: &Ctx, _fresh: bool) -> Vec<Cand> {
    let x = ctx.home.join("Library/Developer/Xcode");
    let mut out = vec![];
    for platform in children(&x).into_iter().filter(|p| p.to_string_lossy().ends_with(" DeviceSupport")) {
        let mut v: Vec<PathBuf> = children(&platform).into_iter().filter(|p| is_dir(p)).collect();
        v.sort_by_key(|p| mtime(p));
        v.pop(); // the newest stays: likely the device in use
        out.extend(v.into_iter().map(|p| {
            leftover(
                p,
                "device_support",
                "symbols copied from an older device OS",
                "Xcode copies them again when that device connects",
            )
        }));
    }
    let cutoff = ctx.now.saturating_sub(ARCHIVE_MONTHS * 30 * DAY);
    for day in children(&x.join("Archives")) {
        let date = day.file_name().unwrap_or_default().to_string_lossy().into_owned();
        let Some(t) = date_secs(&date) else { continue };
        if t >= cutoff {
            continue;
        }
        for a in children(&day).into_iter().filter(|p| p.extension().is_some_and(|e| e == "xcarchive")) {
            let mut c = leftover(a, "xcode_archive", &format!("Xcode archive from {date}"), "");
            // Listed so its size is visible, never deleted: nothing can make it again.
            c.blocked = Some(format!("archives can't be rebuilt (this one is older than {ARCHIVE_MONTHS} months)"));
            c.active = Some(t);
            out.push(c);
        }
    }
    out
}

/// "2024-03-09" -> seconds since 1970 (UTC midnight).
fn date_secs(s: &str) -> Option<u64> {
    let mut it = s.split('-').map(|p| p.parse::<i64>().ok());
    let (y, m, d) = (it.next()??, it.next()??, it.next()??);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    // Days from civil (Howard Hinnant's algorithm).
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    u64::try_from((era * 146_097 + doe - 719_468) * 86_400).ok()
}

/// "2026-11-07T20:34:06Z" (UTC, as simctl prints it) -> seconds since 1970.
fn iso_secs(s: &str) -> Option<u64> {
    let day = date_secs(s.get(..10)?)?;
    let t = s.get(11..19)?;
    let mut hms = t.split(':').map(|p| p.parse::<u64>().ok());
    Some(day + hms.next()?? * 3600 + hms.next()?? * 60 + hms.next()??)
}

pub const SIM_COLD_DAYS: u64 = 30;

/// `simctl create <name> <type> <runtime>` arguments that can't be read as options: a name
/// that is not empty and doesn't start with '-', and CoreSimulator identifiers.
pub fn sim_create_ok(name: &str, kind: &str, runtime: &str) -> bool {
    let id = |s: &str, prefix: &str| {
        s.strip_prefix(prefix)
            .is_some_and(|r| !r.is_empty() && r.chars().all(|c| c.is_ascii_alphanumeric() || ".-_".contains(c)))
    };
    !name.trim().is_empty()
        && !name.starts_with('-')
        && id(kind, "com.apple.CoreSimulator.SimDeviceType.")
        && id(runtime, "com.apple.CoreSimulator.SimRuntime.")
}

/// Every simulator device as simctl lists it, each with its runtime. Cached for six hours in
/// HEADROOM_HOME (simctl is slow to start); the recheck before delete always asks again.
fn sim_devices(ctx: &Ctx, fresh: bool) -> Option<Vec<Json>> {
    let cache = ctx.file("cache-simctl.json");
    if !fresh {
        let hit = state::read_json(&cache)
            .filter(|j| j.get("at").and_then(Json::u64).is_some_and(|t| t + 6 * 3600 > state::wall_clock()));
        if let Some(Json::Arr(d)) = hit.as_ref().and_then(|j| j.get("devices")) {
            return Some(d.clone());
        }
    }
    let mut c = Command::new("/usr/bin/xcrun");
    c.args(["simctl", "list", "devices", "-j"]);
    let j = proc::run(c, 120).filter(proc::Out::ok).and_then(|o| Json::parse(&o.stdout).ok())?;
    let mut v = vec![];
    if let Some(Json::Obj(rt)) = j.get("devices") {
        for (runtime, list) in rt {
            for d in list.arr() {
                let mut d = d.clone();
                d.set("runtime", runtime.as_str());
                v.push(d);
            }
        }
    }
    let _ =
        state::write_atomic(&cache, obj! {"at" => state::wall_clock(), "devices" => v.clone()}.to_string().as_bytes());
    Some(v)
}

/// Simulators. One whose runtime is gone is unusable and may go. A usable one may go when it
/// is shut down, hasn't been used for 30 days (simctl's `lastUsedAt`, else when it was
/// created) and isn't the newest of its device type. Its apps and test data are lost; undo
/// makes a blank device with the same name, type and runtime.
fn simulators(ctx: &Ctx, fresh: bool) -> Vec<Cand> {
    let dir = ctx.home.join("Library/Developer/CoreSimulator/Devices");
    if !is_dir(&dir) {
        return vec![];
    }
    let Some(devices) = sim_devices(ctx, fresh) else { return vec![] };
    let created = |p: &Path| {
        std::fs::metadata(p)
            .and_then(|m| m.created())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs())
    };
    let ok_udid = |u: &str| u.len() == 36 && u.chars().all(|c| c.is_ascii_hexdigit() || c == '-');
    let devs: Vec<(Json, PathBuf, u64)> = devices
        .into_iter()
        .filter(|d| ok_udid(d.s("udid")))
        .map(|d| {
            let path = dir.join(d.s("udid"));
            let used = iso_secs(d.s("lastUsedAt")).unwrap_or_else(|| created(&path));
            (d, path, used)
        })
        .filter(|(_, p, _)| is_dir(p))
        .collect();
    let available = |d: &Json| d.get("isAvailable") != Some(&Json::Bool(false));
    let newest = |kind: &str| {
        devs.iter()
            .filter(|(d, _, _)| available(d) && d.s("deviceTypeIdentifier") == kind)
            // A device that was really used beats one that never was; creation time only ranks never-used ones.
            .max_by_key(|(d, _, used)| (!d.s("lastUsedAt").is_empty(), *used, d.s("udid").to_string()))
            .map(|(d, _, _)| d.s("udid").to_string())
    };
    let loss = "its apps and test data are deleted; undo makes a blank device with the same name, type and runtime";
    let mut out = vec![];
    for (d, path, used) in &devs {
        let (name, kind, runtime) = (d.s("name"), d.s("deviceTypeIdentifier"), d.s("runtime"));
        let mut c = leftover(path.clone(), "simulator", "", "");
        c.how = How::Simctl(d.s("udid").to_string());
        c.active = Some(*used);
        let days = ctx.now.saturating_sub(*used) / DAY;
        if !available(d) {
            c.why = format!("{name} simulator whose runtime is no longer installed");
            c.plan = Some(Plan {
                loss: "its apps and test data are deleted; it was unusable without its runtime".into(),
                ..Plan::note("unusable without its runtime")
            });
            out.push(c);
            continue;
        }
        if !sim_create_ok(name, kind, runtime) {
            c.blocked = Some(format!("{name:?}: its name, type or runtime can't be passed to simctl safely to undo"));
            out.push(c);
            continue;
        }
        c.why = format!("{name} simulator, last used {days} days ago; {loss}");
        c.plan = Some(Plan {
            cwd: ctx.home.clone(),
            steps: vec![["xcrun", "simctl", "create", name, kind, runtime].map(String::from).to_vec()],
            new_device: true,
            loss: loss.into(),
            ..Plan::default()
        });
        c.blocked = if d.s("state") != "Shutdown" {
            Some(format!("{name} simulator is {}", d.s("state").to_lowercase()))
        } else if newest(kind).as_deref() == Some(d.s("udid")) {
            Some(format!("the newest {name} simulator of its type is always kept"))
        } else if *used + SIM_COLD_DAYS * DAY > ctx.now {
            Some(format!("used {days} days ago (simulators: kept for {SIM_COLD_DAYS} days)"))
        } else {
            None
        };
        out.push(c);
    }
    out
}

// ---------------------------------------------------------------- AI coding tools

/// "2.1.309" < "2.1.317" < "2.10.0".
fn version_key(name: &str) -> Vec<u64> {
    name.split(|c: char| !c.is_ascii_digit()).filter(|p| !p.is_empty()).map(|p| p.parse().unwrap_or(0)).collect()
}

/// Every version but the newest, the current one and any a running program uses.
fn old_versions(dir: &Path, current: Option<&Path>, procs: &[String]) -> Vec<PathBuf> {
    let mut all: Vec<PathBuf> = children(dir)
        .into_iter()
        .filter(|p| !version_key(&p.file_name().unwrap_or_default().to_string_lossy()).is_empty())
        .collect();
    all.sort_by_key(|p| version_key(&p.file_name().unwrap_or_default().to_string_lossy()));
    all.pop();
    all.retain(|p| current != Some(p.as_path()) && !procs.iter().any(|c| c.contains(&*p.to_string_lossy())));
    all
}

/// Desktop apps whose Chromium caches (and only those) may go while the app isn't running:
/// (folder in Application Support, process name).
const ELECTRON_APPS: &[(&str, &str)] = &[("Claude", "Claude"), ("Codex", "Codex"), ("Cursor", "Cursor")];
const ELECTRON_CACHES: &[&str] = &["Cache", "Code Cache", "GPUCache"];

fn ai_leftovers(ctx: &Ctx, _fresh: bool) -> Vec<Cand> {
    let h = &ctx.home;
    // Without a process list every app counts as running: nothing in use is touched.
    let listed = proc::processes();
    let ps_failed = listed.is_none();
    let procs = listed.unwrap_or_default();
    let support = h.join("Library/Application Support");
    let mut out = vec![];
    let in_use = |c: &mut Cand, app: &str| {
        if ps_failed {
            c.blocked = Some("ps couldn't list running programs, so it may be in use".into());
        } else if running(&procs, &[app]) {
            c.blocked = Some(format!("{app} is running"));
        }
    };
    let current = std::fs::canonicalize(h.join(".local/bin/claude")).ok();
    for p in old_versions(&h.join(".local/share/claude/versions"), current.as_deref(), &procs) {
        let mut c = leftover(
            p,
            "claude_code_old",
            "an old Claude Code version: not the one ~/.local/bin/claude runs, not the newest",
            "the updater downloads versions it needs",
        );
        if ps_failed {
            c.blocked = Some("ps couldn't list running programs, so it may be in use".into());
        }
        out.push(c);
    }
    for p in old_versions(&support.join("Claude/claude-code"), None, &procs) {
        let mut c = leftover(
            p,
            "claude_code_old",
            "an old Claude Code bundled by the desktop app",
            "the app downloads versions it needs",
        );
        in_use(&mut c, "Claude");
        out.push(c);
    }
    let hour_ago = state::wall_clock().saturating_sub(3600);
    for p in children(&h.join(".cache/codex-runtimes")) {
        if p.file_name().unwrap_or_default().to_string_lossy().starts_with("codex-runtime-install-")
            && mtime(&p) < hour_ago
        {
            out.push(leftover(
                p,
                "codex_temp",
                "a Codex runtime download that never finished",
                "Codex downloads it again if needed",
            ));
        }
    }
    // Only the Chromium caches: never Local Storage, IndexedDB, Preferences or config.
    for (app, process) in ELECTRON_APPS {
        for d in ELECTRON_CACHES {
            let p = support.join(app).join(d);
            if !is_dir(&p) {
                continue;
            }
            let mut c = leftover(
                p,
                "electron_cache",
                &format!("{app} desktop's {d} (Chromium cache)"),
                &format!("{app} refills it on its next launch"),
            );
            in_use(&mut c, process);
            out.push(c);
        }
    }
    for p in [".claude/projects", ".codex/sessions", ".codex/archived_sessions"]
        .map(|d| h.join(d))
        .into_iter()
        .filter(|p| is_dir(p))
    {
        let mut c = leftover(p, "ai_history", "", "");
        c.blocked = Some("AI conversation history: never deleted".into());
        out.push(c);
    }
    out
}

// ---------------------------------------------------------------- local models

/// Ollama's own library models. A model under another name may have been made with
/// `ollama create` from a local file, so it isn't provably downloadable again.
const OLLAMA_LIBRARY: &[&str] = &[
    "all-minilm",
    "bge-large",
    "bge-m3",
    "codegemma",
    "codellama",
    "codestral",
    "command-r",
    "deepseek-coder",
    "deepseek-coder-v2",
    "deepseek-r1",
    "deepseek-v3",
    "dolphin-mixtral",
    "falcon",
    "gemma",
    "gemma2",
    "gemma3",
    "gemma3n",
    "gpt-oss",
    "granite3.1-dense",
    "granite3.3",
    "llama2",
    "llama3",
    "llama3.1",
    "llama3.2",
    "llama3.2-vision",
    "llama3.3",
    "llama4",
    "llava",
    "minicpm-v",
    "mistral",
    "mistral-nemo",
    "mistral-small",
    "mixtral",
    "moondream",
    "mxbai-embed-large",
    "neural-chat",
    "nomic-embed-text",
    "openchat",
    "orca-mini",
    "phi",
    "phi3",
    "phi3.5",
    "phi4",
    "qwen",
    "qwen2",
    "qwen2.5",
    "qwen2.5-coder",
    "qwen3",
    "qwen3-coder",
    "qwq",
    "smollm2",
    "snowflake-arctic-embed",
    "starcoder2",
    "tinyllama",
    "vicuna",
    "wizardlm2",
    "yi",
    "zephyr",
];

struct Manifest {
    path: PathBuf,
    name: String,
    registry: bool,
    from_local: bool,
    weights: Vec<String>,
    blobs: Vec<String>,
}

fn ollama(ctx: &Ctx, _fresh: bool) -> Vec<Cand> {
    let root = ctx.home.join(".ollama/models");
    let mut manifests = vec![];
    // One manifest Headroom can't read hides which blobs it uses: then nothing is touched.
    let mut unreadable: Option<PathBuf> = None;
    let entries = |dir: &Path, unreadable: &mut Option<PathBuf>| -> Vec<PathBuf> {
        let v: Vec<PathBuf> = children(dir).into_iter().filter(|p| !p.ends_with(".DS_Store")).collect();
        if let Some(odd) = v.iter().find(|p| !is_dir(p)) {
            unreadable.get_or_insert_with(|| odd.clone());
        }
        v.into_iter().filter(|p| is_dir(p)).collect()
    };
    let digest_ok =
        |d: &str| d.len() == 71 && d.starts_with("sha256:") && d[7..].chars().all(|c| c.is_ascii_hexdigit());
    for host in entries(&root.join("manifests"), &mut unreadable) {
        for ns in entries(&host, &mut unreadable) {
            for model in entries(&ns, &mut unreadable) {
                for tag in children(&model).into_iter().filter(|p| !p.ends_with(".DS_Store")) {
                    let rel = |p: &Path| p.file_name().unwrap_or_default().to_string_lossy().into_owned();
                    let (h, n, m, t) = (rel(&host), rel(&ns), rel(&model), rel(&tag));
                    let parsed =
                        state::read_json(&tag).filter(|j| j.get("layers").is_some_and(|l| matches!(l, Json::Arr(_))));
                    let Some(j) = parsed else {
                        unreadable.get_or_insert_with(|| tag.clone());
                        continue;
                    };
                    let layers = j.get("layers").map(Json::arr).unwrap_or(&[]);
                    let mut blobs: Vec<String> = layers.iter().map(|l| l.s("digest").to_string()).collect();
                    blobs.extend(j.get("config").map(|c| c.s("digest").to_string()));
                    if !blobs.iter().all(|d| digest_ok(d)) {
                        unreadable.get_or_insert_with(|| tag.clone());
                        continue;
                    }
                    let weights = layers
                        .iter()
                        .filter(|l| {
                            ["model", "projector", "adapter"]
                                .iter()
                                .any(|k| l.s("mediaType").ends_with(&format!("image.{k}")))
                        })
                        .map(|l| l.s("digest").to_string())
                        .collect();
                    let name = match (h.as_str(), n.as_str()) {
                        ("registry.ollama.ai", "library") => format!("{m}:{t}"),
                        ("registry.ollama.ai", _) => format!("{n}/{m}:{t}"),
                        _ => format!("{h}/{n}/{m}:{t}"),
                    };
                    let registry =
                        (h == "registry.ollama.ai" && n == "library" && OLLAMA_LIBRARY.contains(&m.as_str()))
                            || h == "hf.co";
                    let from_local = layers.iter().any(|l| l.get("from").is_some());
                    manifests.push(Manifest { path: tag, name, registry, from_local, weights, blobs });
                }
            }
        }
    }
    let users = |d: &str| manifests.iter().filter(|m| m.blobs.iter().any(|b| b == d)).count();
    let mut out = vec![];
    for m in &manifests {
        let mut files = vec![m.path.clone()];
        files.extend(
            m.blobs
                .iter()
                .filter(|b| users(b) == 1)
                .map(|b| root.join("blobs").join(b.replace(':', "-")))
                .filter(|p| p.is_file()),
        );
        files.dedup();
        let mut c = Cand::new(m.path.clone(), "ollama_model", How::Files(files.clone()));
        let others: Vec<&str> = manifests
            .iter()
            .filter(|o| o.path != m.path && o.blobs.iter().any(|b| m.weights.contains(b)))
            .map(|o| o.name.as_str())
            .collect();
        c.blocked = if let Some(bad) = &unreadable {
            Some(format!("can't read the Ollama manifest {}, so no model is touched", bad.display()))
        } else if !m.registry {
            Some(format!("{}: not a known registry model (it may have been made with `ollama create`)", m.name))
        } else if m.from_local {
            Some(format!("{}: built locally from another model", m.name))
        } else if !others.is_empty() {
            Some(format!("{}: shares its weights with {}", m.name, others.join(", ")))
        } else {
            None
        };
        c.active = files.iter().map(|f| mtime(f).max(atime(f))).max();
        c.why = format!("Ollama registry model {}", m.name);
        c.plan = Some(Plan {
            cwd: ctx.home.clone(),
            steps: vec![vec!["ollama".into(), "pull".into(), m.name.clone()]],
            ..Plan::default()
        });
        out.push(c);
    }
    out
}

fn hf_tool(ctx: &Ctx) -> Option<String> {
    ["hf", "huggingface-cli"].iter().find(|t| proc::which(t, &ctx.path_var, None).is_some()).map(|t| t.to_string())
}

/// Files under `dir`, relative to it (symlinks count as files: that's how the HF cache links blobs).
fn rel_files(dir: &Path) -> Vec<String> {
    let mut out = vec![];
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for p in children(&d) {
            if is_dir(&p) {
                stack.push(p);
            } else if let Ok(r) = p.strip_prefix(dir) {
                out.push(r.to_string_lossy().into_owned());
            }
        }
    }
    out.sort();
    out
}

/// The last time anything under `dir` was written, or a file in it read. A folder's access
/// time doesn't count (listing it, Headroom's own scan included, refreshes it), and links out
/// of the folder aren't followed (a shared store isn't this folder).
fn newest_use(dir: &Path) -> u64 {
    use std::os::unix::fs::MetadataExt;
    let mut t = mtime(dir);
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for p in children(&d) {
            let Ok(m) = std::fs::symlink_metadata(&p) else { continue };
            t = t.max(cold::mtime(&m));
            if m.is_dir() {
                stack.push(p);
            } else if m.is_file() {
                t = t.max(m.atime().max(0) as u64);
            }
        }
    }
    t
}

/// Downloads one cached snapshot again exactly (same revision, same files) and puts its refs
/// back. Run by `rebuild`/`undo` with Headroom's own huggingface_hub (see `huggingface`).
const HF_RESTORE: &str = "import json, os, sys
from huggingface_hub import snapshot_download
repo, kind, cache, rev, refs = sys.argv[1:6]
snapshot_download(repo, repo_type=kind, revision=rev, cache_dir=cache, allow_patterns=sys.argv[6:] or None)
folder = os.path.join(cache, kind + 's--' + repo.replace('/', '--'))
for name, sha in json.loads(refs).items():
    path = os.path.join(folder, 'refs', name)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, 'w') as f:
        f.write(sha)
";

/// The Hugging Face cache: everything in it came from the Hub, so it can come back. The
/// restore needs only `python3`: Headroom keeps its own huggingface_hub in
/// HEADROOM_HOME/tools/hf (made on first restore), whatever the user's environments hold.
fn huggingface(ctx: &Ctx, _fresh: bool) -> Vec<Cand> {
    let hub = ctx.home.join(".cache/huggingface/hub");
    let python = proc::which("python3", &ctx.path_var, None);
    let venv = ctx.file("tools/hf").to_string_lossy().into_owned();
    let vpy = format!("{venv}/bin/python");
    let mut out = vec![];
    for p in children(&hub) {
        let name = p.file_name().unwrap_or_default().to_string_lossy().into_owned();
        let Some((kind, id)) = name.split_once("--") else { continue };
        let kind = match kind {
            "models" => "model",
            "datasets" => "dataset",
            "spaces" => "space",
            _ => continue,
        };
        let repo = id.replace("--", "/");
        let mut c = Cand::new(p.clone(), "hf_model", How::Tree);
        c.active = Some(newest_use(&p));
        c.why = format!("Hugging Face {kind} {repo}, downloaded from the Hub");
        let mut refs = vec![];
        for r in rel_files(&p.join("refs")) {
            let sha = std::fs::read_to_string(p.join("refs").join(&r)).unwrap_or_default().trim().to_string();
            refs.push((r, sha));
        }
        let mut steps = vec![
            vec!["python3".to_string(), "-m".into(), "venv".into(), venv.clone()],
            vec![vpy.clone(), "-m".into(), "pip".into(), "install".into(), "-q".into(), "huggingface_hub".into()],
        ];
        for snap in children(&p.join("snapshots")) {
            let rev = snap.file_name().unwrap_or_default().to_string_lossy().into_owned();
            let mine =
                Json::Obj(refs.iter().filter(|(_, s)| *s == rev).map(|(r, s)| (r.clone(), Json::from(s))).collect());
            let mut s = vec![vpy.clone(), "-c".into(), HF_RESTORE.into(), repo.clone(), kind.into()];
            s.extend([hub.to_string_lossy().into_owned(), rev, mine.to_string()]);
            s.extend(rel_files(&snap));
            steps.push(s);
        }
        c.blocked = if python.is_none() {
            Some("no python3 on PATH to download it again".into())
        } else if steps.len() == 2 {
            Some("no snapshot in it to download again".into())
        } else {
            None
        };
        c.plan = Some(Plan { cwd: ctx.home.clone(), steps, ..Plan::default() });
        out.push(c);
    }
    out
}

/// LM Studio models in its Hugging Face layout (publisher/repo/files), only when one of LM
/// Studio's own records mentions that repo as a download.
fn lmstudio(ctx: &Ctx, _fresh: bool) -> Vec<Cand> {
    let root = ctx.home.join(".lmstudio");
    let models = root.join("models");
    if !is_dir(&models) {
        return vec![];
    }
    let mut records = String::new();
    for p in children(&root.join(".internal")).into_iter().flat_map(|p| if is_dir(&p) { children(&p) } else { vec![p] })
    {
        if p.extension().is_some_and(|e| e == "json") {
            records += &std::fs::read_to_string(&p).unwrap_or_default();
        }
    }
    let tool = hf_tool(ctx);
    let mut out = vec![];
    for publisher in children(&models) {
        for repo_dir in children(&publisher).into_iter().filter(|p| is_dir(p)) {
            let id = format!(
                "{}/{}",
                publisher.file_name().unwrap_or_default().to_string_lossy(),
                repo_dir.file_name().unwrap_or_default().to_string_lossy()
            );
            let files = rel_files(&repo_dir);
            let mut c = Cand::new(repo_dir.clone(), "lmstudio_model", How::Tree);
            c.active = Some(newest_use(&repo_dir));
            c.why = format!("LM Studio download of {id}");
            let mut s = vec![tool.clone().unwrap_or_default(), "download".into(), id.clone()];
            s.extend(files.iter().cloned());
            s.extend(["--local-dir".into(), repo_dir.to_string_lossy().into_owned()]);
            c.plan = Some(Plan { cwd: ctx.home.clone(), steps: vec![s], ..Plan::default() });
            c.blocked = if !records.contains(&id) {
                Some("no LM Studio download record mentions it (it may have been added by hand)".into())
            } else if tool.is_none() {
                Some("no `hf` or `huggingface-cli` on PATH to download it again".into())
            } else {
                None
            };
            out.push(c);
        }
    }
    out
}

// ---------------------------------------------------------------- Docker

/// "1.21GB", "845MB", "1.5GB (100%)", "0B" -> bytes (Docker counts in powers of 1000).
fn docker_size(s: &str) -> u64 {
    let s = s.split_whitespace().next().unwrap_or("");
    let i = s.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(s.len());
    let n: f64 = s[..i].parse().unwrap_or(0.0);
    let mult = match s[i..].to_ascii_lowercase().as_str() {
        "kb" => 1e3,
        "mb" => 1e6,
        "gb" => 1e9,
        "tb" => 1e12,
        _ => 1.0,
    };
    (n * mult) as u64
}

/// Only through the docker CLI found on PATH, and only untagged images (`image rm`, which
/// refuses images a container uses) and build cache (`builder prune`). Never volumes, never
/// the VM disk.
fn docker(ctx: &Ctx, _fresh: bool) -> Vec<Cand> {
    let Some(bin) = proc::which("docker", &ctx.path_var, None) else { return vec![] };
    let rows = |args: &[&str]| -> Vec<Json> {
        let mut c = Command::new(&bin);
        c.args(args);
        proc::run(c, 30)
            .filter(proc::Out::ok)
            .map(|o| o.stdout.lines().filter_map(|l| Json::parse(l).ok()).collect())
            .unwrap_or_default()
    };
    let mut out = vec![];
    let dangling: Vec<Json> = rows(&["images", "--digests", "--format", "json"])
        .into_iter()
        .filter(|r| r.s("Repository") == "<none>" && r.s("Tag") == "<none>")
        .collect();
    let ids: Vec<String> = dangling
        .iter()
        .map(|r| r.s("ID").to_string())
        .filter(|id| !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == ':'))
        .collect();
    if !ids.is_empty() {
        let mut args = vec!["image".to_string(), "rm".into()];
        args.extend(ids.iter().cloned());
        let mut c = Cand::new(PathBuf::from("docker://images/untagged"), "docker_images", How::Docker(args));
        c.bytes = dangling.iter().map(|r| docker_size(r.s("Size"))).sum();
        c.why = format!("{} untagged Docker images left behind by rebuilds", ids.len());
        out.push(c);
    }
    if let Some(cache) = rows(&["system", "df", "--format", "json"]).into_iter().find(|r| r.s("Type") == "Build Cache")
    {
        let bytes = docker_size(cache.s("Reclaimable"));
        if bytes > 0 {
            let mut c = Cand::new(
                PathBuf::from("docker://build-cache"),
                "docker_build_cache",
                How::Docker(vec!["builder".into(), "prune".into(), "-f".into()]),
            );
            c.bytes = bytes;
            c.why = "Docker build cache".into();
            out.push(c);
        }
    }
    for c in &mut out {
        c.sized = true;
        c.frees_host = false;
        c.recent_rule = false;
        c.plan = Some(Plan::note("a cache Docker refills on the next build"));
    }
    out
}

/// A local Time Machine snapshot keeps deleted files' blocks until it expires.
pub fn snapshot_holding() -> bool {
    let mut c = Command::new("/usr/bin/tmutil");
    c.args(["listlocalsnapshots", "/"]);
    proc::run(c, 20).is_some_and(|o| o.stdout.contains("com.apple.TimeMachine"))
}

#[cfg(test)]
mod tests;
