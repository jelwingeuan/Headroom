# headroom-core API

The Rust core is linked into the app as `libheadroom_core.a` (crate `headroom-core`,
`crate-type = ["staticlib", "rlib"]`). Link it with `-framework CoreServices -framework
CoreFoundation`; nothing else is needed (no SQLite, no other libraries).

## C ABI (`app/Sources/CHeadroom/headroom.h`)

```c
int32_t headroom_main(int32_t argc, const char *const *argv); // CLI, MCP server or agent; returns the exit status
char   *headroom_call(const char *request_json);              // one UI request; JSON reply
void    headroom_free(char *reply);                           // frees a reply from headroom_call
```

- `headroom_main` takes the process's arguments with `argv[0]`. It is the whole command line in
  `judge/CONTRACT.md` plus `mcp`, `archive` and `restore` (run `headroom help`).
- `headroom_call` never returns NULL and never unwinds across the boundary. Calls are
  serialized inside the core, so any thread may call it; each call blocks until done (`tick`,
  `undo`, `rescan` and `reclaim` can take minutes: call them off the main thread).
- Every reply is a JSON object. A failed request has an `"error"` string and nothing else is
  promised. Unknown fields should be ignored; missing ones treated as unknown.
- State lives in `$HEADROOM_HOME` (default `~/Library/Application Support/Headroom`).

## UI operations

Requests are `{"op": "<name>", ...}`. Byte counts are integers.

### `status`
Cheap: one `statvfs` and a few small files, no scan. Call it as often as needed.

```json
{"free_bytes": 52000000000, "budget_bytes": 20000000000, "recent_days": 14,
 "dry_run": true, "dry_run_days_left": 5,
 "next": {"path": "/Users/me/code/old/node_modules", "kind": "node_modules", "bytes": 412000000, "reason": "…"},
 "next_action": {"…": "same object as next"},
 "snapshot_holding": false, "permission_errors": 0, "last_tick": 1763251200,
 "reclaimable_bytes": 38000000000, "protected_bytes": 21000000000,
 "protected_by_reason": {"recent": 15000000000, "in_use": 2000000000, "history": 3000000000, "other": 1000000000},
 "totals_at": 1763251200}
```

- `budget_bytes` 0 means no budget chosen yet: autopilot does nothing.
- `next` (alias `next_action`) is what the autopilot would reclaim next, as of the last tick
  that was under pressure; `null` otherwise.
- `snapshot_holding`: a local Time Machine snapshot exists, so deleted bytes may not show up
  as free space until it expires.
- `permission_errors`: folders the last scan couldn't read (Full Disk Access is missing).
- `reclaimable_bytes`: what autopilot could still reclaim under the saved settings (eligible,
  cold, not busy), after that tick's (or `reclaim`'s) own deletes, in private bytes. Docker is
  left out (its space stays inside its VM disk).
- `protected_bytes`: what the same scan found and kept (recent, in use, conversation history,
  tracked, the newest simulator…), in private bytes, nothing counted twice. Kept folders are
  only sized during ticks, never by `scan`, so this costs nothing at status time.
- `protected_by_reason`: `protected_bytes` split into `recent`, `in_use`, `history`, `other`.
- `totals_at`: when that scan ran. All four are `null` until a tick has scanned. A tick
  under budget pressure always scans (dry run included); any other tick, with plenty of space
  or no budget chosen, makes one read-only scan when the saved totals are missing or 24 hours
  old, and deletes nothing. So the totals are at most a day old while ticks run. `rescan` and
  `reclaim` refresh them too.
- `breakdown` lists the folders behind these totals.

