<script lang="ts">
  // Route `queue` (screen-map.md): the drop zone — full size while the queue is empty, a strip
  // above it once jobs exist, so focus order stays drop zone → queue → Settings (design decision
  // 4) — the privacy note, the first-run card while the queue is empty (queue.html §1), the queue
  // itself — this session's conversions — and under it the books of earlier runs of the app
  // ("Previous conversions", from the history the Rust side keeps).
  import Dialog from "../../components/Dialog.svelte";
  import DropZone from "../../components/DropZone.svelte";
  import FirstRunCard from "../../components/FirstRunCard.svelte";
  import HistoryList from "../../components/HistoryList.svelte";
  import PrivacyNote from "../../components/PrivacyNote.svelte";
  import QueueList from "../../components/QueueList.svelte";
  import QueueRow from "../../components/QueueRow.svelte";
  import type { HistoryEntry, ModelRow } from "../../lib/backend";
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
    onpreview,
    onexport,
    onunlock = () => undefined,
    onconsent = () => undefined,
    oneditmeta = () => undefined,
    onedittoc = () => undefined,
    onpage = null,
    firstrun = null,
    onsetup = () => undefined,
    onnotnow = () => undefined,
    library = false,
    history = [],
    historyOpen = true,
    historyNoReader = {},
    onhistorytoggle = () => undefined,
    onhistoryopen = () => undefined,
    onhistoryshow = () => undefined,
    onhistoryagain = () => undefined,
    onhistoryremove = () => undefined,
    onhistoryclear = () => undefined,
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
    onpreview: (id: string) => void;
    onexport: (id: string) => void;
    onunlock?: (id: string, password: string) => void;
    /** "Review consent…" on a row stopped for want of consent (D10). */
    onconsent?: (row: Row) => void;
    oneditmeta?: (id: string) => void;
    onedittoc?: (id: string) => void;
    onpage?: ((id: string, page: string) => void) | null;
    /** The default model, while the first-run card is offered; `null` hides it. */
    firstrun?: ModelRow | null;
    onsetup?: () => void;
    onnotnow?: () => void;
    /** Books go to the OpenConvert folder (Settings › Output folder), which the drop zone says. */
    library?: boolean;
    /** Earlier runs' books, newest first; the section is not drawn without any. */
    history?: HistoryEntry[];
    historyOpen?: boolean;
    historyNoReader?: Record<string, boolean>;
    onhistorytoggle?: () => void;
    onhistoryopen?: (id: string) => void;
    onhistoryshow?: (id: string) => void;
    onhistoryagain?: (entry: HistoryEntry) => void;
    onhistoryremove?: (id: string) => void;
    onhistoryclear?: () => void;
  } = $props();

  let confirming = $state(false);
  const waiting = $derived(store.rows.filter((row) => row.phase === "queued").length);
</script>

<main class="oc-main">
  <DropZone strip={store.rows.length > 0} {dragging} {onselect} {library} />
  {#if store.rows.length === 0}
    <PrivacyNote />
    {#if firstrun !== null}<FirstRunCard model={firstrun} {onsetup} {onnotnow} />{/if}
  {:else}
    <QueueList rows={store.rows} fit={history.length > 0} onremoveall={() => (confirming = true)}>
      {#snippet row(row: Row, active: boolean)}
        <QueueRow {row} {active} now={store.now} {oncancel} {onremove} {onretry} {ontoggle} {onopen} {onshow} {ondetails} {onpreview} {onexport} {onunlock} {onconsent} {oneditmeta} {onedittoc} {onpage} />
      {/snippet}
    </QueueList>
  {/if}
  {#if history.length > 0}
    <HistoryList
      entries={history}
      open={historyOpen}
      noReader={historyNoReader}
      ontoggle={onhistorytoggle}
      onopen={onhistoryopen}
      onshow={onhistoryshow}
      onagain={onhistoryagain}
      onremove={onhistoryremove}
      onclear={onhistoryclear}
    />
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
