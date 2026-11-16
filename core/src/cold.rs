//! When was a project last used? The newest of: git (reflog of HEAD, else the HEAD commit),
//! editors' records of open folders (VS Code and its forks, JetBrains), and what Headroom saw
//! itself (a process working in it, a build writing into it). File times alone are only used
//! for a folder that isn't a git checkout, where nothing better exists.

use crate::json::Json;
use crate::state::Ctx;
use std::path::{Path, PathBuf};

pub struct Signals {
    /// (folder, when an editor last had it open)
    pub editors: Vec<(PathBuf, u64)>,
    pub observed: Vec<(PathBuf, u64)>,
}

pub fn signals(ctx: &Ctx) -> Signals {
    let mut editors = vec![];
    let support = ctx.home.join("Library/Application Support");
    for app in ["Code", "Code - Insiders", "Cursor", "VSCodium", "Windsurf"] {
        editors.extend(vscode_storage(&support.join(app).join("User/globalStorage/storage.json")));
    }
    if let Ok(rd) = std::fs::read_dir(support.join("JetBrains")) {
        for e in rd.flatten() {
            editors.extend(jetbrains(&e.path().join("options/recentProjects.xml"), &ctx.home));
        }
    }
    Signals { editors, observed: crate::state::observed(ctx) }
}

pub fn inside(p: &Path, top: &Path) -> bool {
    p.starts_with(top)
}

/// The folders VS Code (or a fork) has open, stamped with the time it last wrote that list.
pub fn vscode_storage(file: &Path) -> Vec<(PathBuf, u64)> {
    let Ok(m) = std::fs::metadata(file) else { return vec![] };
    let t = mtime(&m);
    let Some(j) = std::fs::read_to_string(file).ok().and_then(|s| Json::parse(&s).ok()) else { return vec![] };
    let mut uris: Vec<&str> = vec![];
    if let Some(w) = j.get("windowsState") {
        let wins = w.get("lastActiveWindow").into_iter().chain(w.get("openedWindows").map(Json::arr).unwrap_or(&[]));
        for win in wins {
            uris.extend(win.get("folder").and_then(Json::str));
            uris.extend(win.get("workspace").and_then(|x| x.get("configPath")).and_then(Json::str));
        }
    }
    if let Some(b) = j.get("backupWorkspaces") {
        uris.extend(b.get("folders").map(Json::arr).unwrap_or(&[]).iter().filter_map(|f| f.get("folderUri")?.str()));
    }
    uris.into_iter()
        .filter_map(file_uri)
        .map(|p| if p.extension().is_some_and(|e| e == "code-workspace") { p.parent().unwrap_or(&p).into() } else { p })
        .map(|p| (p, t))
        .collect()
}

