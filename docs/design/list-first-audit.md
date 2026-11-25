# List-first window: what to delete before adding

A /ponytail audit of `app/Sources/Headroom`, done before any design. Base: `feat/insights-ui` (#12).

**Swift before:** 2,175 lines.

| File | Lines |
|---|---|
| `Core.swift` | 184 |
| `HeadroomApp.swift` | 506 |
| `Insights.swift` | 642 |
| `MainView.swift` | 304 |
| `Model.swift` | 221 |
| `Screens.swift` | 291 |
| `main.swift` | 27 |

## Dead or replaced by the new layout

| What | Where | Why it goes |
|---|---|---|
| `WouldHaveDoneView` and the Dry Run pane | `MainView.swift` (~40 lines), `Pane.wouldHaveDone` | During the dry run the Overview's button reads "Would delete" and only records. Those records show in History as "Would delete" rows, so the separate pane has no job left. |
| `FolderPicker` and `FolderList` | `Insights.swift` (~45 lines) | They are a second, shorter folder list. The Overview is the folder list. The Insights bars stay, and each answer points to the Overview. This also closes M6: Insights stops printing raw paths. |
| `About` repeated as the last Insights card | `Insights.swift` | It moves to its own About pane, shown once (M11). |
| `WouldDoList` in the menu | `HeadroomApp.swift` (~25 lines) | It shows the top three dry-run picks and "Show All". The menu's verdict already says it's a dry run, and Open Headroom lands on the Overview, which shows all of them with the same sizes. |
| `FolderCell`, `SizeCell`, `WhenCell`, `StatusCell` | `MainView.swift` | These are the History cells. History moves to the one shared row view, `ItemRow`. |

## Duplicated

- **The kind names live in three places:** `Candidate.what` (plain words), `Insights.group` (chart groups) and `InsightsContent.color` (group colors). They become one table, `Kind`. It gives the label, the section, the color and the default tick for every core kind, and the Overview, Insights and History all read it.
- **Three ways to draw a folder row:** History's `FolderCell`, Insights' `FolderList` row and the menu's `WouldDoList` row. One `ItemRow` replaces them. It shows the name in mono, `~/` path, tags, the size in right-aligned mono digits and Reveal in Finder, with an optional checkbox and an optional trailing button.
- **`EmptyState` and `Unavailable`** both say "nothing here yet" in two styles. `Unavailable` stays inline inside a card, and `EmptyState` stays for whole panes.

## State that lives in two places

- **The dry run:** `status.dryRun` (the core's effective state) and `settings.dryRun` (the form's choice). The Overview's button reads `status.dryRun` only, because the form value can be ahead of the core by one save.
- **The selected pane:** `model.pane` is optional and defaults to `.insights`, and `MainView` falls back with `?? .insights`. Both become `.overview`, with no optional.
- **`Candidate.freesHost`** falls back to guessing from the kind, because the `history` op didn't return it. #13 fixes that in the core. The guess goes once #13 is merged into this branch's base.

## What the new rows need from the core

These are checked against `status`, `history`, `breakdown` and `scan --json`.

| Row field | Source today | Gap |
|---|---|---|
| Name, path, kind, size, free or protected, reason | `breakdown` entries | none |
| Tags: "git ignores it", "last used 41 days ago", "rebuild: cargo build" | `breakdown`'s `reason` is these clauses, joined by `; ` | none: the row splits on `; ` |
| The project, for grouping into cards | Not returned. Guessing from the path is wrong for DerivedData and worktrees. | **core:** add `project` to each entry (the scan already knows it: `Cand.project`) |
| Docker | Left out of `breakdown`, on purpose, because it frees no space on this disk | **core:** list Docker entries with `frees_host: false`, kept out of `kinds`, so every total stays as it is |
| Rebuild time ("~2 min") | `history` has `rebuild_secs` for folders rebuilt before | none: shown only when a past rebuild of that folder was timed. Otherwise the sheet says "usually a few minutes" |
| Delete the ticked folders | No UI op. The MCP and CLI `clean` deletes everything in a project. | **core:** a `reclaim` op with `{"paths": […]}`. It rescans and rechecks each path under every rule. It refuses any path the scan doesn't mark reclaimable, records each delete in History (origin `manual`), and records it as a dry run while the dry run is on. |
| Rescan | `tick` scans once a day at most | **core:** a `rescan` op that runs the tick's scan now and rewrites `last-scan.json` |

That's one core PR, `core/` only, before the UI PR uses it.

## Cut, because it answers no question or would break a rule

- **Choose Folder…** Headroom measures the whole home folder, and `status`'s totals must equal the list's. A list of one chosen folder would break that, and a folder on another disk frees nothing on this one. If people ask for it, it comes back as a filter on the same list.
- **A "Download caches" section.** The core has no rule for npm, pip or Homebrew caches. Sections with no rows are hidden, and adding cache rules is a core feature of its own, not part of this window.
- **Sorting controls on the list.** Cards are sorted by size, and rows inside a card by size. That's the one order the summary sentence talks about.

## What the window becomes

The sidebar has five panes: Overview (new, and home), Insights, History, Settings and About. Dry Run is gone.

The goal is fewer lines of Swift than before, apart from the new list itself (`Overview.swift`). Both numbers are reported in each round.
