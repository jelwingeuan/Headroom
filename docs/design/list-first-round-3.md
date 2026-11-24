# List-first round 3

The screenshots are in `list-first-round-3/`. The reviewers were fresh subagents that saw only the PNGs.

## Blind /apple-look scores

| Screen | R1 | R2 | R3 |
|---|---|---|---|
| Overview | 6.5 | 6.5 | 7.5 |
| Overview (full list) | 6 | 7 | 7.5 |
| Confirm sheet | 3.5 | 8 | 5.5 |
| Insights | 5 | 7.5 | 7 |
| History | 6.5 | 6 | 7.5 |
| Settings | 6.5 | 8 | 8.5 |
| About | 6 | 7.5 | 8 |
| Menu | 7 | 7.5 | 8 |

**Findings:**
- **Critical, 1.** C1: the acting sheet lists the folder being freed now. This is a render-harness bug: the harness built the sheet from the defaults without leaving that folder out. The app's own selection does leave it out.
- **High, 3:**
  - H1: the chart's 54.62 GB jump is labeled "You freed" next to a Headroom dot.
  - H2: the menu and window times disagree.
  - H3: the dry-run Overview has no budget status line.
- **Medium, 6:**
  - M1: "can go" includes what is being freed and what is unticked.
  - M2: "112 GB free of the 60 GB you keep".
  - M3: the first run has no single next step.
  - M4: capitalization is mixed.
  - M5: pane headers differ.
  - M6: History doesn't show the delete in progress.
- **Low, 11.**

**Checks from the prompt:** totals and chart events **fail** (C1 and H1). Raw paths, dark tints, times and About pass.

## Task test (screenshots only): 3 of 3

- **(a)** atlas, 7.4 GB.
- **(b)** api is kept: used 3 days ago, within the last 14 days.
- **(c)** 8 folders, 22.8 GB, after the sheet; History's Rebuild brings them back.

It was confused by "112 GB free of the 60 GB you keep".
