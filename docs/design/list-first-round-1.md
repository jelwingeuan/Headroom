# List-first round 1

The screenshots are in `list-first-round-1/`. The reviewers were fresh subagents that saw only the PNGs, never the code.

## Blind /apple-look scores (bar: 9 on every screen, 0 major findings)

| Screen | Score |
|---|---|
| Overview | 6.5 |
| Overview (full list) | 6 |
| Confirm sheet | 3.5 |
| Insights | 5 |
| History | 6.5 |
| Settings | 6.5 |
| About | 6 |
| Menu | 7 |

**Findings:**
- **Critical, 3:**
  - C1: dark-mode sheets render blank.
  - C2: Overview, Insights and the menu disagree on totals.
  - C3: the sheet's list is cut off with no sign.
- **High, 8:**
  - H1: chart labels collide.
  - H2: chart dates are in the Mac's locale.
  - H3: the budget label is offset from its marker.
  - H4: the sheet names folders differently.
  - H5: Settings and About are in the sidebar.
  - H6: one color means several things.
  - H7: the Overview doesn't say Headroom is acting.
  - H8: History puts its stats in the title.
- **Medium, 11:**
  - M1: the headline wraps.
  - M2: too many chips.
  - M3: bar labels are inconsistent.
  - M4: dry-run History rows look like deletions.
  - M5: time-since-use is worded three ways.
  - M6: "Would Delete" and "Record" don't match.
  - M7: an empty sheet is reachable, and "Zero KB".
  - M8: measuring shows no progress.
  - M9: the menu's error string.
  - M10: the hatch is weak in dark.
  - M11: the sidebar logo repeats.
- **Low, 8.**

**Checks from the prompt (the old M6–M11):**

| Check | Result |
|---|---|
| Raw paths in Insights | pass |
| Totals that don't add up | **fail** (C2, C3) |
| Chart events unexplained | **fail** (H1) |
| Muddy dark tints | pass |
| "00:30" times | pass |
| About repeated under other content | pass |

## Task test (screenshots only): 2.5 of 3

- **(a) Biggest project: half right.** It answered "game, 7.1 GB", but then noted that Atlas's Xcode data (7.4 GB) sits in another section and might count.
- **(b) Why a protected folder is kept: right.** "api: used 3 days ago (recent: the last 14 days)".
- **(c) What one click deletes and how to get it back: right.** 8 folders, 22.8 GB, after a confirming sheet; History's Rebuild brings them back.

Confusions it reported:
- how a model or Xcode device data comes back
- "Undo" vs "Rebuild"
- fixture contradictions: shop is both deleted and protected, and every History row says "94 days"
- the awkward sheet sentence

## Root causes, and what round 2 changes

- **C1** was the renderer. The sheet was shot without a window background, so dark text sat on white. The real sheet gets the window background.
- **C2** came from the fixtures. They added "unlisted" bytes to the totals that no list row carried. The real core lists the 2,000 largest folders, so its totals equal the list except on very large disks. The fixtures now list everything.
- **H5 and M11 are kept, deliberately.** The prompt asks for Settings and About in the sidebar, and for the icon and tagline at its top. ⌘, still opens the standard Settings window.
- Everything else is fixed in round 2.

## Other outside checks

- **Verifier on the core PR (`feat/core-reclaim-op`): PASS.** Its one note, Docker dry-run picks counted in the digest's `would_bytes`, is fixed, with a test.
- **Not run yet:** Accessibility Inspector, Instruments hitches and the Judge workflow. They come in the final round.
