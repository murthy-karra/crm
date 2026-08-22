<script setup lang="ts">
// A stage name, plus the one stage marker UI_STYLE.md §5/§9 allows: a red
// flame on Hot Prospect (see the exception recorded in both sections).
// Every surface that renders a stage goes through this component so the
// People table, the Select's value, and the Select's options agree.
import { Flame } from 'lucide-vue-next'
import type { Stage, StageRef } from '../api/types'
import { isHotProspect } from '../lib/stages'

defineProps<{ stage: Stage | StageRef }>()
</script>

<template>
  <span class="inline-flex min-w-0 items-center gap-1.5">
    <Flame
      v-if="isHotProspect(stage)"
      class="h-4 w-4 shrink-0 text-danger"
      stroke-width="1.5"
      aria-hidden="true"
    />
    <span class="truncate">{{ stage.name }}</span>
  </span>
</template>
