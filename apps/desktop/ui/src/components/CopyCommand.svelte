<script lang="ts">
  // A command to copy (components.md, CopyCommand): the code, and Copy → "Copied" for
  // `desktop.copied_revert_secs`, announced politely.
  import { t } from "../lib/locale.svelte";

  let { command, revertMs }: { command: string; revertMs: number } = $props();
  let copied = $state(false);

  async function copy() {
    try {
      await navigator.clipboard.writeText(command);
      copied = true;
      setTimeout(() => (copied = false), revertMs);
    } catch {
      copied = false;
    }
  }
</script>

<div class="oc-command" class:is-copied={copied}>
  <code class="oc-command__code">{command}</code>
  <button class="oc-btn oc-btn--sm" onclick={copy}>{copied ? t("command.copied") : t("command.copy")}</button>
  <span class="oc-sr-only" role="status">{copied ? t("command.copied") : ""}</span>
</div>
