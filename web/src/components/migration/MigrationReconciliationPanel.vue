<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useQuery } from '@tanstack/vue-query'
import Card from '../Card.vue'
import { fetchImports, useImportAccess } from '../../api/imports'
import { fetchMigrationReconciliation, type OutcomeTotals } from '../../api/migrationReconciliation'
import { buttonClasses, INPUT_CLASSES } from '../../lib/controls'

const access = useImportAccess('migration-reconciliation')
const selected = ref('')
const pages = ref<string[]>([''])
const enabled = computed(() => access.enabled.value && access.org.value?.workspace_mode === 'migration_review')
const importsKey = computed(() => [...access.prefix.value, 'imports', pages.value.at(-1)])
const summaryKey = computed(() => [...access.prefix.value, 'summary', selected.value])
const imports = useQuery({
  queryKey: importsKey, enabled, retry: false, gcTime: 0, refetchOnWindowFocus: false,
  queryFn: ({ signal }) => access.read(importsKey.value, () => importsKey.value,
    () => fetchImports(pages.value.at(-1) || undefined, signal)),
})
const summary = useQuery({
  queryKey: summaryKey, enabled: computed(() => enabled.value && !!selected.value),
  retry: false, gcTime: 0, refetchOnWindowFocus: false,
  queryFn: ({ signal }) => access.read(summaryKey.value, () => summaryKey.value,
    () => fetchMigrationReconciliation(selected.value, signal)),
})
const completed = computed(() => imports.data.value?.imports.filter(item => item.state === 'completed') ?? [])
watch(access.scope, () => { selected.value = ''; pages.value = [''] }, { flush: 'sync' })
function page(next?: string | null) {
  selected.value = ''
  if (next) pages.value.push(next)
  else if (pages.value.length > 1) pages.value.pop()
}
function refresh() { if (enabled.value && selected.value) void summary.refetch() }
const familyNames: Record<string, string> = {
  people_contacts: 'People and contact details', stages: 'Stages', source_users: 'Source users',
  embedded_person_tags: 'Tags on People', standalone_tag_catalog: 'Standalone tag catalog',
  custom_fields: 'Custom fields', notes: 'Notes', tasks: 'Tasks', historical_events: 'Historical events',
  calls: 'Calls', texts: 'Texts', addresses: 'Addresses', relationships: 'Relationships',
  appointments: 'Appointments', deals: 'Deals', emails: 'Emails', recordings: 'Recordings',
  external_files: 'External files', automation_settings: 'Automation and settings',
  source_deletion_semantics: 'Source deletions', workspace_activation: 'Workspace activation', privacy_erasure: 'Privacy and erasure',
}
const paths: Record<string, string> = {
  implemented: 'Import supported for qualified records', metadata_only: 'Metadata only',
  preserved_unrepresented: 'Retained when returned; no usable destination record',
  unqualified_source: 'Unqualified source catalog', not_captured: 'Not captured',
  unknown: 'Unknown', deferred_prerequisite: 'Further decisions required',
}
const units: Record<string, string> = {
  people: 'People', person_atomic_unit: 'Person metadata units (tags and custom fields together)', catalog_row: 'Catalog records',
  activity_record: 'Activity records', history_fact_identity: 'Historical facts', not_counted: 'No result count',
}
const origins: Record<string, string> = { original: 'Original import', admission: 'Later admitted People', recovery: 'Recovered People' }
const reasons: Record<string, string> = {
  source_scope_not_live_qualified: 'Access and completeness have not been verified against a live source account.',
  source_deletion_semantics_unknown: 'A missing source record is not proof of deletion.',
  migration_review_only: 'This workspace remains available for administrator review only.',
  source_users_do_not_create_identities: 'Source users are mapped to existing members; import does not create login accounts.',
  embedded_person_tags_only: 'Only tags attached to captured People are included.',
  standalone_tag_catalog_unqualified: 'A complete standalone tag catalog is not available through a qualified source path.',
  unsupported_values_or_mapping_holds: 'Unsupported values and unresolved mappings remain held for review.',
  restricted_note_detail_possible: 'Some note details may be inaccessible through the source API.',
  unsupported_replies_or_attachments_possible: 'Note replies and attachments may not be represented.',
  unsupported_task_semantics_possible: 'Some source task behavior cannot be represented by the current import.',
  external_facts_only: 'Imported history records external facts without crediting a new agent interaction.',
  history_content_not_imported: 'Historical content is not imported as correspondence.',
  api_restricted_records_unknown: 'Source API restrictions may hide historical records.',
  call_api_restricted_records_unknown: 'Source API restrictions may hide calls.',
  call_content_and_media_not_imported: 'Call content and recordings are not imported.',
  text_api_restricted_records_unknown: 'Source API restrictions may hide texts.',
  text_content_not_imported: 'Text message content is not imported.',
  destination_model_missing: 'There is no usable destination import for this family yet.',
  retained_only_when_returned: 'Embedded source evidence is preserved only when the source returns it.',
  source_profile_not_captured: 'This source family is not captured by the current migration.',
  email_bulk_storage_unresolved: 'Email content storage still requires an approved design.',
  recording_policy_unresolved: 'Recording handling still requires an approved policy.',
  activation_not_implemented: 'Workspace activation is not implemented yet.',
  o_012_open: 'Protected content erasure still requires an approved design.',
  o_013_open: 'The customer erasure request process is still pending.',
  qualified_initial_coverage_missing: 'There is no qualified initial import evidence for this family.',
  multiple_lineage_runs_grouped: 'These totals group multiple retained import runs.',
  retained_source_truncated: 'The retained source capture is truncated.',
  retained_source_inaccessible: 'Some source data was inaccessible in the retained capture.',
  retained_capture_incomplete: 'The retained source capture is incomplete.',
  held: 'Some records are held for review.',
  excluded: 'Some records were excluded from this import scope.',
  unprocessed: 'Some records have not been processed.',
}
function label(value: string) { return value.replaceAll('_', ' ') }
function reason(value: string) { return reasons[value] ?? label(value) }
function totals(value: OutcomeTotals) {
  return `Applied ${value.applied} · Already current ${value.already_current} · Held ${value.held} · Excluded ${value.excluded} · Unprocessed ${value.unprocessed}`
}
</script>

