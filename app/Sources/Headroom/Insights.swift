import Charts
import SwiftUI

/// What the Insights pane shows, worked out from the core's replies. Pure, so the render-mode
/// self-check can hold it against the core's own numbers.
struct Insights {
    struct Sample: Identifiable {
        let at: Date, free: Int64
        /// Goes up after each gap of more than 3 hours (the Mac slept, Headroom wasn't running),
        /// so the line breaks there instead of drawing data nobody measured.
        let run: Int
        var id: Date { at }
    }

    /// One bar: a group of kinds (question 2) or a reason for keeping (question 3).
    struct Bar: Identifiable {
        let name: String
        var free: Int64 = 0, kept: Int64 = 0
        var color = Color.gray
        var id: String { name }
        var total: Int64 { free + kept }
    }

    /// Question 4 over one period.
    struct Tally {
        /// Still given back: deleted, not rebuilt since, and freed on this disk (not inside Docker's).
        var freed: Int64 = 0
        var actions = 0, rebuilt = 0
        var rebuiltBytes: Int64 = 0
        /// Every delete's size, and the part that was inside Docker's disk: with rebuiltBytes,
        /// the sum that reconciles `freed` with the rows (M7).
        var deleted: Int64 = 0, docker: Int64 = 0
        /// nil when no rebuild in the period reported how long it took.
        var rebuildSecs: Int64?
    }

    var samples: [Sample] = []
    /// Real deletes in the last 30 days: the teal points.
    var acted: [Candidate] = []
    var kinds: [Bar] = []
    var reasons: [Bar] = []
    var totalsAt: Date?
    var week = Tally(), total = Tally()
    var wouldFree: Int64 = 0
    /// history returns the newest 500 rows, so "in total" can only mean those.
    var capped = false

    init(status: Status, actions: [Candidate], samples: JSON, breakdown: JSON, now: Date = Date()) {
        let month = now - 30 * 86_400, weekAgo = now - 7 * 86_400
        var run = 0, last: Date?
        for j in samples.objects("samples") {
            guard let t = j.int("at"), let free = j.int("free") else { continue }
            let at = Date(timeIntervalSince1970: TimeInterval(t))
            guard at >= month else { continue }
            if let last, at.timeIntervalSince(last) > 3 * 3600 { run += 1 }
            self.samples.append(Sample(at: at, free: free, run: run))
            last = at
        }

        var groups: [String: Bar] = [:]
        for (kind, v) in breakdown.object("kinds") ?? [:] {
            let v = v as? JSON ?? [:], k = Kind(kind), name = k.group
            groups[name, default: Bar(name: name, color: k.color)].free += v.int("free_bytes") ?? 0
            groups[name, default: Bar(name: name)].kept += v.int("protected_bytes") ?? 0
        }
        var kept = Dictionary(uniqueKeysWithValues: Self.reasonNames.map {
            ($0.key, Bar(name: $0.name, kept: status.protectedByReason[$0.key] ?? 0))
        })
        let bySize = { (a: Bar, b: Bar) in (a.total, b.name) > (b.total, a.name) }
        kinds = groups.values.filter { $0.total > 0 }.sorted(by: bySize)
        reasons = kept.values.sorted(by: bySize)
        totalsAt = breakdown.int("totals_at").map { Date(timeIntervalSince1970: TimeInterval($0)) } ?? status.totalsAt

        let real = actions.filter { !$0.dryRun }
        acted = real.filter { $0.when >= (self.samples.first?.at ?? month) }
        for a in real {
            Self.count(a, into: &total)
            if a.when >= weekAgo { Self.count(a, into: &week) }
        }
        wouldFree = actions.filter(\.dryRun).reduce(0) { $0 + $1.bytes }
        capped = actions.count >= 500
    }

    /// Rises of 10 GB or more within an hour with no Headroom delete in it: space someone else
    /// freed. Labelled on the chart so a jump never goes unexplained.
    var jumps: [(at: Date, bytes: Int64)] {
        zip(samples, samples.dropFirst()).compactMap { a, b in
            let up = b.free - a.free
            guard a.run == b.run, up >= 10_000_000_000,
                  !acted.contains(where: { $0.when > a.at && $0.when <= b.at }) else { return nil }
            return (b.at, up)
        }
    }

