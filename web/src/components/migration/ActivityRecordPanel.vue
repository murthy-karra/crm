<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { RouterLink } from 'vue-router'
import { useQuery } from '@tanstack/vue-query'
import Card from '../Card.vue'
import ActivityFieldViewer from './ActivityFieldViewer.vue'
import ActivityObservations from './ActivityObservations.vue'
import { fetchActivityRecords, fetchActivityResults, useActivityAccess, type ActivityRecord, type ActivityResult, type ActivityPage } from '../../api/activityImports'
import { buttonClasses, INPUT_CLASSES } from '../../lib/controls'
import { describeApiError } from '../../lib/errors'
import { snapshotLabel } from './format'
const props = defineProps<{ importId: string; planId: string; mode: 'plan' | 'results'; progressVersion: string; sourceTimezone: string | null }>()
const access = useActivityAccess()
const kind = ref('')
const disposition = ref('')
const issue = ref('')
const issueDraft = ref('')
const cursors = ref<string[]>([''])
const selected = ref<string | null>(null)
const observing = ref<string | null>(null)
const branch = computed(() => [...access.prefix.value, props.mode, props.importId, props.planId])
const key = computed(() => [...branch.value, kind.value, disposition.value, issue.value, cursors.value.at(-1) ?? '', props.progressVersion])
const enabled = computed(() => access.enabled.value && !!props.planId)
const reading = useQuery({ queryKey: key, enabled, retry: false, gcTime: 0, queryFn: ({ signal }) => access.read<ActivityPage<ActivityRecord | ActivityResult>>(key.value, () => key.value, () => props.mode === 'plan' ? fetchActivityRecords(props.importId, props.planId, { kind: kind.value, disposition: disposition.value, issue: issue.value }, cursors.value.at(-1) || undefined, signal) : fetchActivityResults(props.importId, props.planId, { kind: kind.value, disposition: disposition.value, issue: issue.value }, cursors.value.at(-1) || undefined, signal)) })
const page = computed(() => enabled.value ? reading.data.value : undefined)
const preview = (row: { id: string }) => 'preview' in row ? (row as ActivityRecord).preview : null
function clear() { cursors.value = ['']; selected.value = null; observing.value = null }
watch(() => JSON.stringify([access.scope.value, props.importId, props.planId, props.progressVersion, kind.value, disposition.value, issue.value]), clear, { flush: 'sync' })
watch(() => props.mode, () => { disposition.value = ''; clear() }, { flush: 'sync' })
watch(() => cursors.value.at(-1), () => { selected.value = null; observing.value = null })
onBeforeUnmount(() => access.remove(branch.value))
</script>
<template>
  <Card
    class="min-w-0"
    data-testid="activity-records"
  >
    <h2 class="text-section font-medium">
      {{ mode === 'plan' ? 'Source records and native previews' : 'Committed activity results' }}
    </h2>
    <p
      v-if="mode === 'plan'"
      class="mt-1 text-small text-text-muted"
    >
      Native text is shown exactly, including preserved line breaks. HTML remains source text. Source timezone: {{ sourceTimezone ?? 'Not confirmed; date-only work remains held' }}.
    </p>
    <div class="mt-3 flex flex-wrap gap-3">
      <label class="min-w-0 text-small">Family<select
        v-model="kind"
        :class="INPUT_CLASSES"
      ><option value="">All families</option><option value="note">Notes</option><option value="task">Tasks</option></select></label>
      <label class="min-w-0 text-small">Disposition<select
        v-model="disposition"
        :class="INPUT_CLASSES"
      ><option value="">All dispositions</option><option :value="mode === 'plan' ? 'eligible' : 'applied'">{{ mode === 'plan' ? 'Eligible' : 'Applied' }}</option><option value="already_present">Already present</option><option value="held">Held</option></select></label>
      <form
        class="flex min-w-0 flex-wrap items-end gap-2"
        @submit.prevent="issue = issueDraft.trim()"
      >
        <label class="min-w-0 text-small">Issue code<input
          v-model="issueDraft"
          :class="INPUT_CLASSES"
          maxlength="100"
          pattern="[a-z_]*"
          placeholder="All issues"
        ></label><button
          type="submit"
          :class="buttonClasses()"
        >
          Filter issues
        </button>
      </form>
    </div>
    <p
      v-if="reading.isFetching.value"
      role="status"
      class="mt-3 text-small"
    >
      Loading activity records…
    </p>
    <p
      v-if="reading.error.value"
      role="alert"
      class="mt-3 text-small text-danger"
    >
      {{ describeApiError(reading.error.value, 'Could not load activity records. Refresh this page to recover.') }}
    </p>
    <ul
      v-if="page"
      class="mt-3 divide-y divide-border"
    >
      <li
        v-for="row in page.items"
        :key="row.id"
        class="min-w-0 space-y-3 py-4"
      >
        <div class="flex flex-wrap items-center justify-between gap-2">
          <h3 class="text-body font-medium">
            {{ snapshotLabel(row.kind) }} · FUB {{ row.source_id ?? 'ID unavailable' }} · {{ snapshotLabel(row.disposition) }}
          </h3>
          <RouterLink
            v-if="row.person_id"
            :to="`/people/${encodeURIComponent(row.person_id)}`"
            :class="buttonClasses('ghost')"
          >
            Review Person
          </RouterLink>
        </div>
        <dl class="space-y-1 text-small">
          <div
            v-for="field in row.source_summary.fields"
            :key="field.key"
            class="min-w-0"
          >
            <dt class="break-all font-medium">
              Source {{ field.label }}{{ field.label_abbreviated ? ' (abbreviated)' : '' }}
            </dt><dd class="whitespace-pre-wrap break-all text-text-muted">
              {{ field.text }}{{ field.abbreviated ? ' (abbreviated)' : '' }}
            </dd>
          </div>
        </dl>
        <p
          v-if="row.source_summary.abbreviated"
          class="text-small text-text-muted"
        >
          Source summary abbreviated. Inspect the retained source for full content.
        </p>
        <template v-if="preview(row)">
          <div
            v-if="preview(row)!.native"
            class="min-w-0 rounded-lg bg-surface-1 p-3"
          >
            <h4 class="text-small font-medium">
              Exact native preview
            </h4>
            <pre
              class="mt-2 max-h-80 overflow-auto whitespace-pre-wrap break-all text-body"
              tabindex="0"
            >{{ preview(row)!.native!.kind === 'note' ? (preview(row)!.native as { body: string }).body : (preview(row)!.native as { title: string }).title }}</pre>
            <dl class="mt-3 space-y-1 break-all text-small">
              <div>
                <dt class="inline font-medium">
                  Created:
                </dt><dd class="inline">
                  {{ preview(row)!.native!.created_at }}
                </dd>
              </div>
              <div>
                <dt class="inline font-medium">
                  Updated:
                </dt><dd class="inline">
                  {{ preview(row)!.native!.updated_at }}
                </dd>
              </div>
              <div v-if="preview(row)!.native!.kind === 'note'">
                <dt class="inline font-medium">
                  Source author ID:
                </dt><dd class="inline">
                  {{ (preview(row)!.native as { author_source_id: string | null }).author_source_id ?? 'Absent or unqualified' }}
                </dd>
              </div>
              <template v-if="preview(row)!.native!.kind === 'task'">
                <div>
                  <dt class="inline font-medium">
                    Source task type:
                  </dt><dd class="inline">
                    {{ (preview(row)!.native as { source_type: string }).source_type }}{{ (preview(row)!.native as { source_type_abbreviated: boolean }).source_type_abbreviated ? ' (abbreviated; inspect full source)' : '' }}
                  </dd>
                </div>
                <div>
                  <dt class="inline font-medium">
                    Source creator ID:
                  </dt><dd class="inline">
                    {{ (preview(row)!.native as { creator_source_id: string | null }).creator_source_id ?? 'Absent or unqualified' }}
                  </dd>
                </div>
                <div>
                  <dt class="inline font-medium">
                    Source assignee ID:
                  </dt><dd class="inline">
                    {{ (preview(row)!.native as { assignee_source_id: string | null }).assignee_source_id ?? 'Absent or unqualified' }}
                  </dd>
                </div>
                <div>
                  <dt class="inline font-medium">
                    Native kind:
                  </dt><dd class="inline">
                    {{ preview(row)!.native_kind ?? 'Not mapped' }}
                  </dd>
                </div>
                <div>
                  <dt class="inline font-medium">
                    Due:
                  </dt><dd class="inline">
                    {{ (preview(row)!.native as { due_at: string | null }).due_at ?? 'Undated' }}
                  </dd>
                </div>
                <div>
                  <dt class="inline font-medium">
                    Completed:
                  </dt><dd class="inline">
                    {{ (preview(row)!.native as { completed_at: string | null }).completed_at ?? 'Open' }}
                  </dd>
                </div>
              </template>
            </dl>
          </div>
          <p
            v-else
            class="text-small text-text-muted"
          >
            No eligible native text preview.
          </p>
          <ul class="space-y-1 text-small">
            <li
              v-for="reason in preview(row)!.reasons"
              :key="reason"
            >
              Held: {{ snapshotLabel(reason) }}
            </li><li
              v-for="change in preview(row)!.transformations"
              :key="change"
            >
              Conversion: {{ snapshotLabel(change) }}
            </li>
          </ul>
          <div class="text-small text-text-muted">
            <p>{{ preview(row)!.source_observations }} retained source observations.</p><p
              v-for="(count, code) in preview(row)!.source_only"
              :key="code"
            >
              {{ snapshotLabel(String(code)) }}: {{ count }} source-only components
            </p>
          </div>
        </template>
        <ul
          v-else
          class="text-small"
        >
          <li
            v-for="reason in ('reasons' in row ? row.reasons : [])"
            :key="reason"
          >
            {{ snapshotLabel(reason) }}
          </li>
        </ul>
        <div class="flex flex-wrap gap-2">
          <button
            type="button"
            :class="buttonClasses()"
            @click="selected = selected === row.id ? null : row.id"
          >
            Inspect full source and conversion
          </button><button
            v-if="mode === 'plan'"
            type="button"
            :class="buttonClasses()"
            @click="observing = observing === row.id ? null : row.id"
          >
            Review source observations
          </button>
        </div>
        <ActivityFieldViewer
          v-if="selected === row.id && enabled"
          :request="{ kind: mode === 'plan' ? 'records' : 'results', importId, planId, rowId: row.id, fieldKey: 'all' }"
          title="Full source and conversion evidence"
        />
        <ActivityObservations
          v-if="observing === row.id && mode === 'plan' && enabled"
          :import-id="importId"
          :plan-id="planId"
          :record-id="row.id"
        />
      </li>
    </ul>
    <p
      v-if="page && !page.items.length"
      class="mt-3 text-small text-text-muted"
    >
      No activity records match these filters.
    </p>
    <div class="mt-3 flex flex-wrap gap-2">
      <button
        type="button"
        :class="buttonClasses()"
        :disabled="cursors.length < 2 || reading.isFetching.value"
        @click="cursors.pop()"
      >
        Previous records
      </button><button
        type="button"
        :class="buttonClasses()"
        :disabled="!page?.next_cursor || reading.isFetching.value"
        @click="page?.next_cursor && cursors.push(page.next_cursor)"
      >
        Next records
      </button><button
        type="button"
        :class="buttonClasses('ghost')"
        @click="clear(); reading.refetch()"
      >
        Refresh records
      </button>
    </div>
  </Card>
</template>
