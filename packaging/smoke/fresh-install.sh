#!/bin/sh
# Row 15.19, the scripted half: on a clean Linux or macOS machine, verify a downloaded release
# artefact, install it, convert a book with the installed app, and check the EPUB it wrote.
#
#   packaging/smoke/fresh-install.sh <OpenConvert…AppImage | OpenConvert…dmg> <book.pdf> [sha256]
#
# The optional sha256 is the one the release notes publish for the artefact; a mismatch stops here.
# The app converts through `--smoke-convert`: its normal start-up (the handshake with its own bundled
# converter), its queue and its supervisor, with the book dropped from the command line; the EPUB is
# written beside the PDF. What a script cannot see is left to the person running it: that no window
# misbehaves, and a drop on the window by hand once (docs/RELEASE_CHECKLIST.md).
set -eu

artefact=${1:?usage: fresh-install.sh <AppImage|dmg> <book.pdf> [sha256]}
pdf=${2:?usage: fresh-install.sh <AppImage|dmg> <book.pdf> [sha256]}
expected=${3:-}

fail() { echo "fresh-install: $*" >&2; exit 1; }

[ -f "$artefact" ] || fail "no such file: $artefact"
[ -f "$pdf" ] || fail "no such file: $pdf"
case "$pdf" in /*) ;; *) pdf="$(pwd)/$pdf" ;; esac

if [ -n "$expected" ]; then
  if command -v sha256sum >/dev/null 2>&1; then
    actual=$(sha256sum "$artefact" | cut -d' ' -f1)
  else
    actual=$(shasum -a 256 "$artefact" | cut -d' ' -f1)
  fi
  [ "$actual" = "$expected" ] || fail "SHA-256 mismatch: the release says $expected, the file is $actual"
  echo "fresh-install: SHA-256 matches the release notes"
fi

epub="${pdf%.*}.epub"
[ -e "$epub" ] && fail "$epub already exists; the app never overwrites, so the check would be void"

case "$artefact" in
  *.AppImage)
    chmod +x "$artefact"
    # A machine with no display (a VM without a session, CI) runs the app under a virtual one.
    if [ -z "${DISPLAY:-}" ] && [ -z "${WAYLAND_DISPLAY:-}" ] && command -v xvfb-run >/dev/null 2>&1; then
      runner="xvfb-run -a"
    else
      runner=""
    fi
    # No FUSE on minimal installs: the AppImage then unpacks itself instead.
    APPIMAGE_EXTRACT_AND_RUN=1 $runner "$artefact" --smoke-convert "$pdf" || fail "the app exited $?"
    ;;
  *.dmg)
    mount=$(mktemp -d)
    hdiutil attach -nobrowse -readonly -mountpoint "$mount" "$artefact" >/dev/null
    apps=$(mktemp -d)
    cp -R "$mount"/*.app "$apps/"
    hdiutil detach "$mount" >/dev/null
    app=$(find "$apps" -maxdepth 1 -name '*.app' | head -n 1)
    spctl --assess --type execute "$app" || fail "Gatekeeper refuses $app"
    main=$(sed -n '/<key>CFBundleExecutable<\/key>/{n;s/.*<string>\(.*\)<\/string>.*/\1/p;}' "$app/Contents/Info.plist")
    "$app/Contents/MacOS/$main" --smoke-convert "$pdf" || fail "the app exited $?"
    ;;
  *) fail "not an AppImage or a dmg: $artefact" ;;
esac

[ -f "$epub" ] || fail "no EPUB at $epub"
# An EPUB's first entry is `mimetype`, stored uncompressed, so its content sits at byte 38.
mimetype=$(dd if="$epub" bs=1 skip=38 count=20 2>/dev/null)
[ "$mimetype" = "application/epub+zip" ] || fail "$epub is not an EPUB (mimetype: $mimetype)"
echo "fresh-install: $epub is an EPUB; the installed app converts a book"
