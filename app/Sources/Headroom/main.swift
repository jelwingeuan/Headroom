import CHeadroom
import Foundation

// With arguments this binary is the CLI, MCP server or agent: hand argv to the Rust core before
// anything touches AppKit, so those modes never connect to the window server.
if CommandLine.arguments.count > 1 {
    let status = CommandLine.unsafeArgv.withMemoryRebound(
        to: UnsafePointer<CChar>?.self, capacity: Int(CommandLine.argc) + 1
    ) { headroom_main(CommandLine.argc, $0) }
    exit(status)
}

HeadroomApp.main()