    /// Free space at `t` on the drawn line, to put an action's point on it.
    func free(at t: Date) -> Int64? {
        guard let i = samples.firstIndex(where: { $0.at >= t }) else { return samples.last?.free }
        guard i > 0 else { return samples[0].free }
        let a = samples[i - 1], b = samples[i]
        return a.free + Int64(Double(b.free - a.free) * t.timeIntervalSince(a.at) / b.at.timeIntervalSince(a.at))
    }

    private static func count(_ a: Candidate, into t: inout Tally) {
        t.actions += 1
        t.deleted += a.bytes
        if !a.freesHost { t.docker += a.bytes }
        if a.undone {
            t.rebuilt += 1
            if a.freesHost { t.rebuiltBytes += a.bytes }
            if let s = a.rebuildSecs { t.rebuildSecs = (t.rebuildSecs ?? 0) + s }
        } else if a.freesHost {
            t.freed += a.bytes
        }
    }

    /// status's protected_by_reason buckets, with a bar name and the phrase the answer uses.
    static let reasonNames = [
        (key: "recent", name: "Opened recently", phrase: "projects you opened recently"),
        (key: "in_use", name: "In use", phrase: "folders in use right now"),
        (key: "history", name: "Conversation history", phrase: "AI conversation history"),
        (key: "other", name: "Other rules", phrase: "folders other safety rules keep"),
    ]

}

private func gb(_ b: Int64) -> Double { Double(b) / 1e9 }
private func when(_ d: Date?) -> String { d?.formatted(date: .abbreviated, time: .shortened) ?? "an unknown time" }
private let sizeRule = "Sizes are private allocated bytes: the disk blocks only that folder uses, with APFS clones counted once and nothing counted twice."

struct InsightsView: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        ScrollView { InsightsContent().padding(20) }
            .navigationTitle("Insights")
            .onAppear { model.loadInsights() }
    }
}

