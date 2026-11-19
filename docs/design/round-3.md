# Design review: Headroom, round 3 (blind)

Reviewed from screenshots only (`round-3/*.png`, 2x, fixed sample data). I read no source code and no earlier round notes. Contrast figures come from sampled pixels (WCAG 2 relative luminance), not from estimates.

## Scores

| Surface | Score | Reason |
| --- | --- | --- |
| Menu: first run | 8 | Two clear setup steps with title-case ellipsis buttons. The orphaned "do." wrap and the missing budget context hold it back. |
| Menu: dry run | 8 | Passes the 5-second test. The "nothing deleted" message appears twice, and the "Can be freed" bar segment is 1.71:1 against the background. |
| Menu: plenty | 8.5 | Calm and Apple-like. "Nothing to do" is said twice. |
| Menu: acting | 7 | No activity spinner in light mode (the dark render has one), the legend wraps onto two lines, and the teal segment is split by the budget marker. |
| Menu: error | 8 | Honest headline ("Free space unknown"), reassurance, and a recovery action. It doesn't say what went wrong. |
| Menu-bar icon | 8 | One family of template symbols, legible in both appearances. The two-arrow "acting" glyph reads as "syncing". |
| History | 7 | A clean native list. The custom header strip should be a window subtitle, and its "3 folders" doesn't match the 4 rows. Four identical Rebuild… buttons, no column headings. |
| Dry Run | 6.5 | The ended-state copy is awkward, the time column is ambiguous, and an empty trailing column wastes about 125 pt. |
| Settings | 8 | A native grouped form with good footers that change with state. "Keep free", "budget" and "amount" all name the same value, and the first-run "60" placeholder is ambiguous. |
| Empty states | 8.5 | System-style unavailable views with a symbol, a title, a sentence and one action, consistent in both panes. |
| First run, end to end | 7.5 | "Set a Budget…" works the same everywhere and the dry run is explained in three places. Vocabulary drift and the placeholder that looks like a value cost points. Full Disk Access is missing from the window. |

**Findings:** Critical 0 · High 1 · Medium 9 · Low 10

**5-second test**
- [menu-dry-run-light.png](round-3/menu-dry-run-light.png): **Pass.** "52 GB free" registers first. "nothing will be deleted yet" and the eye glyph answer "will it act?" right away, and the list says what goes first.
- [menu-acting-light.png](round-3/menu-acting-light.png): **Pass, narrowly.** "57 GB free · freeing space" and the teal segment answer both questions. The status sentence has no leading glyph in light mode (an empty indent), so the one line describing the work in progress looks unfinished.

## Summary

Overall rating: **Good.** This is a deliberately quiet utility that mostly looks like a system extra: system type, semantic grays, grouped Form settings, standard empty states, and teal spent in exactly one place (the "Freeing now" segment). That restraint is the right call, and it is what the app will be remembered by: a storage bar with a budget tick, colored only while Headroom is working. It falls short of 9+ because of a likely missing spinner, a low-contrast chart segment, list panes that lean on custom header strips and per-row buttons, and copy that changes names for the same thing.

## Critical

None.

## Improvements

### High

- **H1: Acting state has no activity indicator in light mode.**
  - **What:** In [menu-acting-light.png](round-3/menu-acting-light.png) and `-light-contrast`, the icon slot in front of "3 GB below your budget. Freeing 4.8 GB…" is pure white: every pixel in the 17×17 pt slot samples #FFFFFF. In [menu-acting-dark.png](round-3/menu-acting-dark.png) a spinner is drawn, but its brightest spoke is #545454 on #1E1E1E, about 2.3:1. Every other status row has a glyph, so the empty indent looks broken. This may be an offscreen-render artifact, because the dark render does draw it. Check it on a real device before dismissing it.
  - **Why:** `loading.md › Showing progress`: "Clearly communicate that content is loading and how long it might take to complete." `feedback.md › Best practices`: "Consider integrating status feedback into your interface."
  - **Fix:** Use a small `ProgressView()` (`.controlSize(.small)`) that follows the system appearance. If it really doesn't render, use a static SF Symbol (for example `arrow.triangle.2.circlepath`) in `.secondary` so the slot is never empty.

### Medium

