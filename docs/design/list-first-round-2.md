# List-first round 2

The screenshots are in `list-first-round-2/`. The reviewers were fresh subagents that saw only the PNGs.

## Blind /apple-look scores

| Screen | Round 1 | Round 2 |
|---|---|---|
| Overview | 6.5 | 6.5 |
| Overview (full list) | 6 | 7 |
| Confirm sheet | 3.5 | 8 |
| Insights | 5 | 7.5 |
| History | 6.5 | 6 |
| Settings | 6.5 | 8 |
| About | 6 | 7.5 |
| Menu | 7 | 7.5 |

**Findings:**
- **Critical, 1.** C1: in the acting world, the folder being freed is also ticked for deletion, and History says it was deleted yesterday.
- **High, 4:**
  - H1: the folder count includes Docker.
  - H2: the menu's last event contradicts History.
  - H3: fresh dry-run rows appear while Dry Run is off.
  - H4: "Xcode" means 7 GB in the Overview and 14.4 GB in Insights.
- **Medium, 8:**
  - M1: Overview never states free space.
  - M2: Overview's gauge has no Freeing segment.
  - M3: the budget label is on opposite sides in the Overview and the menu.
  - M4: the dry-run action has three names.
  - M5: sheet rows are named differently from the list.
  - M6: the first run says "Measuring" in one place and "Checking" in another, and has no Full Disk Access prompt.
  - M7: History has a ragged size column, wrong "git ignores it" tags, and is missing its header in the dry run.
  - M8: About has no heading or version.
- **Low, 7.**

Most of the cross-screen failures (C1, H2, H3, and History's day counts in M7) come from sample data that contradicts itself. The Overview has no "freeing now" state (C1), and the count is a real bug (H1).

**Checks from the prompt:**

| Check | Result |
|---|---|
| Raw paths in Insights | pass |
| Totals | bytes now reconcile on every screen; counts and events **fail** (H1, H2, H3, C1, H4) |
| Chart events | pass |
| Dark tints | pass |
| Times | pass |
| About | pass |

## Task test (screenshots only): 3 of 3

- **(a)** atlas, 7.4 GB (game is next at 7.1 GB).
- **(b)** api is kept: "used 3 days ago (recent: the last 14 days)".
- **(c)** 8 folders, 22.8 GB, after the confirm sheet; History's Rebuild or Download Again brings them back.

What still confused it: the off-by-one count, the sheet's names, "git ignores it" on Docker and models in History, and no visible Recreate button.
