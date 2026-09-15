<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { useQuery } from '@tanstack/vue-query'
import Card from '../Card.vue'
import PeopleMappingRepair from './PeopleMappingRepair.vue'
import ConfirmDialog from '../ConfirmDialog.vue'
import FormField from '../FormField.vue'
import AdmittedPeopleRefreshItems from './AdmittedPeopleRefreshItems.vue'
import CoreChangeCaptureBoundary from './CoreChangeCaptureBoundary.vue'
import { buttonClasses, INPUT_CLASSES } from '../../lib/controls'
import { describeApiError } from '../../lib/errors'
import { fetchImport, fetchImports, importAccessError, uncertainImportError } from '../../api/imports'
import { coreChangeLabel, fetchCoreChangeReport, fetchCoreChangeReports } from '../../api/coreChangeReports'
import { ApiError } from '../../api/client'
import { fetchPeopleAdmissions } from '../../api/peopleAdmissions'
import { cancelAdmittedPeopleRefresh, confirmAdmittedPeopleRefresh, fetchAdmittedPeopleRefresh, fetchAdmittedPeopleRefreshAvailability, fetchAdmittedPeopleRefreshes, prepareAdmittedPeopleRefresh, refreshActive, refreshInteger, refreshLabel, repreviewAdmittedPeopleRefresh, retryAdmittedPeopleRefresh, useAdmittedPeopleRefreshAccess, type RefreshConfirm } from '../../api/admittedPeopleRefreshes'
import { snapshotTime } from './format'

