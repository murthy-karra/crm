<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { useQuery } from '@tanstack/vue-query'
import Card from '../Card.vue'
import ConfirmDialog from '../ConfirmDialog.vue'
import FormField from '../FormField.vue'
import { buttonClasses, INPUT_CLASSES } from '../../lib/controls'
import { ApiError } from '../../api/client'
import { describeApiError } from '../../lib/errors'
import { fetchPeopleAdmissions } from '../../api/peopleAdmissions'
import { fetchImport, fetchImports } from '../../api/imports'
import { fetchHistoryCaptures } from '../../api/historyCaptures'
import { createAdmittedHistoryRemainder, confirmAdmittedHistory, fetchAdmittedHistoryImport, fetchAdmittedHistoryImports, fetchAdmittedHistoryPage, historyAction, historyActive, increaseAdmittedHistoryBudget, prepareAdmittedHistory, useAdmittedHistoryAccess, type Disposition, type Family, type HistoryIssue, type HistoryManifest, type HistoryResult, type Root } from '../../api/admittedHistoryImports'
import { formatBytes } from './format'

const props = defineProps<{ refreshWorkspace: () => Promise<void> }>()
const access = useAdmittedHistoryAccess()
const parentId = ref(''); const admissionId = ref(''); const captureId = ref(''); const selectedId = ref('')
const pending = ref(false); const uncertain = ref(false); const error = ref(''); const confirmOpen = ref(false)
const held = ref(false); const coverage = ref(false); const budget = ref(''); const mode = ref<'manifests' | 'results' | 'issues'>('manifests')
const family = ref<Family | ''>(''); const disposition = ref<Disposition | ''>(''); const pageCursors = ref<string[]>([''])
let disposed = false
type Intent = { identity: string; scope: string; rootId: string; work: () => Promise<{ import: Root }> }
const intent = ref<Intent | null>(null)

const parents = useQuery({ queryKey: computed(() => [...access.prefix.value, 'parents']), enabled: access.enabled, retry: false, queryFn: ({ signal }) => access.read([], () => [], () => fetchImports(undefined, signal)) })
const parent = useQuery({ queryKey: computed(() => [...access.prefix.value, 'parent', parentId.value]), enabled: computed(() => access.enabled.value && !!parentId.value), retry: false, queryFn: ({ signal }) => access.read([], () => [], () => fetchImport(parentId.value, signal)) })
const admissions = useQuery({ queryKey: computed(() => [...access.prefix.value, 'admissions', parentId.value]), enabled: computed(() => access.enabled.value && !!parentId.value), retry: false, queryFn: ({ signal }) => access.read([], () => [], () => fetchPeopleAdmissions(parentId.value, undefined, signal)) })
const captures = useQuery({ queryKey: computed(() => [...access.prefix.value, 'history-captures', parentId.value]), enabled: computed(() => access.enabled.value && !!parentId.value), retry: false, queryFn: ({ signal }) => access.read([], () => [], () => fetchHistoryCaptures(parentId.value, undefined, signal)) })
const roots = useQuery({ queryKey: computed(() => [...access.prefix.value, 'admitted-history', admissionId.value]), enabled: computed(() => access.enabled.value && !!admissionId.value), retry: false, queryFn: ({ signal }) => access.read([], () => [], () => fetchAdmittedHistoryImports(admissionId.value, undefined, signal)), refetchInterval: query => query.state.data?.imports.some(historyActive) ? 2000 : false, refetchOnWindowFocus: 'always', refetchOnReconnect: 'always' })
const detail = useQuery({ queryKey: computed(() => [...access.prefix.value, 'admitted-history-detail', selectedId.value]), enabled: computed(() => access.enabled.value && !!selectedId.value), retry: false, queryFn: ({ signal }) => access.read([], () => [], async () => { const value = await fetchAdmittedHistoryImport(selectedId.value, signal); if (value.workspace_revision !== access.org.value?.workspace_revision) { await props.refreshWorkspace(); throw new Error('Workspace status was refreshed') }; return value }), refetchInterval: query => historyActive(query.state.data) ? 2000 : false, refetchOnWindowFocus: 'always', refetchOnReconnect: 'always' })
const current = computed(() => detail.data.value?.id === selectedId.value ? detail.data.value : undefined); const plan = computed(() => current.value?.latest_plan); const pageReady = computed(() => plan.value?.state === 'ready'); const pageCursor = computed(() => pageCursors.value.at(-1) || undefined)
const pages = useQuery({ queryKey: computed(() => [...access.prefix.value, 'admitted-history-page', selectedId.value, plan.value?.id, mode.value, family.value, disposition.value, pageCursor.value]), enabled: computed(() => access.enabled.value && !!selectedId.value && !!plan.value?.id && pageReady.value), retry: false, queryFn: ({ signal }) => fetchAdmittedHistoryPage<HistoryManifest | HistoryResult | HistoryIssue>(selectedId.value, plan.value!.id, mode.value, { family: mode.value === 'issues' || !family.value ? undefined : family.value, disposition: mode.value === 'issues' || !disposition.value ? undefined : disposition.value }, pageCursor.value, signal) })

