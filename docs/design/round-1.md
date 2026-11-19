# Design review, round 1: Headroom (blind)

Reviewer saw only the screenshots in `round-1/`. No source code and no earlier rounds were read. Contrast figures come from pixels sampled in the 2x PNGs (WCAG relative luminance). Type sizes are estimated from cap heights (SF cap height is about 0.705 em).

## Scores

| Screen | Score | Reason |
| --- | --- | --- |
| Menu: first run | 6/10 | Two clear calls to action with honest copy, but no meter, a large empty gap under the title, and two buttons of equal weight indented 5 pt off the text column |
| Menu: dry run | 7/10 | Passes the 5-second test, but the key subtitle is 3.95:1 at 13 pt, "can come back" reads as the opposite of what it means, and the meter segments differ by hue only |
| Menu: plenty | 7.5/10 | Calm and correct. The budget marker is only 2.00:1 against the fill in dark mode |
| Menu: acting | 7/10 | Says what it is doing and what comes next. No progress indicator while it frees space, and the same "can come back" wording |
| Menu: error | 4/10 | Free space disappears and leaves an empty band at the top. No cause and no fix, only Try Again |
| Menu-bar icon | 7/10 | Template images that adapt to light and dark. Dry run and plenty differ only in the needle angle. The error glyph is a different size (26×23 pt against 24×24 pt) and loses the gauge |
| History | 6/10 | A clean list with a clear undo (Rebuild…). The rebuilt row's status text is 2.10:1, sidebar icons vanish in dark mode, and folder names show build hashes |
| Dry Run | 5/10 | The key closing sentence is truncated, a 110 pt dead column sits on the right, and the timestamps have no meaning attached |
| Settings | 6/10 | A native grouped form, but "Keep free" looks like a label rather than a field, first run shows "0 GB" as a fake value, and the Dry Run footnote contradicts the switch |
| Empty states | 6/10 | Good symbol, title and body pattern. The History call to action is vague, and Dry Run has none and ignores the no-budget state |
| First run, end to end | 5/10 | The menu asks for a budget, Settings shows "0 GB" in a field that doesn't look editable, and the window's empty states never mention the missing budget or Full Disk Access |

**Findings:** Critical 1 · High 7 · Medium 12 (including 1 craft note) · Low 7

**5-second test**
- [menu-dry-run-light.png](round-1/menu-dry-run-light.png): **Pass.** "52 GB free" is the largest text, and "This is a dry run, so nothing is deleted for 4 more days" answers whether Headroom will act. It takes about 4 seconds because the reader has to reconcile "38 GB can come back" with "would free" 8.9 GB.
- [menu-acting-light.png](round-1/menu-acting-light.png): **Pass.** "57 GB free", "freeing space" and "Next: Rust build files in engine, 4.8 GB" are unambiguous. Only "9 GB can come back" invites a misreading ("is something being restored?").

**Render limits (not scored):** the light-contrast and dark-contrast variants are byte-identical to the regular looks for every menu, icon and Settings image, and the window images differ only in a timestamp. Increase Contrast was therefore not applied, so the contrast findings below could not be checked against an increased-contrast palette. Exclusions are out of scope and were not scored.

---

## Design review: Headroom

### Summary
A restrained, very system-native utility. The menu's job (show free space and say whether Headroom will act) is done well in the three healthy states, and the plain-spoken sentences ("This is a dry run, so nothing is deleted for 4 more days") are the best thing about it. It falls short of shippable on three counts: contrast on secondary and dimmed text, the error and first-run states, and a few rough edges in the window and Settings (truncation, a field with no affordance, contradictory copy). Overall rating: **Critical issues** (one Critical, seven High). Thesis: "a quiet fuel gauge for your disk". It will be remembered for the budget meter with its marker, which is also the element that needs the most work.

### Critical

- **What**: The rebuilt row in History dims its path and its status text, "✓ Rebuilt", to #B3B3B3 on #FFFFFF, **2.10:1** (dark mode: #686868 on #1E1E1E, **2.99:1**). The path text is about 10 pt. "Rebuilt" is the only thing that tells people the row's state. ([window-history-plenty-light.png](round-1/window-history-plenty-light.png), [window-history-plenty-dark.png](round-1/window-history-plenty-dark.png))
- **Why**: `accessibility.md › Vision`: text up to 17 pt needs "4.5:1", and "If your app doesn't provide this minimum contrast by default, ensure it at least provides a higher contrast color scheme when the system setting Increase Contrast is turned on." The contrast renders show no change.
- **Fix**: Don't dim the whole row. Keep the title in `labelColor` and the path in `secondaryLabelColor`, and show the state with the checkmark plus "Rebuilt" in `secondaryLabelColor` (#808080, 3.95:1, which Increase Contrast raises). Never use `tertiaryLabelColor` for status text. If you want the row to recede, lower only the size column.