/// Five questions, one section each, at most one chart per section.
struct InsightsContent: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.colorSchemeContrast) private var contrast

    var body: some View {
        let s = model.status
        let i = Insights(status: s, actions: model.actions, samples: model.samples, breakdown: model.breakdown)
        let measured = "Measured by Headroom's check at \(when(i.totalsAt))."
        // Protected is one neutral fill in every bar, never a pale tint of the group's color: it
        // keeps its contrast in dark mode, and color then only says which group a bar is.
        let protectedStyle = Color.primary.opacity(contrast == .increased ? 0.45 : 0.3)
        VStack(alignment: .leading, spacing: 20) {
            Question("Am I OK?", okAnswer(s), info: s.freeBytes == nil ? nil
                : "Free space now comes from macOS, read just now; the budget is the one in Settings. "
                + "Each point on the line is the free space an hourly check saw as it started, before it freed anything. "
                + "Hours when the Mac slept or Headroom wasn't running have no point, so the line breaks there. "
                + "Teal points are deletes from History, at the time of each delete. Latest check: \(when(i.samples.last?.at)).") {
                if let error = model.samples.error {
                    Unavailable(text: error)
                } else if i.samples.isEmpty {
                    Unavailable(text: "Headroom notes free space once an hour while it runs. The chart starts with its first check.")
                } else {
                    FreeSpaceChart(insights: i, budget: s.budgetBytes ?? 0)
                    HStack(spacing: 14) {
                        Key(text: "Free space, hourly") { Capsule().fill(Color.primary.opacity(0.7)).frame(width: 14, height: 2) }
                        if (s.budgetBytes ?? 0) > 0 {
                            Key(text: "Budget") {
                                Path { $0.addLines([CGPoint(x: 0, y: 1), CGPoint(x: 14, y: 1)]) }
                                    .stroke(.secondary, style: StrokeStyle(lineWidth: 1, dash: [4, 3])).frame(width: 14, height: 2)
                            }
                        }
                        if !i.acted.isEmpty {
                            Key(text: "Headroom deleted a folder") { Circle().fill(Color.headroom).frame(width: 8, height: 8) }
                        }
                    }
                }
            }

            let bars = i.kinds.isEmpty ? nil : i.kinds
            Question("What's taking your space?", bars.map { _ in spaceAnswer(i) },
                     info: "\(sizeRule) \(measured) Can be freed: autopilot may delete it when space runs low. "
                        + "Protected: kept, for the reason shown. Docker isn't here: its space stays inside Docker's own disk.") {
                if let error = model.breakdown.error {
                    Unavailable(text: error)
                } else if let bars {
                    BarsChart(title: "Space by kind", bars: bars, keptStyle: protectedStyle)
                    HStack(spacing: 14) {
                        Key(text: "Can be freed, in its group's color") {
                            RoundedRectangle(cornerRadius: 2).fill(LinearGradient(colors: [.green, .orange, .blue], startPoint: .leading, endPoint: .trailing))
                                .frame(width: 10, height: 10)
                        }
                        Key(text: "Protected") { RoundedRectangle(cornerRadius: 2).fill(protectedStyle).frame(width: 10, height: 10) }
                        Spacer()
                        Button("Show Folders in Overview") { model.pane = .overview }
                    }
                } else {
                    Unavailable(text: "Headroom measures this during its checks, at most a day apart. The first one hasn't finished yet.")
                }
            }

            let kept = s.protectedBytes.flatMap { $0 > 0 ? i.reasons : nil }
            Question("Why didn't it clean these?", kept.map { _ in keptAnswer(i, s) },
                     info: "Protected space by the rule that keeps it; a folder kept for several reasons counts once. \(sizeRule) \(measured)") {
                if let kept {
                    BarsChart(title: "Protected space by reason", bars: kept.map { var b = $0; b.color = .secondary; return b },
                              keptStyle: Color.secondary)
                    Button("Show Protected Folders in Overview") { model.pane = .overview }
                } else if s.protectedBytes == 0 {
                    Unavailable(text: "Nothing it found is being kept back.")
                } else {
                    Unavailable(text: model.breakdown.error ?? "Headroom lists what it keeps, and why, after its first check.")
                }
            }

            Question("What did it do for you?", doneAnswer(i, s)) {
                if i.total.actions > 0 { Tallies(insights: i) }
            }

        }
        .frame(maxWidth: 760, alignment: .leading)
        .frame(maxWidth: .infinity)
    }

    private func okAnswer(_ s: Status) -> String {
        guard let free = s.freeBytes, let budget = s.budgetBytes else {
            return s.error == nil ? "Measuring free space…" : "Headroom couldn't read your disk, so it deletes nothing for now."
        }
        if budget == 0 { return "\(bytes(free)) free. No budget is set, so Headroom isn't keeping any space free yet." }
        return free >= budget ? "Yes. \(bytes(free)) free, \(bytes(free - budget)) more than your \(bytes(budget)) budget."
            : "Below budget. \(bytes(free)) free, \(bytes(budget - free)) short of your \(bytes(budget)) budget."
    }

    private func spaceAnswer(_ i: Insights) -> String {
        let free = i.kinds.reduce(0) { $0 + $1.free }, kept = i.kinds.reduce(0) { $0 + $1.kept }
        return "\(bytes(free)) can be freed and \(bytes(kept)) is protected. The largest group is \(i.kinds[0].name), at \(bytes(i.kinds[0].total))."
    }

    private func keptAnswer(_ i: Insights, _ s: Status) -> String {
        let top = i.reasons[0]
        let phrase = Insights.reasonNames.first { $0.name == top.name }?.phrase ?? top.name
        return "\(bytes(s.protectedBytes ?? 0)) is kept safe. The largest share, \(bytes(top.kept)), is \(phrase)."
    }

    private func doneAnswer(_ i: Insights, _ s: Status) -> String {
        if i.total.actions == 0 {
            if s.dryRun && i.wouldFree > 0 {
                return "Nothing deleted yet: in the dry run Headroom only lists what it would delete, \(bytes(i.wouldFree)) so far."
            }
            return s.budgetBytes == 0 ? "Nothing deleted. Headroom deletes nothing until you set a budget."
                : "Nothing deleted yet. Headroom deletes only when free space drops below your budget."
        }
        let given = { (b: Int64) in b > 0 ? bytes(b) : "nothing" }
        return "Headroom gave back \(given(i.week.freed)) this week, and \(given(i.total.freed)) in \(i.capped ? "its last 500 actions" : "total")."
    }
}

