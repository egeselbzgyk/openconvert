/**
 * Keyboard and focus behaviour the component specs require (docs/design/handoff/docs/components.md,
 * UI_UX §6): a focus trap for dialogs that returns focus to its opener, and the roving tabindex a
 * list or radio group uses so that it is one Tab stop with arrow keys inside.
 */

const FOCUSABLE =
  'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

/**
 * Keep Tab inside `node` while it is mounted, close it on Escape, and give focus back to whatever
 * had it before the dialog opened (components.md, Dialog).
 */
export function trapFocus(node: HTMLElement, onescape: () => void) {
  const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
  const handle = (event: KeyboardEvent) => {
    if (event.key === "Escape") {
      event.preventDefault();
      onescape();
      return;
    }
    if (event.key !== "Tab") return;
    const items = [...node.querySelectorAll<HTMLElement>(FOCUSABLE)];
    const first = items[0];
    const last = items[items.length - 1];
    if (first === undefined || last === undefined) return;
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  };
  node.addEventListener("keydown", handle);
  return {
    destroy() {
      node.removeEventListener("keydown", handle);
      opener?.focus();
    },
  };
}

/** The index a roving-tabindex key moves to, or `null` for a key it does not handle. */
export function rovingTarget(
  key: string,
  current: number,
  count: number,
  axis: "vertical" | "horizontal" = "vertical",
): number | null {
  if (count === 0) return null;
  const next = axis === "vertical" ? "ArrowDown" : "ArrowRight";
  const previous = axis === "vertical" ? "ArrowUp" : "ArrowLeft";
  switch (key) {
    case next:
      return Math.min(current + 1, count - 1);
    case previous:
      return Math.max(current - 1, 0);
    case "Home":
      return 0;
    case "End":
      return count - 1;
    default:
      return null;
  }
}
