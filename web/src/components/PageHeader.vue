<script setup lang="ts">
// UI_STYLE.md §1: "Page header: title 30px semibold, tight tracking;
// optional one-line gray subtitle... Primary page action sits at the right
// of the title row... Breadcrumb above the title only when there is a
// parent."
defineProps<{ title: string; subtitle?: string }>()
</script>

<template>
  <div class="mb-6 flex items-start justify-between gap-4">
    <div class="min-w-0">
      <slot name="breadcrumb" />
      <h1 class="text-title font-semibold tracking-title text-text">
        {{ title }}
      </h1>
      <p
        v-if="subtitle"
        class="mt-1 text-body text-text-muted"
      >
        {{ subtitle }}
      </p>
    </div>
    <div
      v-if="$slots.action"
      class="shrink-0"
    >
      <slot name="action" />
    </div>
  </div>
</template>
