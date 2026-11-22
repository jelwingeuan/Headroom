# Insights: what we already have, and the least we must add

This audit comes before any design. For each question the Insights view must answer, it lists the
data that answers it today, what's missing, and the smallest change that fills the gap.
Anything that answers no question is cut.

Sources checked (on `main`, after the merge of #7):
- `core/API.md`: the UI operations `status`, `history`, `undo`, `settings_get`, `settings_set`,
  `tick` and `digest`, and the autopilot events.
- `core/src/rebuild.rs`: undo already records `undone_at` and `rebuild_secs`.
- The redesigned UI in #8: the menu, History, Dry Run and Settings.

## What the core already exposes

| Source | Fields that matter here |
|---|---|
| `status` (no scan, cheap) | `free_bytes`, `budget_bytes`, `dry_run`, `dry_run_days_left`, `next`, `snapshot_holding`, `permission_errors`, `last_tick`, `reclaimable_bytes`, `protected_bytes`, `protected_by_reason` {recent, in_use, history, other}, `totals_at` |
| `history` (last 500 actions) | `at`, `path`, `kind`, `bytes` (private bytes measured right before deleting), `origin`, `reason`, `rebuild`, `build_secs`, `undone`, `undone_at`, `rebuild_secs`, `dry_run`, `frees_host`, `data_loss` |
| `tick` events | `tick`, `reclaim`, `would_reclaim`, `shrink`, `skip` (with a reason, e.g. a project in use), `shortfall`, `error` |
| `digest` | `freed_bytes`, `actions` and `would_bytes` for the week |
| Disk size | read by the UI itself (`volumeTotalCapacity`), as the menu chart does today |

## The questions

### 1. "Am I OK?": free space over the last 30 days, the budget line, and a dot each time Headroom acted
- **We have:** today's free space and budget (`status`), and when it acted and how much (`history`: `at` and `bytes` of rows that aren't dry-run).
- **Missing:** free space over time. Nothing stores past values.
- **Smallest change (core):** a free-space sample log, `HEADROOM_HOME/samples.jsonl`.
  - Each row is `{"at", "free", "budget"}`, about 50 bytes.
  - A tick appends a row only if the last one is an hour old or more.
  - Rows older than 400 days are dropped when the file is rewritten.
  - That's at most 24 × 400 ≈ 9,600 rows, about 0.5 MB, under the 1 MB/year limit.
  - New UI op `samples` returns the rows.
- **Cut:** reclaimed bytes per sample. The action dots already come from `history`, so storing them twice adds nothing.

### 2. "What is taking my space?": what can be freed and what's protected, by kind, in one sorted bar chart; click a bar for its folders
- **We have:** totals only (`reclaimable_bytes`, `protected_bytes`), not split by kind and with no folder list.
- **Missing:** the per-kind split and the folders behind each bar.
- **Smallest change (core):** the tick that already saves the totals also saves the list it measured to `HEADROOM_HOME/last-scan.json`.
  - Each entry is `{path, kind, bytes, free: bool, reason}`.
  - Capped at the 2,000 largest entries, plus a line of totals per kind, so the sums stay exact.
  - New UI op `breakdown` returns it, with `totals_at`.
  - No new scan: the tick already sizes every one of these.
- **UI:** group the kinds into the 7 the question names: Node packages, Rust builds, Xcode, Docker, AI models, worktrees, AI-tool leftovers. Everything else goes under "Other", using the existing `what` names.

### 3. "Why didn't it clean X?": protected bytes by reason, and each item gives its reason in one sentence
- **We have:** `protected_by_reason`, but in only 4 buckets.
- **Missing:** each item's reason sentence. The same `last-scan.json` from question 2 carries it (`reason` is the existing `why` text, for example "opened 3 days ago").
- **Smallest change:** none beyond question 2. The 4 buckets are kept: recent, in use, conversation history, and other (git tracks it, no manifest, newest simulator…). Splitting "other" further answers no extra question, so it's cut.

### 4. "What did it do for me?": space given back this week and in total, the number of actions, and the rebuild time it cost
- **We have all of it in `history`:**
  - Space given back: the sum of `bytes` for rows that aren't dry-run, weren't undone, and have `frees_host` true. This week, or all time.
  - Actions: the count of those rows.
  - Rebuild time it cost: the sum of `rebuild_secs` on undone rows.
- **Smallest change:** none in the core. `digest` gives the same weekly numbers, so the UI uses `history` for both periods, and a test checks the result against `digest`.
- **Limit to note:** `history` keeps the last 500 actions, so "total" means "the last 500 actions". The view says so when the cap is reached. Raising the cap answers no question yet, so it's cut.

### 5. "What is it doing right now?": a live line in the menu
- **We have:**
  - the UI knows when it's running a `tick`, because the call is in flight
  - the `tick` events say what happened (`reclaim` X, `skip` X because it's in use, `would_reclaim`, `shortfall`)
- **Smallest change (UI only):**
  - "Checking your disk…" while the tick runs.
  - Afterwards, one line from its events:
    - "Freed 4.8 GB from engine"
    - "Waiting: shop is in use"
    - "Nothing to do"
  - The line keeps the time of the last check.
- **Cut:** a per-folder "cleaning X now" while the tick runs. It would need the core to report progress partway through a call (streaming). A delete takes well under a second, so the after-the-fact line answers the question.

### 6. "What is this app?": About and safety
- **We have:** the rules, written in `README.md` and `docs/THREAT-MODEL.md`.
- **Smallest change (UI only):** a static About section with 3 sentences on what Headroom does, and the safety rules in plain words:
  - the proof file
  - git has the last word
  - recently opened projects are left alone
  - nothing that's in use is touched
  - undo means rebuild
  - the 7-day dry run

  It also lists what Headroom never touches and says there's no network and no telemetry. Not a single new figure.

## Every number is traceable
- Each figure's ⓘ shows how it was measured and when:
  - "private allocated bytes, clones counted once" from the existing rule
  - "measured at <totals_at>"
  - "deleted <at>"
- **Test that the UI equals the CLI (core, Rust):** after a tick on a temporary home folder:
  - the `breakdown` per-kind totals add up to the `status` totals
  - every entry `scan --json` marks reclaimable is in `breakdown` with the same bytes
  - Kept entries are compared only by path, because `scan` doesn't size kept folders. That's a documented speed choice; `breakdown` does have their sizes.
- **Test (app):** the Insights model's sums for questions 1 to 4 equal the core's numbers on fixed sample data.

## What changes, in order
1. **Core PR, `core/` only:**
   - `samples.jsonl`, written at most hourly and kept 400 days
   - `last-scan.json`, the measured list capped at 2,000 entries with exact per-kind totals
   - the UI ops `samples` and `breakdown`
   - the UI-equals-CLI test
   - API.md

   About 150 lines with tests. No new dependencies.
2. **UI PR, `app/` only, on top of #8:** an Insights pane with 4 charts, one each for questions 1 to 4 (Swift Charts, macOS 13). The live line goes in the menu for question 5, and the About section in Insights for question 6.
3. **Judge:** no change. The new files sit in `HEADROOM_HOME`, which the judge already isolates.

## Cut, because it answers no question
- per-sample reclaimed bytes (already in `history`)
- more than 4 protected-reason buckets
- live per-folder progress during a tick
- a history cap above 500
- export or share
- a database: two JSON files and the existing JSON Lines file are enough