const props = defineProps<{ refreshWorkspace: () => Promise<void> }>()
const emit = defineEmits<{ reviewSnapshot: [snapshotId: string] }>()
const access = useAdmittedPeopleRefreshAccess()
const parentId = ref(''); const admissionId = ref(''); const reportId = ref(''); const refreshId = ref('')
const parentPages = ref(['']); const admissionPages = ref(['']); const reportPages = ref(['']); const refreshPages = ref([''])
const listEpoch = ref(0); const outputEpoch = ref(0)
const pending = ref(false); const uncertain = ref(false); const actionError = ref('')
const mappingAck = ref(false); const coverageAck = ref(false); const exclusionsAck = ref(false); const removalsAck = ref(false)
const confirmation = ref<'confirm' | 'cancel' | null>(null)
const now = ref(Date.now()); const timer = setInterval(() => { now.value = Date.now() }, 1000)
type Intent = { identity: string; parent: string } & (
  { kind: 'prepare'; admission: string; report: string; request: string }
  | { kind: 'confirm'; id: string; body: RefreshConfirm }
  | { kind: 'plans' | 'retry' | 'cancel'; id: string; request: string; revision: number }
)
const intent = ref<Intent | null>(null)
let disposed = false; let identityEpoch = 0; let authorityEpoch = 0; let selectionEpoch = 0
const repairBusy = ref(false)
const busy = computed(() => pending.value || intent.value !== null || repairBusy.value)
const parentsKey = computed(() => [...access.prefix.value, 'parents', parentPages.value.at(-1)])
const parentKey = computed(() => [...access.prefix.value, 'parent', parentId.value])
const admissionsKey = computed(() => [...access.prefix.value, 'admissions', parentId.value, admissionPages.value.at(-1), listEpoch.value])
const reportsKey = computed(() => [...access.prefix.value, 'reports', parentId.value, reportPages.value.at(-1), listEpoch.value])
const listKey = computed(() => [...access.prefix.value, 'list', parentId.value, refreshPages.value.at(-1), listEpoch.value])
const detailKey = computed(() => [...access.prefix.value, 'detail', refreshId.value])
const reportKey = computed(() => [...access.prefix.value, 'report', current.value?.report_id ?? reportId.value])
const availabilityKey = computed(() => [...access.prefix.value, 'availability', admissionId.value, reportId.value])
const parents = useQuery({ queryKey: parentsKey, enabled: access.enabled, retry: false, gcTime: 0, queryFn: ({ signal }) => access.read(parentsKey.value, () => parentsKey.value, () => fetchImports(parentPages.value.at(-1) || undefined, signal)) })
const parent = useQuery({ queryKey: parentKey, enabled: computed(() => access.enabled.value && !!parentId.value), retry: false, gcTime: 0, queryFn: ({ signal }) => access.read(parentKey.value, () => parentKey.value, () => fetchImport(parentId.value, signal)) })
const admissions = useQuery({ queryKey: admissionsKey, enabled: computed(() => access.enabled.value && !!parentId.value), retry: false, gcTime: 0, queryFn: ({ signal }) => access.read(admissionsKey.value, () => admissionsKey.value, () => fetchPeopleAdmissions(parentId.value, admissionPages.value.at(-1) || undefined, signal)) })
const reports = useQuery({ queryKey: reportsKey, enabled: computed(() => access.enabled.value && !!parentId.value), retry: false, gcTime: 0, queryFn: ({ signal }) => access.read(reportsKey.value, () => reportsKey.value, () => fetchCoreChangeReports(parentId.value, reportPages.value.at(-1) || undefined, signal)) })
const firstRefreshPage = ref<Awaited<ReturnType<typeof fetchAdmittedPeopleRefreshes>> | null>(null)
const listing = useQuery({ queryKey: listKey, enabled: computed(() => access.enabled.value && !!admissionId.value), retry: false, gcTime: 0, staleTime: Infinity, refetchOnWindowFocus: false, refetchOnReconnect: false, queryFn: ({ signal }) => access.read(listKey.value, () => listKey.value, async () => {
  const first = refreshPages.value.length === 1; const key = JSON.stringify(listKey.value); const scope = access.scope.value
  if (first && firstRefreshPage.value) return firstRefreshPage.value
  const value = await fetchAdmittedPeopleRefreshes(admissionId.value, refreshPages.value.at(-1) || undefined, signal)
  if (first && !signal.aborted && key === JSON.stringify(listKey.value) && scope === access.scope.value) firstRefreshPage.value = value
  return value
}) })
const detail = useQuery({ queryKey: detailKey, enabled: computed(() => access.enabled.value && !!refreshId.value), retry: false, gcTime: 0, queryFn: ({ signal }) => access.read(detailKey.value, () => detailKey.value, () => fetchAdmittedPeopleRefresh(refreshId.value, signal)), refetchInterval: q => refreshActive(q.state.data) ? Math.min(30_000, 2000 * 2 ** Math.min(q.state.fetchFailureCount, 4)) : false, refetchOnWindowFocus: 'always', refetchOnReconnect: 'always' })
const current = computed(() => access.enabled.value && detail.data.value?.admission_id === admissionId.value ? detail.data.value : undefined)
const report = useQuery({ queryKey: reportKey, enabled: computed(() => access.enabled.value && !!(current.value?.report_id ?? reportId.value)), retry: false, gcTime: 0, queryFn: ({ signal }) => access.read(reportKey.value, () => reportKey.value, () => fetchCoreChangeReport(current.value?.report_id ?? reportId.value, signal)) })
const source = computed(() => access.enabled.value && report.data.value?.parent_import_id === parentId.value && report.data.value.state === 'completed' && !!report.data.value.output_revision ? report.data.value : undefined)
const availability = useQuery({ queryKey: availabilityKey, enabled: computed(() => access.enabled.value && !!admissionId.value && !!reportId.value), retry: false, gcTime: 0, queryFn: ({ signal }) => access.read(availabilityKey.value, () => availabilityKey.value, () => fetchAdmittedPeopleRefreshAvailability(admissionId.value, reportId.value, signal)) })
const selectedAvailability = computed(() => {
  const value = availability.data.value
  return value && value.admission_id === admissionId.value && value.report_id === reportId.value && value.parent_import_id === parentId.value ? value : undefined
})
const plan = computed(() => current.value?.plan)
const expired = computed(() => !!plan.value && (!plan.value.expires_at || Date.parse(plan.value.expires_at) <= now.value || !Number.isFinite(Date.parse(plan.value.expires_at))))
const selectedAdmission = computed(() => admissions.data.value?.items.find(value => value.id === admissionId.value))
const canPrepare = computed(() => access.enabled.value && access.org.value?.workspace_mode === 'migration_review' && parent.data.value?.state === 'completed' && !!parent.data.value.confirmed_plan_id && !!selectedAdmission.value && ['completed', 'cancelled'].includes(selectedAdmission.value.state) && selectedAdmission.value.progress.settled_items !== '0' && !!source.value && source.value.id === reportId.value && selectedAvailability.value?.available === true && !busy.value && !refreshId.value)
const canConfirm = computed(() => !!current.value?.actions.confirm && !!plan.value && (plan.value.counts.eligible !== '0' || !!plan.value.mapping_repair && plan.value.mapping_repair.approval_only_count !== '0') && (!plan.value.mapping_repair || mappingAck.value) && !expired.value && !!source.value && coverageAck.value && exclusionsAck.value && removalsAck.value && !busy.value)
const errors = computed(() => [parents.error.value, parent.error.value, reports.error.value, listing.error.value, detail.error.value, report.error.value, availability.error.value].filter(Boolean))
function resetAcknowledgments() { mappingAck.value = false; coverageAck.value = false; exclusionsAck.value = false; removalsAck.value = false; confirmation.value = null }
function resetLists() { firstRefreshPage.value = null; refreshPages.value = ['']; admissionPages.value = ['']; reportPages.value = ['']; listEpoch.value++ }
function deny() { access.denied.value = true; resetAcknowledgments(); void props.refreshWorkspace().catch(() => {}) }
watch(parents.data, value => { if (access.enabled.value && !parentId.value) parentId.value = value?.imports.find(v => v.state === 'completed' && v.confirmed_plan_id)?.id ?? '' })
watch(parentId, () => { selectionEpoch++; reportId.value = ''; refreshId.value = ''; resetLists(); resetAcknowledgments() }, { flush: 'sync' })
watch(admissions.data, value => { if (access.enabled.value && !admissionId.value) admissionId.value = value?.items.find(item => ['completed', 'cancelled'].includes(item.state) && item.progress.settled_items !== '0')?.id ?? '' })
watch(parentId, () => { admissionId.value = '' }, { flush: 'sync' })
watch(admissionId, () => { selectionEpoch++; reportId.value = ''; refreshId.value = ''; resetLists(); resetAcknowledgments() }, { flush: 'sync' })
watch([reportId, refreshId], () => { selectionEpoch++; resetAcknowledgments() }, { flush: 'sync' })
watch(() => `${plan.value?.id}:${plan.value?.revision}:${plan.value?.digest}:${current.value?.state}`, resetAcknowledgments, { flush: 'sync' })
watch(expired, value => { if (value) confirmation.value = null }, { flush: 'sync' })
watch(access.scope, () => { authorityEpoch++; resetAcknowledgments(); resetLists() }, { flush: 'sync' })
watch(access.identity, () => { identityEpoch++; intent.value = null; pending.value = false; uncertain.value = false; actionError.value = ''; parentId.value = ''; reportId.value = ''; refreshId.value = ''; parentPages.value = ['']; resetLists() }, { flush: 'sync' })
watch(errors, values => { if (values.some(importAccessError)) deny() })
onBeforeUnmount(() => { disposed = true; clearInterval(timer); intent.value = null; firstRefreshPage.value = null; access.remove(access.prefix.value) })
function reload() {
  if (!access.enabled.value) return
  resetLists(); resetAcknowledgments(); outputEpoch.value++
  for (const key of [parentsKey.value, parentKey.value, admissionsKey.value, detailKey.value, reportKey.value, availabilityKey.value]) void access.client.invalidateQueries({ queryKey: key, exact: true })
}
async function submit(value: Intent) {
  if (pending.value || !access.enabled.value || value.identity !== access.identity.value) return
  const identity = identityEpoch; const authority = authorityEpoch; const selection = selectionEpoch
  const sameIdentity = () => !disposed && identity === identityEpoch && value.identity === access.identity.value
  const sameRequest = () => sameIdentity() && access.enabled.value && authority === authorityEpoch && selection === selectionEpoch
  intent.value = value; pending.value = true; uncertain.value = false; actionError.value = ''; confirmation.value = null
  try {
    const result = value.kind === 'prepare' ? await prepareAdmittedPeopleRefresh(value.admission, value.report, value.request)
      : value.kind === 'confirm' ? await confirmAdmittedPeopleRefresh(value.id, value.body)
        : value.kind === 'plans' ? await repreviewAdmittedPeopleRefresh(value.id, value.request, value.revision)
          : value.kind === 'retry' ? await retryAdmittedPeopleRefresh(value.id, value.request, value.revision)
            : await cancelAdmittedPeopleRefresh(value.id, value.request, value.revision)
    if (!sameRequest()) { if (sameIdentity()) { uncertain.value = true; actionError.value = 'Access changed while the request was pending. Verify access, then retry the same request to recover its outcome.' }; return }
    // An idempotent receipt records acceptance. The following GET supplies current state.
    intent.value = null; uncertain.value = false; parentId.value = value.parent; refreshId.value = result.refresh_id; reload()
  } catch (error) {
    if (!sameIdentity()) return
    if (importAccessError(error)) { intent.value = null; uncertain.value = false; deny(); return }
    uncertain.value = uncertainImportError(error)
    if (!uncertain.value) intent.value = null
    actionError.value = uncertain.value ? 'The outcome is uncertain. Retry the same request to recover its saved receipt.'
      : error instanceof ApiError && error.status === 409 ? 'The plan, destination, authority, storage allowance or release changed. Reload and review before continuing.'
        : describeApiError(error, 'Could not complete the Admitted People refresh request.')
    if (sameRequest()) reload()
  } finally { if (sameIdentity()) pending.value = false }
}
function prepare() { if (canPrepare.value) void submit({ kind: 'prepare', identity: access.identity.value, parent: parentId.value, admission: admissionId.value, report: reportId.value, request: crypto.randomUUID() }) }
function lifecycle(kind: 'plans' | 'retry' | 'cancel') {
  const value = current.value
  if (!value || busy.value || !access.enabled.value || (kind === 'plans' ? !value.actions.repreview || !plan.value : !value.actions[kind]) || kind === 'cancel' && confirmation.value !== 'cancel') return
  try { void submit({ kind, identity: access.identity.value, parent: parentId.value, id: value.id, request: crypto.randomUUID(), revision: refreshInteger(kind === 'plans' ? plan.value!.revision : value.lifecycle_revision) }) }
  catch { actionError.value = 'The revision cannot be submitted safely. Reload the current refresh.' }
}
function confirm() {
  const value = current.value; const preview = plan.value
  if (!value || !preview || !canConfirm.value || confirmation.value !== 'confirm') return
  try { void submit({ kind: 'confirm', identity: access.identity.value, parent: parentId.value, id: value.id, body: {
    request_id: crypto.randomUUID(), plan_id: preview.id, plan_revision: refreshInteger(preview.revision), plan_digest: preview.digest,
    ...(preview.mapping_repair ? { mapping_repair: { choices_digest: preview.mapping_repair.choices_digest, candidate_count: refreshInteger(preview.mapping_repair.candidate_count), approval_only_count: refreshInteger(preview.mapping_repair.approval_only_count), unassigned_count: refreshInteger(preview.mapping_repair.unassigned_count) } } : {}),
    acknowledged_eligible_count: refreshInteger(preview.counts.eligible), acknowledged_coverage: true, acknowledged_exclusions: true,
    acknowledged_name_clears: refreshInteger(preview.counts.name_clears), acknowledged_assignment_clears: refreshInteger(preview.counts.assignment_clears), acknowledged_contact_removals: refreshInteger(preview.counts.contact_removals),
  } }) } catch { actionError.value = 'The plan counts cannot be submitted safely. Reload the preview.' }
}
</script>

