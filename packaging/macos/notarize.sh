#!/bin/sh
# Notarize a signed artefact, staple the ticket, and verify it (PHASE 15 detail 2).
#
#   packaging/macos/notarize.sh OpenConvert.dmg [OpenConvert.app ...]
#
# The first argument is submitted (a .dmg, or a .zip of the app); every argument is stapled and
# validated afterwards, so the .app inside the release and the .dmg around it both carry the ticket
# (row 15.4).
#
# Environment (all required): APPLE_ID, APPLE_TEAM_ID, APPLE_APP_PASSWORD (app-specific).
# NOTARY_TIMEOUT bounds one submission (notarytool's own duration syntax, default 1h): notarization
# is asynchronous and occasionally slow, so the wait is generous and a failed submission is retried
# once. A submission Apple *rejects* is not retried - that is a verdict, and the log is printed.
# Anything but "Accepted" blocks the release: an unstapled bundle is never shipped.
set -eu

artefact=${1:?usage: notarize.sh artefact.dmg [bundle.app ...]}
: "${APPLE_ID:?APPLE_ID is not set}"
: "${APPLE_TEAM_ID:?APPLE_TEAM_ID is not set}"
: "${APPLE_APP_PASSWORD:?APPLE_APP_PASSWORD is not set}"
timeout=${NOTARY_TIMEOUT:-1h}
xcrun=${XCRUN:-xcrun}

submit() {
  "$xcrun" notarytool submit "$artefact" \
    --apple-id "$APPLE_ID" --team-id "$APPLE_TEAM_ID" --password "$APPLE_APP_PASSWORD" \
    --wait --timeout "$timeout" --output-format plist > "$1"
}

result=$(mktemp)
trap 'rm -f "$result"' EXIT
if ! submit "$result" && ! grep -q '<string>Invalid</string>' "$result"; then
  echo "notarize: submission did not complete; retrying once" >&2
  submit "$result" || true
fi
status=$(sed -n '/<key>status<\/key>/{n;s/.*<string>\(.*\)<\/string>.*/\1/p;}' "$result")
id=$(sed -n '/<key>id<\/key>/{n;s/.*<string>\(.*\)<\/string>.*/\1/p;}' "$result")
if [ "$status" != "Accepted" ]; then
  echo "notarize: $artefact was not accepted (status: ${status:-unknown}, id: ${id:-none})" >&2
  if [ -n "$id" ]; then
    "$xcrun" notarytool log "$id" \
      --apple-id "$APPLE_ID" --team-id "$APPLE_TEAM_ID" --password "$APPLE_APP_PASSWORD" >&2 || true
  fi
  exit 1
fi

for target in "$@"; do
  "$xcrun" stapler staple "$target"
  "$xcrun" stapler validate "$target"
done
echo "notarize: $artefact accepted ($id) and stapled"
