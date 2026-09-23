#!/bin/sh
# Sign every Mach-O inside OpenConvert.app, inside out, then the app itself (PHASE 15 detail 2).
#
#   packaging/macos/sign_nested.sh path/to/OpenConvert.app
#
# Environment:
#   APPLE_SIGNING_IDENTITY  the Developer ID Application identity (required)
#   ENTITLEMENTS            the outer app's entitlements (default: entitlements.plist beside this
#                           script); nested code is signed with none
#   CODESIGN, FILE          the tools to run (default: codesign, file) - replaced by stubs in the
#                           test that runs this script on Linux
#
# The list of binaries is *enumerated*, never written down: every regular file under Contents/
# whose `file` output says Mach-O, whatever its permissions (a dylib is usually not executable),
# so a binary added to the bundle later cannot slip through unsigned. Libraries are signed before
# executables and deeper paths before shallower ones, the main executable is signed as part of the
# app, and the app is signed last - the only order in which each signature seals what it contains.
# Every signature is Developer ID with the Hardened Runtime, a secure timestamp and --force, so an
# upstream ad-hoc signature (llama.cpp's) is replaced rather than kept.
set -eu

app=${1:?usage: sign_nested.sh path/to/OpenConvert.app}
identity=${APPLE_SIGNING_IDENTITY:?APPLE_SIGNING_IDENTITY is not set}
here=$(cd "$(dirname "$0")" && pwd)
entitlements=${ENTITLEMENTS:-$here/entitlements.plist}
codesign=${CODESIGN:-codesign}
file_cmd=${FILE:-file}

contents="$app/Contents"
if [ ! -f "$contents/Info.plist" ]; then
  echo "sign_nested: $app is not an application bundle (no Contents/Info.plist)" >&2
  exit 2
fi
if [ ! -f "$entitlements" ]; then
  echo "sign_nested: no entitlements at $entitlements" >&2
  exit 2
fi

# CFBundleExecutable, read with sed so the script needs nothing beyond POSIX tools.
main=$(sed -n '/<key>CFBundleExecutable<\/key>/{n;s/.*<string>\(.*\)<\/string>.*/\1/p;}' "$contents/Info.plist")
if [ -z "$main" ]; then
  echo "sign_nested: Info.plist names no CFBundleExecutable" >&2
  exit 2
fi
main_path="$contents/MacOS/$main"

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
: > "$work/libraries"
: > "$work/executables"

find "$contents" -type f | while IFS= read -r path; do
  [ "$path" = "$main_path" ] && continue
  kind=$("$file_cmd" -b "$path")
  case "$kind" in
    Mach-O*)
      depth=$(printf '%s\n' "$path" | awk -F/ '{ print NF }')
      case "$kind" in
        *"shared library"* | *bundle*) printf '%s\t%s\n' "$depth" "$path" >> "$work/libraries" ;;
        *) printf '%s\t%s\n' "$depth" "$path" >> "$work/executables" ;;
      esac
      ;;
  esac
done

signed=0
for list in libraries executables; do
  # Deepest first; the path breaks ties so the order is the same on every run.
  sort -t "$(printf '\t')" -k1,1nr -k2,2 "$work/$list" | cut -f2- > "$work/$list.sorted"
  while IFS= read -r path; do
    "$codesign" --force --options runtime --timestamp --sign "$identity" "$path"
    signed=$((signed + 1))
  done < "$work/$list.sorted"
done

"$codesign" --force --options runtime --timestamp --entitlements "$entitlements" \
  --sign "$identity" "$app"
echo "sign_nested: signed $signed nested Mach-O file(s), then $app"
