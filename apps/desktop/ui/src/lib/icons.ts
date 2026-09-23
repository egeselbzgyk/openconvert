/**
 * The icon sprite: Lucide (ISC) symbols plus the two logo marks, inlined into the DOM once at
 * start-up (`App.svelte`). Imported as text at build time, because the gallery's network request
 * for it would be blocked by `connect-src 'none'` — and a sprite that lives in the bundle cannot
 * be swapped by anything outside it.
 *
 * Source: `docs/design/handoff/icons.svg`, copied byte for byte to `src/assets/icons.svg`.
 */

import sprite from "../assets/icons.svg?raw";

export const SPRITE: string = sprite;

/** Every symbol id the sprite defines, for tests and for a typo to fail loudly. */
export const ICON_IDS: readonly string[] = [...sprite.matchAll(/<symbol id="([^"]+)"/g)].map(
  (match) => match[1] ?? "",
);