### `history`
```json
{"actions": [{"id": "1763251200-3", "path": "…", "kind": "target", "bytes": 1200000000, "at": 1763251200,
              "origin": "autopilot", "reason": "…", "rebuild": "cargo build", "undone": false, "dry_run": false},
             {"id": "dry-1763164800-0", "path": "…", "kind": "node_modules", "bytes": 300000000, "at": 1763164800,
              "dry_run": true, "undone": false}]}
```
Newest first, at most 500. `dry_run: true` rows are what the dry run would have reclaimed
(the "Would Have Done" list); they can't be undone because nothing was deleted. `origin` is
`clean`, `autopilot`, `mcp`, `archive` or `manual` (the `reclaim` op). `frees_host` (false for Docker), `undone_at` and `rebuild_secs` (seconds the rebuild took) come from the history file. `data_loss` (when present) says what undo can't
bring back: a simulator's apps and test data. `partial: true` marks a delete that failed
midway; undo rebuilds over what was left.

### `undo`
`{"op": "undo", "id": "1763251200-3"}` brings one action back by re-running its recorded
command (npm ci, cargo build, xcodebuild, `git worktree add`, `ollama pull`, `hf download`…),
or restoring its archive. It never writes over a folder that exists again. Reply
`{"ok": true, "result": {…}}` or `{"error": "…"}`. Needs the network for package managers.

### `settings_get`
```json
{"budget_bytes": 20000000000, "recent_days": 14, "dry_run": true, "dry_run_choice": null}
```
`dry_run` is what is in effect. `dry_run_choice` is `null` while it follows the automatic
first-7-days rule, else the user's explicit `true`/`false`.

### `settings_set`
`{"op": "settings_set", "budget_bytes": 20000000000, "recent_days": 14, "dry_run": true}`.
Only the fields present change. `recent_days` must be 1–3650. `dry_run` becomes an explicit
choice only when it differs from what is in effect, so sending the whole form back after
changing the budget keeps the automatic dry run. Replies like `settings_get`.

### `tick`
One autopilot pass with the saved settings, honoring the dry run:
`{"events": [{"event": "tick", …}, {"event": "reclaim", …}, …]}` (the events below).

### `digest`
The weekly summary for a notification:
```json
{"title": "Your week with Headroom", "body": "Headroom freed 3.2 GB from 5 folders it can rebuild. …",
 "freed_bytes": 3200000000, "actions": 5, "would_bytes": 0, "shortfalls": 0}
```

### `samples`
Free space over time, for the Insights chart. Cheap: reads `samples.jsonl`, no scan.
```json
{"samples": [{"at": 1763251200, "free": 52000000000, "budget": 20000000000},
             {"at": 1763254800, "free": 51800000000, "budget": 20000000000}]}
```
- Oldest first, at least an hour apart. `free` is the free space a tick saw as it started
  (before it reclaimed anything), and `budget` the budget then (0: none chosen).
- Every tick (the `tick` op, `autopilot --once`, the agent) adds a row when the last one is at
  least an hour old, so the app's 10-minute ticks give one row an hour. Hours with no tick
  have no row.
- Rows older than 400 days are dropped, at most once a day, so up to a day of rows 400 to 401
  days old can still be there. The file never holds more than 24 × 401 + 1 rows, about 0.6 MB.
- `[]` before the first tick.

### `breakdown`
What the scan behind `status`'s totals measured (a tick's, `rescan`'s or `reclaim`'s), folder
by folder. Cheap: reads `last-scan.json`, no scan.
```json
{"entries": [{"path": "/Users/me/code/old/node_modules", "kind": "node_modules", "bytes": 412000000,
              "free": true, "frees_host": true, "project": "/Users/me/code/old",
              "reason": "Node project (package.json); git ignores it; last used 92 days ago; rebuild: npm ci"},
             {"path": "docker://build-cache", "kind": "docker_build_cache", "bytes": 1500000000,
              "free": true, "frees_host": false, "project": null, "reason": "Docker build cache"},
             {"path": "/Users/me/code/app/node_modules", "kind": "node_modules", "bytes": 380000000,
              "free": false, "frees_host": true, "project": "/Users/me/code/app",
              "reason": "used 2 days ago (recent: the last 14 days)"}],
 "kinds": {"node_modules": {"free_bytes": 30000000000, "protected_bytes": 15000000000},
           "ai_history": {"free_bytes": 0, "protected_bytes": 3000000000}},
 "totals_at": 1763251200}
```
- `free: true`: it may go (counted in `reclaimable_bytes` when `frees_host`).
  `free: false`: kept, and `reason` says why (counted in `protected_bytes`). `reason` is the
  text `scan --json` gives.
