<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { useQuery } from '@tanstack/vue-query'
import Card from '../Card.vue'
import ImportFieldViewer from './ImportFieldViewer.vue'
import { fetchImportRecords, fetchImportResults, sourceName, useImportAccess, type ImportFieldRequest, type ImportRecordPage, type ImportResult, type ImportResultPage } from '../../api/imports'
import { buttonClasses, INPUT_CLASSES } from '../../lib/controls'
import { describeApiError } from '../../lib/errors'
import { formatBytes, snapshotLabel, snapshotTime } from './format'
const props = defineProps<{ importId: string; planId: string; mode: 'plan' | 'results'; resultVersion?: string }>()
const access = useImportAccess()
const disposition = ref('')
const cursors = ref<string[]>([''])
const selected = ref<{ request: ImportFieldRequest; title: string } | null>(null)
const branch = computed(() => [...access.prefix.value, 'records', props.importId, props.planId, props.mode])
// Existing parent progress reads refresh only the visible result page. Its
// filters and cursor stay in place; immutable plan pages need no refresh.
const key = computed(() => [...branch.value, disposition.value, cursors.value.at(-1) ?? '', props.mode === 'results' ? props.resultVersion ?? '' : ''])
const enabled = computed(() => access.enabled.value && !!props.importId && !!props.planId)
const reading = useQuery({ queryKey: key, queryFn: ({ signal }) => access.read<ImportRecordPage | ImportResultPage>(key.value, () => key.value, () => props.mode === 'plan' ? fetchImportRecords(props.importId, props.planId, disposition.value as 'eligible' | 'held' || undefined, cursors.value.at(-1) || undefined, signal) : fetchImportResults(props.importId, disposition.value as ImportResult['disposition'] || undefined, cursors.value.at(-1) || undefined, signal)), enabled, retry: false, gcTime: 0 })
const page = computed(() => enabled.value ? reading.data.value : undefined)
const records = computed(() => page.value && 'records' in page.value ? page.value.records : [])
const results = computed(() => page.value && 'results' in page.value ? page.value.results : [])
const options = computed(() => props.mode === 'plan' ? ['eligible', 'held'] : ['imported', 'already_imported', 'held', 'pending'])
watch(() => JSON.stringify([access.scope.value, props.importId, props.planId, props.mode]), () => { disposition.value = ''; cursors.value = ['']; selected.value = null }, { flush: 'sync' })
watch(disposition, () => { cursors.value = ['']; selected.value = null })
watch(() => cursors.value.at(-1), () => { selected.value = null })
onBeforeUnmount(() => access.remove(branch.value))
function inspect(recordId: string, fieldKey: string, title: string) { selected.value = { request: { kind: 'record', importId: props.importId, planId: props.planId, recordId, fieldKey }, title } }
</script>
<template>
  <Card class="min-w-0">
    <div class="flex flex-wrap items-center justify-between gap-3">
      <h2 class="text-section font-semibold text-text">
        {{ mode === 'plan' ? 'People review' : 'Import results' }}
      </h2><label class="text-small">{{ mode === 'plan' ? 'People disposition' : 'Result disposition' }}<select
        v-model="disposition"
        :class="INPUT_CLASSES"
      ><option value="">All dispositions</option><option
        v-for="value in options"
        :key="value"
        :value="value"
      >{{ snapshotLabel(value) }}</option></select></label>
    </div>
    <p
      v-if="!enabled"
      role="alert"
      class="mt-3 text-small text-danger"
    >
      People review requires a current administrator session.
    </p>
    <p
      v-if="reading.isFetching.value && enabled"
      role="status"
      class="mt-3 text-small text-text-muted"
    >
      Loading People…
    </p>
    <p
      v-if="reading.error.value"
      role="alert"
      class="mt-3 text-small text-danger"
    >
      {{ describeApiError(reading.error.value, 'Could not load this page. Refresh to recover.') }}
    </p>
    <div
      v-if="page"
      class="mt-3 min-w-0 overflow-x-auto"
    >
      <table class="w-full min-w-[600px] text-left text-small">
        <thead class="text-text-muted">
          <tr>
            <th class="p-2 font-medium">
              Person
            </th><th class="p-2 font-medium">
              Outcome
            </th><th class="p-2 font-medium">
              Review details
            </th>
          </tr>
        </thead><tbody>
          <tr
            v-for="row in records"
            :key="row.id"
            class="border-t border-border align-top"
          >
            <td class="max-w-64 p-2">
              <p class="whitespace-pre-wrap break-all font-medium">
                {{ sourceName(row.proposed) }}
              </p><p class="break-all text-text-muted">
                FUB Person {{ row.source_id }}
              </p><p
                v-if="row.proposed.first_name?.abbreviated || row.proposed.last_name?.abbreviated"
                class="text-text-muted"
              >
                Name abbreviated for display.
              </p>
            </td>
            <td class="p-2">
              <p>{{ snapshotLabel(row.disposition) }}</p><p>{{ row.proposed.contacts?.total_count ?? '0' }} contacts</p><p>{{ row.overlap_count }} shared contact values</p><p class="text-text-muted">
                Reservation up to {{ formatBytes(row.added_byte_bound) }}
              </p>
            </td>
            <td class="max-w-96 p-2">
              <ul>
                <li
                  v-for="reason in row.held_reasons"
                  :key="reason"
                >
                  {{ snapshotLabel(reason) }}
                </li><li
                  v-for="change in row.transformations"
                  :key="change"
                >
                  {{ snapshotLabel(change) }}
                </li>
              </ul><p
                v-if="row.proposed.stage_label"
                class="break-all"
              >
                Source stage: {{ row.proposed.stage_label.value }}
              </p><p
                v-if="row.proposed.assignee_key"
                class="break-all"
              >
                Source assignment: {{ row.proposed.assignee_key }}
              </p><ul class="mt-1">
                <li
                  v-for="(contact, index) in row.proposed.contacts?.entries ?? []"
                  :key="index"
                  class="break-all"
                >
                  {{ contact.kind }}: {{ contact.value }}{{ contact.import_order === 0 ? ' · Primary' : '' }}
                </li>
              </ul><p
                v-if="row.proposed.contacts?.abbreviated"
                class="text-text-muted"
              >
                Contact display abbreviated; complete values remain available.
              </p><div class="mt-2 flex flex-wrap gap-2">
                <button
                  type="button"
                  :class="buttonClasses('ghost')"
                  @click="inspect(row.id, 'provenance', `Source fields for FUB Person ${row.source_id}`)"
                >
                  Inspect source {{ row.source_id }}
                </button><button
                  v-if="row.proposed.contacts"
                  type="button"
                  :class="buttonClasses('ghost')"
                  @click="inspect(row.id, 'contacts', `Contacts for FUB Person ${row.source_id}`)"
                >
                  Inspect contacts {{ row.source_id }}
                </button>
              </div>
            </td>
          </tr>
          <tr
            v-for="row in results"
            :key="row.id"
            class="border-t border-border align-top"
          >
            <td class="max-w-64 break-all p-2">
              FUB Person {{ row.source_id }}<p v-if="row.person_id">
                <a
                  :href="`/people/${encodeURIComponent(row.person_id)}`"
                  class="underline"
                >View imported Person {{ row.source_id }}</a>
              </p>
            </td><td class="p-2">
              {{ snapshotLabel(row.disposition) }}<p v-if="row.contact_count !== null">
                {{ row.contact_count }} contacts
              </p><p
                v-if="row.committed_at"
                class="text-text-muted"
              >
                {{ snapshotTime(row.committed_at) }}
              </p>
            </td><td class="max-w-96 p-2">
              <ul>
                <li
                  v-for="reason in row.held_reasons"
                  :key="reason"
                >
                  {{ snapshotLabel(reason) }}
                </li>
              </ul><button
                type="button"
                :class="buttonClasses('ghost')"
                @click="inspect(row.id, 'provenance', `Source fields for FUB Person ${row.source_id}`)"
              >
                Inspect source {{ row.source_id }}
              </button>
            </td>
          </tr>
        </tbody>
      </table><p
        v-if="!records.length && !results.length"
        class="py-3 text-small text-text-muted"
      >
        No People in this page.
      </p>
    </div>
    <div
      v-if="enabled"
      class="mt-3 flex flex-wrap gap-2"
    >
      <button
        type="button"
        :class="buttonClasses('secondary')"
        :disabled="cursors.length < 2 || reading.isFetching.value"
        @click="cursors.pop()"
      >
        Previous People
      </button><button
        type="button"
        :class="buttonClasses('secondary')"
        :disabled="!page?.next_cursor || reading.isFetching.value"
        @click="page?.next_cursor && cursors.push(page.next_cursor)"
      >
        Next People
      </button><button
        type="button"
        :class="buttonClasses('ghost')"
        @click="reading.refetch()"
      >
        Refresh People review
      </button>
    </div>
    <ImportFieldViewer
      v-if="selected && enabled"
      :request="selected.request"
      :title="selected.title"
    />
  </Card>
</template>
