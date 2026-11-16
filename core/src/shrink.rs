//! Shrinking without losing anything: APFS clone dedupe of identical dependency files across
//! projects, and moving a whole project to another drive with a verified SHA-256 manifest.

use crate::json::Json;
use crate::scan::{self, par_map, par_walk};
use crate::state::Ctx;
use crate::sys::{self, VDIR, VLNK, VREG};
use crate::{delete, obj, sha256};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};

const MIN_DEDUPE: u64 = 16 * 1024;

#[derive(Default, Debug)]
pub struct Dedupe {
    pub files: usize,
    /// Private bytes the replaced copies held: freed once they are gone.
    pub bytes: u64,
}

#[derive(Clone)]
struct F {
    path: PathBuf,
    dev: u64,
    ino: u64,
    len: u64,
    private: u64,
    mtime: i64,
}

/// Replaces byte-identical files under `dirs` with APFS clones of one copy (hash, clone to a
/// temporary name beside the copy, keep its mode and mtime, atomic rename over it).
pub fn dedupe(dirs: &[PathBuf]) -> Dedupe {
    let files: Vec<F> = par_walk(dirs.to_vec(), |d: PathBuf, more, out: &mut Vec<F>| {
        let Ok(es) = sys::list_path(&d) else { return };
        for e in es.into_iter().filter(|e| e.error == 0) {
            let p = d.join(e.name_os());
            if e.kind == VDIR {
                more.push(p);
            } else if e.kind == VREG && e.nlink == 1 && e.len >= MIN_DEDUPE && e.private > 0 {
                out.push(F { path: p, dev: e.dev, ino: e.ino, len: e.len, private: e.private, mtime: e.mtime });
            }
        }
    });
    let mut by_size: HashMap<(u64, u64), Vec<F>> = HashMap::new();
    for f in files {
        by_size.entry((f.dev, f.len)).or_default().push(f);
    }
    let candidates: Vec<F> = by_size.into_values().filter(|v| v.len() > 1).flatten().collect();
    let hashed: Vec<(String, F)> = par_map(candidates, |f: F| (sha256::file(&f.path).unwrap_or_default(), f))
        .into_iter()
        .filter(|(h, _)| !h.is_empty())
        .collect();
    let mut groups: HashMap<(u64, String), Vec<F>> = HashMap::new();
    for (h, f) in hashed {
        groups.entry((f.dev, h)).or_default().push(f);
    }
    let mut r = Dedupe::default();
    for (_, mut g) in groups.into_iter().filter(|(_, g)| g.len() > 1) {
        g.sort_by(|a, b| a.path.cmp(&b.path));
        let src = g[0].path.clone();
        for dup in &g[1..] {
            if replace_with_clone(&src, dup).is_ok() {
                r.files += 1;
                r.bytes += dup.private;
            }
        }
    }
    r
}

fn replace_with_clone(src: &Path, dup: &F) -> std::io::Result<()> {
    let name = dup.path.file_name().unwrap_or_default().to_string_lossy();
    let tmp = dup.path.with_file_name(format!(".{name}.headroom-clone"));
    let _ = fs::remove_file(&tmp);
    sys::clone_file(src, &tmp)?;
    let done = (|| {
        let m = fs::symlink_metadata(&dup.path)?;
        if (m.dev(), m.ino(), m.len(), m.mtime()) != (dup.dev, dup.ino, dup.len, dup.mtime) || !m.is_file() {
            return Err(std::io::Error::other("changed while deduping"));
        }
        fs::set_permissions(&tmp, fs::Permissions::from_mode(m.mode() & 0o7777))?;
        OpenOptions::new().write(true).open(&tmp)?.set_modified(m.modified()?)?;
        fs::rename(&tmp, &dup.path)
    })();
    if done.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    done
}

// ---------------------------------------------------------------- archive / restore

