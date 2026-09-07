<script setup lang="ts">
// UI_STYLE.md §1: "Page header: title 22px medium, tight tracking;
// optional one-line gray subtitle... Primary page action sits at the right
// of the title row... Breadcrumb above the title only when there is a
// parent."
defineProps<{
  title: string
  subtitle?: string
  /** Keep a page's controls below its title on narrow viewports. */
  stackActionOnNarrow?: boolean
}>()
</script>

<template>
  <div
    :class="[
      'mb-6 flex justify-between gap-4',
      stackActionOnNarrow ? 'flex-col items-start sm:flex-row sm:items-center' : 'items-center',
    ]"
  >
    <div class="min-w-0">
      <slot name="breadcrumb" />
      <h1 class="text-title font-medium tracking-title text-text">
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
      :class="stackActionOnNarrow ? 'shrink-0 sm:self-auto' : 'shrink-0'"
    >
      <slot name="action" />
    </div>
  </div>
</template>
