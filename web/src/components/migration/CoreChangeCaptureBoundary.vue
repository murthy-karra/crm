<script setup lang="ts">
import { coreChangeLabel, type CoreChangeBoundary } from '../../api/coreChangeReports'
import { buttonClasses } from '../../lib/controls'
import { snapshotTime } from './format'
defineProps<{ boundary: CoreChangeBoundary; title: string }>()
const emit = defineEmits<{ reviewSnapshot: [snapshotId: string] }>()
</script>
<template>
  <section
    class="min-w-0 rounded border border-border p-3"
    :aria-label="title"
  >
    <h4 class="text-body font-medium">
      {{ title }}
    </h4>
    <p class="mt-1 break-all text-small text-text-muted">
      {{ boundary.snapshot_id }}
    </p>
    <p class="mt-2 text-small">
      {{ snapshotTime(boundary.started_at) }} – {{ snapshotTime(boundary.completed_at) }}
    </p>
    <p class="mt-1 break-all text-small text-text-muted">
      Frozen sequence {{ boundary.capture_sequence }} · profile {{ boundary.profile_version }} · schema {{ boundary.schema_version }}
    </p>
    <button
      type="button"
      :class="buttonClasses('ghost')"
      @click="emit('reviewSnapshot', boundary.snapshot_id)"
    >
      Review {{ title.toLowerCase() }}
    </button>
    <div class="mt-2 max-w-full overflow-x-auto">
      <table class="w-full text-left text-small">
        <caption class="sr-only">
          {{ title }} source coverage
        </caption>
        <thead class="border-b border-border text-text-muted">
          <tr>
            <th class="p-2 font-medium">
              Source stream
            </th><th class="p-2 font-medium">
              Returned / reported
            </th><th class="p-2 font-medium">
              Gaps / captures
            </th>
          </tr>
        </thead>
        <tbody class="divide-y divide-border">
          <tr
            v-for="(stream, index) in boundary.streams"
            :key="index"
          >
            <td class="p-2">
              {{ coreChangeLabel(stream.stream) }}<p class="text-text-muted">
                {{ coreChangeLabel(stream.state) }}
              </p>
            </td><td class="p-2">
              {{ stream.returned_items }} / {{ stream.reported_total ?? 'unknown' }}
            </td><td class="p-2">
              {{ stream.content_gaps }} / {{ stream.accepted_captures }}
            </td>
          </tr>
        </tbody>
      </table>
    </div>
    <p
      v-if="!boundary.streams.length"
      class="mt-2 text-small text-text-muted"
    >
      No stream coverage was reported. Completeness is unknown.
    </p>
  </section>
</template>
