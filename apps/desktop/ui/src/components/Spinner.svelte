<script lang="ts">
  // Indeterminate: turns only while heartbeats arrive, and stops — turning to the warning colour —
  // when they stop (motion.md). Hidden from assistive technology; the stage label says it.
  let { stopped = false }: { stopped?: boolean } = $props();
</script>

<svg class="oc-spinner" class:is-spinning={!stopped} class:is-stopped={stopped} viewBox="0 0 16 16" aria-hidden="true"><circle cx="8" cy="8" r="6"></circle><path d="M8 2a6 6 0 0 1 6 6"></path></svg>