/// One question: its title, a one-sentence answer, then the evidence.
struct Question<Content: View>: View {
    let title: String
    let answer: String?
    /// How the section's figures were measured and when: the ⓘ beside the answer.
    let info: String?
    let content: Content

    init(_ title: String, _ answer: String?, info: String? = nil, @ViewBuilder content: () -> Content) {
        self.title = title
        self.answer = answer
        self.info = info
        self.content = content()
    }

    var body: some View {
        GroupBox {
            VStack(alignment: .leading, spacing: 12) {
                if let answer {
                    HStack(alignment: .firstTextBaseline) {
                        Text(answer).font(.title3).fixedSize(horizontal: false, vertical: true)
                        Spacer(minLength: 8)
                        if let info { Info(text: info) }
                    }
                }
                content
            }
            .padding(8)
            .frame(maxWidth: .infinity, alignment: .leading)
        } label: {
            Text(title).font(.headline).padding(.bottom, 2)
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel(title)
    }
}

struct Unavailable: View {
    let text: String
    var body: some View { Text(text).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true) }
}

/// A legend entry: a small sample of the mark, then its name.
struct Key<Mark: View>: View {
    let text: String
    @ViewBuilder let mark: Mark
    var body: some View {
        HStack(spacing: 5) { mark; Text(text).foregroundStyle(.secondary) }.font(.subheadline)
    }
}

/// ⓘ: how a figure was measured and when. A tooltip on hover; a popover on click, so keyboard
/// and VoiceOver users reach it too.
struct Info: View {
    let text: String
    @State private var shown = false

    var body: some View {
        Button { shown.toggle() } label: { Image(systemName: "info.circle") }
            .buttonStyle(.borderless)
            .foregroundStyle(.secondary)
            .help(text)
            .accessibilityLabel("How this is measured")
            .popover(isPresented: $shown, arrowEdge: .bottom) {
                Text(text).padding(12).frame(width: 300).fixedSize(horizontal: false, vertical: true)
            }
    }
}

/// Charts arrive once: marks rise from zero on a critically damped spring (no overshoot), and
/// later data moves to its new place the same way. Nothing loops. With Reduce Motion the chart
/// crossfades in with its values already in place.
struct Appear<Key: Equatable, Content: View>: View {
    let key: Key
    @ViewBuilder let content: (Bool) -> Content
    @State private var shown = Screens.rendering
    @Environment(\.accessibilityReduceMotion) private var reduce

    var body: some View {
        content(shown || reduce)
            .opacity(shown ? 1 : 0)
            .animation(reduce ? nil : .spring(response: 0.5, dampingFraction: 1), value: key)
            .onAppear {
                withAnimation(reduce ? .easeInOut(duration: 0.25) : .spring(response: 0.5, dampingFraction: 1)) { shown = true }
            }
    }
}

/// Question 1: free space over the last 30 days, on a zero-based scale, with the budget as a
/// labelled line and a teal point at each delete.
struct FreeSpaceChart: View, AXChartDescriptorRepresentable {
    let insights: Insights
    let budget: Int64

    private var top: Double { gb(max(insights.samples.map(\.free).max() ?? 0, budget)) * 1.15 }
    private var span: ClosedRange<Date> { (insights.samples.first?.at ?? Date())...(insights.samples.last?.at ?? Date()) }

    /// Midnights, a week apart (a day apart under 10 days), none so near an end that its label clips.
    private var days: [Date] {
        let length = span.upperBound.timeIntervalSince(span.lowerBound), cal = Calendar.current
        var day = cal.startOfDay(for: span.upperBound), out: [Date] = []
        while day > span.lowerBound {
            if min(day.timeIntervalSince(span.lowerBound), span.upperBound.timeIntervalSince(day)) > length * 0.08 { out.append(day) }
            day = cal.date(byAdding: .day, value: length > 10 * 86_400 ? -7 : -1, to: day) ?? span.lowerBound
        }
        return out
    }

