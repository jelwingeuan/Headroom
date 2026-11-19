# Round 5: blind design review

Blind review from screenshots only. I read no source code and no earlier round reports. Guidelines come from the `apple-look` skill's HIG pages: accessibility, layout, typography, color, designing-for-macos, the-menu-bar, menus, windows, sidebars, lists-and-tables, settings, feedback, loading, onboarding, writing, dark-mode, charting-data, undo-and-redo.

## Scores

| Surface | Score | Reason |
| --- | --- | --- |
| Menu: first run | 7.5 | Two clear setup tasks in the right order, but "18 GB free" has no bar, so the urgency is missing |
| Menu: dry run | 8 | Free space and "won't act for 4 days" both read in seconds; the budget key is far from its marker and the free track is faint |
| Menu: plenty | 8.5 | Calm and complete: "52 GB above your budget, so there's nothing to do" |
| Menu: acting | 7.5 | Teal icon and sentence say "acting", but the Freeing segment is about 5 pt wide and only 1.7:1 against the gray Used fill |
| Menu: error | 8 | Honest, safe and has a fix (Try Again); the copy runs one sentence long |
| Menu bar icon | 7 | Error and acting change shape; dry run and plenty differ only by needle angle at about 16 pt |
| History | 6.5 | "Rebuild…" in a Status column reads as truncated or in-progress text, not as the undo action |
| Dry Run | 7 | Clean table, but its row style differs from History and the subtitle is a full two-clause sentence |
| Settings | 8.5 | Textbook grouped form with live footers; small naming mismatch on Full Disk Access |
| Empty states | 8.5 | Symbol, title, one-line why and the next action, in both panes |
| First run end to end | 7.5 | Consistent "deletes nothing until you set a budget" story; the Full Disk Access action has two names and the menu hides disk context |

**Findings:** Critical 0 · High 1 · Medium 9 · Low 8

**5-second test**
- [menu-dry-run-light.png](round-5/menu-dry-run-light.png): **Pass.** "52 GB free" is the first and boldest thing. "Will it act?" is answered in the fourth block: "This is a dry run, so nothing is deleted for 4 more days." It's in body text, so you have to read it rather than glance, and the menu bar icon doesn't say "dry run" at all.
- [menu-acting-light.png](round-5/menu-acting-light.png): **Pass, narrowly.** "57 GB free" reads first. The teal icon and "Freeing 4.8 GB of Rust build files in engine" carry "it's acting now". The second line, "9 GB can be freed", competes with it and reads like an offer rather than a state. In the bar, the teal segment is too small and too close in luminance to the gray to carry the message.

## Design review: Headroom (round 5)

### Summary
A quiet, well-made menu bar utility. It uses system type, semantic grays, standard grouped Settings and one restrained teal accent that appears only while Headroom is freeing space. Overall rating: **Good** (no Critical, one High). The thesis is "one glance tells you free space and whether Headroom will act". It will be remembered by the disk bar with a budget marker and a hatched "can be freed" band: a clear, product-specific chart that fits the job. The weak spots are in the History window, where the undo action (Rebuild) doesn't look like an action, and in small legibility details in the bar and the menu bar icon.

### Critical
None. Secondary text measures #808080 on #FFFFFF = **3.95:1** in light (#9A9A9A on #1E1E1E = 5.92:1 in dark). That is the system `secondaryLabelColor`, which Apple ships at this value, so it's reported under Low, not scored as a failure (`accessibility.md › Vision`: "Strive to meet color contrast minimum standards").

### Improvements

**High**

