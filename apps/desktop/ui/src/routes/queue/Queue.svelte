<script lang="ts">
  // Route `queue` (screen-map.md): the drop zone — full size while the queue is empty, a strip
  // above it once jobs exist, so focus order stays drop zone → queue → Settings (design decision
  // 4) — the privacy note, and the queue itself.
  import Dialog from "../../components/Dialog.svelte";
  import DropZone from "../../components/DropZone.svelte";
  import PrivacyNote from "../../components/PrivacyNote.svelte";
  import QueueList from "../../components/QueueList.svelte";
  import QueueRow from "../../components/QueueRow.svelte";
  import type { JobStore } from "../../lib/jobs.svelte";
  import type { Row } from "../../lib/jobstate";
  import { tn } from "../../lib/locale.svelte";

  let {
    store,
    dragging,
    onselect,
    oncancel,
    onremove,
    onretry,
    onremovewaiting,
    ontoggle,
    onopen,
    onshow,
    ondetails,
    onpage = null,
  }: {
    store: JobStore;
    dragging: { pdfs: number; skipped: string[] } | null;
    onselect: () => void;
    oncancel: (id: string) => void;
    onremove: (id: string) => void;
    onretry: (row: Row) => void;
    onremovewaiting: () => void;
    ontoggle: (id: string) => void;
    onopen: (id: string) => void;
    onshow: (id: string) => void;
    ondetails: (id: string) => void;
    onpage?: ((id: string, page: string) => void) | null;
  } = $props();

  let confirming = $state(false);
  const waiting = $derived(store.rows.filter((row) => row.phase === "queued").length);
</script>

<main class="oc-main">
  <DropZone strip={store.rows.length > 0} {dragging} {onselect} />
  {#if store.rows.length === 0}
    <PrivacyNote />
  {:else}
    <QueueList rows={store.rows} onremoveall={() => (confirming = true)}>
      {#snippet row(row: Row, active: boolean)}
        <QueueRow {row} {active} now={store.now} {oncancel} {onremove} {onretry} {ontoggle} {onopen} {onshow} {ondetails} {onpage} />
      {/snippet}
    </QueueList>
  {/if}
</main>

{#if confirming}
  <!-- "Remove all waiting…" asks once (design decision 14). -->
  <Dialog
    title={tn("queue.removeAllTitle", waiting)}
    confirm={tn("queue.removeAllConfirm", waiting)}
    danger
    oncancel={() => (confirming = false)}
    onconfirm={() => {
      confirming = false;
      onremovewaiting();
    }}
  >
    <p>{tn("queue.removeAllBody", waiting)}</p>
  </Dialog>
{/if}
