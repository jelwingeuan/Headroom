import XCTest

/// Runs Apple's accessibility audit (the engine behind Accessibility Inspector) on every screen of
/// the built app, each on one fixture world, and prints every issue it finds. Nothing touches the
/// real disk: HEADROOM_FIXTURE makes the app use sample data.
/// Run: TEST_RUNNER_HEADROOM_APP=<path to Headroom.app> xcodebuild test -project … -scheme HeadroomUITests
final class AccessibilityAuditTests: XCTestCase {
    private var issues: [String] = []

    private func launch(_ world: String, open: String) -> XCUIApplication {
        let path = ProcessInfo.processInfo.environment["HEADROOM_APP"] ?? "/Applications/Headroom.app"
        let app = XCUIApplication(url: URL(fileURLWithPath: path))
        app.launchEnvironment["HEADROOM_FIXTURE"] = world
        app.launchEnvironment["HEADROOM_OPEN"] = open
        app.launch()
        _ = app.windows.firstMatch.waitForExistence(timeout: 10)
        return app
    }

    private func audit(_ app: XCUIApplication, _ screen: String) {
        do {
            try app.performAccessibilityAudit { issue in
                self.issues.append("\(screen) | \(issue.auditType) | \(issue.compactDescription) | \(issue.element?.debugDescription.prefix(160) ?? "")")
                return true // record, keep going
            }
        } catch {
            issues.append("\(screen) | audit failed | \(error)")
        }
    }

    func testEveryScreen() {
        for world in ["first-run", "dry-run", "plenty", "acting", "error"] {
            let menu = launch(world, open: "menu")
            audit(menu, "menu \(world)")
            menu.terminate()
            guard world == "plenty" || world == "first-run" else { continue }
            let app = launch(world, open: "main")
            audit(app, "window history \(world)")
            app.outlines.staticTexts["Dry Run"].firstMatch.click()
            audit(app, "window dry-run \(world)")
            app.terminate()
            let settings = launch(world, open: "settings")
            sleep(1)
            audit(settings, "settings \(world)")
            settings.terminate()
        }
        print("AUDIT-COUNT \(issues.count)")
        issues.forEach { print("AUDIT-ISSUE \($0)") }
    }
}
