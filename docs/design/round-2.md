# Design review, round 2: Headroom (blind)

I reviewed this blind. I judged only the 64 screenshots in `round-2/`. I did not read any source code or earlier rounds. Contrast figures come from pixels sampled in the 2x PNGs (WCAG relative luminance).

## Scores

| Surface | Score | Reason |
| --- | --- | --- |
| Menu: first run | 8 / 10 | Clear headline, two actions that say what they do, and it states plainly that nothing will be deleted. The dry run isn't mentioned yet |
| Menu: dry run | 7 / 10 | Both 5-second answers are there, but the orange bar looks alarming even though nothing will happen, and "dry run" is said twice |
| Menu: plenty | 8 / 10 | Calm and correct. The dark-gray "Free" segment reads as used space |
| Menu: acting | 7 / 10 | It's clear that work is happening, but there's no progress for the 4.8 GB job. The Time Machine note is excellent |
| Menu: error | 7.5 / 10 | Honest title, it promises nothing will be deleted, and offers a recovery. It doesn't give a cause |
| Menu-bar icon | 7 / 10 | A consistent template-glyph family. Three of five states use a different glyph than the popover, and two gauges differ only by the needle |
| History | 7 / 10 | Clean rows with Rebuild… on each one and a "Rebuilt" confirmation. The header count disagrees with the rows, and the header has weak hierarchy |
| Dry Run | 5.5 / 10 | The key message ("the dry run is over") is truncated and buried under the rows, and the header says "Nothing was deleted" while deletions are happening |
| Settings | 7.5 / 10 | Reads like System Settings. Two footers misfire, and Full Disk Access has no status |
| Empty states | 7.5 / 10 | Both explain themselves and offer the next step. The Dry Run copy is awkward |
| First run, end to end | 7 / 10 | "Set a Budget…" is the same everywhere. The dry run is explained only in a Settings footer, and the 18 GB headline gives no context |

**Findings:** Critical 0 · High 2 · Medium 9 · Low 9 (render suspects not scored).

**5-second test**

- [`menu-dry-run-light.png`](round-2/menu-dry-run-light.png): **Pass, with friction.** "52 GB free" registers instantly. "Will it act?" is answered in line 2 ("dry run, nothing deleted") and again in the status line ("nothing is deleted for 4 more days"). The full-width orange bar says "alarm" before the text says "relax", so the eye gets two opposite signals in the first second.
- [`menu-acting-light.png`](round-2/menu-acting-light.png): **Pass.** "57 GB free · freeing space" answers both questions in the first two lines. The spinner and "Freeing 4.8 GB of Rust build files" confirm it. There's no indication of how far along it is.

---

## Design review: Headroom

### Summary

This is a quiet, well-mannered menu-bar utility that mostly sounds and looks like macOS: system type, grouped Settings form, template menu-bar glyphs, menu-style rows with real shortcuts, and honest plain-language copy ("Headroom won't delete anything until you do."). Its thesis is clear: one headline number, one bar, one sentence saying whether Headroom will act. The thing it will be remembered by is the **budget bar**: free space, plus what Headroom can reclaim, measured against a budget tick. It's a good idea, but it's the weakest-specified element, and the Dry Run pane has a clipped, misplaced status message.
**Overall rating: Good.** There are no Critical findings and two High findings. Apple wouldn't ship it as is, but it's close in the popover and Settings.

### Critical

None scored. See **Suspected render artifacts**: the dark-mode sidebar icons would be Critical if they reproduce on a real device.

### Improvements

