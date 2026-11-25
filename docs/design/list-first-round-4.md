# List-first round 4

The screenshots are in `list-first-round-4/`. The reviewers were fresh subagents that saw only the PNGs.

## Blind /apple-look scores

| Screen | R1 | R2 | R3 | R4 |
|---|---|---|---|---|
| Overview | 6.5 | 6.5 | 7.5 | 8 |
| Overview (full list) | 6 | 7 | 7.5 | 8 |
| Confirm sheet | 3.5 | 8 | 5.5 | 8 |
| Insights | 5 | 7.5 | 7 | 7.5 |
| History | 6.5 | 6 | 7.5 | 7.5 |
| Settings | 6.5 | 8 | 8.5 | 8.5 |
| About | 6 | 7.5 | 8 | 8 |
| Menu | 7 | 7.5 | 8 | 8 |

**Findings:**
- **Critical: 0.**
- **High, 3:**
  - H1: in the acting world, Insights counts the folder being freed as "can be freed".
  - H2: menu and History times differ for the same event. In the renders this is a clock bug: the menu's time is taken when the render runs, minutes after the fixtures' "now".
  - H3: "Rebuild brings it back" is promised for things that come back another way.
- **Medium, 8:**
  - M1: the headline and button show different numbers.
  - M2: pane margins differ.
  - M3: the "Now" dot reuses teal.
  - M4: a chart label collides with the sleep marker.
  - M5: ghost text in History.
  - M6: the History total has no time scope.
  - M7: a warning triangle is used for a neutral note.
  - M8: two styles for one empty state.
- **Low, 10.**

**Checks from the prompt:** totals across screens **fail** (H1). Raw paths, chart events, dark tints, times and About pass.

## Task test (screenshots only): 3 of 3

- **(a)** atlas, 7.4 GB.
- **(b)** api is kept: used 3 days ago, within the last 14 days.
- **(c)** 8 folders, 22.8 GB, after the sheet; History brings them back.

It was still confused by how device symbols and an old Claude Code version come back.