- **H1. "Rebuild…" in History reads as a status or truncated text, not as the undo action.**
  [window-history-plenty-light.png](round-5/window-history-plenty-light.png), [dark](round-5/window-history-plenty-dark.png)
  - **What**: The Status column mixes a state ("✓ Rebuilt") with an action ("Rebuild…"). The action is secondary gray (#7F7F7F, 4.0:1) with no bezel, so it looks like the label "Rebuilding…" cut off, which suggests a rebuild is already running. This is the app's only undo, and it's the least visible thing in the row.
  - **Why**: `lists-and-tables.md › Use descriptive column headings`: a column headed "Status" should hold states. `undo-and-redo.md › Help people predict the results of undo and redo as much as possible`. `color.md › Avoid using the same color to mean different things`: the same gray is used for dates and paths, which aren't interactive.
  - **Fix**: Show a state in Status ("Freed" / "✓ Rebuilt" / "Rebuilding" with a spinner). Put the action in an inline borderless or small bordered `Button("Rebuild…")` in its own trailing column, or on row hover, plus a context menu item and a menu bar command (Edit or a "Folder" menu, for example "Rebuild Folder… ⌘R").

**Medium**

- **M1. In the menu bar icon, the dry-run (below budget) and plenty states differ only by needle angle.**
  [icon-dry-run-light.png](round-5/icon-dry-run-light.png), [icon-plenty-light.png](round-5/icon-plenty-light.png)
  - **What**: At menu bar size, a gauge needle at 10 o'clock and one at 2 o'clock are the only difference between "you're under budget and Headroom will act when the dry run ends" and "nothing to do". First run uses a third gauge variant (dots only on top), which is just as subtle. Error (triangle) and acting (minus badge) change shape and work well.
  - **Why**: `accessibility.md › Convey information with more than color alone`: the same principle applies to a tiny angle change. `the-menu-bar.md › Menu bar extras › Consider using a symbol`.
  - **Fix**: Give below-budget states a badge, as acting already has (for example a small dot or clock badge for dry run). Or keep one gauge and reserve shape changes for "needs attention", "acting" and "error".

- **M2. The free part of the disk bar barely shows against the popover.**
  [menu-plenty-light.png](round-5/menu-plenty-light.png)
  - **What**: The free track #E6E6E6 on #FFFFFF = **1.25:1** in light; #343434 on #1E1E1E = 1.34:1 in dark. In plenty state, most of the bar is "free", so the bar's length and the budget marker's position against the full disk are hard to judge.
  - **Why**: `color.md › Inclusive color`: "insufficient contrast can cause icons and text to blend with the background". `charting-data.md › Make every chart in your app accessible`.
  - **Fix**: Give the track a hairline stroke (separator color) or use `quaternaryLabelColor`/`systemFill` at a stronger tone, at least 3:1 at the bar ends.

- **M3. The Freeing segment depends on hue and is about 5 pt wide.**
  [menu-acting-light.png](round-5/menu-acting-light.png), [dark](round-5/menu-acting-dark.png)
  - **What**: Teal #0F6E5C against Used gray #888888 = **1.74:1** (dark: #3FB39A vs #878787 = 1.39:1). It's a sliver next to the budget marker and a hatch, so for a color-blind viewer it merges with Used.
  - **Why**: `accessibility.md › Convey information with more than color alone`; `color.md › Avoid relying solely on color`.
  - **Fix**: Give the Freeing segment a distinct texture or a light inner stroke, or set a minimum drawn width (for example 8 pt). The sentence already says "Freeing 4.8 GB", so that's the backup, but the chart should stand on its own.

- **M4. The budget key sits far from the budget marker.**
  [menu-dry-run-light.png](round-5/menu-dry-run-light.png)
  - **What**: "▌60 GB budget" is pinned at the leading edge, under the start of the bar. The marker it explains is about 75% of the way along. You have to match the glyph to the line.
  - **Why**: `charting-data.md › Aid comprehension by adding descriptive text to the chart`: annotations belong near what they annotate. `layout.md › Group related items`.
  - **Fix**: Put "60 GB budget" directly under the marker, flipping its alignment near the ends, and keep "245 GB disk" at the trailing end.

- **M5. The first-run menu drops the bar, so "18 GB free" has no context.**
  [menu-first-run-light.png](round-5/menu-first-run-light.png)
  - **What**: The one state where the disk is tight (18 GB) shows no bar, no disk size and no "can be freed" figure. Every other state with a reading shows the bar.
  - **Why**: `onboarding.md › Teach through interactivity`: showing the bar now teaches it before a budget exists. `charting-data.md › Prefer consistency across multiple charts`.
  - **Fix**: Show the same bar without a marker, and the "can be freed" line if a scan has run. The budget prompt then explains the missing marker.

- **M6. History and Dry Run use different table row styles.**
  [window-history-plenty-light.png](round-5/window-history-plenty-light.png) vs [window-dry-run-plenty-light.png](round-5/window-dry-run-plenty-light.png)
  - **What**: History uses full-width alternating stripes; Dry Run uses inset rounded alternating rows. The two panes have the same kind of data and the same columns, but look like two apps.
  - **Why**: `lists-and-tables.md › Choose a table or list style that coordinates with your data and platform`; `charting-data.md › Maintain continuity`, by analogy (judgment).
  - **Fix**: Use one `Table` style (`.inset(alternatesRowBackgrounds:)` or full-width) in both panes.

- **M7. The Dry Run window subtitle is a full, two-clause sentence.**
  [window-dry-run-plenty-light.png](round-5/window-dry-run-plenty-light.png)
  - **What**: "Ended. It would have freed 8.9 GB; Headroom now frees space for real." fills the entire title bar width and will truncate in a real toolbar or a narrower window.
  - **Why**: `writing.md › Be clear`: "If you can use fewer words, do so." `windows.md › Make sure that your windows adapt fluidly to different sizes`.
  - **Fix**: Subtitle "Ended · would have freed 8.9 GB". Move "Headroom now frees space for real" into a one-line banner above the table, or into the empty-state style header.

- **M8. Full Disk Access has two different action names.**
  [menu-first-run-light.png](round-5/menu-first-run-light.png) vs [settings-first-run-light.png](round-5/settings-first-run-light.png)
  - **What**: The menu offers "Allow Full Disk Access…" and Settings offers "Open Privacy Settings…" for the same task.
  - **Why**: `writing.md › Build language patterns`; `writing.md › Give clear guidance and use consistent language throughout processes with multiple steps`.
  - **Fix**: Use "Allow Full Disk Access…" in both places. The status line under it already explains the state.

- **M9. The footer controls are icon-only and one icon isn't a Mac convention.**
  All menu states
  - **What**: Pause, gear and power glyphs have no text. A power symbol for Quit isn't standard on macOS, where Quit is a word, as in menus. Pause doesn't show what it pauses or whether it's already paused, though the disabled state in first run and error is handled.
  - **Why**: `menus.md › Represent common actions consistently`; `accessibility.md › Describe your app's interface and content for VoiceOver`. Help tags and accessibility labels can't be verified from screenshots.
  - **Fix**: Keep the icons but add `.help()` tooltips ("Pause Headroom", "Settings…", "Quit Headroom") and accessibility labels. Consider a "Paused" state line in the status area when paused. Optionally move Quit into a "⋯" menu with Settings…, About and Quit, as many Apple menu bar extras do.

**Low**

- **L1.** Secondary text 3.95:1 in light (popover legend, "First to go…", list types, History dates and paths, empty-state bodies). Settings footers measure #7B7B7B on #F6F6F6 = 3.92:1. This is the system `secondaryLabelColor`; dark is 5.3–5.9:1. It's acceptable as Apple ships it, but make sure no essential fact lives only in secondary text (`accessibility.md › Vision`).
- **L2.** In acting state, "9 GB can be freed" next to "Freeing 4.8 GB" leaves you guessing whether the 4.8 GB is part of the 9 GB (`writing.md › Be clear`). Consider "9 GB can be freed · 4.8 GB in progress".
- **L3.** The History empty state promises "rebuild it with one click", but the action is "Rebuild…" with an ellipsis, which implies another step (`menus.md › Append an ellipsis…`). Make the two agree.
- **L4.** The History subtitle "6.9 GB freed now" is unclear ("now" means "still freed, not rebuilt"). Consider "6.9 GB freed · 1 of 4 rebuilt" (`writing.md › Be clear`).
- **L5.** The paths are truncated inconsistently: "/Users/…/code/ml" is cut while longer paths in other rows show in full. This may be sample data, so it's low confidence (`lists-and-tables.md › Consider ways to preserve readability of text that might otherwise get clipped`).
- **L6.** A full sidebar for two panes is heavy. It's acceptable, and it leaves room to grow; a toolbar segmented control would be the lighter Mac idiom (judgment; `sidebars.md › Consider letting people hide the sidebar`).
- **L7.** The Settings Budget field is free text with a unit. A `TextField` with a `Stepper`, or a validated number formatter, would prevent bad input (`writing.md › Show hints in text fields`; the "e.g. 60" placeholder is good).
- **L8.** The error copy carries a fallback instruction ("If Try Again doesn't help, quit and reopen Headroom"). Fine, but it makes the error block four lines. It could shorten to "Headroom couldn't read your disk, so nothing will be deleted." with Try Again, and show the fallback only after a failed retry (`feedback.md › Show people when a command can't be carried out and help them understand why`).

### Craft notes
- **Point of view (strong).** The disk bar with a budget tick, a hatched "can be freed" band and a teal "Freeing" segment is the signature. It's specific to this product and spends its boldness in one place. Teal is properly reserved: it appears only in acting state (icon, swatch and segment) and nowhere else. It saves brand color for status and lets the system accent own the controls (judgment, in line with the skill's branding guidance; `color.md › Avoid using the same color to mean different things`).
- **Typography.** One family, with hierarchy from a bold title ("52 GB free"), a primary subline, then body and secondary. No custom type, which is right for a utility.
- **Remove one accessory.** The legend row could go if the bar segments were labelled in place (Used / Can be freed / Free), but at 300 pt wide the legend is the safer call. The design is otherwise lean.