<template>
  <Card
    v-if="enabled"
    data-testid="migration-reconciliation"
    class="min-w-0 space-y-4"
  >
    <h2 class="text-section font-semibold">
      Migration reconciliation
    </h2>
    <p class="text-small text-text-muted">
      Review results and remaining coverage gaps together. The workspace stays in administrator review; this report does not approve cutover.
    </p>
    <p class="text-small text-text-muted">
      Initial import results and later metadata, activity, and history refresh plans are shown separately.
      Later People core refresh and mapping-repair outcomes remain in their detailed review panels.
    </p>
    <label class="block space-y-1">
      <span class="text-small font-medium">People import for reconciliation</span>
      <select
        v-model="selected"
        aria-label="People import for reconciliation"
        :class="INPUT_CLASSES"
      >
        <option value="">Choose a completed import</option>
        <option
          v-for="item in completed"
          :key="item.id"
          :value="item.id"
        >{{ item.id }}</option>
      </select>
    </label>
    <p
      v-if="imports.isPending.value"
      role="status"
    >
      Loading imports…
    </p>
    <p
      v-else-if="imports.error.value"
      role="alert"
    >
      Imports could not be loaded. <button
        :class="buttonClasses('ghost')"
        @click="imports.refetch()"
      >
        Retry imports
      </button>
    </p>
    <p
      v-else-if="!completed.length"
      class="text-small text-text-muted"
    >
      No completed imports on this page.
    </p>
    <div class="flex flex-wrap gap-2">
      <button
        v-if="pages.length > 1"
        :class="buttonClasses('secondary')"
        @click="page()"
      >
        Previous imports
      </button>
      <button
        v-if="imports.data.value?.next_cursor"
        :class="buttonClasses('secondary')"
        @click="page(imports.data.value.next_cursor)"
      >
        More imports
      </button>
      <button
        :disabled="!selected || summary.isFetching.value"
        :class="buttonClasses('secondary')"
        @click="refresh"
      >
        Refresh reconciliation
      </button>
    </div>
    <p
      v-if="selected && summary.isFetching.value"
      role="status"
    >
      Reading migration results…
    </p>
    <p
      v-if="selected && summary.error.value"
      role="alert"
    >
      The reconciliation summary could not be read. Retry with Refresh reconciliation.
    </p>
    <template v-if="selected && !summary.error.value && summary.data.value">
      <p class="text-small text-text-muted">
        Observed {{ new Date(summary.data.value.generated_at).toLocaleString() }}. Counts describe retained evidence, not everything in the source account. Earlier import outcomes remain recorded after later recovery or refresh.
      </p>
      <p
        v-for="blocker in summary.data.value.blockers"
        :key="`${blocker.code}:${blocker.evidence_id ?? ''}`"
        class="text-small"
      >
        {{ reason(blocker.code) }}
        <span
          v-if="blocker.evidence_id"
          class="block break-all font-mono"
        >Evidence: {{ blocker.evidence_id }}</span>
      </p>
      <section
        v-for="family in summary.data.value.families"
        :key="family.coverage.family"
        :data-family="family.coverage.family"
        class="min-w-0 border-t border-border-subtle pt-3"
      >
        <h3 class="text-body font-medium">
          {{ familyNames[family.coverage.family] ?? label(family.coverage.family) }}
        </h3>
        <p class="text-small text-text-muted">
          {{ paths[family.coverage.path] ?? label(family.coverage.path) }}
        </p>
        <div
          v-for="cohort in family.cohorts"
          :key="cohort.cohort_origin"
          class="mt-2 space-y-1 text-small"
        >
          <p class="font-medium">
            {{ origins[cohort.cohort_origin] }} · {{ units[family.unit] ?? label(family.unit) }}
          </p>
          <p>Initial import results: {{ totals(cohort.result_totals) }}</p>
          <p v-if="cohort.latest_bundle">
            Latest refresh plan: {{ label(cohort.latest_bundle.state) }} · {{ totals(cohort.latest_bundle.outcome_totals) }}
          </p>
          <p
            v-for="blocker in cohort.blockers"
            :key="`${blocker.code}:${blocker.evidence_id ?? ''}`"
          >
            {{ reason(blocker.code) }}
            <span
              v-if="blocker.evidence_id"
              class="block break-all font-mono"
            >Evidence: {{ blocker.evidence_id }}</span>
          </p>
          <details class="text-text-muted">
            <summary class="cursor-pointer">
              Evidence references
            </summary>
            <p class="mt-1">
              Initial import evidence
            </p>
            <ul class="space-y-1">
              <li
                v-for="id in cohort.evidence_ids"
                :key="id"
                class="break-all font-mono"
              >
                {{ id }}
              </li>
            </ul>
            <template v-if="cohort.latest_bundle">
              <p class="mt-1">
                Latest refresh evidence
              </p>
              <ul class="space-y-1">
                <li
                  v-for="id in cohort.latest_bundle.evidence_ids"
                  :key="id"
                  class="break-all font-mono"
                >
                  {{ id }}
                </li>
              </ul>
            </template>
          </details>
        </div>
        <p
          v-for="blocker in family.blockers.filter(item => !family.coverage.blocker_codes.includes(item.code))"
          :key="`${blocker.code}:${blocker.evidence_id ?? ''}`"
          class="text-small"
        >
          {{ reason(blocker.code) }}
          <span
            v-if="blocker.evidence_id"
            class="block break-all font-mono"
          >Evidence: {{ blocker.evidence_id }}</span>
        </p>
        <details
          v-if="family.coverage.blocker_codes.length"
          class="mt-2 text-small text-text-muted"
        >
          <summary class="cursor-pointer">
            Coverage limitations
          </summary>
          <ul class="mt-1 list-disc space-y-1 pl-5">
            <li
              v-for="code in family.coverage.blocker_codes"
              :key="code"
            >
              {{ reason(code) }}
            </li>
          </ul>
        </details>
      </section>
    </template>
  </Card>
  <p
    v-else-if="access.denied.value"
    role="alert"
  >
    Migration reconciliation is unavailable with your current access.
  </p>
</template>