    var body: some View {
        Appear(key: insights.samples.count + insights.acted.count) { grow in
            Chart {
                ForEach(insights.samples) { s in
                    LineMark(x: .value("Time", s.at), y: .value("Free", grow ? gb(s.free) : 0), series: .value("Run", s.run))
                        .foregroundStyle(Color.primary.opacity(0.7))
                        .lineStyle(StrokeStyle(lineWidth: 1.5))
                }
                if budget > 0 {
                    RuleMark(y: .value("Budget", gb(budget)))
                        .foregroundStyle(Color.secondary)
                        .lineStyle(StrokeStyle(lineWidth: 1, dash: [4, 3]))
                }
                // M8: nothing on the chart goes unexplained. Jumps Headroom didn't make, the breaks
                // where the Mac slept, and where the line ends today all carry a label.
                let jumps = insights.jumps, end = insights.samples.last
                ForEach(jumps.filter { !near($0.at, end?.at) }, id: \.at) { j in
                    PointMark(x: .value("Time", j.at), y: .value("Free", grow ? gb(insights.free(at: j.at) ?? 0) : 0))
                        .symbol(.circle).symbolSize(30).foregroundStyle(Color.primary)
                        .annotation(position: .top, alignment: .trailing) { Text("You freed \(bytes(j.bytes))").font(.caption) }
                }
                ForEach(gaps, id: \.self) { t in
                    RuleMark(x: .value("Time", t)).foregroundStyle(Color.secondary.opacity(0.4))
                        .lineStyle(StrokeStyle(lineWidth: 1, dash: [2, 3]))
                        .annotation(position: .top) { Text("Mac asleep").font(.caption).foregroundStyle(.secondary) }
                }
                if let end {
                    let jump = jumps.first { near($0.at, end.at) }.map { "You freed \(bytes($0.bytes)) · " } ?? ""
                    PointMark(x: .value("Time", end.at), y: .value("Free", grow ? gb(end.free) : 0))
                        .symbolSize(30).foregroundStyle(Color.primary)
                        .annotation(position: .top, alignment: .trailing) { Text("\(jump)Now \(bytes(end.free))").font(.caption).monospacedDigit() }
                }
                ForEach(insights.acted) { a in
                    PointMark(x: .value("Time", a.when), y: .value("Free", grow ? gb(insights.free(at: a.when) ?? 0) : 0))
                        .foregroundStyle(Color.headroom)
                        .symbolSize(44)
                }
            }
            .chartXScale(domain: span)
            .chartXAxis {
                AxisMarks(values: days) { v in
                    AxisGridLine()
                    AxisValueLabel { if let d = v.as(Date.self) { Text(d.formatted(.dateTime.month(.abbreviated).day().locale(Locale(identifier: "en")))) } }
                }
            }
            .chartYScale(domain: 0...max(top, 1))
            .chartYAxis {
                AxisMarks { v in
                    AxisGridLine()
                    AxisValueLabel { if let g = v.as(Double.self) { Text("\(Int(g)) GB").monospacedDigit() } }
                }
                if budget > 0 { // the budget's label sits in the margin, so it never covers the line
                    AxisMarks(position: .leading, values: [gb(budget)]) { _ in
                        AxisValueLabel { Text("\(bytes(budget)) budget").monospacedDigit() }
                    }
                }
            }
            .frame(height: 200)
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Free space over time")
        .accessibilityValue(summary)
        .accessibilityChartDescriptor(self)
    }

    /// Close enough to the end that two labels would collide: they become one.
    private func near(_ a: Date, _ b: Date?) -> Bool { b.map { abs(a.timeIntervalSince($0)) < 12 * 3600 } ?? false }

    /// The middle of each break in the line.
    private var gaps: [Date] {
        zip(insights.samples, insights.samples.dropFirst()).filter { $0.run != $1.run }
            .map { $0.at.addingTimeInterval($1.at.timeIntervalSince($0.at) / 2) }
    }

    private var summary: String {
        guard let now = insights.samples.last, let low = insights.samples.min(by: { $0.free < $1.free }) else { return "No data" }
        return "\(bytes(now.free)) free at the latest check. Lowest \(bytes(low.free)), \(low.at.formatted(date: .abbreviated, time: .omitted))."
            + (budget > 0 ? " Budget \(bytes(budget))." : "")
            + " Headroom deleted \(insights.acted.count) \(insights.acted.count == 1 ? "folder" : "folders") in this time."
    }

    func makeChartDescriptor() -> AXChartDescriptor {
        let t = { (d: Date) in d.timeIntervalSince1970 }
        let x = AXNumericDataAxisDescriptor(title: "Time", range: t(span.lowerBound)...t(span.upperBound), gridlinePositions: []) {
            Date(timeIntervalSince1970: $0).formatted(date: .abbreviated, time: .shortened)
        }
        let y = AXNumericDataAxisDescriptor(title: "Free space", range: 0...max(top, 1),
                                            gridlinePositions: budget > 0 ? [gb(budget)] : []) { bytes(Int64($0 * 1e9)) }
        let line = AXDataSeriesDescriptor(name: "Free space", isContinuous: true,
                                          dataPoints: insights.samples.map { AXDataPoint(x: t($0.at), y: gb($0.free)) })
        let deletes = AXDataSeriesDescriptor(name: "Headroom deleted a folder", isContinuous: false, dataPoints: insights.acted.map {
            AXDataPoint(x: t($0.when), y: gb(insights.free(at: $0.when) ?? 0), additionalValues: [], label: "\($0.name), \(bytes($0.bytes))")
        })
        return AXChartDescriptor(title: "Free space over time", summary: summary, xAxis: x, yAxis: y, additionalAxes: [],
                                 series: [line, deletes])
    }
}

/// One horizontal bar per group, largest first: can be freed in the group's color, protected in
/// one neutral gray, and the split written beside it.
struct BarsChart: View, AXChartDescriptorRepresentable {
    let title: String
    let bars: [Insights.Bar]
    let keptStyle: Color