const qualified = computed(() => captures.data.value?.captures.filter(c => c.parent_import_id === parentId.value && c.state === 'completed_with_gaps' && ['events', 'calls', 'text_messages'].every(stream => c.streams.some(s => s.family === stream && s.state === 'enumerated'))) ?? [])
const allowedAdmission = computed(() => admissions.data.value?.items.filter(a => a.parent_import_id === parentId.value && ['completed', 'cancelled'].includes(a.state) && BigInt(a.progress.settled_items) > 0n) ?? [])
const canPrepare = computed(() => !!parentId.value && parent.data.value?.state === 'completed' && !!parent.data.value.confirmed_plan_id && !!admissionId.value && !!captureId.value && !pending.value && roots.isSuccess.value && !roots.data.value?.imports.some(historyActive))
const canConfirm = computed(() => !!current.value && pageReady.value && current.value.actions.confirm && held.value && coverage.value && !pending.value)
const busy = computed(() => pending.value || !!intent.value)
const pageRows = computed(() => mode.value === 'manifests' ? pages.data.value?.manifests ?? [] : mode.value === 'results' ? pages.data.value?.results ?? [] : pages.data.value?.issues ?? [])
const pageNext = computed(() => pages.data.value?.next_cursor ?? null); const streamWarnings = computed(() => current.value?.coverage.warnings ?? []); const sourceBinding = computed(() => current.value?.source_binding ?? {})
const sourceInterval = computed(() => { const start = typeof sourceBinding.value.started_at === 'string' ? sourceBinding.value.started_at : null; const end = typeof sourceBinding.value.completed_at === 'string' ? sourceBinding.value.completed_at : null; return start || end ? `${formatDate(start)} – ${formatDate(end)}` : 'Interval unavailable' })
const sourceAccess = computed(() => String(sourceBinding.value.source_access_user_id ?? 'Unavailable'))
const countEntries = computed(() => { const counts = plan.value?.counts ?? {}; const results = current.value?.results ?? {}; return [['Unique planned', counts.unique_planned ?? '0'], ['Occurrences', counts.occurrences ?? '0'], ['Eligible', counts.eligible ?? '0'], ['Already present', results.already_present ?? '0'], ['Imported', results.imported ?? '0'], ['Held', counts.held ?? results.held ?? '0'], ['Equal repeats', counts.equal_repeats ?? results.equal_repeat ?? '0'], ['Excluded', counts.excluded ?? results.excluded ?? '0'], ['Date unknown', counts.unknown_dates ?? '0']] })
const selectedCapture = computed(() => qualified.value.find(c => c.id === captureId.value))

function formatDate(value: string | null): string { return value ? new Date(value).toLocaleString() : 'Unknown' }
function label(value: string): string { return value.replaceAll('_', ' ').replace(/\b\w/g, character => character.toUpperCase()) }
function clearReview() { confirmOpen.value = false; held.value = false; coverage.value = false }
function clearScope() { intent.value = null; uncertain.value = false; pending.value = false; error.value = ''; clearReview(); pageCursors.value = [''] }
function selectRoot(id: string) { selectedId.value = id; pageCursors.value = ['']; mode.value = 'manifests'; family.value = ''; disposition.value = '' }
function previousPage() { if (pageCursors.value.length > 1) pageCursors.value.pop() }
function nextPage() { if (pageNext.value) pageCursors.value.push(pageNext.value) }

