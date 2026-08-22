<script setup lang="ts">
// UI_STYLE.md §3/§5: "Badge: 13px/500, radius 6px, 2×8px padding, one of
// the tints above." Tint meaning is fixed by §3's table — callers choose a
// tint, not a color.
import type { BadgeTint } from '../lib/controls'

const props = withDefaults(defineProps<{ tint?: BadgeTint }>(), { tint: 'neutral' })

const TINT_CLASSES: Record<BadgeTint, string> = {
  warm: 'bg-tint-warm-bg text-tint-warm-text',
  neutral: 'bg-tint-neutral-bg text-tint-neutral-text',
  indigo: 'bg-tint-indigo-bg text-tint-indigo-text',
  green: 'bg-tint-green-bg text-tint-green-text',
  red: 'bg-tint-red-bg text-tint-red-text',
}

defineOptions({ inheritAttrs: false })
const classes = () => TINT_CLASSES[props.tint]
</script>

<template>
  <span
    class="inline-flex items-center rounded-md px-2 py-0.5 text-small font-medium"
    :class="classes()"
  >
    <slot />
  </span>
</template>