    private var top: Double { gb(bars.map(\.total).max() ?? 0) * 1.6 } // room for the words

    var body: some View {
        Appear(key: bars.map(\.total)) { grow in
            Chart {
                ForEach(bars) { b in
                    BarMark(x: .value("Size", grow ? gb(b.free) : 0), y: .value("Group", b.name), height: .ratio(0.6))
                        .foregroundStyle(b.color)
                    BarMark(x: .value("Size", grow ? gb(b.kept) : 0), y: .value("Group", b.name), height: .ratio(0.6))
                        .foregroundStyle(keptStyle)
                        .annotation(position: .trailing) { // the split in words, so it never rests on color
                            Text(keptStyle == .secondary ? bytes(b.total) : "\(bytes(b.free)) of \(bytes(b.total)) can be freed")
                                .font(.caption).monospacedDigit()
                        }
                }
            }
            .chartXScale(domain: 0...max(top, 1))
            .chartXAxis(.hidden)
            .chartYAxis {
                AxisMarks(position: .leading) { v in
                    AxisValueLabel { Text(v.as(String.self) ?? "").foregroundStyle(.primary) }
                }
            }
            .frame(height: CGFloat(bars.count) * 30)
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(title)
        .accessibilityValue(summary)
        .accessibilityChartDescriptor(self)
    }

    private var summary: String { bars.map { "\($0.name) \(bytes($0.total))" }.joined(separator: ", ") }

    func makeChartDescriptor() -> AXChartDescriptor {
        let x = AXCategoricalDataAxisDescriptor(title: "Group", categoryOrder: bars.map(\.name))
        let y = AXNumericDataAxisDescriptor(title: "Size", range: 0...max(top, 1), gridlinePositions: []) { bytes(Int64($0 * 1e9)) }
        let series = [("Can be freed", \Insights.Bar.free), ("Protected", \Insights.Bar.kept)]
            .filter { _, part in bars.contains { $0[keyPath: part] > 0 } }
            .map { name, part in
                AXDataSeriesDescriptor(name: name, isContinuous: false, dataPoints: bars.map { AXDataPoint(x: $0.name, y: gb($0[keyPath: part])) })
            }
        return AXChartDescriptor(title: title, summary: summary, xAxis: x, yAxis: y, additionalAxes: [], series: series)
    }
}

/// Question 4 as a small table: this week against everything History still holds.
struct Tallies: View {
    let insights: Insights