const STUB: &str = "HEADROOM-ARCHIVED.json";

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Copies `project` to `<volume>/Headroom Archives/<name>-<time>/`, verifies every copy
/// against its SHA-256, writes the manifest, then deletes the project and leaves a stub.
/// Rebuildable folders are not copied; the manifest lists how to bring them back.
pub fn archive(ctx: &Ctx, project: &Path, volume: &Path, require_external: bool) -> Result<Json, String> {
    let project = project.canonicalize().map_err(err)?;
    let volume = volume.canonicalize().map_err(err)?;
    scan::refuse(ctx, &project)?;
    if project == ctx.home || !project.is_dir() {
        return Err("archive takes one project folder".into());
    }
    let pm = fs::symlink_metadata(&project).map_err(err)?;
    if require_external && (fs::metadata(&volume).map_err(err)?.dev() == pm.dev() || !volume.starts_with("/Volumes")) {
        return Err("the archive must go to another drive (under /Volumes)".into());
    }
    if volume.starts_with(&project) {
        return Err("the archive can't go inside the project".into());
    }
    // Rebuildable folders stay behind (they're deleted with the project), work is copied.
    let mut sctx = ctx.clone();
    sctx.machine = false;
    sctx.recent_days = 0;
    let s = scan::scan(&sctx, &project)?;
    if let Some(busy) = s.cands.iter().find_map(|c| c.blocked.as_deref().filter(|b| b.starts_with("in use"))) {
        return Err(busy.to_string());
    }
    let skip: Vec<&scan::Cand> = s.cands.iter().filter(|c| c.reclaimable() && c.how == scan::How::Tree).collect();
    let mut entries = vec![];
    let mut stack = vec![project.clone()];
    while let Some(d) = stack.pop() {
        for e in sys::list_path(&d).map_err(|e| format!("{}: {e}", d.display()))? {
            let p = d.join(e.name_os());
            if skip.iter().any(|c| c.path == p) {
                continue;
            }
            let rel = p.strip_prefix(&project).map_err(err)?.to_string_lossy().into_owned();
            match e.kind {
                VDIR => {
                    entries.push((rel, None));
                    stack.push(p);
                }
                VREG => entries.push((rel, Some(e.len))),
                VLNK => return Err(format!("{rel} is a symlink; Headroom archives only plain files and folders")),
                _ => return Err(format!("{rel} is not a plain file")),
            }
        }
    }
    let name = project.file_name().unwrap_or_default().to_string_lossy().into_owned();
    let dest = volume.join("Headroom Archives").join(format!("{name}-{}", ctx.now));
    let files_dir = dest.join("files");
    fs::create_dir_all(&files_dir).map_err(err)?;
    let mut manifest = vec![];
    for (rel, len) in &entries {
        let (src, dst) = (project.join(rel), files_dir.join(rel));
        let mode = fs::symlink_metadata(&src).map_err(err)?.mode() & 0o777;
        match len {
            None => {
                fs::create_dir_all(&dst).map_err(err)?;
                manifest.push(obj! {"path" => rel.as_str(), "dir" => true, "mode" => mode});
            }
            Some(len) => {
                let h = copy_hashed(&src, &dst)?;
                if sha256::file(&dst).map_err(err)? != h {
                    return Err(format!("the copy of {rel} doesn't match: nothing was deleted"));
                }
                manifest.push(obj! {"path" => rel.as_str(), "size" => *len, "sha256" => h, "mode" => mode});
            }
        }
    }
    let rebuild: Vec<Json> = skip
        .iter()
        .map(|c| obj! {"path" => &c.path, "kind" => &c.kind, "how" => c.plan.as_ref().map(|p| p.describe())})
        .collect();
    let m =
        obj! {"version" => 1, "origin" => &project, "created" => ctx.now, "files" => manifest, "rebuild" => rebuild};
    crate::state::write_atomic(&dest.join("manifest.json"), m.to_string().as_bytes()).map_err(err)?;
    delete::delete_tree(&project, pm.dev(), pm.ino())
        .map_err(|e| format!("archived to {}, but deleting the original failed: {e}", dest.display()))?;
    fs::create_dir(&project).map_err(err)?;
    let stub = obj! {"archive" => &dest, "restore" => format!("headroom restore '{}'", dest.display())};
    fs::write(project.join(STUB), stub.to_string()).map_err(err)?;
    let action = obj! {"at" => ctx.now, "origin" => "archive", "path" => &project, "kind" => "archive", "bytes" => entries.iter().filter_map(|(_, l)| *l).sum::<u64>(), "archive" => &dest, "undone" => false, "steps" => Vec::<Json>::new()};
    let id = crate::state::record(ctx, action);
    Ok(obj! {"id" => id, "archive" => &dest, "files" => entries.len(), "left_to_rebuild" => skip.len()})
}

