<script setup lang="ts">
// A Person card in the Ask drawer (docs/specs/SLICE_005.md §10): reuses the
// People summary-row styling (name, stage chip, assignee, primary contact)
// and links to `/people/:id`. Built only from `references.people` — never
// from anything in the reply text. Clicking navigates and leaves the
// drawer open (the drawer is owned by AppShell, not by the route).
import { RouterLink } from 'vue-router'
import type { OperatorPersonCard } from '../api/types'
import Badge from './Badge.vue'
import StageLabel from './StageLabel.vue'

defineProps<{ card: OperatorPersonCard }>()
</script>

<template>
  <RouterLink
    :to="`/people/${card.id}`"
    class="block rounded-lg border border-border bg-surface-0 px-3 py-2.5 transition-colors duration-150 ease-out hover:bg-surface-1 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-focus focus-visible:ring-offset-2"
    data-testid="operator-person-card"
  >
    <div class="flex items-start justify-between gap-3">
      <div class="min-w-0">
        <p class="truncate text-body font-medium text-text">
          {{ card.display_name }}
        </p>
        <p class="truncate text-small text-text-muted">
          {{ card.primary_email ?? card.primary_phone ?? '' }}
        </p>
      </div>
      <!-- The wire card carries no stage id (§3/§5); StageLabel reads only the name. -->
      <Badge tint="neutral">
        <StageLabel :stage="{ id: '', name: card.stage_name }" />
      </Badge>
    </div>
    <p class="mt-1 text-small text-text-muted">
      {{ card.assigned_user_display_name ?? 'Unassigned' }}
    </p>
  </RouterLink>
</template>
