# Motion spec

Motion only confirms that something real happened. Nothing moves on a timer except the spinner, and the spinner stops when heartbeats stop.

| Animation | Trigger (real event) | Default | Reduced motion (`prefers-reduced-motion: reduce`) |
|---|---|---|---|
| Progress fill | `progress{stage,done,total}` (≤ 10/s) | `width` transition, `--oc-dur-base` 200 ms, `--oc-ease` | jumps to the new value (0 ms) |
| Heartbeat pulse | each `heartbeat` (~2 s) | `.is-beat` adds a 0.3 rem halo that fades over `--oc-dur-heartbeat` 900 ms | static ring; its shade toggles once per heartbeat |
| Spinner (uncounted stage) | stage begins with no count | 1 turn per `--oc-dur-spin` 1 s while heartbeats arrive; after 6 s without one it stops and turns warning | no rotation; static ring + "still working" |
| Not responding | no heartbeat for more than 6 s | row changes to warning tint (120 ms) | instant |
| Cancelling → Cancelled | click Cancel → `done{cancelled}` (≤ 2 s, hard stop 5 s) | label change only | same |
| Row expands into result | `done{ok}` or user toggle | height/opacity 200 ms | instant |
| First-run card, dialogs | mount | opacity 320 ms (`--oc-dur-slow`) | instant |
| Copy → Copied | click | color change 120 ms, reverts after 2 s | label change only, reverts after 2 s |
| Hover, press | pointer | background 120 ms (`--oc-dur-fast`) | instant |

No transform-based page transitions, no parallax, no blur. All motion is opacity, color or width, so it renders the same on WebKitGTK 2.44, WKWebView and WebView2.
