#!/usr/bin/env bash
# Builds build/Headroom.app: the Rust core for both Mac architectures, lipo'd into one static
# library, linked into the SwiftUI shell, then ad hoc signed.
#   scripts/bundle.sh          the app
#   scripts/bundle.sh --dmg    also build/Headroom_<arch>.dmg and its .sha256
# Version: $HEADROOM_VERSION, else the VERSION file, else 0.1.0.
set -euo pipefail

cd "$(dirname "$0")/.."
root=$PWD

dmg=false
for arg in "$@"; do
  case $arg in
    --dmg) dmg=true ;;
    *) echo "usage: scripts/bundle.sh [--dmg]" >&2; exit 2 ;;
  esac
done

version=${HEADROOM_VERSION:-$(cat VERSION 2>/dev/null || echo 0.1.0)}
version=${version#v}
if ! [[ $version =~ ^[0-9]+(\.[0-9]+){0,2}$ ]]; then
  echo "error: version '$version' must be numbers and dots, like 1.2.3" >&2
  exit 1
fi
export MACOSX_DEPLOYMENT_TARGET=13.0

# 1. Rust core: one slice per architecture. CI installs both targets; locally a missing one is
#    skipped with a warning and the app is built for this Mac only.
targets=()
if command -v rustup >/dev/null; then
  if [[ -n ${CI:-} ]]; then rustup target add aarch64-apple-darwin x86_64-apple-darwin; fi
  installed=$(rustup target list --installed)
  for t in aarch64-apple-darwin x86_64-apple-darwin; do
    if grep -qx "$t" <<<"$installed"; then
      targets+=("$t")
    else
      echo "warning: Rust target $t is missing (run: rustup target add $t); building without it" >&2
    fi
  done
else
  echo "warning: rustup not found; building for this Mac only" >&2
fi
if ((${#targets[@]} == 0)); then
  targets=("$(rustc -vV | sed -n 's/^host: //p')")
fi

libs=()
for t in "${targets[@]}"; do
  cargo build --release --manifest-path core/Cargo.toml --target "$t" --target-dir core/target
  libs+=("core/target/$t/release/libheadroom_core.a")
done
mkdir -p core/target/universal
lipo -create "${libs[@]}" -output core/target/universal/libheadroom_core.a

# 2. Swift shell, linked against that library.
swift_flags=(--package-path app -c release -Xlinker "-L$root/core/target/universal")
if ((${#targets[@]} > 1)); then
  swift_flags+=(--arch arm64 --arch x86_64)
  label=universal
else
  label=${targets[0]%%-*}
  label=${label/aarch64/arm64}
fi
bin_dir=$(swift build "${swift_flags[@]}" --show-bin-path)
rm -f "$bin_dir/Headroom" # SwiftPM doesn't track the Rust library, so force a fresh link
swift build "${swift_flags[@]}"

# 3. The .app bundle.
app=build/Headroom.app
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$bin_dir/Headroom" "$app/Contents/MacOS/Headroom"
cp app/Resources/AppIcon.icns "$app/Contents/Resources/AppIcon.icns"
cat >"$app/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key><string>en</string>
  <key>CFBundleDisplayName</key><string>Headroom</string>
  <key>CFBundleExecutable</key><string>Headroom</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundleIdentifier</key><string>io.github.jelwingeuan.headroom</string>
  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
  <key>CFBundleName</key><string>Headroom</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundleVersion</key><string>$version</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.utilities</string>
  <key>LSMinimumSystemVersion</key><string>13.0</string>
  <key>LSUIElement</key><true/>
  <key>NSHumanReadableCopyright</key><string>MIT License</string>
</dict>
</plist>
EOF
plutil -lint -s "$app/Contents/Info.plist"
codesign --force --deep --sign - "$app"
echo "Built $app ($(lipo -archs "$app/Contents/MacOS/Headroom"), version $version, ad hoc signed)"

# 4. Optional DMG with an Applications link, plus its checksum.
if $dmg; then
  name=Headroom_$label.dmg
  stage=$(mktemp -d)
  trap 'rm -rf "$stage"' EXIT
  chmod 755 "$stage" # the DMG's root folder
  ditto "$app" "$stage/Headroom.app"
  ln -s /Applications "$stage/Applications"
  # hdiutil sometimes fails with "Resource busy" on CI runners; a retry clears it.
  for try in 1 2 3; do
    hdiutil create -volname Headroom -srcfolder "$stage" -format UDZO -ov "build/$name" && break
    ((try == 3)) && exit 1
    sleep 5
  done
  (cd build && shasum -a 256 "$name" >"$name.sha256")
  echo "Built build/$name and build/$name.sha256"
fi