**H1. Dry Run: the state message is truncated, buried, and contradicts the header (High)**
- **What**: In [`window-dry-run-plenty-light.png`](round-2/window-dry-run-plenty-light.png) and [dark](round-2/window-dry-run-plenty-dark.png), the most important sentence on the pane, "The dry run is over. Headroom now frees space for real, and everything it deletes appea…", is cut off with an ellipsis at the window's full 720 pt width. It sits below the last row in the same left inset, where it looks like another row. Above it, the header says "Would have freed 8.9 GB. Nothing was deleted.", yet History in the same state shows 6.9 GB actually deleted. A reader can't tell which statement describes now.
- **Why**: `layout.md › Visual hierarchy`: "place the most important items near the top and leading side of the window." `lists-and-tables.md › Content`: "Consider ways to preserve readability of text that might otherwise get clipped or truncated." `writing.md › Best practices`: "put the most important information first."
- **Fix**: Move the state into the header and let it wrap: "Dry run ended Nov 13. These are the folders it would have deleted (8.9 GB). Headroom now deletes for real; see History." Use `.lineLimit(nil)` / `fixedSize(horizontal: false, vertical: true)` and never truncate prose. Drop "Nothing was deleted." from the header once the dry run has ended, or change it to "Nothing was deleted during the dry run."

**H2. The budget bar's encoding changes per state and is partly meaningless (High)**
- **What**: The bar answers the 5-second test, but:
  1. The **"Free" series changes color by state**: orange `#FF8D28` in [dry run](round-2/menu-dry-run-light.png) and [acting](round-2/menu-acting-light.png), dark gray `#686868` in [plenty](round-2/menu-plenty-light.png). The legend swatch changes with it. In the plenty state, a dark-gray "free" bar reads as *used* space, which is the opposite of the convention in the macOS Storage settings.
  2. **The trailing light track isn't in the legend and isn't data.** Measured from the pixels, the track's total length is always about 1.1 × (free + can be freed): 99 GB in dry run, 72 GB in acting, 150 GB in plenty. So the gray tail is padding, but it looks like "the rest of the disk".
  3. In the dry-run state, the bar uses its warning color even though the copy says nothing will happen for 4 days.
  4. In light mode, adjacent segments differ only by hue: Free versus Can be freed is 1.35:1, and Can be freed versus the track is 1.37:1. The 4 px gaps help, so this part is a Low on its own.
- **Why**: `color.md › Best practices`: "Avoid using the same color to mean different things. Use color consistently throughout your interface, especially when you use it to help communicate information like status." `charting-data.md › Designing effective charts`: "If you need to create a chart that presents data in a novel way, help people learn how to interpret the chart." Also from that section: "Maintain continuity among multiple charts that use the same data … consistent colors."
- **Fix**: Keep "Free" one color in every state, either `Color.primary.opacity(0.75)` or the accent color. Show the status through the budget tick and the status glyph and sentence instead. If you want a warning, color only the shortfall between the free edge and the tick (`.orange`, and only when Headroom will actually act, not during the dry run). Either make the bar's full width mean disk capacity and add "Used" to the legend, or end the track at free + can be freed.

**M1. History header count disagrees with the rows (Medium)**
- **What**: [`window-history-plenty-light.png`](round-2/window-history-plenty-light.png) says "Headroom freed 6.9 GB from 3 folders" above four rows totaling 9.0 GB. The math works only if you notice that Atlas is "Rebuilt" and excluded.
- **Why**: `writing.md › Best practices`: "Consider each screen's purpose … put the most important information first. Format your text to make it easy to read."
- **Fix**: "6.9 GB freed now · 1 folder rebuilt" or "Freed 9.0 GB from 4 folders · 2.1 GB rebuilt since". You could also move rebuilt rows into a "Rebuilt" section.

**M2. Pane summaries are styled like chrome, not titles (Medium)**
- **What**: "Headroom freed 6.9 GB…" and "Would have freed 8.9 GB…" are about 11 pt semibold in secondary gray (`#808080` on white, 3.95:1). They sit in a hairline-bordered strip that reads as a toolbar or column header rather than the pane's headline. The row titles under them are visually louder.
- **Why**: `typography.md › Conveying hierarchy`: "Adjust font weight, size, and color as needed to emphasize important information." `layout.md › Visual hierarchy`: "Order content by relative importance."
- **Fix**: Use `.font(.title3).fontWeight(.semibold)` in `.primary` for the summary. Put the secondary fact on a `.subheadline` `.secondary` line, or use the window subtitle (`.navigationSubtitle`) under the pane title.