### Improvements

**High: secondary text sits at 3.95:1 at 10–13 pt across the app.** Sampled light-mode values: the menu subtitle "38 GB can come back · dry run, nothing deleted" (about 13 pt), the "60 GB budget" caption and the "After the dry run, Headroom would free" caption (about 10–11 pt), the window row paths and times (about 10 pt), the pane header (about 11 pt bold), the empty-state body, the Settings footnotes (#7B7B7B on #F6F6F6, 3.92:1) and the "GB" unit (#797979 on #F2F2F2, 3.89:1). All are #808080-class on white, **3.95:1**. Dark mode passes (#9A9A9A on #1E1E1E, 5.92:1).
- **Why**: `accessibility.md › Vision` sets a minimum of 4.5:1 up to 17 pt, and lists macOS text sizes as "13 pt" default and "10 pt" minimum, so these captions sit at the floor. `color.md › Best practices`: "Make sure all your app's colors work well in light, dark, and increased contrast contexts."
- **Fix**: If these are `secondaryLabelColor`, keep it and confirm in a real Increase Contrast session that they darken (the renders couldn't show this). Promote the one line that carries the state ("dry run, nothing deleted" / "freeing space") to `labelColor`, because it is half of the 5-second answer. Raise captions from about 10 pt to 11 pt (`.caption` → `.callout` / `.subheadline`).

**High: the sidebar icons disappear in dark mode and ignore the accent color.** Dark: the History icon is #1C1C1C on #282828 (**1.16:1**), and #2E2E2E on the #424242 selection (**1.35:1**). The icons are drawn darker than the sidebar, so they read as holes. Light: #A8A8A8 on #F0F0F0 (2.09:1), gray rather than accent. ([window-history-plenty-dark.png](round-1/window-history-plenty-dark.png))
- **Why**: `sidebars.md › Best practices`: "By default, sidebar icons use your app's accent color… make sure your sidebar icons display the color people choose." `dark-mode.md › Icons and images`: "Symbols work well in both appearance modes when you use dynamic colors to tint them."
- **Fix**: Use plain `Label("History", systemImage: "clock.arrow.circlepath")` in a `List(selection:)` with `.listStyle(.sidebar)`, and remove any `foregroundStyle`/`opacity` on the image so the system can tint it with the accent color.

**High: the budget meter distinguishes its segments by hue alone.** In [menu-dry-run-light.png](round-1/menu-dry-run-light.png), "free" (#FF8D28) against "can come back" (#AAAAAA) is **1.01:1** in luminance, so to someone with protanopia or deuteranopia the two segments merge into one bar. "Can come back" against the empty track (#E6E6E6) is 1.86:1, and the track against the background is 1.25:1. There is no legend: the reader has to work out that the gray band means "reclaimable".
- **Why**: `accessibility.md › Vision`: "Convey information with more than color alone… Offer visual indicators, like distinct shapes or icons." `charting-data.md › Best practices`: "Make every chart in your app accessible… provide both accessibility labels that describe chart values and components". `charting-data.md › Designing effective charts`: "Aid comprehension by adding descriptive text to the chart."
- **Fix**: Draw "can come back" as a hatched or 40%-opacity extension of the free fill, or put a 1 pt gap between the segments, so the split survives without color. Add a two-item mini legend or a caption under the bar ("■ free ▨ can be freed"). Give the whole meter one accessibility label, for example "52 GB free of a 60 GB budget; 38 GB more can be freed".

**High: "can come back" says the opposite of what it means.** In a tool whose undo is "rebuild", "38 GB can come back" reads as "38 GB of deleted files can be restored". It actually means "38 GB more could be freed". This phrase sits on the second most-read line of every healthy menu state.
- **Why**: `writing.md › Getting started`: "Be clear. Choose words that are easily understood and convey the right thing."
- **Fix**: Use "38 GB can be freed · dry run" in dry run, "9 GB more can be freed · freeing space" while acting, and "24 GB can be freed if needed" when plenty. Use the same verb, "free", that the rest of the UI uses ("Headroom would free", "Headroom freed").

**High: the error state hides the one number the app exists to show, and never says why.** In [menu-error-light.png](round-1/menu-error-light.png), the headline is gone, leaving about 40 pt of blank band at the top. The message says "Headroom can't check your disk right now" with no cause and no fix beyond Try Again.
- **Why**: `writing.md › Best practices`: "Write clear error messages… be clear about what someone can do to fix it." `feedback.md › Best practices`: "Show people when a command can't be carried out and help them understand why." `layout.md › Visual hierarchy`: "place the most important items near the top."
- **Fix**: Keep the headline. Free space is readable from `volumeAvailableCapacityForImportantUsage` even when scanning fails, so show "52 GB free" or "Free space unknown". Name the failure ("Couldn't read ~/code: permission denied" / "The disk is busy"), and pair Try Again with the specific fix ("Allow Full Disk Access…"). Remove the empty band.

**High: the closing sentence in the Dry Run pane is truncated.** In [window-dry-run-plenty-light.png](round-1/window-dry-run-plenty-light.png), the sentence ends "…and everything it deletes appears i…". It is the one sentence explaining that the app is now live, and it's cut at a 720 pt window.
- **Why**: `lists-and-tables.md › Content`: "Keep item text succinct so row content is comfortable to read. Short, succinct text can help minimize truncation and wrapping." `layout.md › Adaptability`: "Design a layout that adapts gracefully."
- **Fix**: Let it wrap (`.lineLimit(nil)`, `.fixedSize(horizontal: false, vertical: true)`) and shorten it to "The dry run is over. Headroom now frees space, and lists each folder in History." Better still, move this into a banner above the list (a `GroupBox` or an inline info row with an `info.circle`) rather than a bold gray footer.

**High: in Settings, "Keep free" has no field affordance, and first run shows "0 GB".** In [settings-plenty-light.png](round-1/settings-plenty-light.png), "60 GB" is bare text with no bezel, no stepper and no focus ring, so nothing says it is editable. In [settings-first-run-light.png](round-1/settings-first-run-light.png), "0 GB" presents a fake value: zero is a real budget ("keep 0 GB free"), while the footnote says no amount has been chosen.
- **Why**: `writing.md › Best practices`: "Show hints in text fields… use hint or placeholder text." `settings.md › Best practices`: "Aim to provide default settings that give the best experience to the largest number of people." `layout.md › Visual hierarchy`: "Differentiate controls from content."
- **Fix**: Use `TextField("Amount", value: $gb, format: .number)` with `.textFieldStyle(.roundedBorder)` and a `Stepper` beside it, or a `Picker` of sensible budgets (20/40/60/80/120 GB, plus Custom…). On first run, leave it empty with a "60" placeholder, or preselect a recommended value with "Recommended" next to it.

**Medium: a popover that imitates a menu.** The lower half of the menu is menu items with ⌘O, ⌘, and ⌘Q shortcuts, but per the brief this is a popover. Popover rows don't get menu keyboard navigation, highlight-on-hover or the menu dismissal behavior that people expect from those glyphs.
- **Why**: `the-menu-bar.md › Menu bar extras`: "Display a menu — not a popover — when people click your menu bar extra. Unless the app functionality you want to expose is too complex for a menu, avoid presenting it in a popover."
- **Fix**: Use `MenuBarExtra { … }` with `.menuBarExtraStyle(.menu)`, or an `NSMenu` whose first item has a custom view (headline, meter, status sentence) and whose standard items follow. This is how the Wi-Fi and Battery menus work. If you keep the window style, make the rows real buttons with hover highlights, and verify that the shortcuts work while the popover is key.

**Medium: no progress while acting.** [menu-acting-light.png](round-1/menu-acting-light.png) says "freeing space" with a static arrows icon. Nothing moves, and nothing says how far along it is.
- **Why**: `loading.md › Showing progress`: "Clearly communicate that content is loading and how long it might take… use a determinate progress indicator when you know how long."
- **Fix**: Put a small determinate `ProgressView(value: freed, total: target)` next to "Next: Rust build files in engine, 4.8 GB", or animate the free segment growing toward the marker. Respect Reduce Motion.

**Medium: two numbers that don't reconcile in dry run.** "38 GB can come back" sits next to "Headroom would free" 600 MB + 6.3 GB + 2 GB = 8.9 GB. A reader has to infer that Headroom frees only enough to get back to the budget.
- **Why**: `charting-data.md › Designing effective charts`: "Descriptive text… can highlight actionable takeaways."
- **Fix**: Caption the list "To get back to 60 GB free, Headroom would free 8.9 GB".

**Medium: the Settings Dry Run footnote contradicts the switch.** In [settings-plenty-light.png](round-1/settings-plenty-light.png), the switch is off but the footnote still says "The dry run is on for your first 7 days."
- **Why**: `writing.md › Best practices`: "Keep settings labels clear and simple… Describe what it does when turned on, and people can infer the opposite."
- **Fix**: Make the footnote state-aware: "On for your first 7 days (4 days left)" or "The first-week dry run ended on 14 Nov." Or keep a timeless description: "When on, Headroom only lists what it would delete, under Dry Run."

**Medium: "Autopilot" appears only once.** "Pause Autopilot" is the only place this name appears. Settings, the menu copy and the window never call the feature Autopilot.
- **Why**: `writing.md › Best practices`: "Build language patterns. Consistency builds familiarity."
- **Fix**: Use "Pause Headroom" / "Resume Headroom" (or "Pause for 1 Hour" and "Pause Until Tomorrow" as a submenu), and have the headline subtitle reflect it ("paused").

**Medium: names people won't recognize.** "Atlas-bxjqk" is a DerivedData folder name with its hash.
- **Why**: `writing.md › Getting started`: "Choose words that are easily understood."
- **Fix**: Show "Atlas" and keep the full path in the subtitle or a help tag (`.help(path)`).

**Medium: the empty states don't lead anywhere useful on first run.** The History empty state offers "Open Settings…" when the task is to set a budget. The Dry Run empty state has no button, and says "For its first 7 days Headroom only shows what it would delete" while no budget exists and the dry run hasn't started. Neither mentions Full Disk Access. "Its picks" is vague. ([window-history-first-run-light.png](round-1/window-history-first-run-light.png), [window-dry-run-first-run-light.png](round-1/window-dry-run-first-run-light.png))
- **Why**: `writing.md › Best practices`: "Provide clear next steps on any blank screens… guide people on actions they can take, and give them a button or link to do so if possible."
- **Fix**: When no budget is set, both panes show "Set a Budget…", which is the same label the menu uses. Dry Run reads "Once you set a budget, Headroom spends 7 days only listing what it would delete. Those folders appear here."

**Medium: the first-run menu has an orphaned layout.** [menu-first-run-light.png](round-1/menu-first-run-light.png) leaves about 30 pt of empty space under "18 GB free", where the subtitle and meter would go. The two buttons start 5 pt to the right of the text they belong to (88 px against 78 px at 2x), and the two shield and gauge glyphs differ in size and baseline. Both buttons have equal weight, so neither reads as the next step.
- **Why**: `layout.md › Visual hierarchy`: "Align elements to make them easier to scan." `onboarding.md › Additional requests`: "Postpone nonessential setup flows… Provide reasonable default settings."
- **Fix**: Show the meter with no marker and the caption "No budget yet". Left-align the buttons with the text column. Make "Set a Budget…" the default (`.buttonStyle(.borderedProminent)`) and Full Disk Access secondary. Consider offering a one-click recommended budget ("Keep 60 GB Free") instead of sending people to Settings.

**Medium: the budget marker gets lost on the fill when plenty.** The marker against the free fill is #F2F2F2 on #ADADAD, **2.00:1**, in [menu-plenty-dark.png](round-1/menu-plenty-dark.png) (light: #0E0E0E on #5B5B5B, 2.84:1).
- **Why**: `accessibility.md › Vision`: "Convey information with more than color alone." The 3:1 threshold for graphics is WCAG 1.4.11, my judgment rather than HIG.
- **Fix**: Give the marker a 1 pt halo in the window background color, or extend it 3 pt above and below the bar so it reads against the track as well as the fill.

**Medium: the window's content has no title hierarchy.** Each pane opens with a gray, roughly 11 pt bold line ("Headroom freed 6.9 GB from 3 folders") that reads as a column header, not a summary. Its punctuation also differs from the Dry Run header ("…Nothing was deleted." ends with a period).
- **Why**: `layout.md › Visual hierarchy`: "Order content by relative importance." `writing.md › Best practices`: "Adopt capitalization rules… then apply them consistently."
- **Fix**: Put the pane name in the toolbar title (`.navigationTitle("History")`) and the summary in `.navigationSubtitle("Freed 6.9 GB from 3 folders")`, or set it in `.headline`/`labelColor`. Use one punctuation rule for both panes.

**Medium: the menu-bar icon says little about dry run, and changes width on error.** Dry run (below budget) and plenty differ only in needle angle at 14 pt. There's no cue that Headroom is in a dry run or about to act, and the error triangle is 26×23 pt against 24×24 pt for the others, so neighboring extras shift. ([icon-dry-run-light.png](round-1/icon-dry-run-light.png), [icon-plenty-light.png](round-1/icon-plenty-light.png), [icon-error-light.png](round-1/icon-error-light.png))
- **Why**: `the-menu-bar.md › Menu bar extras`: "Consider using a symbol to represent your menu bar extra… using it as-is or customizing it to suit your needs."
- **Fix**: Keep the gauge in every state for identity and add a badge: `gauge.with.dots.needle.33percent` plus an exclamation badge for error, a dotted outline for dry run and a slash for paused. Use one fixed 24×24 pt canvas.

**Low: timestamps in Dry Run carry no meaning.** "15 seconds ago" and "yesterday" could mean "would have been deleted then" or "last used then".
- **Fix**: Use "Would have freed 15 sec ago" in a help tag, or a column heading (`lists-and-tables.md › Content`: "Use descriptive column headings").

**Low: Dry Run leaves a 110 pt empty trailing column.** The size and time column is aligned for the History buttons, so the Dry Run rows end early (`layout.md › Visual hierarchy`: "Align elements").
- **Fix**: Share a column layout so the sizes sit flush right when there is no button.

**Low: the selected sidebar label turns semibold.** macOS sidebars show selection with the highlight, not with weight (`sidebars.md › Best practices`, my judgment from system apps).

**Low: switches show as gray when on** (#B1B1B1 light, #606060 dark, in [settings-first-run-light.png](round-1/settings-first-run-light.png)). This may be the inactive-window render. Verify in a key window that the switch uses the accent color.

**Low: rebuild isn't Undo.** Rebuild… is the undo for a deletion, but there is no evidence of Edit › Undo "Rebuild shop" (`undo-and-redo.md › Desktop (macOS)`: "Place undo and redo commands in the Edit menu and support the standard keyboard shortcuts."). This can't be verified from the screenshots.

**Low: single-word orphans in the menu.** "do." and "day." wrap onto their own lines in first run and acting (`writing.md › Getting started`: "If you can use fewer words, do so"). Trim the copy, for example "macOS releases it within a day" → "macOS frees it within a day".

**Low: Settings has no visible window title.** Check that the window title is "Headroom Settings" (`settings.md › Desktop (macOS)`: "If your settings window doesn't have multiple panes, use the title App Name Settings"). The title isn't visible in the render.

### Craft notes

**Medium: the meter is the signature, so make it deliberate.** The budget marker with its label is a good, product-specific idea: it is literally "headroom". Today it is three flat gray tones and an orange fill. Spend the craft here: a clear free/reclaimable treatment, a marker that reads in every state, and a color change of the free fill only when the marker is crossed (this already happens: orange below budget, neutral above, which is good). Everything else should stay as quiet as it is now. The quietness is the right call for a utility.

### What works

- The headline is a single large number, "52 GB free", at about 18 pt semibold. It wins the 5-second test on its own.
- The status sentences are plain, specific and honest: "8 GB below your budget. This is a dry run, so nothing is deleted for 4 more days." and "Next: Rust build files in engine, 4.8 GB." Keep this voice.
- The menu-bar icons are true template images (black plus alpha) that the system tints, and they render correctly in light and dark.
- Orange is the system orange and adapts (#FF8D28 light, #FF9230 dark). It means one thing: below budget.
- Rebuild… uses an ellipsis correctly, and the rebuilt state replaces the button inline instead of raising an alert (`feedback.md › Best practices`: "Consider integrating status feedback into your interface").
- Settings uses the native grouped form, with section headers, footnotes, a pop-up for "Leave projects alone for", and few options (`settings.md › Best practices`: "Minimize the number of settings you offer").
- Ellipses, title-case menu items, ⌘, for Settings and ⌘Q for Quit are all correct (`menus.md › Labels`).
- Middle truncation of long paths ("/Users/me/Li…/Developer/Xcode/DerivedData") follows `lists-and-tables.md › Content`.

### Platform notes

- macOS 13+ (SwiftUI `MenuBarExtra` is available). Prefer `.menuBarExtraStyle(.menu)` with a custom header view where possible. A window-style extra should at least behave like a menu: hover highlights, arrow-key navigation, and Escape to dismiss.
- Not verifiable from these renders: Increase Contrast (variants identical), Reduce Transparency, VoiceOver labels on the meter and the icon-only status glyphs, keyboard focus rings, and window title and toolbar. These need a pass on a real Mac before release.