### What works
- Status sentences are written as outcomes with a reason: "52 GB above your budget, so there's nothing to do", "nothing is deleted for 4 more days", "Nothing will be deleted until it can". This is the best part of the product (`writing.md › Consider each screen's purpose`).
- Safety is honest. The Time Machine snapshot caveat in acting state, and "Until then, it deletes nothing" in first run, are exactly the transparency the Responsibility principle asks for (`feedback.md › Warn people when they initiate a task that can cause data loss`).
- Settings is a standard grouped form with three short sections and live footers that change with the toggle ("Off: Headroom frees space for real…" vs "Nothing is deleted while this is on…"). The "Off: …" pattern is reused under Full Disk Access (`settings.md › Minimize the number of settings`; `writing.md › Keep settings labels clear and simple`).
- Buttons and menu-style actions use title case and correct ellipses ("Set a Budget…", "Allow Full Disk Access…", "Open Privacy Settings…", "Try Again" with no ellipsis).
- Empty states give a symbol, a short title, the reason and the next action, matching `writing.md › Provide clear next steps on any blank screens`.
- History has sortable columns with a visible sort indicator, right-aligned sizes, relative dates and middle-truncated paths (`lists-and-tables.md › Desktop`).
- Dark mode is complete. Every surface was checked and primary text measures 12.3:1 on #1E1E1E; the teal shifts lighter (#3FB39A, 6.45:1 on the popover) instead of reusing the light value (`dark-mode.md › Embrace colors that adapt`).
- Disabled Pause in first run and error is dimmed rather than hidden (`menus.md › Show people when a menu item is unavailable`).