**M3. Acting state shows no progress (Medium)**
- **What**: [`menu-acting-light.png`](round-2/menu-acting-light.png) shows an indeterminate spinner for "Freeing 4.8 GB". Headroom knows both the total and how much it has deleted.
- **Why**: `loading.md › Showing progress`: "use a *determinate* progress indicator when you know how long loading will take."
- **Fix**: Add a small `ProgressView(value: freed, total: 4.8 GB)` under the line, or "Freeing 4.8 GB … 2.1 GB done". You could also let the bar's Free segment grow live.

**M4. Menu-bar glyphs don't match the popover's glyphs, and two are near-twins (Medium)**
- **What**: The icon and popover glyphs differ for the same state: plenty is a gauge in the menu bar and a check circle in the popover; error is `exclamationmark.circle` versus a warning triangle; acting is circular arrows versus a spinner. First run and plenty are both gauges that differ only by the needle angle, about 2 pt of difference at menu-bar size. A needle pegged right can also read as "disk full".
- **Why**: `the-menu-bar.md › Menu bar extras`: "Consider using a symbol to represent your menu bar extra." `accessibility.md › Vision`: "Offer visual indicators, like distinct shapes or icons … to help people perceive differences in function and changes in state." Pairing each icon with its popover glyph is my judgment.
- **Fix**: Use one glyph per state in both places. For example: first run `gauge.with.dots.needle.0percent` with a badge, plenty `checkmark.circle`, dry run `eye.circle`, acting `arrow.trianglehead.2.clockwise.rotate.90.circle`, error `exclamationmark.triangle`. Or keep one base gauge and badge it per state.

**M5. "Autopilot" appears only in the menu (Medium)**
- **What**: "Pause Autopilot" is the only place the word Autopilot appears. Settings, empty states and status text all talk about "Headroom", "budget" and "dry run".
- **Why**: `writing.md › Best practices`: "Build language patterns. Consistency builds familiarity."
- **Fix**: "Pause Headroom" / "Resume Headroom", or "Pause Freeing Space". Make sure the label changes when paused (`menus.md › Toggled items`: "Consider using a changeable label that describes an item's current state").

**M6. The Dry Run footer in Settings describes the Off state (Medium)**
- **What**: [`settings-plenty-light.png`](round-2/settings-plenty-light.png): "Off: Headroom frees space for real, and every deletion appears in History." The footer changes with the toggle, which also makes the form shift height.
- **Why**: `writing.md › Best practices`: "Describe what it does when turned on, and people can infer the opposite."
- **Fix**: Use one fixed footer: "While on, Headroom only lists what it would delete. On for your first 7 days."

**M7. The Free Space footer refers to the wrong row and skips the hard one (Medium)**
- **What**: "When free space drops below this…" sits under "Leave projects alone for", so "this" is ambiguous. The less obvious setting, "Leave projects alone for 14 days", isn't explained at all.
- **Why**: `writing.md › Best practices`: "Keep settings labels clear and simple … If the setting label isn't enough, add an explanation."
- **Fix**: "When free space drops below 60 GB, Headroom deletes rebuildable files from projects you haven't opened in 14 days, oldest first." It uses both values and covers both rows.

**M8. Full Disk Access row has no status (Medium)**
- **What**: The row shows only "Allow…" or "Review…". The state (granted or not) has to be inferred from the button verb.
- **Why**: `feedback.md › Best practices`: "Consider integrating status feedback into your interface."
- **Fix**: Show a trailing status, "Allowed" with a checkmark or "Not allowed" in secondary text, before the button. Use the same pattern as the system's Privacy & Security rows.

