# Headroom CLI contract

The judge treats Headroom as a black box and relies only on what this file says. The app
implements it exactly. This file is part of the judge: a PR may change `judge/` or
`core/` + `app/`, never both.

## The binary

`bash scripts/bundle.sh`, run from the repo root on macOS, builds `Headroom.app`. The judge
runs the first `*/Headroom.app/Contents/MacOS/Headroom` it finds in the checkout. Given
arguments, it is a CLI: no window, no prompt, it exits when done.

- Exit status 0 means success; anything else is a failure.
- stdout carries only the output described below. Logs go to stderr.
- `HEADROOM_HOME`, when set, is the only place Headroom keeps state (history, recorded rebuild
  commands, autopilot memory). The judge sets it to a fresh folder.
- External tools such as `docker` are looked up on `PATH`. The judge puts a recording
  `docker` first on `PATH`.
- `scan`, `clean`, `autopilot` and `agent` run under
  `sandbox-exec -p '(version 1)(allow default)(deny network*)(allow network* (local unix))'`
  and must still exit 0. TCP to localhost is denied too; unix sockets work. `rebuild` and
  `undo` run without the sandbox because they run package managers.

## Commands

### `headroom scan <root> --json`

Changes nothing. Prints a JSON array with one object per candidate:

```json
{"path": "/absolute/path", "kind": "node_modules", "bytes": 123, "reclaimable": true, "reason": "text"}
```

`bytes` is private allocated bytes. Candidates come from `<root>` and from the machine-wide
places Headroom knows (Xcode DerivedData, model caches, Docker, AI-tool leftovers).

### `headroom clean <root> --apply --json`

Deletes, for real (not to the Trash), every candidate that `scan <root>` marks reclaimable,
and records how to rebuild each one. Prints one JSON object:

```json
{"freed_bytes": 123, "actions": [{"path": "/absolute/path", "kind": "node_modules", "bytes": 123}]}
```

### `headroom rebuild --all`

Re-runs the recorded install or build command of everything `clean` or `autopilot` deleted,
so everything it deleted exists again.

### `headroom undo --all`

Undoes every recorded `clean` and `autopilot` action not undone yet. For a rebuildable
folder, undo is rebuild.

### `headroom autopilot --once --now <unix> --budget-bytes <N> --recent-days <D>`

One autopilot tick, as if the clock read `<unix>`. Goal: free space on the volume that holds
the home folder (`statvfs`: `f_bavail * f_frsize`) stays at or above `<N>`. Never touches a
project opened or changed in the `<D>` days before `<unix>`. State carries over between ticks
in `HEADROOM_HOME`. Prints JSON Lines, one event per line. The judge reads two kinds:

```json
{"event": "reclaim", "path": "/absolute/path", "kind": "node_modules", "bytes": 123}
{"event": "shortfall", "needed_bytes": 123, "eligible_bytes": 0}
```

One `reclaim` per deletion. A `shortfall` when the budget cannot be reached. Other events
are allowed and ignored.

### `headroom agent --idle`

Runs the background agent, without UI, until SIGTERM. It must stay alive.

## How the judge calls it

