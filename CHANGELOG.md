# Changelog

All notable changes to Headroom are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
- Menu-bar app with a free-space gauge, History with Undo, a dry-run report and Settings.
- Autopilot that keeps a free-space budget, dry-run for its first 7 days.
- Reclaims project dependencies and build output by proof-file rules, Xcode DerivedData and
  DeviceSupport, simulators unused for 30 days, old AI-tool versions, AI desktop app caches while
  the app is closed, Docker images and build cache, registry-sourced local models, and merged
  agent worktrees.
- APFS clone dedupe of identical files across copies of the same `node_modules`.
- The same binary works as a CLI and an MCP server.
- An outside judge and an independent home-folder census that score every change.
