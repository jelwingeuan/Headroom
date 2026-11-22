# Design review, round 4: Headroom (blind)

Blind review: the reviewer saw only the PNGs in `round-4/`. No source code and no earlier round reports were read. HIG citations are `file › heading` from the apple-look skill's references. Contrast figures are WCAG ratios computed from sampled pixels in the 2x renders.

## Scores

| Surface | Score | Reason |
| --- | --- | --- |
| Menu: first run | 8.0 | Two clear jobs, each with its own button. Disabled Pause is correct. The copy is calm and honest. |
| Menu: dry run | 7.5 | The free number reads at once. "Will it act?" takes reading a two-line sentence. "First to go" is a strong addition. |
| Menu: plenty | 8.0 | The quietest and clearest state. The marker sits inside the fill and the sentence confirms it. |
| Menu: acting | 6.5 | The thing happening (teal) is a 20 pt sliver at the far right. The legend wraps. A minus glyph stands for "freeing". |
| Menu: error | 8.0 | Says what failed, that nothing is at risk, and what to try next. |
| Menu-bar icon | 6.0 | Clean template glyphs at the right size, but five unrelated symbols give no constant identity, and minus.circle misreads. |
| History window | 7.0 | Native table with sorting, stripes and a clear Rebuilt state. The subtitle count, the column headings and the truncation need work. |
| Dry Run window | 6.5 | Table style differs from History. "Picked" is jargon. The ended state gives no next step and drops the menu's order. |
| Settings | 8.0 | Grouped form, units beside the field, honest footers. Small wording issues only. |
| Empty states | 8.0 | Symbol, title, one sentence and one action. The Dry Run copy is awkward. |
| First run, end to end | 7.5 | Budget and Full Disk Access are asked for in context in the menu and Settings. The window's empty states mention only the budget. |

**Findings:** Critical 0 · High 1 · Medium 11 · Low 10