/// "file:///Users/me/My%20App" -> /Users/me/My App. Other schemes (remote, vscode-vfs) -> None.
pub fn file_uri(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let b = rest.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex =
            b.get(i + 1..i + 3).and_then(|h| std::str::from_utf8(h).ok()).and_then(|h| u8::from_str_radix(h, 16).ok());
        if let (b'%', Some(v)) = (b[i], hex) {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    let p = PathBuf::from(String::from_utf8(out).ok()?);
    p.is_absolute().then_some(p)
}

/// JetBrains' recentProjects.xml: `<entry key="$USER_HOME$/x"> … activationTimestamp="ms"`.
fn jetbrains(file: &Path, home: &Path) -> Vec<(PathBuf, u64)> {
    let Ok(text) = std::fs::read_to_string(file) else { return vec![] };
    let mut out = vec![];
    for chunk in text.split("<entry key=\"").skip(1) {
        let Some((key, rest)) = chunk.split_once('"') else { continue };
        let body = rest.split("</entry>").next().unwrap_or(rest);
        let stamp = ["activationTimestamp=\"", "projectOpenTimestamp=\""]
            .iter()
            .filter_map(|a| body.split(a).nth(1)?.split('"').next()?.parse::<u64>().ok())
            .max();
        if let Some(ms) = stamp {
            let path = key.replace("$USER_HOME$", &home.to_string_lossy());
            out.push((PathBuf::from(path), ms / 1000));
        }
    }
    out
}

pub fn mtime(m: &std::fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    m.mtime().max(0) as u64
}

/// The last time `project` was used, or None when nothing at all is known.
pub fn last_active(ctx: &Ctx, sig: &Signals, project: &Path, repo: Option<&Path>) -> Option<u64> {
    let related = |p: &Path| inside(p, project) || (inside(project, p) && !inside(&ctx.home, p));
    let mut best = sig.editors.iter().chain(&sig.observed).filter(|(p, _)| related(p)).map(|(_, t)| *t).max();
    let git = repo.and_then(crate::git::last_activity);
    best = best.max(git);
    if repo.is_none() {
        best = best.max(top_level_mtime(project));
    }
    best
}

/// Newest mtime of a non-git project's own top-level files (build folders and dot entries skipped).
fn top_level_mtime(dir: &Path) -> Option<u64> {
    let names: Vec<String> =
        std::fs::read_dir(dir).ok()?.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    let rule_dirs: Vec<&str> = crate::rules::rules_for(&names).flat_map(|r| r.dirs.iter().copied()).collect();
    names
        .iter()
        .filter(|n| !n.starts_with('.') && !rule_dirs.contains(&n.as_str()))
        .filter_map(|n| std::fs::symlink_metadata(dir.join(n)).ok())
        .map(|m| mtime(&m))
        .max()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_vscode_storage_like_the_judge_writes_it() {
        let t = crate::testutil::tmp("cold-vscode");
        let f = t.join("storage.json");
        let data = r#"{"windowsState":{"openedWindows":[{"folder":"file:///Users/a/My%20App"}],
            "lastActiveWindow":{"folder":"file:///Users/a/one","uiState":{"mode":1}}},
            "backupWorkspaces":{"workspaces":[],"folders":[{"folderUri":"file:///Users/a/two"}],"emptyWindows":[]},
            "profileAssociations":{"workspaces":{"file:///Users/a/never":"__default__profile__"}}}"#;
        std::fs::write(&f, data).unwrap();
        let mut got: Vec<_> = vscode_storage(&f).into_iter().map(|(p, _)| p).collect();
        got.sort();
        let want: Vec<PathBuf> =
            ["/Users/a/My App", "/Users/a/one", "/Users/a/two"].iter().map(PathBuf::from).collect();
        assert_eq!(got, want);
        assert_eq!(file_uri("vscode-remote://ssh/x"), None);
    }

    #[test]
    fn reads_jetbrains_recents() {
        let t = crate::testutil::tmp("cold-jb");
        let f = t.join("recentProjects.xml");
        std::fs::write(
            &f,
            r#"<application><component name="RecentProjectsManager"><option name="additionalInfo"><map>
            <entry key="$USER_HOME$/code/app"><value><RecentProjectMetaInfo projectOpenTimestamp="1700000000000" activationTimestamp="1700000500000"/></value></entry>
            </map></option></component></application>"#,
        )
        .unwrap();
        assert_eq!(jetbrains(&f, Path::new("/Users/a")), vec![(PathBuf::from("/Users/a/code/app"), 1_700_000_500)]);
    }

    #[test]
    fn an_editor_open_on_home_does_not_make_everything_recent() {
        let ctx = crate::testutil::ctx("cold-home");
        let sig = Signals { editors: vec![(ctx.home.clone(), ctx.now)], observed: vec![] };
        let proj = ctx.home.join("p");
        std::fs::create_dir_all(&proj).unwrap();
        crate::testutil::git(&proj, &["init", "-q"], None);
        std::fs::write(proj.join("f"), "x").unwrap();
        crate::testutil::git(&proj, &["add", "f"], None);
        crate::testutil::git(&proj, &["commit", "-qm", "c"], Some(1_000_000_000));
        assert_eq!(last_active(&ctx, &sig, &proj, Some(&proj)), Some(1_000_000_000));
        let sig = Signals { editors: vec![(proj.join("src"), ctx.now)], observed: vec![] };
        assert_eq!(last_active(&ctx, &sig, &proj, Some(&proj)), Some(ctx.now));
    }
}