watch(parents.data, value => { if (!parentId.value) parentId.value = value?.imports.find(p => p.state === 'completed' && !!p.confirmed_plan_id)?.id ?? '' })
watch(parentId, () => { admissionId.value = ''; captureId.value = ''; selectedId.value = ''; clearScope() }, { flush: 'sync' })
watch(allowedAdmission, value => { if (!admissionId.value && value[0]) admissionId.value = value[0].id })
watch(qualified, value => { if (!captureId.value && value[0]) captureId.value = value[0].id })
watch(admissionId, () => { captureId.value = ''; selectedId.value = ''; clearScope() }, { flush: 'sync' })
watch(captureId, () => { selectedId.value = ''; clearScope() }, { flush: 'sync' })
watch(roots.data, value => { if (!pending.value && !selectedId.value && value?.imports[0]) selectRoot(value.imports[0].id) })
watch(selectedId, () => { clearScope(); mode.value = 'manifests'; family.value = ''; disposition.value = '' }, { flush: 'sync' })
watch([mode, family, disposition, () => plan.value?.id], () => { pageCursors.value = [''] })
watch(() => current.value?.revision, () => { clearReview(); pageCursors.value = [''] })
watch(access.identity, () => { parentId.value = ''; admissionId.value = ''; captureId.value = ''; selectedId.value = ''; clearScope() }, { flush: 'sync' })
watch(access.scope, clearScope, { flush: 'sync' })
onBeforeUnmount(() => { disposed = true; intent.value = null; access.remove(access.prefix.value) })

function sameIntent(value: Intent): boolean { return !disposed && value.identity === access.identity.value && value.scope === access.scope.value && (!value.rootId || value.rootId === selectedId.value) }
async function run(work: () => Promise<{ import: Root }>, rootId = selectedId.value) {
  const request: Intent = { identity: access.identity.value, scope: access.scope.value, rootId, work }
  if (pending.value || !access.enabled.value) return
  pending.value = true; intent.value = request; uncertain.value = false; error.value = ''
  try {
    const result = await request.work()
    if (!sameIntent(request)) { if (request.identity === access.identity.value && !disposed) { uncertain.value = true; error.value = 'Access or selection changed while this request was pending. Retry the same request to recover its outcome.' }; return }
    intent.value = null; uncertain.value = false; pending.value = false; selectRoot(result.import.id); void roots.refetch(); void detail.refetch()
  } catch (cause) {
    if (!sameIntent(request)) return
    if (cause instanceof ApiError && [401, 403].includes(cause.status)) { intent.value = null; uncertain.value = false; await props.refreshWorkspace().catch(() => {}); return }
    uncertain.value = !(cause instanceof ApiError) || cause.status === 0 || cause.status >= 500
    if (!uncertain.value) intent.value = null
    error.value = uncertain.value ? 'The result could not be confirmed. Retry the same request to recover its outcome.' : cause instanceof ApiError && cause.status === 409 ? 'The plan or source coverage changed. Refresh the review before trying again.' : describeApiError(cause, 'Could not complete the admitted history action.')
    void roots.refetch()
  } finally { if (sameIntent(request)) pending.value = false }
}
function prepare() { if (!canPrepare.value) return; const body = { request_id: crypto.randomUUID(), admission_id: admissionId.value, history_capture_id: captureId.value }; void run(() => prepareAdmittedHistory(body), '') }
function confirm() { const root = current.value; const selectedPlan = plan.value; if (!root || !selectedPlan || !canConfirm.value) return; const body = { request_id: crypto.randomUUID(), plan_id: selectedPlan.id, expected_revision: root.revision, acknowledge_held: true, acknowledge_coverage: true }; void run(() => confirmAdmittedHistory(root.id, body)) }
function action(actionName: 'resume' | 'cancel') { const root = current.value; if (!root) return; const body = { request_id: crypto.randomUUID(), expected_revision: root.revision }; void run(() => historyAction(root.id, actionName, body)) }
function remainder() { const root = current.value; if (!root?.current_attempt_id) return; const body = { request_id: crypto.randomUUID(), attempt_id: root.current_attempt_id, expected_revision: root.revision }; void run(() => createAdmittedHistoryRemainder(root.id, body)) }
function increase() { const root = current.value; if (!root || !budget.value) return; const body = { request_id: crypto.randomUUID(), expected_revision: root.revision, run_byte_limit: budget.value }; void run(() => increaseAdmittedHistoryBudget(root.id, body)) }
</script>

