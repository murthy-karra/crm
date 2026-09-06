<script setup lang="ts">
// D-045: muted stage badges; D-020 normalized-name matching is preserved.
import { Flame } from 'lucide-vue-next'
import type { Stage, StageRef } from '../api/types'
import { isHotProspect, stageTintClasses } from '../lib/stages'

defineProps<{ stage: Stage | StageRef; badge?: boolean }>()
</script>

<template>
  <span
    class="inline-flex min-w-0 items-center gap-1.5"
    :class="badge ? ['stage-badge', stageTintClasses(stage)] : ''"
  >
    <Flame
      v-if="isHotProspect(stage)"
      class="h-3.5 w-3.5 shrink-0 text-stage-hot-text"
      stroke-width="1.5"
      aria-hidden="true"
    />
    <span
      v-else-if="badge"
      class="h-1.5 w-1.5 shrink-0 rounded-full bg-current opacity-70"
      aria-hidden="true"
    />
    <span class="truncate">{{ stage.name }}</span>
  </span>
</template>
