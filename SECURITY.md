# Security policy

## Reporting a vulnerability

Report privately via GitHub's **Report a vulnerability** button on the
[Security tab](https://github.com/jelwingeuan/Headroom/security). Please don't open a public
issue for anything that could make Headroom delete, expose or corrupt files it shouldn't.

## Scope

In scope:
- Anything that makes Headroom delete or alter a file that isn't provably rebuildable.
- Path, symlink or race tricks that redirect a deletion.
- Code execution through scanned projects (git config, hooks, planted binaries).
- MCP calls that escape their project or change settings.
- Archive restores that write outside the original path.
- Any network traffic.

The design rules are in [docs/THREAT-MODEL.md](docs/THREAT-MODEL.md).

## Supported versions

Only the latest release gets fixes.