    var body: some View {
        let w = insights.week, t = insights.total
        let period = insights.capped ? "the last 500 actions (History keeps no more)" : "all of History"
        VStack(alignment: .leading) {
        Grid(alignment: .leading, horizontalSpacing: 28, verticalSpacing: 8) {
            GridRow {
                Color.clear.frame(width: 1, height: 1)
                Text("This week").gridColumnAlignment(.trailing)
                Text(insights.capped ? "Last 500 actions" : "In total").gridColumnAlignment(.trailing)
            }
            .font(.subheadline).foregroundStyle(.secondary)
            Divider()
            row("Space given back", w.freed > 0 ? bytes(w.freed) : "None", t.freed > 0 ? bytes(t.freed) : "None",
                info: "The size of each deleted folder, measured right before the delete. \(sizeRule) "
                    + "Folders you rebuilt since and Docker data (it stays inside Docker's disk) aren't counted. "
                    + "From History, by the time of each delete; the second column covers \(period).")
            row("Folders deleted", "\(w.actions)", "\(t.actions)",
                info: "Every delete in History in that period, including folders you rebuilt later and Docker cleanups.")
            row("Rebuilt, and time it took", rebuilt(w), rebuilt(t),
                info: "Folders brought back with Rebuild, and how long those rebuilds ran, timed while they ran.")
        }
        .monospacedDigit()
        if t.rebuiltBytes + t.docker > 0 {
            Text("Of \(bytes(t.deleted)) deleted, \(bytes(t.rebuiltBytes)) was rebuilt since and \(bytes(t.docker)) was inside Docker's disk, "
                + "so \(bytes(t.freed)) is still free on this Mac.")
                .font(.subheadline).fixedSize(horizontal: false, vertical: true).padding(.top, 8)
        }
        }
    }

    private func row(_ label: String, _ week: String, _ total: String, info: String) -> some View {
        GridRow {
            HStack(spacing: 4) { Text(label); Info(text: info) }
            Text(week)
            Text(total)
        }
        .accessibilityElement(children: .combine)
    }

    private func rebuilt(_ t: Insights.Tally) -> String {
        guard t.rebuilt > 0 else { return "None" }
        let time = t.rebuildSecs.map {
            Duration.seconds($0).formatted(.units(allowed: [.hours, .minutes, .seconds], width: .abbreviated, maximumUnitCount: 2).locale(Locale(identifier: "en")))
        }
        return "\(t.rebuilt) · \(time ?? "time not recorded")"
    }
}

/// Question 6: what Headroom is and the rules it keeps, in plain words. No figures.
struct About: View {
    let recentDays: Int

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Headroom keeps the free space you chose on this Mac. When free space drops below your budget, it deletes folders your tools can rebuild, such as dependencies, build output and downloaded AI models, starting with the ones unused the longest. Every delete is listed in History, where Rebuild brings it back.")
                .fixedSize(horizontal: false, vertical: true)
            VStack(alignment: .leading, spacing: 8) {
                rule("checkmark.seal", "Proof first: a folder counts only when its project file sits beside it, like node_modules beside package.json.")
                rule("arrow.triangle.branch", "Git has the last word: if git tracks any file inside a folder, the folder stays.")
                rule("clock", "Projects you opened in the last \(recentDays) days are left alone.")
                rule("hand.raised", "Nothing in use is touched: not a project a program is working in, nor an app's cache while the app runs.")
                rule("arrow.uturn.backward", "Undo means rebuild: Headroom runs the project's own install or build command again. The one exception is an iOS simulator unused for 30 days, which comes back blank.")
                rule("eye", "For its first 7 days it only lists what it would delete, in History.")
            }
            Text("It never touches your documents or source files, anything git tracks, AI conversation history, Xcode archives or Docker volumes. It has no account and no telemetry, and never goes online itself; only Rebuild runs your package managers, which may download.")
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
        }
    }

    private func rule(_ symbol: String, _ text: String) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Image(systemName: symbol).foregroundStyle(.secondary).frame(width: 20)
            Text(text).fixedSize(horizontal: false, vertical: true)
        }
    }
}
