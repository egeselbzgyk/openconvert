/**
 * Motion that confirms something real happened (docs/design/handoff/docs/motion.md).
 *
 * Nothing here runs on a timer of its own: the pulse fires because a `heartbeat` event arrived.
 */

/**
 * One halo per heartbeat: `is-beat` for a frame, then the CSS transition fades it over
 * `--oc-dur-heartbeat`. Under reduced motion the durations are zero, so the halo is replaced by a
 * static ring whose shade flips once per heartbeat (`is-odd`).
 */
export function pulse(node: HTMLElement, beats: number) {
  const apply = (count: number) => {
    node.classList.toggle("is-odd", count % 2 === 1);
    if (count === 0) return;
    node.classList.add("is-beat");
    const clear = () => node.classList.remove("is-beat");
    if (typeof requestAnimationFrame === "function") {
      requestAnimationFrame(() => requestAnimationFrame(clear));
    } else {
      clear();
    }
  };
  node.classList.toggle("is-odd", beats % 2 === 1);
  return { update: apply };
}

/** Focus this element when it mounts: the safe button of a dialog, "Copy details" (a11y). */
export function focusOnMount(node: HTMLElement) {
  node.focus();
}