- `frees_host: false`: Docker, whose space stays inside its VM disk. It is listed when it may
  go (so it can be picked for `reclaim`) but counted in no kind and no total, and autopilot
  never reclaims it. Every other entry has `frees_host: true`.
- `project`: the project `scan --json` gives (git top level, else the folder with the project
  file), or `null` (machine-wide places).
- The same numbers as `status`: summed over every kind, `free_bytes` is `reclaimable_bytes`
  and `protected_bytes` is `protected_bytes`, and `totals_at` is the same scan's. Each folder
  is counted once: a kept folder holding folders that may go counts only the rest, and a kept
  folder inside another candidate is counted in that one and not listed.
- `entries` holds the 2,000 largest, largest first; `kinds` covers everything. A kind's bytes
  not in `entries` are its totals minus its listed entries.
- Against `scan <home> --json` on an unchanged disk: every entry it marks reclaimable is here
  with the same `bytes`, `reason` and `project`. Kept entries (other than those inside
  another candidate) are here too, but match by path only: for speed `scan` doesn't size kept
  folders (most show 0 bytes there), while the tick sizes them for these totals.
- All three are `null` until a tick, `rescan` or `reclaim` has scanned.

### `rescan`
`{"op": "rescan"}` scans home now, as a tick refreshing its totals does (nothing is deleted),
saves `status`'s totals and `last-scan.json` the same way, and replies with the new
`breakdown` object. It is not a tick: `last_tick`, `next` and the samples don't change.

### `reclaim`
`{"op": "reclaim", "paths": ["/Users/me/code/old/node_modules", "docker://build-cache"]}`
reclaims folders picked from `breakdown`.
- `paths` must be a non-empty array of strings, else `{"error": "…"}`. A path given twice is
  handled once.
- One scan of home. A path goes only if that scan found a candidate with exactly that path and
  says it may go (`free: true`; Docker too, since the user picked it). Each is rechecked from
  scratch right before its delete, as autopilot's are, and recorded in history with `origin`
  `manual`.
- In the dry run nothing is deleted: each is added to the dry-run log, as a tick does.
- Reply `{"events": […], "breakdown": {…}}`. One event per path, in order: `reclaim` or
  `would_reclaim` (as in the table below), or `skip` with `path` and `reason`: `"not found in
  the latest scan"` (no `kind`), the scan's reason for keeping it, or why the recheck refused
  it. `breakdown` is the new one, saved as `rescan` saves it, without what was reclaimed.

## Autopilot events (`tick` op and `headroom autopilot --once`, one JSON object per line)

| event | fields |
|---|---|
| `tick` | `now`, `free_bytes`, `budget_bytes`, `dry_run`, `dry_run_days_left` |
| `reclaim` | `id`, `path`, `kind`, `bytes` (private bytes, measured right before deleting) |
| `would_reclaim` | `path`, `kind`, `bytes`, `reason` (dry run: nothing deleted) |
| `shrink` | `files`, `bytes` (free space statfs saw come back, at most `private_bytes`), `private_bytes` (what the replaced copies held) |
| `skip` | `path`, `kind`, `reason` (the recheck before delete refused it) |
| `shortfall` | `needed_bytes`, `eligible_bytes` (still eligible but not reclaimed; 0 = nothing left), `dry_run` |
| `error` | `error` |

## MCP server (`headroom mcp`)

JSON-RPC 2.0, one message per line on stdin/stdout. Methods: `initialize`, `ping`,
`tools/list`, `tools/call`; notifications get no reply. Two tools, no arguments, both fixed
to the client's working project (its git top level, else its working folder):