**5-second test**
- [menu-dry-run-light.png](round-4/menu-dry-run-light.png): *How much is free?* Under 1 s ("52 GB free", bold title). *Will Headroom act?* About 3 to 4 s. You have to read past "8 GB below your budget" (which sounds like it's about to act) to reach "This is a dry run, so nothing is deleted for 4 more days." The eye glyph helps only if you already know what it means. **Pass, narrowly.**
- [menu-acting-light.png](round-4/menu-acting-light.png): *Free?* Under 1 s ("57 GB free"). *Acting?* About 2 s, from "· freeing space" in the subtitle. The bar's teal "Freeing now" segment is about 20 pt wide, squeezed against the right end, so the chart barely helps. **Pass, carried by text, not by the graphic.**

## Design review: Headroom

### Summary
A restrained, native-feeling menu-bar utility. System type, semantic label colors, a grouped Settings form, a standard sidebar-and-table window, and copy that always says whether anything will be deleted. The thesis comes through: "a calm sentence telling you whether Headroom will touch your disk." The one thing it will be remembered by is the free-space bar with a budget tick. That is also where most remaining problems are: the scale changes from state to state, the fill contrast is low, and the acting state crams the key information into a sliver. **Overall rating: Good.** Nothing is Critical, and there is one High finding (popover instead of menu). It isn't 9+ yet: Apple would not ship the acting state's bar, the History and Dry Run column wording, or the mismatch between the two tables.

### Critical
None. All body text measured passes 4.5:1, except system secondary label text, which is Apple's own standard (see L1). Disabled text is exempt.

### Improvements

**H1. The menu-bar extra opens a window styled as a menu instead of a real menu. High**
- **What**: Every menu state ([dry run](round-4/menu-dry-run-light.png), [plenty](round-4/menu-plenty-light.png)) is a MenuBarExtra window. Its lower half imitates menu items ("Open Headroom ⌘O", "Settings… ⌘,", "Pause Headroom", "Quit Headroom ⌘Q") with separators and right-aligned shortcuts. Rows drawn to look like menu items have to copy menu behavior exactly (hover highlight, arrow-key navigation, type-select, close on choose). The screenshots can't show whether they do. If they don't, people will notice right away.
- **Why**: `the-menu-bar.md › Menu bar extras`: "Display a menu — not a popover — when people click your menu bar extra. Unless the app functionality you want to expose is too complex for a menu, avoid presenting it in a popover." Also `windows.md › Best practices`: "don't try to replicate the system-provided appearance. Doing so without perfectly matching the system's look and behavior…"
- **Fix**: Either use a real `NSMenu` (`MenuBarExtra` with `.menuBarExtraStyle(.menu)`) with the summary header, bar and sentence as one custom view item at the top and real menu items below, or keep the window and make the rows real controls with menu-equivalent highlight and full keyboard navigation. Verify the second option with Full Keyboard Access on.

**M1. The acting state's chart hides the action. Medium**
- **What**: In [menu-acting-light.png](round-4/menu-acting-light.png) about 86% of the track is gray "Free". The teal "Freeing now" segment (≈ x 530 to 572 px at 2x, so ~21 pt) and the "Can be freed" outline sit squeezed against the right end, split by the budget tick. The legend gains a third key and "60 GB budget" wraps to its own line, unlike every other state. The status line uses `minus.circle`, gray, with no teal.
- **Why**: `charting-data.md › Designing effective charts`: "Match the size of a chart to its functionality… make it easy for people to read a chart's details", and "Prefer consistency across multiple charts, deviating only when you need to highlight differences."
- **Fix**: In the acting state, give the budget and freeing range room. For example, scale the track from 0 to budget + reclaimable, or zoom it to the last ~20 GB around the budget. Keep the legend on one line (drop "Can be freed" from the legend when it's under ~5% of the track, or shorten it to "Freeable"). Put the brand teal on the status glyph or the "Freeing" phrase so the action reads in color as well as text.

**M2. The "Free" bar fill is low contrast. Medium**
- **What**: Free fill #BABABA on the #FFFFFF panel is **1.94:1** in light. #5B5B5B on #1E1E1E is **2.45:1** in dark. This is the most important mark in the chart. The outlined "Can be freed" segment (#808080, 3.95:1) is more visible than the actual free space.
- **Why**: `accessibility.md › Vision`: "it's important that there's enough contrast between foreground text and icons and background colors." Also `color.md › Inclusive color`: "insufficient contrast can cause icons and text to blend with the background."
- **Fix**: Use a darker semantic fill (`Color.secondary` / `NSColor.secondaryLabelColor`, ≈ #808080 light, 3.95:1) or give the track its own fill (`quaternarySystemFill`) so the free segment's edge is clear. Aim for at least 3:1 for the free segment's edge. The number is also in text, so this is Medium, not Critical.

**M3. The bar's scale is unexplained and changes with every state. Medium**
- **What**: The full track equals *free + can be freed*: 90 GB in dry run, 136 GB in plenty, 66 GB in acting. It is not the disk's capacity, which is what most people assume a storage bar means. So the budget tick moves from 44% to 66% to 91% of the width while the budget stays 60 GB. Nothing on screen says what the right end means.
- **Why**: `charting-data.md › Designing effective charts`: "If you need to create a chart that presents data in a novel way, help people learn how to interpret the chart."
- **Fix**: Label the right end once (for example, "112 GB free → 136 GB if everything were freed") or add a VoiceOver label and a help tooltip on the bar. Better still, keep a stable scale (0 to max(budget × 1.5, free + reclaimable)) so the tick barely moves between states.

**M4. The acting glyph `minus.circle` reads as "remove / blocked". Medium** *(judgment)*
- **What**: In the [icons](round-4/icon-acting-light.png) and the acting status line, a circled minus marks "freeing space". On macOS, minus.circle usually means remove, Do Not Disturb or unavailable, not "work in progress". It also has the same visual weight as the idle states, so Headroom acting looks no different from Headroom waiting.
- **Why**: Judgment, informed by `the-menu-bar.md › Menu bar extras` ("Consider using a symbol to represent your menu bar extra") and `feedback.md › Best practices` ("Consider integrating status feedback into your interface").
- **Fix**: Use a symbol that shows motion or reduction (for example `arrow.down.circle`, or a gauge whose needle drops). Keep the menu-bar image a template. The status line inside the menu can be teal.

**M5. The menu-bar icon has no constant identity. Medium**
- **What**: Five unrelated circled symbols (gauge, eye, checkmark, minus, exclamation mark: see [icon-first-run](round-4/icon-first-run-light.png) … [icon-error](round-4/icon-error-light.png)). In a crowded menu bar, nothing says "this is Headroom" once the gauge from first run is gone. The state symbols also look like system status items (checkmark, exclamation mark).
- **Why**: `the-menu-bar.md › Menu bar extras`: "Avoid relying on the presence of menu bar extras… you can't… predict the location of your menu bar extra." A changing glyph makes the item harder to find again.
- **Fix**: Keep one base silhouette (for example the gauge) and show state with a small change: needle position, a badge, a slash for paused, an exclamation-mark badge for error.

**M6. Teal means two different things. Medium**
- **What**: In the menu, teal means "Headroom is freeing this now" (the bar segment). In [History](round-4/window-history-plenty-light.png), teal is the color of every "Rebuild…" button, which is a user action (#0F6E5C on white, 6.17:1, fine for contrast). The brand rule ("teal only where Headroom is acting or acted") is broken in the window: the four teal links are the loudest thing in the window.
- **Why**: `color.md › Best practices`: "Avoid using the same color to mean different things… if you use your brand color to indicate that a borderless button is interactive, using the same or similar color to stylize noninteractive text is confusing."
- **Fix**: Draw "Rebuild…" as a small bordered button (`.buttonStyle(.bordered)`, `.controlSize(.small)`) in label color, or show it on row hover, and keep teal for acting and acted states only.

**M7. Column headings don't describe the columns. Medium**
- **What**: In History, "Freed" heads a column of relative *dates*, next to a "Size" column that already holds the amount freed, so "Freed" reads as an amount. The trailing action column has no heading at all. In Dry Run, "Picked" ([window-dry-run-plenty-light.png](round-4/window-dry-run-plenty-light.png)) is internal jargon.
- **Why**: `lists-and-tables.md › Content`: "Use descriptive column headings in a multicolumn table. Use nouns or short noun phrases with title-style capitalization."
- **Fix**: "Date Freed" (or "Deleted"), "Size", and "Status" for the action column. In Dry Run use "Would Delete" / "Date Chosen", or drop the date column.

**M8. The History subtitle counts differently from the table. Medium**
- **What**: "History – Freed 6.9 GB from 3 folders · 1 rebuilt" sits above **four** rows totalling 9.0 GB. The math only works if you work out that the rebuilt Atlas row is excluded.
- **Why**: `writing.md › Getting started`: "Be clear. Choose words that are easily understood and convey the right thing."
- **Fix**: "4 folders freed (9 GB) · 1 rebuilt" or "6.9 GB still freed · 1 of 4 rebuilt".

**M9. The History and Dry Run tables use different styles. Medium**
- **What**: History uses full-width rows with edge-to-edge alternating stripes. Dry Run uses inset rows with rounded stripes inside a 10 pt margin. They are two panes of one window showing the same kind of data (folder, kind, path, size, date).
- **Why**: `lists-and-tables.md › Style`: "Choose a table or list style that coordinates with your data and platform… macOS defines a bordered style that uses alternating row backgrounds." Also `charting-data.md › Designing effective charts` on continuity across views of the same data.
- **Fix**: Use one `Table` style in both panes. `.bordered` with alternating rows, which History already almost uses, is the macOS default for multicolumn data.

**M10. The wrong part of the folder text is truncated. Medium**
- **What**: In History row 4, "Python environme…· /Users/me/code/ml" clips the *kind* label, the thing people scan for, while keeping the full path. The ellipsis then collides with the "·" separator.
- **Why**: `lists-and-tables.md › Content`: "Sometimes, an ellipsis in the middle of text can make an item easier to distinguish because it preserves both the beginning and the end of the content."
- **Fix**: Give the kind priority (`.layoutPriority(1)`) and truncate the path in the middle (`.truncationMode(.middle)`), for example "Python environment · /Users/…/code/ml".

**M11. The Dry Run pane loses the menu's story and gives no next step. Medium**
- **What**: The menu says "First to go when the dry run ends" and lists items in order. The window calls the same items "Picked", sorts them by Size, and shows no rank. In the ended state the subtitle reads "Ended. These 3 folders (8.9 GB) would have been freed." It doesn't say what happens now (will Headroom delete these the next time space runs low?) and doesn't point to the Settings toggle.
- **Why**: `writing.md › Best practices`: "Build language patterns. Consistency builds familiarity", and "Give clear guidance and use consistent language throughout processes with multiple steps."
- **Fix**: Use one term everywhere ("First to Go"). Add an "Order" column or default-sort by it. In the ended state, use a one-line banner: "Dry run is over. Headroom now frees space when you drop below 60 GB." with a "Keep Dry Run On" button.

**L1. Secondary text is 3.95:1 in light. Low (Apple standard, reported)**
- Captions, kinds, paths, dates, legend and settings footers sample at #808080 on #FFFFFF = **3.95:1** (light) and #9A9A9A on #1E1E1E = **5.92:1** (dark). This is `secondaryLabelColor`, so it adapts under Increase Contrast. `accessibility.md › Vision`: "If your app doesn't provide this minimum contrast by default, ensure it at least provides a higher contrast color scheme when the system setting Increase Contrast is turned on." Keep the semantic color. Don't hard-code gray. Disabled "Pause Headroom" is 3.07:1, which is fine because disabled controls are exempt.

**L2. Numbers in the Size columns are left-aligned. Low** *(judgment)* Right-align numbers with tabular figures (`.monospacedDigit()`) so 900 MB and 4.8 GB line up by magnitude, as the menu's list already does.

**L3. The sidebar holds only two items. Low** *(judgment)* A 180 pt sidebar with History and Dry Run and nothing else uses a lot of chrome. A segmented control in the toolbar would be lighter. The selected row's label also renders bold, which macOS sidebars don't do. Check whether that's the app or the renderer. `sidebars.md › Best practices`.

**L4. Settings footer grammar and jargon. Low** "the files you haven't used for longest" → "for the longest". "its picks appear under Dry Run" → "what it would delete appears under Dry Run". `writing.md › Getting started` ("avoiding jargon").

**L5. The Dry Run footer describes the Off state. Low** "Off: Headroom frees space for real…" `writing.md › Best practices`: "Describe what it does when turned on, and people can infer the opposite." Suggested: "While on, nothing is deleted. Headroom only lists what it would delete."

**L6. "Open…" next to Full Disk Access is vague. Low** Once access is granted, "Open…" doesn't say what opens. Use "Open Privacy Settings…" or hide the button when nothing is blocked. `writing.md › Best practices` ("Be action oriented").

**L7. General is the last Settings section. Low** *(judgment)* Login and access usually come first, or this single page should simply be titled "Headroom Settings" (`settings.md › Desktop (macOS)`: "If your settings window doesn't have multiple panes, use the title *App Name* Settings"). The title wasn't visible in the render.

**L8. The Dry Run empty-state copy is awkward. Low** "For the next 7 days, Headroom then only lists here what it would delete." → "For your first 7 days, Headroom only lists what it would delete. Nothing is removed." `writing.md › Best practices` ("Provide clear next steps on any blank screens").

**L9. The acting subtitle repeats the body. Low** "9 GB can be freed · freeing space" (lowercase fragment) duplicates "Freeing 4.8 GB of Rust build files in engine." Use "Freeing 4.8 GB…" as the subtitle and keep the sentence for the budget gap. `writing.md › Getting started` ("If you can use fewer words, do so").

**L10. Widow in the Time Machine note. Low** "…releases it within a / day." leaves one word on the last line at this width. Shorten to "macOS releases it within a day." on its own line, or "…holding some freed space for up to a day." Craft.

### Craft notes
- **Point of view**: A deliberately quiet utility, which is the right call. The signature is the free-space bar with a budget tick and the one sentence under it. Nothing is templated: system type, no gradients, no hero number on a gradient, no decorative markers. The bar's current inconsistencies (M1 to M3) blunt the one element meant to be memorable.
- **Restraint**: Teal is almost entirely confined to the acting state in the menu. The History "Rebuild…" links are the one place it leaks (M6).
- **Remove one accessory**: The "Can be freed" legend key. The outline segment explains itself once the right end of the track is labeled (M3).

### What works
- Every state answers "will anything be deleted?" in words, including error ("Nothing will be deleted until it can") and first run ("Until then, it deletes nothing").
- Buttons in the menu use title case with correct ellipses ("Set a Budget…", "Allow Full Disk Access…", "Rebuild…") and `menus.md › Labels` conventions: "Settings…" ⌘, and a dimmed "Pause Headroom" when there's nothing to pause.
- Empty states follow the ContentUnavailableView pattern: symbol, title, one sentence, one action (`writing.md › Best practices`, "Provide clear next steps on any blank screens").
- The Budget field shows its unit ("GB free") and a hint ("e.g. 60"), as `writing.md › Show hints in text fields` asks.
- The History table has sortable headers with a sort indicator and a clear "✓ Rebuilt" end state, which shows the result of undo (`undo-and-redo.md › Best practices`).
- The error message names the failure, takes no blame and gives a fallback ("If Try Again doesn't help, quit and reopen Headroom").
- Dark mode uses semantic colors throughout. Teal is lifted for dark (#3FB39A on #1E1E1E, 6.45:1). The text hierarchy holds.

### Platform notes
- On macOS there is no Dynamic Type (`typography.md › Desktop (macOS)`). The text sizes look like Body 13 pt and Subheadline/Callout 11 to 12 pt, all at or above the 10 pt minimum.
- Can't be verified from screenshots: hover and keyboard behavior of the menu rows (H1), VoiceOver labels for the bar (`charting-data.md › Best practices`, "Make every chart in your app accessible"), the Settings window title and toolbar, and Increase Contrast.

### Render limits (not scored)
- No menu material or vibrancy: the menu panel renders as flat #FFFFFF / #1E1E1E.
- The window draws as inactive: gray traffic lights, gray sidebar selection, a gray Dry Run switch in Settings, near-black sidebar icons in dark, and a dim title (#B0B0B0 on #F0F0F0, 1.9:1). By default sidebar icons would pick up the accent color.
- The title and subtitle show inline because no toolbar is rendered.
- Increase Contrast isn't applied. 28 of 32 contrast pairs are byte-identical, and the 4 that differ (window-*-plenty) differ only in relative-time text.
- Sample paths are /Users/me/… and relative times drift ("20 seconds ago" vs "23/26 seconds ago").