<template>
  <Card
    class="min-w-0"
    data-testid="admitted-history-import-panel"
  >
    <div class="flex flex-wrap items-start justify-between gap-3">
      <div>
        <h2 class="text-section font-medium">
          Historical activity for admitted People
        </h2><p class="mt-1 text-small text-text-muted">
          Review retained event, call and text metadata for one completed People admission. Bodies, subjects, phone endpoints and raw source data stay private.
        </p>
      </div><button
        v-if="access.enabled.value"
        type="button"
        :class="buttonClasses()"
        @click="void roots.refetch()"
      >
        Refresh review
      </button>
    </div>
    <p
      v-if="!access.enabled.value"
      role="status"
      class="mt-3 text-small text-text-muted"
    >
      A verified Organization administrator session is required.
    </p>
    <template v-else>
      <p
        v-if="error"
        role="alert"
        class="mt-3 text-small text-danger"
      >
        {{ error }}
      </p><button
        v-if="uncertain && intent"
        type="button"
        :class="buttonClasses('primary')"
        :disabled="pending"
        class="mt-3"
        @click="run(intent.work, intent.rootId)"
      >
        Retry the same request
      </button>
      <div class="mt-4 grid min-w-0 gap-4 lg:grid-cols-3">
        <FormField
          v-slot="{ id }"
          label="Completed People import"
          bare
        >
          <select
            :id="id"
            v-model="parentId"
            :class="INPUT_CLASSES"
            :disabled="busy"
          >
            <option value="">
              Choose a completed People import
            </option><option
              v-for="item in parents.data.value?.imports ?? []"
              :key="item.id"
              :value="item.id"
              :disabled="item.state !== 'completed'"
            >
              {{ item.id }}
            </option>
          </select>
        </FormField><FormField
          v-slot="{ id }"
          label="Terminal admission cohort"
          bare
        >
          <select
            :id="id"
            v-model="admissionId"
            :class="INPUT_CLASSES"
            :disabled="busy || !parentId"
          >
            <option value="">
              Choose an admission cohort
            </option><option
              v-for="item in allowedAdmission"
              :key="item.id"
              :value="item.id"
            >
              {{ item.id }} · {{ item.progress.settled_items }} settled People
            </option>
          </select>
        </FormField><FormField
          v-slot="{ id }"
          label="Qualified history capture"
          bare
        >
          <select
            :id="id"
            v-model="captureId"
            :class="INPUT_CLASSES"
            :disabled="busy || !admissionId"
          >
            <option value="">
              Choose a retained capture
            </option><option
              v-for="item in qualified"
              :key="item.id"
              :value="item.id"
            >
              {{ item.id }} · {{ formatDate(item.started_at) }}–{{ formatDate(item.completed_at) }}
            </option>
          </select>
        </FormField>
      </div>
      <p
        v-if="selectedCapture"
        class="mt-2 text-small text-text-muted"
      >
        Selected capture: {{ formatDate(selectedCapture.started_at) }} – {{ formatDate(selectedCapture.completed_at) }} · source access user {{ selectedCapture.source_user_id }}
      </p><button
        type="button"
        :class="buttonClasses('primary')"
        :disabled="!canPrepare"
        @click="prepare"
      >
        Prepare retained history
      </button>
      <template v-if="current">
        <div class="mt-5 grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
          <div
            v-for="entry in countEntries"
            :key="entry[0]"
            class="rounded-lg border border-border bg-surface-1 p-3"
          >
            <p class="text-small text-text-muted">
              {{ entry[0] }}
            </p><p class="text-section font-medium">
              {{ entry[1] }}
            </p>
          </div>
        </div>
        <div class="mt-4 grid gap-3 sm:grid-cols-2">
          <div>
            <span class="text-small text-text-muted">Status</span><p class="font-medium">
              {{ label(current.state) }} · {{ label(plan?.state ?? 'unknown') }}
            </p>
          </div><div>
            <span class="text-small text-text-muted">Retained storage</span><p class="font-medium">
              {{ formatBytes(current.retained_bytes) }} reserved {{ formatBytes(current.reserved_bytes) }}
            </p>
          </div><div>
            <span class="text-small text-text-muted">Capture interval</span><p class="font-medium">
              {{ sourceInterval }}
            </p>
          </div><div>
            <span class="text-small text-text-muted">Source access</span><p class="font-medium">
              User {{ sourceAccess }}
            </p>
          </div>
        </div>
        <div class="mt-4 rounded-lg border border-border bg-surface-1 p-4">
          <h3 class="font-medium">
            Coverage: review notes
          </h3><p class="mt-1 text-small text-text-muted">
            This is retained source metadata coverage. It does not claim complete account history and does not change operational People data.
          </p><ul
            v-if="streamWarnings.length"
            class="mt-2 list-disc pl-5 text-small text-text-muted"
          >
            <li
              v-for="warning in streamWarnings"
              :key="warning"
            >
              {{ label(warning) }}
            </li>
          </ul><ul class="mt-2 grid gap-1 text-small sm:grid-cols-3">
            <li
              v-for="stream in current.coverage.streams"
              :key="stream.family"
            >
              {{ label(stream.family) }}: {{ label(stream.state) }}<span v-if="stream.reported_total !== null"> · {{ stream.reported_total }} records</span>
            </li>
          </ul>
        </div><p
          v-if="current.pause_reason"
          class="mt-3 text-small text-danger"
        >
          Paused: {{ label(current.pause_reason) }}
        </p>
        <div class="mt-5 flex flex-wrap items-end gap-3">
          <div
            class="flex gap-2"
            role="tablist"
            aria-label="History review data"
          >
            <button
              v-for="kind in ['manifests', 'results', 'issues']"
              :key="kind"
              type="button"
              role="tab"
              :aria-selected="mode === kind"
              :class="buttonClasses(mode === kind ? 'primary' : 'secondary')"
              :disabled="busy"
              @click="mode = kind as typeof mode"
            >
              {{ label(kind) }}
            </button>
          </div><FormField
            v-if="mode !== 'issues'"
            v-slot="{ id }"
            label="Family"
            bare
          >
            <select
              :id="id"
              v-model="family"
              :class="INPUT_CLASSES"
              :disabled="busy || !pageReady"
            >
              <option value="">
                All families
              </option><option value="events">
                Events
              </option><option value="calls">
                Calls
              </option><option value="text_messages">
                Text messages
              </option>
            </select>
          </FormField><FormField
            v-if="mode !== 'issues'"
            v-slot="{ id }"
            label="Outcome"
            bare
          >
            <select
              :id="id"
              v-model="disposition"
              :class="INPUT_CLASSES"
              :disabled="busy || !pageReady"
            >
              <option value="">
                All outcomes
              </option><template v-if="mode === 'manifests'">
                <option value="eligible">
                  Eligible
                </option><option value="equal_repeat">
                  Equal repeat
                </option><option value="held">
                  Held
                </option><option value="excluded">
                  Excluded
                </option>
              </template><template v-else>
                <option value="imported">
                  Imported
                </option><option value="already_present">
                  Already present
                </option><option value="equal_repeat">
                  Equal repeat
                </option><option value="held">
                  Held
                </option><option value="excluded">
                  Excluded
                </option>
              </template>
            </select>
          </FormField>
        </div>
        <p
          v-if="!pageReady"
          class="mt-3 text-small text-text-muted"
        >
          Review rows become available after preparation and classification finish.
        </p><div
          v-else
          class="mt-3 overflow-x-auto rounded-lg border border-border"
        >
          <table class="w-full min-w-[680px] text-small">
            <thead class="bg-surface-1 text-left text-text-muted">
              <tr>
                <th class="px-3 py-2">
                  Position
                </th><th class="px-3 py-2">
                  Family
                </th><th class="px-3 py-2">
                  Outcome
                </th><th class="px-3 py-2">
                  Person or record
                </th><th class="px-3 py-2">
                  Date or reason
                </th>
              </tr>
            </thead><tbody>
              <tr
                v-for="row in pageRows"
                :key="'id' in row ? row.id : row.code"
                class="border-t border-border"
              >
                <template v-if="mode === 'issues'">
                  <td
                    colspan="3"
                    class="px-3 py-2"
                  >
                    {{ label((row as HistoryIssue).code) }}
                  </td><td
                    colspan="2"
                    class="px-3 py-2"
                  >
                    {{ (row as HistoryIssue).record_count }} occurrences
                  </td>
                </template><template v-else>
                  <td class="px-3 py-2">
                    {{ (row as HistoryManifest | HistoryResult).position }}
                  </td><td class="px-3 py-2">
                    {{ label((row as HistoryManifest | HistoryResult).family) }}
                  </td><td class="px-3 py-2">
                    {{ label((row as HistoryManifest | HistoryResult).disposition) }}
                  </td><td class="px-3 py-2">
                    {{ mode === 'manifests' ? ((row as HistoryManifest).person_id ? 'Admitted Person' : 'Unresolved') : ((row as HistoryResult).fact_id ? 'Fact committed' : 'No fact') }}
                  </td><td class="px-3 py-2">
                    {{ mode === 'manifests' ? ((row as HistoryManifest).source_created_at ? formatDate((row as HistoryManifest).source_created_at) : 'Date unknown') : ((row as HistoryResult).reason ? label((row as HistoryResult).reason!) : 'Settled') }}
                  </td>
                </template>
              </tr><tr v-if="!pageRows.length">
                <td
                  colspan="5"
                  class="px-3 py-4 text-center text-text-muted"
                >
                  No rows match these filters.
                </td>
              </tr>
            </tbody>
          </table>
        </div>
        <div
          v-if="pageReady"
          class="mt-3 flex flex-wrap items-center gap-2"
        >
          <button
            type="button"
            :class="buttonClasses('ghost')"
            :disabled="busy || pageCursors.length < 2"
            @click="previousPage"
          >
            Previous page
          </button><button
            type="button"
            :class="buttonClasses('ghost')"
            :disabled="busy || !pageNext"
            @click="nextPage"
          >
            Next page
          </button><span class="text-small text-text-muted">Pages contain at most 25 rows.</span>
        </div>
        <div
          v-if="current.actions.confirm"
          class="mt-4 rounded-lg border border-border bg-surface-1 p-4"
        >
          <h3 class="font-medium">
            Confirm retained metadata import
          </h3><label class="mt-3 flex items-start gap-2 text-small"><input
            v-model="held"
            type="checkbox"
            class="mt-1"
          > I reviewed held outcomes and understand those records will stay held.</label><label class="mt-2 flex items-start gap-2 text-small"><input
            v-model="coverage"
            type="checkbox"
            class="mt-1"
          > I acknowledge the coverage limits and review-only workspace.</label><button
            type="button"
            :class="buttonClasses('primary')"
            :disabled="!canConfirm"
            class="mt-3"
            @click="confirmOpen = true"
          >
            Confirm metadata import
          </button>
        </div>
        <div class="mt-4 flex flex-wrap gap-2">
          <button
            v-if="current.actions.resume"
            type="button"
            :class="buttonClasses()"
            :disabled="busy"
            @click="action('resume')"
          >
            Resume
          </button><button
            v-if="current.actions.cancel"
            type="button"
            :class="buttonClasses('danger')"
            :disabled="busy"
            @click="action('cancel')"
          >
            Cancel
          </button><button
            v-if="current.actions.remainder"
            type="button"
            :class="buttonClasses('primary')"
            :disabled="busy"
            @click="remainder"
          >
            Continue never-settled remainder
          </button>
        </div>
        <div class="mt-4 flex flex-wrap items-end gap-2">
          <FormField
            v-slot="{ id }"
            label="New run budget (bytes)"
            description="Enter a larger decimal byte limit."
            bare
          >
            <input
              :id="id"
              v-model="budget"
              :class="INPUT_CLASSES"
              inputmode="numeric"
              placeholder="e.g. 2147483648"
            >
          </FormField><button
            type="button"
            :class="buttonClasses()"
            :disabled="busy || !budget"
            @click="increase"
          >
            Increase budget
          </button>
        </div>
      </template>
    </template>
  </Card><ConfirmDialog
    :visible="confirmOpen"
    title="Import admitted history metadata"
    message="Committed facts remain after cancellation; only never-settled units can continue in one exact remainder."
    :is-pending="pending"
    confirm-label="Confirm import"
    @update:visible="value => confirmOpen = value"
    @confirm="confirm"
  />
</template>
