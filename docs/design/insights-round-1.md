# Insights round 1: blind design review

Reviewer saw only the screenshots in `docs/design/insights-round-1/` (2x, fixed sample data). No source or design docs were read. Render limits (not findings): no menu vibrancy, inactive windows, Increase Contrast not applied (the `-contrast` renders match the plain ones pixel for pixel), fake paths and times.

## Part A: five-question test

Only `insights-acting-light.png` and `insights-plenty-light.png` were opened for Q1 to Q4, and only `menu-acting-light.png` for Q5. Times are my own estimates of how long it took to read each answer.

| Q | Acting world | Plenty world | Confidence, time |
| --- | --- | --- | --- |
| 1. Am I OK? | No. "Below budget": 57 GB free, 3 GB short of the 60 GB budget. | Yes: 112 GB free, 52 GB more than the 60 GB budget. | High, about 2 s each. The headline answers it in its first word. |
| 2. What is taking my space? | 9 GB can be freed and 21 GB is protected. Largest group: Rust builds, 14.6 GB. Within Rust, 4.8 GB can be freed (`engine/target`, read from the folder list because the bar has no number). | 24 GB can be freed and 21 GB is protected. Largest group: Rust builds, 16.1 GB (Xcode close behind at 15.1 GB). Within Rust, 6.3 GB can be freed (`game/target`). | High for totals, about 4 s. Medium for the per-group freeable part, about 10 s: the bar split has no figure, and I had to work it out from the rows. |
| 3. Why didn't it clean things? | 21 GB kept safe. Largest reason: projects opened recently, 11.9 GB. | Same: 21 GB kept, 11.9 GB opened recently. | High, about 3 s. |
| 4. What did it do this week? | 6.9 GB given back this week (6.9 GB in total), 4 folders deleted, 1 rebuilt, taking 3 min 4 s. | 6.9 GB this week (6.9 GB in total), 5 folders deleted, 1 rebuilt, taking 3 min 4 s. | Medium, about 8 s. "1 · 3 min, 4 secs" took some decoding, and having "this week" equal "in total" made me check whether I had read it right. |
| 5. Menu: what is it doing now? | It is freeing 4.8 GB of Rust build files in "engine" because free space is 3 GB below the budget (57 GB free). Another 4.2 GB can be freed. The last finished action freed 1.2 GB from "shop" at 00:30. A Time Machine snapshot may hold some freed space for up to a day. | n/a | High, about 4 s. "00:30" is ambiguous: it could be a clock time or a duration. |

Every question could be answered from the screens. The weakest is question 2: the split between what can be freed and what is protected, per group, has no number.

## Scores

9 or above means Apple could ship it.

| Surface | Score | Reason |
| --- | --- | --- |
| Insights, acting | 7 | All four questions are answered in sentence headlines, but the pane doesn't show that a delete is running, and the freed/protected bars depend on low-contrast tint alone. |
| Insights, plenty | 7 | Same strengths. The jump to 112 GB on the chart has no note, and the "this week" and "in total" columns are identical. |
| Insights, dry run | 6 | The headline says "Below budget… 8 GB short" without saying it is a dry run that won't act for 4 more days. You only learn that in the fourth card. |
| Insights, first run | 6 | Honest, calm copy, but no card offers Set a Budget… even though every card waits on it. |
| Insights, error | 5 | "Headroom's core gave no reply" is internal jargon, repeated three times. There is no Try Again button, though the menu has one. |
| Menu live line (all worlds) | 8 | One clear status sentence plus a last-action line. The "· 00:30" timestamp is ambiguous, the Freeing segment is a sliver, and the plenty menu says "nothing to do" twice. |
| About section | 7 | Plain, trustworthy rules with fitting symbols. It is long, sits under every state including errors, and its footer text is 3.8:1. |

**Counts:** Critical 1 · High 3 · Medium 11 · Low 7

## Critical

