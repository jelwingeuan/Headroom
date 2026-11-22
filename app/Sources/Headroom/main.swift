import CHeadroom
import Foundation

// With arguments this binary is the CLI, MCP server or agent: hand argv to the Rust core before
// anything touches AppKit, so those modes never connect to the window server.
// AppKit-style flags (-NSDocumentRevisionsDebugMode, -ApplePersistenceIgnoreState…) that launchers
// and UI tests add are not CLI commands.
if CommandLine.arguments.count > 1, !["-NS", "-Apple", "-XC"].contains(where: CommandLine.arguments[1].hasPrefix) {
    let status = CommandLine.unsafeArgv.withMemoryRebound(
        to: UnsafePointer<CChar>?.self, capacity: Int(CommandLine.argc) + 1
    ) { headroom_main(CommandLine.argc, $0) }
    exit(status)
}

// Developer-only: HEADROOM_RENDER=<folder> draws every screen from fixtures into PNGs and exits.
if let out = ProcessInfo.processInfo.environment["HEADROOM_RENDER"] {
    Screens.render(to: URL(fileURLWithPath: out))
    exit(0)
}

// Developer-only: HEADROOM_FIXTURE=<world> runs the real app on one screenshot world's sample data,
// so UI tests and Instruments see a known state and nothing touches the real disk.
if let world = ProcessInfo.processInfo.environment["HEADROOM_FIXTURE"] {
    Core.fixture = Screens.fixture(world)
}

HeadroomApp.main()