**M9. Dry Run empty-state copy is awkward (Medium)**
- **What**: [`window-dry-run-first-run-light.png`](round-2/window-dry-run-first-run-light.png): "Choose how much space to keep free first. For its first 7 days Headroom then only shows what it would delete, here." The clauses come in the wrong order and the sentence ends in a dangling comma before "here". "Nothing to show yet" is generic.
- **Why**: `writing.md › Best practices`: "Provide clear next steps on any blank screens … guide people on actions they can take."
- **Fix**: Title: "No Dry Run Yet". Body: "Set a budget to start. For the first 7 days, Headroom lists what it would delete here and deletes nothing."

**L1. Dry-run popover says "dry run" twice (Low).** Line 2 says "dry run, nothing deleted" and the status says "This is a dry run, so nothing is deleted for 4 more days." Shorten line 2 to "38 GB can be freed" and let the eye line carry the mode, or the other way round. This follows the "remove one accessory" check from the craft lens.

**L2. "Freeing 4.8 GB of Rust build files in engine" (Low).** The project name runs into the sentence. Use "…from **engine**" or "in the engine project", matching the dry-run list where the project name is primary and the kind is secondary.

**L3. Two-item sidebar (Low).** A full sidebar for History and Dry Run spends 180 pt on two destinations, and the selected row is bolded, which macOS sidebars don't do. `sidebars.md › Best practices`: "Consider letting people hide the sidebar." A toolbar segmented `Picker` would also fit (my judgment).

**L4. Disabled "Pause Autopilot" is lighter than the system's disabled text (Low).** It measures `#D4D4D4` on white, 1.48:1 (`#444` on `#1E1E1E`, 1.71:1, in dark). Disabled text is exempt from the 4.5:1 rule, but use `disabledControlTextColor` / `.disabled(true)` rather than a custom tint (`color.md › Desktop (macOS)`).

**L5. Settings first run: the "60" placeholder reads as a value (Low).** It's `#888888` on `#F2F2F2`, 3.17:1, with no unit hint. Pair it with the footer ("…until you choose an amount") or use an empty field with a "60" prompt plus "Suggested" text.

**L6. Error gives no cause (Low).** "Headroom couldn't read your disk" doesn't say why (permission, disk ejected, APFS busy). `writing.md › Best practices`: "be clear about what someone can do to fix it." If the cause is known, name it and give the fix ("Allow Full Disk Access…").

**L7. Secondary text at 3.95:1 in light mode (Low, Apple standard).** Captions, paths, timestamps, legend and shortcuts are `#808080` on `#FFFFFF`, 3.95:1. That is `secondaryLabelColor` exactly. Dark mode is `#9A9A9A` on `#1E1E1E`, 5.92:1, and the Settings footers are `#7B7B7B` on `#F6F6F6`, 3.92:1. These fall below the 4.5:1 guidance in `accessibility.md › Vision`, but they're Apple's own semantic color, which adapts under Increase Contrast (`dark-mode.md › Text`: "Use the system-provided label colors for labels"). Keep them.

**L8. No way to hide the menu-bar extra (Low).** `the-menu-bar.md › Menu bar extras`: "Let people, not your app, decide whether to put your menu bar extra in the menu bar." For a menu-bar-only app, this is a "Show in menu bar" toggle plus reopening from the Dock or Finder.

**L9. "Rebuild it with one click" versus "Rebuild…" (Low).** The empty History promises one click, but the ellipsis signals a confirmation step (`menus.md › Labels`: the ellipsis "signals that people need to input information or make additional choices"). Say "rebuild it from here".

### Craft notes

- **Point of view (keep it):** The budget bar plus a one-sentence verdict is the right signature for this product. It shows the free edge, the tick for the budget, and how far Headroom's reach (the gray) extends past it. Once H2 is fixed, it's distinctive without decoration. Everything else is correctly quiet. This isn't a template look; it's system type and system controls throughout.
- **Voice (keep it):** The copy is plain, specific and reassuring: "Nothing will be deleted until it can.", "macOS releases it within a day.", "Headroom can't see some folders yet." That's the right tone for an app that deletes files.
- **One accessory to remove:** the duplicated dry-run sentence (L1).