**C1. Can-be-freed vs protected is shown by tint alone, at contrast too low to see**
- **What:** Each category bar has a saturated part (can be freed) and a pale part (protected). The only cue is lightness. Against the card (#E8E8E8, light), the protected tints measure Rust #F1C39B 1.32:1, Node #A0DBAF 1.29:1, Xcode #8BC1F1 1.56:1, AI-tool #F19DAD 1.69:1. In dark (card #353535): Rust #865A33 2.06:1, Xcode #205A86 1.68:1, Worktrees #4C5286 1.67:1. Even the saturated Rust orange is 1.88:1 in light. The legend swatches are black and gray in light and white and gray in dark, and neither matches any bar's colors. The Worktrees bar shows one tone, so you can't tell whether it is all freeable or all protected. No number gives the split either.
- **Why:** `charts.md › Color`: "Avoid relying solely on color to differentiate between different pieces of data…". `color.md › Inclusive color`: "Avoid relying solely on color…". `accessibility.md › Vision`: "Strive to meet color contrast minimum standards". Graphical objects need 3:1.
- **Fix:** Draw protected as a hatched or outlined part, matching the menu's hatch so one mark means one thing everywhere (see H1). Put the freeable figure in text, for example "Rust builds · 4.8 GB of 14.6 GB can be freed". Make the legend swatches show the real marks (solid vs hatched in a neutral color). Raise the fills to at least 3:1 against the card in both appearances.

## High

**H1. Insights and the menu use opposite marks for "can be freed"** (`menu-*.png`, `insights-*.png`)
- **What:** In the menu, "Can be freed" is a hatched gray segment. In Insights it is the saturated colored part of a bar, and the hatch never appears. Someone who learned the menu's language misreads Insights.
- **Why:** `charting-data.md › Designing effective charts`: "Maintain continuity among multiple charts that use the same data… consistent colors, annotations…".
- **Fix:** Use one encoding in both places. The hatch already exists in the menu, so use it for the freeable part in Insights and keep category hue for identity.

**H2. Secondary text is below 4.5:1 throughout Insights in light mode**
- **What:** Bar values ("14.6 GB"), the chart legend, axis labels, row subtitles ("Protected: used 3 days ago…"), "Plus 1 GB in smaller folders" and the About footer sample at #747474 on #E8E8E8, 3.81:1. In the menu, the last-action line is #808080 on white, 3.95:1. Dark mode passes at 4.9:1. Much of this text is small (about 11 pt).
- **Why:** `accessibility.md › Vision`: "Strive to meet color contrast minimum standards." Small text needs 4.5:1.
- **Fix:** Use the primary label color for the figures (sizes and paths), keeping secondary only for supporting words, or drop the grouped-box fill so secondary text sits on the window background. Check that Increase Contrast raises it; the renders could not show this.

**H3. The dry-run headline reads like a failure** (`insights-dry-run-*.png`)
- **What:** "Below budget. 52 GB free, 8 GB short of your 60 GB budget." The chart shows no deletions, and 38 GB "can be freed", so a reader concludes Headroom is broken. Only the fourth card explains that it is a dry run. The menu says it in its first line ("This is a dry run, so nothing is deleted for 4 more days").
- **Why:** `feedback.md › Best practices`: "Show people when a command can't be carried out and help them understand why." `writing.md › Best practices`: "Be clear."
- **Fix:** Lead the Am I OK? card with the mode: "Below budget, dry run. 52 GB free, 8 GB short. Headroom starts deleting in 4 days; it would free 8.9 GB now."

## Medium

- **M1. Insights doesn't reflect live action** (`insights-acting-*` vs `menu-acting-*`). The menu says it is freeing 4.8 GB in engine right now. Insights says "Below budget" and lists `engine/target` 4.8 GB as "Can be freed". *`charting-data.md › Maintain continuity…` and `feedback.md › Consider integrating status feedback into your interface.`* Fix: add "Freeing 4.8 GB in engine now" to the Am I OK? headline and mark that row as "Freeing…".
- **M2. "Rebuilt, and time it took: 1 · 3 min, 4 secs" is cryptic.** A count and a duration share one cell with a middle dot, and "secs" is not a standard abbreviation. *`writing.md › Be clear`.* Fix: split into "Rebuilt 1 folder" and "Rebuild time 3 min 4 s", using the system duration formatter.
- **M3. Identical "This week" and "In total" columns, plus a headline that repeats them** ("6.9 GB this week, and 6.9 GB in total"). *`writing.md › Be clear` and design principle Simplicity.* Fix: when the two are equal, show one column with "All time (since 16 Nov)", and drop the duplicate clause.
- **M4. First-run empty states offer no next step** (`insights-first-run-*`). "Headroom deletes nothing until you set a budget." has no Set a Budget… button, though the menu has one. *`writing.md › Provide clear next steps on any blank screens`.* Fix: put a Set a Budget… button in the Am I OK? card.
- **M5. The error state uses jargon and gives no action** (`insights-error-*`). "Headroom's core gave no reply. Quit and reopen Headroom." is repeated in three cards. There is no Try Again, though the menu offers one, and "What did it do for you?" still states facts it may not be able to know. *`writing.md › Write clear error messages`: "be clear about what someone can do to fix it".* Fix: show one error card ("Headroom stopped responding in the background") with Try Again, and collapse the other cards to "Unavailable until Headroom reconnects".
- **M6. Paths are shown raw and truncated inconsistently.** Insights shows `/Users/me/code/...` in full. History truncates in the middle at random points (`/Users/m…de/engine`, `/Users/…erivedData`), and the Docker row reads just `docker:`. *`lists-and-tables.md › Content`: "Consider ways to preserve readability of text that might otherwise get clipped or truncated."* Fix: abbreviate the home folder to `~`, truncate keeping the project name, and give Docker rows a readable source ("Docker image cache").
- **M7. Freed totals don't reconcile** (`window-history-plenty-*`, `insights-plenty-*`). The History title and Insights say 6.9 GB freed across 5 folders, but the five rows sum to 12.0 GB (1.2 + 4.8 + 3 + 2.1 + 0.9). It seems the Docker 3 GB and the rebuilt 2.1 GB are excluded, with no explanation. This may be sample data, but the screen doesn't explain the rule either way. *`writing.md › Build language patterns`.* Fix: make the totals add up, or say "6.9 GB still freed (2.1 GB rebuilt…)".
- **M8. Chart events go unexplained** (`insights-plenty-*`). Free space jumps from about 55 to 112 GB with no Headroom dot, there are unexplained gaps in the hourly line, and there is no "now" marker. *`charting-data.md › Aid comprehension by adding descriptive text to the chart`.* Fix: annotate large jumps that aren't Headroom's ("You freed 57 GB"), draw gaps with a note ("Mac asleep"), and label the latest point. Also check that the chart has accessibility labels (`charting-data.md › Make every chart in your app accessible`); a screenshot can't show this.
- **M9. Dark-mode protected tints turn muddy** (`insights-*-dark`). The Rust protected part is brown, and Worktrees (#4C5286) and Xcode protected nearly vanish into the card. *`dark-mode.md › Aim for sufficient color contrast in all appearances`.* This is related to C1 but needs its own dark palette.
- **M10. The "· 00:30" timestamp is ambiguous** (menu, all worlds). Zero-padded 24-hour time with no "at" reads like a duration. *`writing.md › Be clear`.* Fix: use a relative time ("2 min ago") or a localized time ("at 12:30 AM") from the system formatter.
- **M11. About Headroom repeats under every state, including errors, and is the longest card.** *`layout.md › Visual hierarchy`: "Use progressive disclosure…".* Fix: collapse it to a one-line "How Headroom decides" disclosure, or move it to Help and first-run onboarding.

## Low

- L1. In the acting menu, the Freeing segment is a sliver about 2 pt wide, so the most important state is the hardest to see. Consider a minimum visible width.
- L2. The info buttons are about 14 pt glyphs. I can't verify that the hit area is at least 20 × 20 pt (`accessibility.md › Mobility`).
- L3. The plenty menu says "nothing to do" twice (status line and last-action line).
- L4. The error menu gives the quit-and-reopen advice twice.
- L5. "Other rules" (3 GB protected) names no rule, so the user can't act on it.
- L6. The red/pink hue for "AI-tool leftovers" reads as danger or error.
- L7. The menu bar extra opens a panel with buttons rather than a menu (`the-menu-bar.md › Menu bar extras`: "Display a menu — not a popover"). The content is arguably complex enough to justify this, so it's noted rather than counted against it.

## What works

- Section titles that are the user's own questions ("Am I OK?", "Why didn't it clean these?"), each answered by a full-sentence headline with exact figures. This is the design's thesis, and it carries the five-question test.
- The budget is drawn as a labeled dashed line, with deletion dots in the same teal as the menu's "Freeing" mark.
- Each folder row says why it was kept, with its rule ("recent: the last 14 days"), and the protected totals reconcile (9.8 + 1.1 + 1 = 11.9 GB).
- The dry-run menu's "First to go when the dry run ends" list adds up to the Insights figure of 8.9 GB.
- The About copy is plain, specific and trust-building ("Git has the last word").
- Native controls (pop-up buttons, sidebar, sortable History table), and both appearances are supported.
