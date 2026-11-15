# Headroom

Your Mac never drops below the free space you chose, and you never lose work getting there.

Headroom is a free, open-source menu-bar app for macOS 13+ (Apple silicon and Intel). You set a
budget, like "keep 60 GB free". When free space nears it, Headroom reclaims the coldest
**rebuildable** bytes first: dependencies and build output of projects you haven't touched,
old AI-tool versions, Xcode leftovers, unused Docker images and local AI models, and merged
agent worktrees. It never touches anything it can't rebuild, anything you opened recently, or
anything whose dev server or build is running.

No account, no telemetry, no network. MIT licensed.

> **Status:** in development. Nothing is released yet. The outside judge in `judge/` was written
> before the features, and every change is scored against it.

## How it decides

- **Proof, not names.** A folder counts only when its project's marker file sits beside it, like
  `node_modules` beside `package.json`. Folders such as `build` or `dist` count only when git says
  they are ignored, and nothing tracked by git is ever touched.
- **Cold first.** Coldness is measured as days since the last build, git commit, editor open or
  dev-server run, never by file dates alone.
- **Undo means rebuild.** Moving files to the Trash frees no space, so Headroom deletes for real,
  records the exact lockfile and command, and Undo runs that command again.
- **Real numbers.** Freed space is measured in allocated blocks, with APFS clones counted once.
  Headroom tells you when a Time Machine snapshot is still holding the space.
- **Dry run first.** For the first 7 days Headroom only shows what it *would* have done.

See [docs/THREAT-MODEL.md](docs/THREAT-MODEL.md) for every rule.

## Install

1. Download the DMG for your Mac from [Releases](https://github.com/jelwingeuan/Headroom/releases)
   and drag Headroom to Applications.
2. Headroom is ad hoc signed, not notarized, so macOS blocks the first launch once. Open
   **System Settings → Privacy & Security**, scroll down and click **Open Anyway**, or run:

   ```bash
   xattr -dr com.apple.quarantine /Applications/Headroom.app
   ```

3. Grant **Full Disk Access** (System Settings → Privacy & Security → Full Disk Access). Headroom
   needs it to measure and clean project folders, `~/Library/Developer` and model caches.

Check the download against the `.sha256` file published with each release.

## Command line and agents

The app binary is also the CLI and an MCP server:

```bash
/Applications/Headroom.app/Contents/MacOS/Headroom scan ~/code
```

```bash
/Applications/Headroom.app/Contents/MacOS/Headroom mcp
```

## Build from source

Requires Xcode 15+ and Rust (stable).

```bash
scripts/bundle.sh
```

## Credits

Inspired by [Shed](https://github.com/DerrickDaVinci/shed). The idea of telling build output by
project marker files comes from [kondo](https://github.com/tbillington/kondo) (MIT).

## Security

See [SECURITY.md](SECURITY.md).

## License

[MIT](LICENSE)