<template>
  <Card
    id="admitted-people-refresh"
    class="mb-6 min-w-0"
    data-testid="admitted-people-refresh-panel"
  >
    <div class="flex flex-wrap items-start justify-between gap-3">
      <div>
        <h2 class="text-section font-medium">
          Refresh later-admitted People
        </h2><p class="mt-1 max-w-3xl text-small text-text-muted">
          Review names, contacts, stage and assignment from a completed retained change report. Local changes are held for review. This workspace stays in migration review.
        </p>
      </div>
      <button
        type="button"
        :class="buttonClasses()"
        :disabled="!access.enabled.value"
        @click="reload"
      >
        Reload Admitted People refreshes
      </button>
    </div>
    <template v-if="access.enabled.value">
      <div class="mt-4 grid gap-4 lg:grid-cols-2">
        <div class="min-w-0">
          <FormField
            v-slot="{ id }"
            label="Refresh parent import"
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
                v-if="parentId && !parents.data.value?.imports.some(v => v.id === parentId)"
                :value="parentId"
              >
                Selected import · {{ parentId }}
              </option><option
                v-for="value in parents.data.value?.imports.filter(v => v.state === 'completed' && v.confirmed_plan_id) ?? []"
                :key="value.id"
                :value="value.id"
              >
                {{ snapshotTime(value.created_at) }} · {{ value.id }}
              </option>
            </select>
          </FormField>
          <div class="mt-2 flex flex-wrap gap-2">
            <button
              type="button"
              :class="buttonClasses('ghost')"
              :disabled="busy || parentPages.length < 2 || parents.isFetching.value"
              @click="parentPages.pop()"
            >
              Previous parent imports
            </button><button
              type="button"
              :class="buttonClasses('ghost')"
              :disabled="busy || !parents.data.value?.next_cursor || parents.isFetching.value"
              @click="parents.data.value?.next_cursor && parentPages.push(parents.data.value.next_cursor)"
            >
              More parent imports
            </button>
          </div>
          <FormField
            v-slot="{ id }"
            class="mt-4"
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
                Choose an admission with committed People
              </option><option
                v-for="value in admissions.data.value?.items.filter(item => ['completed', 'cancelled'].includes(item.state) && item.progress.settled_items !== '0') ?? []"
                :key="value.id"
                :value="value.id"
              >
                {{ value.state === 'cancelled' ? 'Cancelled with committed People' : 'Completed' }} · {{ value.progress.settled_items }} committed · {{ value.id }}
              </option>
            </select>
          </FormField>
          <div class="mt-2 flex flex-wrap gap-2">
            <button
              type="button"
              :class="buttonClasses('ghost')"
              :disabled="busy || admissionPages.length < 2 || admissions.isFetching.value"
              @click="admissionPages.pop()"
            >
              Previous admission cohorts
            </button><button
              type="button"
              :class="buttonClasses('ghost')"
              :disabled="busy || !admissions.data.value?.next_cursor || admissions.isFetching.value"
              @click="admissions.data.value?.next_cursor && admissionPages.push(admissions.data.value.next_cursor)"
            >
              More admission cohorts
            </button>
          </div>
        </div>
        <div class="min-w-0">
          <FormField
            v-slot="{ id }"
            label="Completed report for a new preview"
            bare
          >
            <select
              :id="id"
              v-model="reportId"
              :class="INPUT_CLASSES"
              :disabled="busy || !admissionId"
              @change="refreshId = ''"
            >
              <option value="">
                Choose a sealed change report
              </option><option
                v-for="value in reports.data.value?.reports.filter(v => v.state === 'completed' && v.output_revision) ?? []"
                :key="value.id"
                :value="value.id"
              >
                {{ snapshotTime(value.created_at) }} · {{ value.id }}
              </option>
            </select>
          </FormField>
          <div class="mt-2 flex flex-wrap gap-2">
            <button
              type="button"
              :class="buttonClasses('ghost')"
              :disabled="busy || reportPages.length < 2 || reports.isFetching.value"
              @click="reportPages.pop()"
            >
              Previous change reports
            </button><button
              type="button"
              :class="buttonClasses('ghost')"
              :disabled="busy || !reports.data.value?.next_cursor || reports.isFetching.value"
              @click="reports.data.value?.next_cursor && reportPages.push(reports.data.value.next_cursor)"
            >
              More change reports
            </button>
          </div>
          <p
            v-if="reportId && availability.isFetching.value"
            class="mt-3 text-small text-text-muted"
            role="status"
          >
            Checking whether this admission and report can be prepared.
          </p><p
            v-else-if="selectedAvailability && !selectedAvailability.available"
            class="mt-3 text-small text-danger"
            role="status"
          >
            {{ selectedAvailability.closed_reason_code ? refreshLabel(selectedAvailability.closed_reason_code) : 'This admission and report cannot be prepared.' }}
          </p><p
            v-else-if="reportId && availability.error.value"
            class="mt-3 text-small text-danger"
            role="status"
          >
            Preparation is unavailable until the server can verify this admission and report.
          </p>
          <button
            type="button"
            class="mt-3"
            :class="buttonClasses('primary')"
            :disabled="!canPrepare"
            @click="prepare"
          >
            Prepare People preview
          </button>
        </div>
      </div>
      <div
        v-if="parentId"
        class="mt-4"
      >
        <FormField
          v-slot="{ id }"
          label="Saved Admitted People refresh"
          bare
        >
          <select
            :id="id"
            v-model="refreshId"
            :class="INPUT_CLASSES"
            :disabled="busy"
          >
            <option value="">
              Choose a saved refresh
            </option><option
              v-if="refreshId && !listing.data.value?.refreshes.some(v => v.id === refreshId)"
              :value="refreshId"
            >
              Selected refresh · {{ refreshId }}
            </option><option
              v-for="value in listing.data.value?.refreshes ?? []"
              :key="value.id"
              :value="value.id"
            >
              {{ refreshLabel(value.state) }} · {{ snapshotTime(value.created_at) }} · {{ value.id }}
            </option>
          </select>
        </FormField>
        <div class="mt-2 flex flex-wrap gap-2">
          <button
            type="button"
            :class="buttonClasses('ghost')"
            :disabled="busy || refreshPages.length < 2 || listing.isFetching.value"
            @click="refreshPages.pop()"
          >
            Previous saved refreshes
          </button><button
            type="button"
            :class="buttonClasses('ghost')"
            :disabled="busy || !listing.data.value?.next_cursor || listing.isFetching.value"
            @click="listing.data.value?.next_cursor && refreshPages.push(listing.data.value.next_cursor)"
          >
            More saved refreshes
          </button>
        </div>
      </div>
      <section
        v-if="source"
        class="mt-5 space-y-3 rounded border border-border p-3"
      >
        <h3 class="text-body font-medium">
          Retained evidence and coverage
        </h3><p class="break-all text-small">
          Report {{ source.id }} · sealed revision {{ source.output_revision }}
        </p>
        <div class="grid gap-3 lg:grid-cols-2">
          <CoreChangeCaptureBoundary
            title="Original capture"
            :boundary="source.inputs.baseline"
            @review-snapshot="emit('reviewSnapshot', $event)"
          /><CoreChangeCaptureBoundary
            title="Newer capture"
            :boundary="source.inputs.newer"
            @review-snapshot="emit('reviewSnapshot', $event)"
          />
        </div>
        <p class="text-small">
          Effective source access: {{ source.inputs.source_scope === 'consistent_identity' ? 'Source identity is consistent; coverage limitations still apply.' : 'Source identity changed or remains uncertain.' }}
        </p>
        <ul class="list-disc space-y-1 pl-5 text-small">
          <li
            v-for="warning in source.inputs.warnings"
            :key="warning"
          >
            {{ coreChangeLabel(warning) }}
          </li>
        </ul>
        <p class="text-small text-text-muted">
          New People, notes, tasks, tags, custom fields and historical activity are excluded. Mapping repairs require their own reviewed choices. Missing source records do not authorize deletion. This refresh does not establish cutover readiness.
        </p>
      </section>
      <section
        v-if="current"
        class="mt-5 space-y-3"
      >
        <h3 class="text-body font-medium">
          {{ refreshLabel(current.state) }} · {{ current.progress.settled_items }} settled
        </h3>
        <p
          v-if="current.pause_reason"
          class="text-small"
        >
          {{ refreshLabel(current.pause_reason) }}.<template v-if="current.pause_reason !== 'awaiting_mapping_choices'">
            Resume requires the initiating administrator and current capacity and release checks.
          </template>
        </p>
        <p
          v-if="current.state === 'completed'"
          class="text-small"
        >
          The planned units have settled. Check the results for held and excluded records; completion does not mean every Person was updated.
        </p>
        <p
          v-if="current.state === 'cancelled'"
          class="text-small"
        >
          Further writes are cancelled. Previously committed updates and their evidence are retained.
        </p>
        <PeopleMappingRepair
          :owner="'admitted'"
          :current="current"
          :disabled="busy"
          @selected="refreshId = $event"
          @reload="reload"
          @denied="deny"
          @busy="repairBusy = $event"
        />
        <template v-if="plan">
          <div class="grid gap-2 rounded border border-border p-3 text-small sm:grid-cols-2">
            <p>Eligible updates: <strong>{{ plan.counts.eligible }}</strong></p><p>Already current: {{ plan.counts.already_current }}</p><p>Held conflicts or gaps: {{ plan.counts.held }}</p><p>Excluded changes: {{ plan.counts.excluded }}</p><p>Components without a new instruction: {{ plan.counts.no_instruction }}</p><p>Plan revision {{ plan.revision }}</p>
          </div>
          <p class="text-small">
            Review every proposed clear or removal: <strong>{{ plan.counts.name_clears }} name fields, {{ plan.counts.assignment_clears }} assignments, {{ plan.counts.contact_removals }} contacts.</strong>
          </p>
          <label
            v-if="plan?.mapping_repair && current.state === 'ready'"
            class="my-3 flex gap-2 text-small"
          >
            <input
              v-model="mappingAck"
              type="checkbox"
              :disabled="busy"
            >
            I reviewed mappings for {{ plan.mapping_repair.candidate_count }} People, including {{ plan.mapping_repair.approval_only_count }} approvals with no CRM field changes, and {{ plan.mapping_repair.unassigned_count }} explicit unassigned approvals.
          </label>
          <AdmittedPeopleRefreshItems
            :key="`${access.scope.value}:${current.id}:${plan.id}:${plan.revision}:${outputEpoch}`"
            :refresh="current"
            :plan="plan"
            @access-denied="deny"
          />
          <div
            v-if="current.state === 'ready'"
            class="space-y-3 rounded border border-border p-3 text-small"
          >
            <p
              v-if="expired"
              role="status"
            >
              This preview expired. Prepare a new preview before confirming.
            </p><p v-else>
              Preview expires {{ snapshotTime(plan.expires_at) }}.
            </p>
            <p v-if="plan.counts.eligible === '0' && (!plan.mapping_repair || plan.mapping_repair.approval_only_count === '0')">
              This plan has no native updates to confirm. Its held, excluded and already-current entries remain available for inspection.
            </p>
            <label class="flex items-start gap-2"><input
              v-model="coverageAck"
              type="checkbox"
              :disabled="busy || expired"
              class="mt-1"
            >I reviewed the retained capture coverage and access limitations.</label>
            <label class="flex items-start gap-2"><input
              v-model="exclusionsAck"
              type="checkbox"
              :disabled="busy || expired"
              class="mt-1"
            >I reviewed the excluded changes and understand that the workspace stays in migration review.</label>
            <label class="flex items-start gap-2"><input
              v-model="removalsAck"
              type="checkbox"
              :disabled="busy || expired"
              class="mt-1"
            >I reviewed every listed clear and removal: {{ plan.counts.name_clears }} name fields, {{ plan.counts.assignment_clears }} assignments and {{ plan.counts.contact_removals }} contacts.</label>
            <button
              type="button"
              :class="buttonClasses('primary')"
              :disabled="!canConfirm"
              @click="confirmation = 'confirm'"
            >
              Review confirmation
            </button>
          </div>
        </template>
        <div class="flex flex-wrap gap-3">
          <button
            v-if="current.actions.repreview && current.mode !== 'mapping_repair'"
            type="button"
            :class="buttonClasses()"
            :disabled="busy"
            @click="lifecycle('plans')"
          >
            Prepare a new preview
          </button><button
            v-if="current.actions.retry"
            type="button"
            :class="buttonClasses()"
            :disabled="busy"
            @click="lifecycle('retry')"
          >
            Resume Admitted People refresh
          </button><button
            v-if="current.actions.cancel"
            type="button"
            :class="buttonClasses('danger')"
            :disabled="busy"
            @click="confirmation = 'cancel'"
          >
            Cancel Admitted People refresh
          </button>
        </div>
      </section>
      <p
        v-if="parents.isFetching.value || listing.isFetching.value || detail.isFetching.value"
        role="status"
        class="mt-3 text-small"
      >
        Loading Admitted People refresh status…
      </p>
      <p
        v-for="(error, index) in errors"
        :key="index"
        role="alert"
        class="mt-3 text-small text-danger"
      >
        {{ describeApiError(error, 'Could not load this refresh. Reload to recover current status.') }}
      </p>
      <p
        v-if="actionError"
        role="alert"
        class="mt-3 text-small text-danger"
      >
        {{ actionError }}
      </p><button
        v-if="uncertain && intent"
        type="button"
        class="mt-3"
        :class="buttonClasses()"
        :disabled="pending"
        @click="submit(intent)"
      >
        Retry the same refresh request
      </button>
    </template>
    <p
      v-else
      class="mt-3 text-small text-text-muted"
    >
      Admitted People refresh evidence is available only to a currently verified Organization administrator.
    </p>
    <ConfirmDialog
      :visible="confirmation !== null && access.enabled.value"
      :title="confirmation === 'cancel' ? 'Cancel Admitted People refresh?' : 'Apply this exact People plan?'"
      :message="confirmation === 'cancel' ? 'Stop future writes. Already committed updates and retained evidence will remain.' : `Apply ${plan?.counts.eligible ?? '0'} eligible updates, including ${plan?.counts.name_clears ?? '0'} name clears, ${plan?.counts.assignment_clears ?? '0'} assignment clears and ${plan?.counts.contact_removals ?? '0'} contact removals. ${plan?.mapping_repair ? `Also approve ${plan.mapping_repair.approval_only_count} mapping-only outcomes for this repair.` : ''} Local conflicts will be held. The workspace stays in migration review.`"
      :confirm-label="confirmation === 'cancel' ? 'Stop future refresh writes' : 'Confirm exact People plan'"
      :confirm-variant="confirmation === 'cancel' ? 'danger' : 'primary'"
      :is-pending="pending"
      @update:visible="!$event && (confirmation = null)"
      @confirm="confirmation === 'cancel' ? lifecycle('cancel') : confirm()"
    />
  </Card>
</template>
