<script setup lang="ts">
import { computed } from 'vue'
import ImportedHistoryMetadata from './ImportedHistoryMetadata.vue'
import { externalHistoryEntry, historyKindLabel, type TimelineEntry } from '../../api/historyReview'
import type { ImportedMetadata } from '../../api/historyImports'
import { formatAbsoluteTime } from '../../lib/format'
const props = defineProps<{ entry: TimelineEntry; expanded?: boolean }>()
const external = computed(() => externalHistoryEntry(props.entry))
// Only declared fields reach the renderer. Never iterate arbitrary metadata,
// follow a response URL, or use the complete operational call-history fold.
const imported = computed<ImportedMetadata>(() => {
  const metadata = props.entry.metadata
  const value = (key: string) => typeof metadata[key] === 'string' ? metadata[key] as string : null
  return {
    source_id: value('source_id'), source_person_id: value('source_person_id'), source_access_user_id: value('source_access_user_id'), source_attributed_user_id: value('source_attributed_user_id'), source_creator_user_id: value('source_creator_user_id'), source_editor_user_id: value('source_editor_user_id'), source_created: value('source_created'), source_updated: value('source_updated'), source_sent: value('source_sent'), source_event_type: value('source_event_type'), source_note_id: value('source_note_id'), source_is_incoming: typeof metadata.source_is_incoming === 'boolean' ? metadata.source_is_incoming : null, source_duration: value('source_duration'), source_duration_unit: value('source_duration_unit'), source_outcome: value('source_outcome'), source_status: value('source_status'), content_availability: value('content_availability'), preview_truncated: metadata.preview_truncated === true,
  }
})
const nativeFields = computed(() => {
  const metadata = props.entry.metadata
  const text = (key: string, fallback = 'Unknown') => typeof metadata[key] === 'string' || typeof metadata[key] === 'number' ? String(metadata[key]) : fallback
  const name = (key: string, field: string, fallback: string) => { const value = metadata[key]; return value && typeof value === 'object' && field in value && typeof (value as Record<string, unknown>)[field] === 'string' ? String((value as Record<string, unknown>)[field]) : fallback }
  switch (props.entry.kind) {
    case 'person_imported': return [{ label: 'Origin', value: 'Person imported from Follow Up Boss' }]
    case 'inquiry_received': return [{ label: 'Source', value: text('source') }, { label: 'Person match', value: metadata.person_created === true ? 'Person created' : metadata.person_created === false ? 'Existing Person' : 'Unknown' }]
    case 'routing_decision': return [{ label: 'Assignee', value: name('assignee', 'display_name', 'Unassigned') }, { label: 'Strategy', value: text('strategy') }]
    case 'assignment_changed': return [{ label: 'Assignment', value: `${name('from', 'display_name', 'Unassigned')} → ${name('to', 'display_name', 'Unassigned')}` }, { label: 'Reason', value: text('reason') }]
    case 'stage_changed': return [{ label: 'Stage', value: `${name('from_stage', 'name', 'New Person')} → ${name('to_stage', 'name', 'Unknown')}` }, { label: 'Reason', value: text('reason') }]
    case 'contact_attempted': return [{ label: 'Channel', value: text('channel') }, { label: 'Recorded outcome', value: text('outcome') }, ...(metadata.corrects_id ? [{ label: 'Correction', value: 'Corrects a previous contact attempt; shown at its recorded time.' }] : []), ...(metadata.superseded === true ? [{ label: 'Status', value: 'Superseded by a later correction' }] : [])]
    case 'call_completed': return [{ label: 'Recorded outcome', value: text('outcome') }, { label: 'Talk time (seconds)', value: text('talk_seconds') }, { label: 'Answered at', value: text('answered_at', 'Not recorded') }]
    case 'correspondence': return [{ label: 'Direction', value: text('direction') }, { label: 'Agent', value: name('agent', 'display_name', 'Not recorded') }, { label: 'Captured at', value: text('captured_at') }, { label: 'Via', value: text('via') }]
    default: return []
  }
})
</script>
<template>
  <div class="min-w-0 space-y-2">
    <div class="flex flex-wrap items-center gap-2">
      <h4 class="text-body font-medium">
        {{ historyKindLabel(entry.kind) }}
      </h4><span
        v-if="external"
        class="rounded-lg border border-border px-2 py-0.5 text-small text-text-muted"
      >Imported external record</span>
    </div>
    <p class="text-small text-text-muted">
      {{ external ? 'FUB record created' : 'Displayed at' }}: {{ entry.display_at ? formatAbsoluteTime(entry.display_at) : 'Date unknown' }}
    </p>
    <template v-if="external">
      <p class="text-small text-text-muted">
        Imported by {{ entry.actor?.display_name ?? 'Unresolved executor' }} · {{ formatAbsoluteTime(entry.occurred_at) }}
      </p>
      <ImportedHistoryMetadata
        :metadata="imported"
        :compact="!expanded"
      />
      <p
        v-if="expanded"
        class="text-small text-text-muted"
      >
        Source user references describe historical evidence. The import executor is separate from the historical actor. Source flags do not establish native call outcomes, delivery or consent.
      </p>
    </template>
    <template v-else>
      <p class="text-small text-text-muted">
        {{ entry.actor?.display_name ?? 'System' }}
      </p>
      <dl class="grid min-w-0 gap-2 text-small sm:grid-cols-2">
        <div
          v-for="field in nativeFields"
          :key="field.label"
        >
          <dt class="text-text-muted">
            {{ field.label }}
          </dt><dd class="break-words">
            {{ field.value }}
          </dd>
        </div>
      </dl>
    </template>
    <dl
      v-if="expanded"
      class="grid min-w-0 gap-2 border-t border-border pt-3 text-small sm:grid-cols-2"
    >
      <div>
        <dt class="text-text-muted">
          {{ external ? 'Local import occurrence' : 'Fact occurred' }}
        </dt><dd class="break-all">
          {{ entry.occurred_at }}
        </dd>
      </div><div>
        <dt class="text-text-muted">
          Recorded
        </dt><dd class="break-all">
          {{ entry.recorded_at }}
        </dd>
      </div><div>
        <dt class="text-text-muted">
          Origin
        </dt><dd class="break-words">
          {{ entry.origin }}
        </dd>
      </div><div>
        <dt class="text-text-muted">
          Local fact ID
        </dt><dd class="break-all">
          {{ entry.id }}
        </dd>
      </div>
    </dl>
  </div>
</template>
