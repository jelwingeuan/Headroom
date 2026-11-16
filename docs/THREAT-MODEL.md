# Headroom threat model

Headroom deletes files on its own. The main risk is losing someone's work, not an attacker
breaking in. Every rule below exists to make that loss impossible, and each one is backed by a
test or a judge check.

## What Headroom may delete

Only bytes it can prove are rebuildable. "Undo" means rebuilding them by running the recorded
install or build command again.

| Area | Proof rule | Never |
|---|---|---|
| Project build output and dependencies | A marker file sits beside the folder (`node_modules` beside `package.json`, `target` beside `Cargo.toml`…). Ambiguous names (`build`, `dist`, `out`, `vendor`…) only count when git says they are ignored. Nothing inside may be tracked by git. | Folders with tracked files; a venv with no dependency list; a project whose dev server or build is running; a project opened in the last N days |
| AI-tool leftovers | Old app/CLI versions not in use, stale install temp folders | Conversation history (`~/.claude/projects`, `~/.codex/sessions`, …) |
| Docker | Images and build cache, only through `docker` commands | Volumes (they often hold databases); the Docker VM disk |
| Local AI models | A registry source exists (Ollama registry manifest, Hugging Face repo id, LM Studio download record), and it hasn't been loaded recently | Models built locally (`ollama create`) or imported by hand |
| Xcode | DerivedData, unavailable simulators (each one rechecked, then `xcrun simctl delete <udid>`), DeviceSupport | Archives, at any age: they hold dSYMs and can't be rebuilt. They're listed so you can see their size. |
| Agent worktrees | The branch is merged, or the worktree is clean with no unpushed commits, **and** every ignored file in it matches a rebuildable rule. Removal goes through `git worktree remove` | `--force`; a worktree holding an ignored `.env` or other unique file |

## Invariants

1. **Deletion can't be redirected.** The target is opened with `O_NOFOLLOW`, and its (device, inode)
   must match the scan. Deletion walks by file descriptor (`unlinkat`), never follows a symlink and
   never leaves that subtree. This closes the race where a folder is swapped for a symlink between
   the check and the delete.
2. **Running git never runs the repository's code.** Every git call passes
   `-c core.fsmonitor=false -c core.hooksPath=/dev/null`, uses an argument array (no shell), and
   uses an absolute path to git, so a `git` planted in a project's `node_modules/.bin` is never
   called.
3. **Every check is repeated right before deleting.** A scan result is never trusted on its own.
4. **The MCP server is the low-trust door.** A coding agent can be prompt-injected. Over MCP an
   agent can scan, report, and clean inside its own project under every rule above. It can't
   change the budget, N days or dry-run, trigger Undo (which runs install scripts), or reach any
   path outside its project. No override or force flag exists anywhere in the API.
5. **Archives on external drives are untrusted.** Restoring verifies the SHA-256 manifest, refuses
   symlinks and `..` paths, writes only to the original path, and only if that path is empty.
6. **No network.** Headroom opens no sockets other than local Unix ones. The judge runs it under a
   sandbox profile that denies network access. The exceptions are `rebuild` and `undo`, which
   run your own package managers (npm, cargo, pip), and those need the registry.
7. **Real numbers only.** Reported freed bytes come from allocated blocks, with clone-shared
   extents counted once (private size). If a local Time Machine snapshot is holding deleted
   bytes, Headroom says so instead of claiming the space.

## Out of scope

Malware already running as the same user can edit Headroom's settings and history. It could also
just delete the files itself, so Headroom doesn't try to defend against it.

## Supply chain

- No runtime dependencies beyond the OS and the Rust/Swift standard libraries.
- GitHub Actions are pinned to full commit SHAs with minimal `permissions:`.
- There's no Dependabot (the maintainer's choice), so pins are updated by hand.
- Releases are ad hoc signed, which proves no identity. Each DMG ships with a SHA-256 checksum and
  a GitHub build-provenance attestation, so a download can be checked against the workflow that
  built it.