1. Safety and reclaim: `clean <world> --apply --json`, then `rebuild --all`. Free space is
   read before and after `clean`; the gain must be at least 80% of the eligible bytes: the
   allocated size (hard links once), measured right before `clean`, of everything listed
   below as "may be deleted" that is not protected, recently opened or busy. After the
   rebuild the judge runs each touched project's own build or check command (`npm ls`,
   `next build`, `cargo build`, `xcodebuild`, ...); all must exit 0. The summary adds a gap
   table (eligible freed, eligible kept, protected, freed but not counted) built from
   `truth.json` and `HEADROOM_HOME/history.jsonl`: one JSON object per line with at least
   `path`, `kind`, `bytes` and `frees_host` (false when deleting it frees nothing on this
   volume, such as Docker's VM disk). It explains the score and never changes it.
2. Autopilot: 30 calls of `autopilot --once`, `--now` advancing by 86400 each call,
   `--recent-days 14`. Before each call the judge writes one growth file (user data, not
   reclaimable). The budget leaves the first 8 ticks without pressure. Then `undo --all`.
3. Speed: `scan <tree> --json` twice on 1,000,000 empty files shaped like node_modules; the
   second run must take under 3 s. Then `agent --idle`: `ps -o %cpu=` every second for 60 s
   after a 5 s settle, mean under 1%.

## What must survive

The judge hashes (SHA-256) every file below before and after each step. A lost or changed
file fails the gate.

- Every file in the world outside the rebuildable folders: sources, lockfiles, IDE metadata
  and `.git`, `.git/index` included (run git with `GIT_OPTIONAL_LOCKS=0`).
- `build/` and `dist/` that git tracks, or that hold untracked files git does not ignore.
- A hand-edited file inside a `node_modules` (its mtime is a day after the install).
- The dependencies of a project with a running process whose cwd is inside it.
- Everything in a project opened in the last 14 days.
- A venv with no manifest next to it (`requirements*.txt`, `pyproject.toml`, `Pipfile`, ...).
- Agent worktrees with unpushed commits or with ignored files such as `.env`.
- `~/.claude/projects` and `~/.codex/sessions`.
- An `ollama create` model and every blob it uses, shared ones included.
- The newest iOS simulator of each device type (the judge hashes its test data).
- The current Claude Code version (the one `~/.local/bin/claude` points at) and the launcher.
- An Electron app's caches while that app is running, and its data (config, `Preferences`,
  `Local Storage`, `IndexedDB`) always.
- VS Code's `storage.json` and Docker Desktop's folder (its VM disk).
- The bare repos in `~/judge-remotes` and the judge's growth files.

May be deleted, and must come back on `rebuild --all` / `undo --all`: `node_modules` (also
several copies of one project that share a lockfile), `.next`, Rust `target`, a `.venv` with a
manifest, Xcode DerivedData, a merged and clean agent worktree (with its
`.git/worktrees/<name>`), a pulled Ollama model's own files, the Hugging Face cache snapshot.

May be deleted, and come back as a new blank device with the same name, device type and
runtime: an iOS simulator not booted for 30 days or more that is not the newest of its device
type (its test data is lost; History says so).

May be deleted, and need not come back: old Claude Code versions in
`~/.local/share/claude/versions` (not the current one), and the Chromium caches (`Cache`,
`Code Cache`) of Claude desktop while Claude is not running; the app refills them.

Docker: read-only commands plus `image rm`, `rmi`, `image prune` and `builder prune` only.
Any other docker command, `volume rm`, `volume prune` and `system prune` included, fails.

## Fixtures Headroom can read

- Every project is a git repo. Cold ones have commit dates and file mtimes 60 to 200 days
  old, warm ones 20 to 29, recent ones 2.
- Recently opened projects are listed in
  `~/Library/Application Support/Code/User/globalStorage/storage.json`
  (`windowsState.lastActiveWindow`, `windowsState.openedWindows`, `backupWorkspaces.folders`).
  The file's mtime is when VS Code last saw them open; during autopilot the judge sets it to
  `--now` minus one hour before each tick.
- The busy project runs `node server.js` with its cwd in the project.
- Agent worktrees live in `<repo>/.claude/worktrees/`; the remote is a local bare repo.
- Ollama models in `~/.ollama/models`: two pulled registry models and one `ollama create`
  model built on the first. Hugging Face cache in `~/.cache/huggingface/hub`.
- iOS simulators made with `xcrun simctl create` on the newest iOS runtime the runner has,
  never booted: two cold ones (folder mtimes 75 and 140 days old, each holding test data under
  `data/Containers/Data/Application/`) and the newest device of the same type. No iOS runtime
  on the runner: no simulators, reported as not measured.
- Claude Code: `~/.local/share/claude/versions/<version>` executables, three old ones and the
  current one, with `~/.local/bin/claude` a symlink to the current one.
- Claude desktop: `~/Library/Application Support/Claude` with `Cache/Cache_Data` and
  `Code Cache/js` filled, plus app data. Claude is not running on the runner.
- Five or more copies of one Node project with the same `package.json` and
  `package-lock.json`, so their `node_modules` are identical (a dedupe target).
- All of the stock above is sized from the runner's free space, planned to leave 15 GB free.
- No Docker daemon. The shim answers `images`, `image ls`, `system df`, `builder du`,
  `volume ls`, `ps`, `version` and `info` (JSON Lines with `--format json`); every other
  command prints nothing and exits 0.