- **M1: "Can be freed" bar segment is below 3:1 against the menu background.**
  - **What:** In light mode the segment is #C6C6C6 on #FFFFFF, 1.71:1. In dark mode it is #565656 on #1E1E1E, 2.27:1 ([menu-dry-run-light.png](round-3/menu-dry-run-light.png), [menu-plenty-dark.png](round-3/menu-plenty-dark.png)). The same swatch colors are used in the legend. The number is also given in text ("38 GB can be freed"), so this isn't the only place the value appears, which is why it is Medium rather than High.
  - **Why:** `accessibility.md › Vision`: "it's important that there's enough contrast between foreground text and icons and background colors." `color.md › Inclusive color`: "Avoid using colors that make it hard to perceive content."
  - **Fix:** Draw the segment in `.tertiary` fill with a 1 pt `.separator` stroke, or use a hatched pattern. Under Increase Contrast, move it to `.secondary`.

- **M2: Teal "Freeing now" and the gray "Free" segment differ only in hue.**
  - **What:** Luminance contrast between them is 1.11:1 in light (#0F6E5C vs #686868) and 1.02:1 in dark (#3FB39A vs #A3A3A3). For people who can't tell the hues apart, the segments merge. The teal run is only about 22 pt wide, and the budget tick at x≈556 px cuts it into an 11 pt piece and a 4 pt piece ([menu-acting-light.png](round-3/menu-acting-light.png)).
  - **Why:** `accessibility.md › Vision`: "Convey information with more than color alone." `color.md › Inclusive color`.
  - **Fix:** Give "Freeing now" a luminance step as well as the hue. Lighter teal in light mode, or a diagonal stripe, would work, and so would a soft pulse that turns off under Reduce Motion. Draw the budget tick above the segments with a 1 pt background-colored halo so the teal run reads as one piece.

- **M3: The legend layout changes in the acting state.**
  - **What:** With three legend items, "| 60 GB budget" wraps to a second line at the leading edge. In every other state it sits at the trailing edge of the first line ([menu-acting-light.png](round-3/menu-acting-light.png) vs [menu-dry-run-light.png](round-3/menu-dry-run-light.png)). The menu also grows by about 18 pt.
  - **Why:** `layout.md › Visual hierarchy`: "Align elements to make them easier to scan." `charting-data.md › Designing effective charts`: "Prefer consistency across multiple charts."
  - **Fix:** Always put the budget label on its own line, trailing-aligned and under the tick. Alternatively, drop the "Free" swatch, since the headline already says it, so three items fit on one line.

- **M4: The bar's scale and fill go against what people expect from a disk-space bar.**
  - **What:** The filled dark segment means free space, and the bar's full length is free plus reclaimable, not the disk. I checked this: in plenty, 471 px to 100 px matches 112 GB to 24 GB. Finder's and System Settings' storage bars fill what's used. Someone seeing this bar for the first time may read the dark fill as "used". The bar has no end label.
  - **Why:** `charting-data.md › Designing effective charts`: "In general, prefer using common chart types" and "Aid comprehension by adding descriptive text to the chart."
  - **Fix:** Keep the model, but make it easier to read. Either draw "Free" in a lighter tint with "Can be freed" as an outlined extension, so it reads as space you could gain, or add a trailing end label such as "76 GB possible".

- **M5: History and Dry Run use custom header strips instead of the window title and subtitle, and the History count doesn't match.**
  - **What:** "Freed 6.9 GB from 3 folders · 1 rebuilt" sits in a strip with a hairline above the list. The list shows 4 rows, so "3 folders" makes the reader do arithmetic: the rebuilt Atlas row is excluded. The Dry Run strip has two lines and pushes the list down ([window-history-plenty-light.png](round-3/window-history-plenty-light.png), [window-dry-run-plenty-light.png](round-3/window-dry-run-plenty-light.png)).
  - **Why:** `windows.md › Best practices`: "Avoid creating custom window UI." `designing-for-macos.md › Best practices`: present content "in fewer nested levels."
  - **Fix:** Use `.navigationTitle("History")` with `.navigationSubtitle("6.9 GB freed · 4 folders, 1 rebuilt")`, and mark the rebuilt row in the list itself. For the Dry Run explanation, use a `Section` header or footer in the list.

- **M6: List columns have no headings, and the time column means different things in each pane.**
  - **What:** In History, "15 seconds ago" is presumably when the folder was freed. In Dry Run, "15 seconds ago" or "yesterday" could mean last used, picked, or would be deleted. Size, time and action columns have no headings, and the rows can't be sorted.
  - **Why:** `lists-and-tables.md › Content`: "Use descriptive column headings in a multicolumn table." `lists-and-tables.md › Desktop (macOS)`: "let people click a column heading to sort."
  - **Fix:** Use a SwiftUI `Table` with Name, Kind, Size and Freed columns (Last Used in Dry Run), sortable by Size and date. Or keep the list and prefix the times: "Freed 15 s ago", "Last used yesterday".

- **M7: The Dry Run "ended" copy is awkward.**
  - **What:** "The dry run picked these 8.9 GB. It has ended." The subject is vague, the sentence order is backwards, and "picked these 8.9 GB" treats an amount as a set of items.
  - **Why:** `writing.md › Getting started`: "Be clear… If you can use fewer words, do so." `writing.md › Best practices`: "put the most important information first."
  - **Fix:** Use "Dry run ended" with the subtitle "These 3 folders (8.9 GB) would have been freed. Headroom now frees space for real; deletions appear in History."

- **M8: One setting has three names.**
  - **What:** The menu and the empty states say "Set a Budget…" and "60 GB budget". The settings label is "Keep free". The settings footer says "until you choose an amount".
  - **Why:** `writing.md › Best practices`: "Build language patterns. Consistency builds familiarity." `writing.md › Getting started`: "Create a list of common terms."
  - **Fix:** Pick one name. "Free-Space Budget" as the settings label, with a "Keep at least" field, fits the menu's "budget" tick, and the footer becomes "until you set a budget."

- **M9: The first-run "Keep free" placeholder looks like a value.**
  - **What:** [settings-first-run-light.png](round-3/settings-first-run-light.png) shows "60" in placeholder gray (#888888 on #F2F2F2, 3.17:1, the system placeholder color) beside "GB". Next to the footer "won't delete anything until you choose an amount", it reads as either a disabled field or an already-chosen default. People can't tell whether they need to type anything.
  - **Why:** `writing.md › Best practices`: "Show hints in text fields." `onboarding.md › Additional requests`: "Provide reasonable default settings."
  - **Fix:** Make the step explicit. Show an empty field with the placeholder "e.g. 60" and a prominent "Use 60 GB" button beside it, or have "Set a Budget…" open a small sheet with a stepper preset to a suggestion and a Set button.

### Low

- **L1:** The dry-run menu says "nothing will be deleted yet" and then "This is a dry run, so nothing is deleted for 4 more days." Keep the second and change the subtitle to "38 GB can be freed · dry run". (`writing.md › Getting started`, "Be clear")
- **L2:** The plenty menu says "nothing to do" in the subtitle and "Nothing to do." in the status. Drop one. (`writing.md › Getting started`)
- **L3:** First-run status wraps to leave "do." alone on the last line ([menu-first-run-light.png](round-3/menu-first-run-light.png)). Shorten it to "Choose how much space to keep free. Nothing is deleted until you do." or turn on balanced line breaking. (Judgment, Craft)
- **L4:** Paths are shown as "/Users/me/code/shop". Abbreviate the home folder to "~/code/shop" the way Finder and Terminal do, which also avoids the "/Users/m…eveloper" middle truncation. (`lists-and-tables.md › Content`)
- **L5:** History repeats four identical bordered "Rebuild…" buttons, which adds visual noise to a quiet list. Show the button on hover or selection, and add Rebuild… to the context menu and the menu bar. (`designing-for-macos.md › Best practices`, "Use the menu bar to give people easy access to all the commands")
- **L6:** Dry Run keeps the empty action column, about 125 pt between the sizes and the trailing edge ([window-dry-run-plenty-light.png](round-3/window-dry-run-plenty-light.png)). Let the size column run to the trailing margin. (`layout.md › Visual hierarchy`)
- **L7:** The sidebar holds only History and Dry Run. A two-item sidebar is a lot of furniture for two views. It is defensible if more panes are planned. Otherwise a segmented control in the toolbar would do. (`sidebars.md › Best practices`, judgment)
- **L8:** Full Disk Access shows "Review…" when "No folders blocked". If nothing needs action, use "Open System Settings" or drop the button. (`writing.md › Best practices`, "Keep settings labels clear and simple")
- **L9:** The acting menu-bar glyph (two circular arrows) is the macOS "sync" metaphor. Something like a circled down-arrow or a gauge moving would say "freeing space" more directly. (`the-menu-bar.md › Menu bar extras`, judgment)
- **L10:** Secondary text samples at #808080 on white, 3.95:1, below 4.5:1, in the legend, row subtitles, "First to go…" and the empty-state body. In dark mode it is #9A9A9A on #1E1E1E, 5.92:1. This is the system `secondaryLabelColor`, so Apple ships it too; it is reported here but not weighted. Don't make it any lighter, and keep important numbers in the primary label color, as you already do. (`color.md › Desktop (macOS)`, `accessibility.md › Vision`)

## Craft notes

- **Point of view (strong):** the budget tick on the bar is the signature element, and it is the right one. It sits beside the free figure and the 60 GB tick, so you can see whether Headroom will act without reading. Keep it, and make it read in every state (M2, M3).
- **Restraint (strong):** teal appears only in the acting bar segment. The dry run uses the eye glyph and plenty uses a gray check. This is what `branding.md` asks for, and the spinner being gray rather than teal is also correct.
- **One accessory to remove:** the "Free" legend swatch. The headline already says "52 GB free", and removing it fixes the acting-state wrap (M3).

## What works

- The headline goes number first ("52 GB free", "Free space unknown") at a clear title weight in every state, with no hero gradient or big-number template.
- Each status row has an SF Symbol glyph and a plain sentence that says what Headroom will do and when ("…nothing is deleted for 4 more days").
- The menu items follow the system: title case, standard ⌘O, ⌘, and ⌘Q, an ellipsis on "Settings…", and "Pause Headroom" dimmed when there is nothing to pause (`menus.md › Labels`).
- The error state: "Headroom couldn't read your disk. Nothing will be deleted until it can." reassures, has no "Oops", and offers a recovery button (`writing.md › Best practices`).
- Undo carries one name through the flow: "Rebuild…" becomes "✓ Rebuilt" (`writing.md › Best practices`, "Build language patterns").
- Empty states are system-style and actionable ("Nothing deleted yet" with "Set a Budget…") (`writing.md › Best practices`, "Provide clear next steps on any blank screens").
- Settings is a grouped `Form` with footers that change with state ("Off: Headroom frees space for real…") and has no custom appearance toggle (`settings.md`, `dark-mode.md › Best practices`).
- The menu-bar icons are monochrome template symbols. The glyph is 14 pt in the 24 pt bar, they invert correctly in dark mode, and the shape (eye, check, exclamation, gauge) carries the state, not color.
- Dark mode uses semantic colors throughout. Contrast doesn't collapse, and the teal is lifted to #3FB39A in dark.

## Platform notes

- **Popover instead of a menu:** `the-menu-bar.md › Menu bar extras` says to "Display a menu — not a popover" unless the content is too complex. The bar chart and the list justify a window-style extra. The bottom rows are styled as menu items, though, so they must behave like them: a highlight on hover, arrow-key navigation, Return to activate, and the menu closing after an item is chosen. None of this can be verified from stills.
- **Undo:** `undo-and-redo.md › Desktop (macOS)` expects undo in the Edit menu with ⌘Z. Since Rebuild is the undo, consider an "Undo Delete of 'shop'" or "Rebuild 'shop'" item in the main window's Edit menu. This can't be verified from stills.
- **Increase Contrast:** the custom bar fills (M1, M2) need their own high-contrast variants. System colors adapt on their own; hand-picked grays don't.

## Render limits (not scored)

- There is no window title bar, toolbar or traffic lights, so window titles, the Settings title ("Headroom Settings") and toolbar placement can't be judged.
- There is no menu material or vibrancy; the popovers render on flat white or #1E1E1E.
- Windows draw as inactive: gray sidebar selection, gray toggles and buttons, and near-black sidebar icons in dark mode. The gray "on" toggle in first-run Settings is assumed to be accent-colored in a key window.
- Increase Contrast isn't applied, so the `-contrast` images match their base looks and were not scored separately.
- Relative timestamps drift between renders ("15 seconds ago" vs "17/18 seconds ago").
- The missing light-mode spinner (H1) may be a render artifact. It is scored as High only if it reproduces on device.
