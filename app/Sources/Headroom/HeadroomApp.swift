import SwiftUI

struct HeadroomApp: App {
    @StateObject private var model = AppModel()

    var body: some Scene {
        MenuBarExtra {
            MenuView().environmentObject(model)
        } label: {
            MenuBarIcon(status: model.status, paused: model.paused)
        }
        .menuBarExtraStyle(.window)

        Window("Headroom", id: "main") {
            MainView().environmentObject(model)
        }
        .defaultSize(width: 720, height: 480)
    }
}

struct MenuBarIcon: View {
    let status: Status
    let paused: Bool

    var body: some View {
        Image(systemName: symbol).accessibilityLabel("Headroom")
    }

    private var symbol: String {
        if paused { return "pause.circle" }
        if let free = status.freeBytes, let budget = status.budgetBytes, budget > 0, free < budget { return "gauge.low" }
        return "gauge.medium"
    }
}

/// The one signature element: a slim bar where the budget sits in the middle and the colored
/// stretch is the gap between free space and that budget. Twice the budget fills the bar.
struct HeadroomGauge: View {
    let free: Int64
    let budget: Int64
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        let fill = min(Double(free) / Double(max(budget, 1) * 2), 1)
        let above = free >= budget
        GeometryReader { geo in
            let w = geo.size.width
            ZStack(alignment: .leading) {
                Rectangle().fill(.quaternary)
                Rectangle().fill(.secondary).frame(width: w * min(fill, 0.5))
                Rectangle()
                    .fill(above ? Color.accentColor : Color.orange)
                    .frame(width: w * abs(fill - 0.5))
                    .offset(x: w * min(fill, 0.5))
            }
            .clipShape(Capsule())
            .overlay(alignment: .leading) {
                RoundedRectangle(cornerRadius: 1)
                    .fill(.primary)
                    .frame(width: 2, height: 12)
                    .offset(x: w * 0.5 - 1)
            }
        }
        .frame(height: 6)
        .padding(.vertical, 3)
        .animation(reduceMotion ? nil : .spring(response: 0.35, dampingFraction: 1), value: fill)
        .accessibilityElement()
        .accessibilityLabel("Headroom")
        .accessibilityValue(gapText(free: free, budget: budget))
    }
}

func gapText(free: Int64, budget: Int64) -> String {
    free >= budget
        ? "\(bytes(free - budget)) above your \(bytes(budget)) budget"
        : "\(bytes(budget - free)) below your \(bytes(budget)) budget"
}

struct MenuView: View {
    @EnvironmentObject private var model: AppModel
    @Environment(\.openWindow) private var openWindow

    var body: some View {
        let s = model.status
        VStack(alignment: .leading, spacing: 12) {
            if s.budgetBytes == 0 {
                VStack(alignment: .leading, spacing: 6) {
                    if let free = s.freeBytes {
                        Text("\(bytes(free)) free").font(.title3.weight(.semibold)).monospacedDigit()
                    }
                    Text("Choose how much space to keep free. Headroom won't delete anything until you do.")
                        .fixedSize(horizontal: false, vertical: true)
                    Button("Set Budget") { open(.settings) }
                }
            } else if let free = s.freeBytes, let budget = s.budgetBytes {
                VStack(alignment: .leading, spacing: 6) {
                    Text("\(bytes(free)) free").font(.title3.weight(.semibold)).monospacedDigit()
                    HeadroomGauge(free: free, budget: budget)
                    Text(gapText(free: free, budget: budget))
                        .font(.callout).foregroundStyle(.secondary).monospacedDigit()
                }
            } else if s.error == nil {
                Text("Measuring free space…").foregroundStyle(.secondary)
            }

            if let error = s.error {
                Label(error, systemImage: "exclamationmark.triangle").foregroundStyle(.orange)
            }

            if s.dryRun {
                Button { open(.wouldHaveDone) } label: {
                    Label(dryRunText(s), systemImage: "eye")
                        .fixedSize(horizontal: false, vertical: true)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
                .buttonStyle(.plain)
                .padding(8)
                .background(Color.accentColor.opacity(0.12), in: RoundedRectangle(cornerRadius: 6))
            }

            if model.paused {
                Label("Autopilot is paused. Nothing will be deleted.", systemImage: "pause.circle")
                    .foregroundStyle(.secondary)
            } else if let next = s.next {
                Label("Next: \(next.kind) in \(next.place), \(bytes(next.bytes))",
                      systemImage: "arrow.forward.circle")
                    .lineLimit(2)
            } else if let budget = s.budgetBytes, budget > 0, s.freeBytes ?? 0 >= budget {
                Label("Nothing planned. You're above budget.", systemImage: "checkmark.circle")
                    .foregroundStyle(.secondary)
            }

            if s.snapshotHolding {
                Label("A local Time Machine snapshot is holding some freed space until macOS removes it.",
                      systemImage: "clock.badge.exclamationmark")
                    .fixedSize(horizontal: false, vertical: true)
                    .foregroundStyle(.secondary)
            }

            if s.needsFullDiskAccess {
                FullDiskAccessRow()
            }

            Divider()
            HStack {
                Button("Open Headroom") { open(.history) }
                    .keyboardShortcut(.defaultAction)
                Button(model.paused ? "Resume Autopilot" : "Pause Autopilot") { model.paused.toggle() }
                Spacer()
                Button("Quit") { NSApp.terminate(nil) }
                    .keyboardShortcut("q")
            }
        }
        .padding(14)
        .frame(width: 320)
        .onAppear { model.refresh() }
    }

    private func dryRunText(_ s: Status) -> String {
        if let days = s.dryRunDaysLeft, days > 0 {
            return "Dry run: \(days) \(days == 1 ? "day" : "days") left — here's what I would have done"
        }
        return "Dry run is on — here's what I would have done"
    }

    private func open(_ pane: Pane) {
        model.pane = pane
        openWindow(id: "main")
        NSApp.activate(ignoringOtherApps: true)
    }
}

struct FullDiskAccessRow: View {
    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Label("Headroom can't read some folders. Give it Full Disk Access to measure them.",
                  systemImage: "lock.shield")
                .fixedSize(horizontal: false, vertical: true)
            Button("Open Full Disk Access Settings") { openFullDiskAccessSettings() }
        }
    }
}