### Platform notes
- A window-style menu bar extra breaks the letter of `the-menu-bar.md › Display a menu — not a popover`, but that rule allows the exception ("Unless the app functionality you want to expose is too complex for a menu"). A chart justifies it, as with Control Center modules.
- From screenshots I can't verify the menu bar commands for Rebuild, Pause and the pane switch, keyboard shortcuts (⌘, for Settings), help tags, VoiceOver labels on the chart, or the "Headroom Settings" window title (`settings.md › Update the window's title`; `the-menu-bar.md › Provide app-specific menus for custom commands`; `undo-and-redo.md › Desktop`). Check these in the running app.

## Known render limits (not scored)
- No menu material or vibrancy: popovers draw as flat #FFFFFF and #1E1E1E.
- The windows draw as inactive: gray traffic lights, gray sidebar selection, gray switches ([settings-first-run-light.png](round-5/settings-first-run-light.png)), near-black sidebar symbols in dark, and gray bordered buttons in dark. The title bar text measures 1.9:1 (light) and 2.7:1 (dark) because of this.
- Title and subtitle sit inline in the title bar because no toolbar is rendered.
- Increase Contrast isn't applied: every `*-contrast.png` is byte-identical to its base image, so increased contrast is unreviewed.
- Sample paths are /Users/me/…, and relative times drift between renders ("20 seconds ago" vs "23 seconds ago").
