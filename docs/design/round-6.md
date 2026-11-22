# Design review, round 6: Headroom (blind)

Reviewed from the round-6 screenshots only (no source code, no earlier rounds). Method: `apple-look` skill, five lenses. HIG files opened: accessibility, layout, typography, color, designing-for-macos, the-menu-bar, menus, windows, sidebars, lists-and-tables, settings, feedback, loading, onboarding, writing, dark-mode, charting-data, undo-and-redo.

## Scores

| Surface | Score | Reason |
| --- | --- | --- |
| Menu: first run | 8 / 10 | Shows free space at once, explains the budget in one sentence, two clear next steps; both buttons carry equal weight, so the first step isn't marked as first |
| Menu: dry run | 8 / 10 | Passes the 5-second test; the budget tick is hard to read (its label sits below the legend, and "distance from the right edge" isn't explained) |
| Menu: plenty | 8.5 / 10 | Calm and complete: free space, what can be freed, and "nothing to do" in one line |
| Menu: acting | 7 / 10 | The text says it's acting, but the freeing segment is a 4.5 pt sliver, nothing shows that work is running, and there's no stop control in view |
| Menu: error | 7.5 / 10 | Honest and reassuring ("Nothing will be deleted"), but no cause, and the only fallback is "quit and reopen" |
| Menu-bar icon | 7 / 10 | Template SF-style gauges, right in light and dark; first run, dry run and plenty differ only in needle angle and tick style, the error state drops the gauge, and the width changes between states |
| History | 7.5 / 10 | Native sortable bordered table with Rebuild… on each row; paths are cut mid-word while about 350 pt of the row stays empty, and the Size header doesn't line up with its numbers |
| Dry Run | 7.5 / 10 | Clean list whose 8.9 GB total matches the menu, but its inset rounded row style differs from History's full-width bordered style |
| Settings | 8.5 / 10 | Grouped form like System Settings, with helpful footers; one footer describes the Off state, and the window title can't be checked |
| Empty states | 8.5 / 10 | Symbol, title, one-line explanation and a button; Apple's pattern done right in both panes |
| First run, end to end | 8 / 10 | Menu, window and Settings all lead to "Set a Budget…", and every surface says nothing is deleted yet; no suggested budget, and Full Disk Access is a separate, equal-weight step |

**Findings:** Critical 0 · High 1 · Medium 7 · Low 10

**5-second test**

- [menu-dry-run-light.png](round-6/menu-dry-run-light.png): **Pass.** "52 GB free" reads first. "8 GB below your budget… nothing is deleted for 4 more days" answers "will it act?" (not now, in 4 days) on the second line you read. It costs one extra beat because "38 GB can be freed" sits in the subtitle, the second-most prominent spot, and doesn't answer the question; the budget relationship is in body text further down.
- [menu-acting-light.png](round-6/menu-acting-light.png): **Pass, narrowly.** "57 GB free" and "Freeing 4.8 GB now" answer both questions, and the teal gauge marks Headroom as acting. Nothing on screen moves or progresses (as far as a still can show), and the teal bar segment is about 4.5 pt wide, so the chart adds almost nothing to the answer.

## Summary

Rating: **Good.** The thesis is clear: *a quiet, honest disk gauge: one bar shows free space against your budget, and one sentence says what Headroom will do about it.* People will remember the budget tick on a storage bar with a hatched "can be freed" region, a native-looking chart that reads like About This Mac's storage bar given a goal line. The app uses system type, system secondary colors and system controls, and saves teal for the acting state, as the brief intends. What keeps it below 9: the acting moment is the least developed state, the chart's encoding and non-text contrast need work, and the main window lists records without showing current status.

## Critical

None. All primary text measures 12:1 to 15:1. Secondary text is the system secondary label color (see L1). Every status that color shows is also spelled out in text.

## Improvements

### High

**H1. The acting state shows no live activity and no way to stop in context.**
[menu-acting-light.png](round-6/menu-acting-light.png) · [menu-acting-dark.png](round-6/menu-acting-dark.png)

- **What**: While Headroom deletes 4.8 GB, the popover shows static text, a teal segment about 9 px (4.5 pt) wide wedged against the budget tick, and a teal icon. Nothing shows that work is running (no indeterminate indicator, which needs no progress data), and stopping takes the "⋯" menu, then Pause. This is the most consequential moment in the app, because it is deleting your files right now.
- **Why**: `loading.md › Showing progress`: "use an *indeterminate* progress indicator when you don't [know how long]." `feedback.md › Best practices`: "Consider integrating status feedback into your interface… near the items it describes." Agency is one of Apple's principles (`design-principles`, as summarized in the skill): people can recover from what the app does.
- **Fix**: In the "Freeing 4.8 GB of Rust build files in engine" row, add a small `ProgressView()` (indeterminate, `.controlSize(.small)`) and a trailing "Pause" or "Stop" button (`.buttonStyle(.borderless)`, or `.link` in teal). Keep Pause in "⋯" as well. If you can't add a control, at least animate the icon's needle in the menu bar while acting (respecting Reduce Motion).

### Medium

**M1. Chart parts fall below 3:1 non-text contrast, and "Freeing" relies mostly on hue.**
[menu-dry-run-light.png](round-6/menu-dry-run-light.png) · [menu-acting-light.png](round-6/menu-acting-light.png) · [menu-acting-dark.png](round-6/menu-acting-dark.png)

- **What** (sampled pixels):
  - Light Free fill #E6E6E6 on #FFFFFF: 1.25:1. Bar outline #ACACAC: 2.27:1. Used #888888 against Free #E6E6E6: 2.84:1.
  - Dark Free fill #343434 on #1E1E1E: 1.34:1.
  - Freeing teal #0F6E5C against Used #888888: 1.74:1 (light). #3FB39A against #878787: 1.39:1 (dark).
  - Free space, the headline quantity, is the region with the weakest edge. "Can be freed" has a hatch pattern; "Freeing" is a hue change only.
- **Why**: `accessibility.md › Vision`: "Convey information with more than color alone… Offer visual indicators, like distinct shapes." `color.md › Inclusive color`: "Avoid relying solely on color to differentiate between objects."
- **Fix**: Darken the bar outline to `Color.secondary` (about 3.9:1) or use a 1 pt `separatorColor` plus a slightly darker free fill (`Color.primary.opacity(0.12)`). Give Freeing a shape cue as well as teal: a 1 pt white inner divider on each side plus a minimum drawn width of 6 pt, or animated diagonal stripes that respect Reduce Motion. Also check that the bar carries an `accessibilityLabel` such as "52 GB free of 245 GB, budget 60 GB, 38 GB can be freed" (`charting-data.md › Best practices`: "Make every chart in your app accessible."). The screenshots can't show this.

**M2. The budget tick is a new chart encoding with no explanation, and its label sits away from it.**
[menu-dry-run-light.png](round-6/menu-dry-run-light.png) · [menu-plenty-light.png](round-6/menu-plenty-light.png)

- **What**: The tick marks "60 GB from the right end", so you're under budget when the free segment is shorter than the gap from the tick to the right edge. That is a new reading of a storage bar. Its label "| 60 GB budget" is centered under the tick but sits *below* the legend row, about 45 pt from the bar, so the legend breaks the link between them.
- **Why**: `charting-data.md › Designing effective charts`: "If you need to create a chart that presents data in a novel way, help people learn how to interpret the chart." `layout.md › Visual hierarchy`: "Group related items to clearly express related information."
- **Fix**: Put the budget label directly under the tick, between the bar and the legend, and draw a thin bracket or tinted span from the tick to the right edge labeled "keep 60 GB free". Or rewrite the subtitle as the summary: "8 GB below your 60 GB budget" (`charting-data.md`: "display brief descriptive text that serves as a headline or summary for a chart"). That also fixes the 5-second test's extra beat.

**M3. The menu-bar icon states are hard to tell apart at menu-bar size, and the error state loses the app's identity.**
[icon-first-run-light.png](round-6/icon-first-run-light.png) · [icon-dry-run-light.png](round-6/icon-dry-run-light.png) · [icon-plenty-light.png](round-6/icon-plenty-light.png) · [icon-error-light.png](round-6/icon-error-light.png)

- **What**: First run (no budget, Full Disk Access missing: needs you), dry run and plenty are all gauges. They differ by needle angle and by partial versus full tick rings, which disappear at 16 pt. Error replaces the gauge with a bare warning triangle that any app could own. Widths: 24 pt for the gauges, 26 pt for error, 24×25 pt for acting, so neighboring extras shift when the state changes.
- **Why**: `the-menu-bar.md › Menu bar extras`: "Consider using a symbol to represent your menu bar extra… customizing it to suit your needs." `accessibility.md › Vision`: "Offer visual indicators, like distinct shapes or icons… to help people perceive… changes in state."
- **Fix**: Keep the gauge on every state and add badges, as acting does with its minus badge: an exclamation badge for error, a small dot badge for "needs setup", and a distinct dry-run mark such as a dashed needle or an eye badge. Give all states one fixed frame (for example 24 × 24 pt template image) so the item never changes width.

**M4. History and Dry Run use two different table styles.**
[window-history-plenty-light.png](round-6/window-history-plenty-light.png) · [window-dry-run-plenty-light.png](round-6/window-dry-run-plenty-light.png)

- **What**: History uses full-width alternating rows (bordered style). Dry Run, a sibling pane with the same columns, uses inset, rounded alternating rows. Switching panes in the sidebar visibly changes the window's design.
- **Why**: `lists-and-tables.md › Style`: "Choose a table or list style that coordinates with your data and platform… macOS defines a bordered style that uses alternating row backgrounds." `writing.md › Best practices` (Build language patterns): "Consistency builds familiarity."
- **Fix**: Use the same `Table` style in both panes. `.tableStyle(.bordered(alternatesRowBackgrounds: true))`, as History already uses, suits a multicolumn log.

**M5. History cuts paths mid-word while the row has room.**
[window-history-plenty-light.png](round-6/window-history-plenty-light.png)

- **What**: "/Users/m…de/engine" and "/Users/…erivedData" break inside words, while Status and the unlabeled Rebuild column hold about 350 pt of mostly empty space. Dry Run shows the same kind of paths in full.
- **Why**: `lists-and-tables.md › Content`: "Consider ways to preserve readability of text that might otherwise get clipped or truncated… an ellipsis in the middle of text can make an item easier to distinguish because it preserves both the beginning and the end."
- **Fix**: Abbreviate the home folder as `~` (`(path as NSString).abbreviatingWithTildeInPath`), which turns "/Users/me/code/engine" into "~/code/engine" and fits. Truncate whole path components in the middle ("~/…/DerivedData"), never characters. Give Folder the width that Status doesn't need.

**M6. The main window shows records but no current status.**
[window-history-plenty-light.png](round-6/window-history-plenty-light.png)

- **What**: "Open Headroom" opens a log. The window never shows free space, the budget, or whether Headroom will act, which is the product's own 5-second question. With two sidebar items and three or four rows, the 800 × 500 pt window is mostly empty stripes.
- **Why**: `writing.md › Best practices`: "Consider each screen's purpose… put the most important information first." `designing-for-macos.md › Best practices`: "Leverage large displays to present more content in fewer nested levels."
- **Fix**: Without adding features, reuse the menu's header (free space, bar, status sentence) as a top section in the History pane or as the window subtitle's first clause, for example "52 GB free · 8 GB below budget". The data already exists.

**M7. The error message names no cause and offers a weak fallback.**
[menu-error-light.png](round-6/menu-error-light.png)

- **What**: "Headroom couldn't read your disk… If Try Again doesn't help, quit and reopen Headroom." No cause, and the fallback is a restart.
- **Why**: `writing.md › Best practices`: "be clear about what someone can do to fix it… If you find that language alone can't address an error… rethink the interaction." `feedback.md › Best practices`: "Show people when a command can't be carried out and help them understand why."
- **Fix**: Name the volume and the likely cause when it's known ("Headroom couldn't read Macintosh HD. Free space will show again once the disk responds."). Have the app retry on its own and say so ("Trying again in 1 minute"), and keep Try Again. Drop "quit and reopen". Point to Disk Utility only if the failure persists.

### Low

- **L1. Secondary text measures 3.9:1 in light** (#808080 on #FFFFFF 3.95:1; table #7A7B7B on stripe #F4F5F5 3.89:1; settings footers #7B7B7B on #F6F6F6 3.92:1; placeholder "e.g. 60" #888888 on #F2F2F2 3.17:1). In dark it's 5.4 to 5.9:1. This is the system `secondaryLabelColor` / `placeholderTextColor`, which Apple ships in every app, so it's reported and weighed as Apple does: not a defect, as long as it carries only secondary content. It does here, except that History's **Date Freed** column, which you sort by, is all secondary color; consider `labelColor` for that column (`accessibility.md › Vision`, `dark-mode.md › Text`: "Use the system-provided label colors for labels").
- **L2. Column headers don't align with their data.** "Size" is left-aligned over right-aligned numbers ([window-history-plenty-light.png](round-6/window-history-plenty-light.png)). Right-align numeric headers (`TableColumn(...).alignment(.trailing)` or the header cell's alignment). Judgment, plus `layout.md › Visual hierarchy`: "Align elements to make them easier to scan."
- **L3. A settings footer describes the Off state.** "Off: Headroom frees space for real…" ([settings-plenty-light.png](round-6/settings-plenty-light.png)), and the footer text swaps with the toggle. `writing.md › Best practices`: "Describe what it does when turned on, and people can infer the opposite." Use one fixed footer: "While on, nothing is deleted; what Headroom would delete appears under Dry Run."
- **L4. Capitalization is mixed.** The window subtitle reads "Ended · would have freed 8.9 GB" (capital, then lowercase). The menu section label is sentence case ("First to go when the dry run ends") while Settings headers are title case ("Free Space"). `writing.md › Best practices`: "Choose a style for each UI element type and use it consistently."
- **L5. The vocabulary drifts between free, delete and freed.** "Can be freed", "Freeing", "Freed", "Nothing deleted yet", "Headroom deletes…", "would have freed", "would delete". Using "delete" in Settings for honesty is defensible; pick one word for the status surfaces (`writing.md › Build language patterns`).
- **L6. First-run buttons have equal weight.** "Set a Budget…" and "Full Disk Access Settings…" look the same ([menu-first-run-light.png](round-6/menu-first-run-light.png)). Make the required step the default button (`.keyboardShortcut(.defaultAction)` or `.buttonStyle(.borderedProminent)` in the system accent, not teal) (`onboarding.md › Additional requests`).
- **L7. There's no suggested budget.** The placeholder "e.g. 60" leaves the number to the user. A computed suggestion (for example 15% of the disk) with a one-click accept would remove a decision. `settings.md › Best practices`: "Aim to provide default settings that give the best experience to the largest number of people." Trade-off: "deletes nothing until you set a budget" is a deliberate safety stance, so a *suggested* value is better than a *default* one.
- **L8. The popover footer uses a bordered push button.** Apple's window-style extras (Wi-Fi, Bluetooth, Battery) end with a full-width, menu-row-style "… Settings…" item rather than a bordered button. Consider an "Open Headroom" row styled like a menu item. Judgment, informed by `the-menu-bar.md › Menu bar extras` (which prefers menus over popovers; the chart justifies the popover here).
- **L9. The Status column repeats "Freed" on most rows.** The column mostly says the obvious. Fold the state into the Rebuild column ("Rebuild…" button, or "✓ Rebuilt") and give Folder the space (`lists-and-tables.md › Content`: "Keep item text succinct").
- **L10. The selected sidebar label looks semibold** ("History", "Dry Run"). macOS sidebars don't bold the selection. It may be a render artifact; check in a live window (`sidebars.md`).

## Craft notes

- **Point of view (strong).** The budget-tick storage bar is the signature, and everything else stays quiet: system type (headline about 17 pt bold, body 13 pt), system grays, one teal moment. It's a deliberately quiet utility, and that's the right call. It doesn't look like a template.
- **Restraint (strong).** Teal appears only in the acting state (icon, legend swatch, bar segment, row icon) and nowhere else. Dark teal #0F6E5C is 6.17:1 on white and #3FB39A is 6.45:1 on #1E1E1E, so both pass as text-strength color.
- **What to remove.** The "60 GB budget" line could go if the subtitle became "8 GB below your 60 GB budget" (see M2). The Status column in History could go (L9).

## What works

- The header carries a large number and the status sentence carries a sentence, so a glance answers "how much is free". Keep the number first.
- Status sentences that commit: "52 GB above your budget, so there's nothing to do." "This is a dry run, so nothing is deleted for 4 more days." Every state says whether anything will be deleted. Keep this pattern.
- "First to go when the dry run ends" previews the next action with sizes that add up to more than the 8 GB deficit (0.6 + 6.3 + 2 = 8.9 GB) and match the Dry Run window and its title.
- The Time Machine note explains why free space won't rise right away, before anyone wonders.
- The empty states follow Apple's symbol, title, explanation and button pattern, with the next action on the button (`writing.md`: "give them a button or link to do so if possible").
- History: sortable "Date Freed" column with an indicator, a per-row "Rebuild…" button, and "✓ Rebuilt" that shows the result of the undo in place (`undo-and-redo.md › Best practices`: "Show the results of an undo or redo").
- Settings look like System Settings: grouped `Form`, label left, control right, explanatory footers, "Full Disk Access Settings…" with the correct ellipsis.
- Dark mode uses system dark surfaces (#1E1E1E menu, #282828 grouped rows). Every color is adapted, nothing glows, and secondary text gets *more* contrast in dark (5.9:1).

## Platform notes

- The window-style popover is justified because the chart can't live in an `NSMenu`. This is the trade-off `the-menu-bar.md › Menu bar extras` allows ("Unless the app functionality… is too complex for a menu").
- Keep everything reachable from the menu bar too (`designing-for-macos.md`): Pause, Settings… ⌘, and Quit ⌘Q are in "⋯". When the main window is key, make sure the app menu also has Settings… and a Pause/Resume item.
- With the popover the only status surface, `the-menu-bar.md` warns: "Avoid relying on the presence of menu bar extras." M6 (status in the window) also covers people who hide the extra.
- Settings window title: `settings.md › Platform considerations` asks for "*App Name* Settings" on a single-pane settings window, with minimize and zoom dimmed. The content-only render can't show either.

## Known render limits (not scored)

- No menu material or vibrancy in the popover; it draws on flat #FFFFFF / #1E1E1E.
- Windows draw as inactive: gray traffic lights, gray selection, gray title text (title #B0B0B0 on #F0F0F0 is 1.9:1, the inactive title color), gray toggles in Settings, and near-black sidebar symbols in dark.
- Title and subtitle sit inline because the renderer adds no toolbar.
- Increase Contrast isn't applied: 28 of the 32 `-contrast` files are byte-identical to their base looks. The 4 plenty-state window files differ only where the relative times drifted. So increased-contrast behavior isn't reviewed.
- Sample paths are /Users/me/… and relative times drift between captures ("19 seconds ago" vs "22 seconds ago").
- Not checkable from stills: VoiceOver labels on the chart and icon-only "⋯" button, keyboard focus order, hover states, motion, and the Settings window title and toolbar.