fn copy_hashed(src: &Path, dst: &Path) -> Result<String, String> {
    let mut r = OpenOptions::new()
        .read(true)
        .custom_flags(0x100 /* O_NOFOLLOW */)
        .open(src)
        .map_err(|e| format!("{}: {e}", src.display()))?;
    let mut w = OpenOptions::new()
        .write(true)
        .create_new(true)
        .custom_flags(0x100)
        .open(dst)
        .map_err(|e| format!("{}: {e}", dst.display()))?;
    let mut h = sha256::Sha256::default();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = r.read(&mut buf).map_err(err)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
        w.write_all(&buf[..n]).map_err(err)?;
    }
    w.sync_all().map_err(err)?;
    Ok(h.hex())
}

/// A manifest path that stays inside its folder: relative, no `..`, no `.`, not empty.
fn safe_rel(rel: &str) -> bool {
    !rel.is_empty() && Path::new(rel).components().all(|c| matches!(c, Component::Normal(_)))
}

/// Restores an archive into its original path, which must be missing, empty, or hold only
/// the stub. Everything is verified before anything appears there; symlinks are refused.
pub fn restore(archive: &Path) -> Result<Json, String> {
    let mpath = archive.join("manifest.json");
    if !fs::symlink_metadata(&mpath).map_err(err)?.is_file() {
        return Err("manifest.json is not a plain file".into());
    }
    let m = Json::parse(&fs::read_to_string(&mpath).map_err(err)?)?;
    let origin = PathBuf::from(m.s("origin"));
    if !origin.is_absolute() || !safe_rel(&origin.to_string_lossy()[1..]) {
        return Err("the manifest's original path is not a clean absolute path".into());
    }
    if let Ok(rd) = fs::read_dir(&origin) {
        let left: Vec<String> = rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        if left.iter().any(|n| n != STUB) {
            return Err(format!("{} is not empty: nothing restored", origin.display()));
        }
    } else if fs::symlink_metadata(&origin).is_ok() {
        return Err(format!("{} exists and is not a folder", origin.display()));
    }
    let parent = origin.parent().ok_or("no parent folder")?;
    let staging = parent.join(format!(
        ".{}.headroom-restore-{}",
        origin.file_name().unwrap_or_default().to_string_lossy(),
        std::process::id()
    ));
    fs::create_dir(&staging).map_err(err)?;
    let result = (|| {
        let files = archive.join("files");
        let mut n = 0;
        for e in m.get("files").map(Json::arr).unwrap_or(&[]) {
            let rel = e.s("path");
            if !safe_rel(rel) {
                return Err(format!("refused: unsafe path {rel:?} in the manifest"));
            }
            // No component of the source may be a symlink (the drive is untrusted).
            let mut p = files.clone();
            for c in Path::new(rel).components() {
                p.push(c);
                if fs::symlink_metadata(&p).map_err(|e| format!("{rel}: {e}"))?.file_type().is_symlink() {
                    return Err(format!("refused: {rel} is a symlink in the archive"));
                }
            }
            let mode = e.get("mode").and_then(Json::u64).unwrap_or(0o644) as u32 & 0o777;
            let dst = staging.join(rel);
            if e.get("dir") == Some(&Json::Bool(true)) {
                fs::create_dir_all(&dst).map_err(err)?;
            } else {
                if let Some(d) = dst.parent() {
                    fs::create_dir_all(d).map_err(err)?;
                }
                if copy_hashed(&p, &dst)? != e.s("sha256") {
                    return Err(format!("refused: {rel} doesn't match its SHA-256"));
                }
            }
            fs::set_permissions(
                &dst,
                fs::Permissions::from_mode(mode | if e.get("dir").is_some() { 0o700 } else { 0 }),
            )
            .map_err(err)?;
            n += 1;
        }
        Ok(n)
    })();
    let n = match result {
        Ok(n) => n,
        Err(e) => {
            if let Ok(sm) = fs::symlink_metadata(&staging) {
                let _ = delete::delete_tree(&staging, sm.dev(), sm.ino());
            }
            return Err(e);
        }
    };
    if origin.is_dir() {
        let _ = fs::remove_file(origin.join(STUB));
        fs::remove_dir(&origin).map_err(err)?;
    }
    fs::rename(&staging, &origin).map_err(err)?;
    Ok(obj! {"restored" => &origin, "files" => n})
}

#[cfg(test)]
mod tests;