- `scan`: the `scan --json` array for that project only.
- `clean`: the `clean --apply --json` object for that project only, under every rule, and
  during the dry run it only reports (`"dry_run": true`).

The project must be a git top level or a folder with a project file (`package.json`,
`Cargo.toml`…) at its top, and never the home folder or above it (a dotfiles repo at `~`
included); otherwise both tools return an error. There is no tool for settings, undo, other
paths or forcing anything (docs/THREAT-MODEL.md, invariant 4).

## What the history file holds

`HEADROOM_HOME/history.jsonl`, one action per line: `id`, `at`, `origin`, `path`, `kind`,
`bytes` (private bytes measured right before the delete), `frees_host` (false for Docker,
whose space stays inside its VM disk), `cwd`, `steps` (argument arrays undo runs), `fallback`,
`note` (why nothing needs to run), `lock_file`, `lock_sha256`, `build_secs`, `why`, `undone`,
and when they apply `new_device` (undo makes a new simulator instead of the same path),
`data_loss`, `partial`, `error`.

Kinds: the folder names of the project rules (`node_modules`, `target`, `.next`, …), `venv`,
`worktree`, `DerivedData`, `device_support`, `xcode_archive` (listed, never deleted),
`simulator`, `claude_code_old`, `codex_temp`, `electron_cache`, `ai_history` (listed, never
deleted), `ollama_model`, `hf_model`, `lmstudio_model`, `docker_images`, `docker_build_cache`.

`clean --apply --json` also prints `dedupe: {files, bytes}`: identical files in kept
`node_modules` of projects sharing a lockfile, swapped for clones after the deletes; `bytes` is
what statfs saw come free and is included in `freed_bytes`.

## What the Insights files hold

- `HEADROOM_HOME/samples.jsonl`, one sample per line, oldest first: `at` (unix seconds),
  `free` and `budget` (bytes). Written by ticks under the HEADROOM_HOME lock: appended at most
  hourly; rewritten atomically (temporary file and rename) at most once a day to drop rows
  older than 400 days. Lines that don't parse are skipped.
- `HEADROOM_HOME/last-scan.json`, one object: exactly what `breakdown` returns. Rewritten
  atomically by every tick that scans (any tick under pressure, else at most daily) and by
  `rescan` and `reclaim`, under the same lock as the state file holding `status`'s totals, so
  both come from the same scan.

## Known limits

- Ollama: "came from the registry" is judged by name (a built-in list of library models or
  `hf.co/…`), no `from` on any layer, and weights no other model uses; a model made locally
  under an official library name would pass that test.
- LM Studio: the "download record" is any JSON under `~/.lmstudio/.internal` naming the repo;
  that format wasn't checked against a real install.
- "Recently changed" means git activity (reflog, commits), editor records and what Headroom
  saw; uncommitted edits made in other editors don't count. Reflog bookkeeping (the entries
  `git worktree add` writes, `reset: moving to HEAD`) doesn't count as use.
- Ollama and Hugging Face "last used" includes file access times, which anything reading the
  files refreshes (a backup, a hashing script): such a read makes a model look recently used.
- Simulators: "last used" is simctl's `lastUsedAt`, else the device folder's creation time.
- Dedupe: the original stays open and linked during each swap and is put back if it was
  written meanwhile; a write that lands after that final check and before the spare link is
  removed (microseconds) would be lost from the path.
- Hugging Face restore passes the snapshot's file names as `allow_patterns`, which are globs: a
  name containing `*`, `?` or `[` may match other files of that revision, or none.
- Simulators: "newest of its type" means most recently used (else created), so a type nobody
  has used for months still keeps one device.
- Hugging Face: newer huggingface_hub keeps file contents in a store shared by every repo
  (`hub/blobs/xx/…`) and links to it from each repo folder. Headroom never deletes that shared
  store, so deleting such a repo folder frees only its links and small files.
