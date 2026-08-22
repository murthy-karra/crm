<script setup lang="ts">
// UI_STYLE.md §2: "Card: white, 1px border, radius 12px, no shadow. Cards
// are the only container; never nest a card in a card." Every screen's
// surfaces are built from this one primitive.
withDefaults(defineProps<{ padded?: boolean }>(), { padded: true })
</script>

<template>
  <div
    class="bg-surface-0 border border-border rounded-xl"
    :class="padded ? 'p-5' : ''"
  >
    <slot />
  </div>
</template>
