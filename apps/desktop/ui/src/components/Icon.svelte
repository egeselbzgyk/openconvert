<script lang="ts">
  // One Lucide symbol from the inlined sprite. Decorative unless it has a label, in which case it
  // is an image with that name (WarningLine's severity icon).
  let {
    name,
    size = "",
    label = null,
  }: { name: string; size?: "" | "md" | "lg" | "xl"; label?: string | null } = $props();
</script>

{#if label === null}
  <svg class="oc-icon" class:oc-icon--md={size === "md"} class:oc-icon--lg={size === "lg"} class:oc-icon--xl={size === "xl"} aria-hidden="true"><use href="#i-{name}"></use></svg>
{:else}
  <svg class="oc-icon" class:oc-icon--md={size === "md"} class:oc-icon--lg={size === "lg"} class:oc-icon--xl={size === "xl"} role="img" aria-label={label}><use href="#i-{name}"></use></svg>
{/if}
