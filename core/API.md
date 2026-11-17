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
  serialized inside the core, so any thread may call it; each call blocks until done (`tick`
  and `undo` can take minutes: call them off the main thread).
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
 "snapshot_holding": false, "permission_errors": 0, "last_tick": 1763251200}
```

- `budget_bytes` 0 means no budget chosen yet: autopilot does nothing.
- `next` (alias `next_action`) is what the autopilot would reclaim next, as of the last tick
  that was under pressure; `null` otherwise.
- `snapshot_holding`: a local Time Machine snapshot exists, so deleted bytes may not show up
  as free space until it expires.
- `permission_errors`: folders the last scan couldn't read (Full Disk Access is missing).

### `history`
```json
{"actions": [{"id": "1763251200-3", "path": "…", "kind": "target", "bytes": 1200000000, "at": 1763251200,
              "origin": "autopilot", "reason": "…", "rebuild": "cargo build", "undone": false, "dry_run": false},
             {"id": "dry-1763164800-0", "path": "…", "kind": "node_modules", "bytes": 300000000, "at": 1763164800,
              "dry_run": true, "undone": false}]}
```
Newest first, at most 500. `dry_run: true` rows are what the dry run would have reclaimed
(the "Would Have Done" list); they can't be undone because nothing was deleted. `origin` is
`clean`, `autopilot`, `mcp` or `archive`. `data_loss` (when present) says what undo can't
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