### What works

- **Popover structure**: headline number, then a one-line state, then the bar, then a status line with a glyph, then actions, then menu rows with real shortcuts (⌘O, ⌘, ⌘Q) and separators. "Settings…" and "Quit Headroom" are worded exactly as macOS expects. A popover rather than a menu is justified here because of the bar (`the-menu-bar.md › Menu bar extras`: "Unless the app functionality you want to expose is too complex for a menu").
- **Recovery is always visible**: Rebuild… on every History row, with a "✓ Rebuilt" end state (`undo-and-redo.md › Best practices`: "Show the results of an undo or redo"; `feedback.md`: "confirm that a significant action or task has completed").
- **The Time Machine note** explains a confusing macOS behavior right where people would see it (`feedback.md › Best practices`).
- **Settings** use a grouped form with title-case section headers and sentence-case rows, matching System Settings. Units sit next to the field ("60 GB").
- **Empty states** each have a glyph, a title, an explanation and a button (`writing.md`: "give them a button or link to do so if possible").
- **The dry-run list** ("First to go when the dry run ends") totals 8.9 GB, matching the Dry Run pane, so the numbers stay consistent across surfaces.
- **Dark mode** is built from semantic colors throughout. Primary text is `#DDDDDD` on `#1E1E1E`, 12.3:1, and nothing is pure white on pure black.
- **Menu-bar glyphs** are true template images, one family of circled glyphs, and they render correctly on light and dark menu bars.

### Platform notes

- **Settings window** (not visible in this render): it should be titled "Headroom Settings" with minimize and zoom dimmed (`settings.md › Desktop (macOS)`: "If your settings window doesn't have multiple panes, use the title *App Name* Settings"). A single pane with no toolbar is fine.
- **Main window** (not visible in this render): add View › Show/Hide Sidebar and ⌘1 / ⌘2 for the panes, so the menu bar reaches every pane (`designing-for-macos.md › Best practices`: "Use the menu bar to give people easy access to all the commands they need").
- **Increase Contrast** can't be verified, because the renderer doesn't apply it. The custom orange and the three bar grays need higher-contrast variants (`color.md › Best practices`: "supply light and dark variants, and an increased contrast option for each"). Check this on a device.
- **Bar accessibility**: give the bar one combined accessibility label ("52 GB free, 38 GB can be freed, budget 60 GB") (`charting-data.md › Best practices`: "Make every chart in your app accessible"). I can't verify this from the screenshots.

### Suspected render artifacts (not scored)

- **Dark-mode sidebar icons are almost black**: `#1C1C1C` on `#282828`, **1.16:1**, in [`window-history-plenty-dark.png`](round-2/window-history-plenty-dark.png) and every dark window shot. The empty-state glyphs and popover glyphs render light correctly in dark mode, so this looks like the offscreen inactive-window tint. **Check on a real Mac.** If it reproduces, it's Critical (`accessibility.md › Vision`, contrast for icons). The probable cause would be a non-template image or a hard-coded tint.
- Light sidebar icons are `#A8A8A8` on `#F0F0F0`, 2.09:1. The gray selection, the gray Dry Run toggle in Settings, and the gray buttons in dark mode are the known inactive-window artifacts.
- The `-contrast` variants are byte-identical to the base images, except that the relative timestamp in the plenty windows reads "15 seconds ago" in one and "17 seconds ago" in the other (render time drift). Increase Contrast wasn't applied.
- The dry run is over in the plenty state, yet its rows show "15 seconds ago" for would-have-deleted items. This is likely the fixed sample data, not a design choice.
- The bar's total length differs per state (99, 72 and 150 GB). If that's the sample data using different disks, H2.2 becomes "label the track". If not, H2.2 stands.
