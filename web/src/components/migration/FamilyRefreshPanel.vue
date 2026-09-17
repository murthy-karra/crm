<script setup lang="ts">
import { computed, onBeforeUnmount, ref, useId, watch } from 'vue'
import { useQuery } from '@tanstack/vue-query'
import Card from '../Card.vue'
import Dialog from 'primevue/dialog'
import FamilyRefreshMappings from './FamilyRefreshMappings.vue'
import FamilyRefreshFields from './FamilyRefreshFields.vue'
import { fetchImports, uncertainImportError } from '../../api/imports'
import { fetchCoreChangeReports } from '../../api/coreChangeReports'
import { fetchHistoryCaptures } from '../../api/historyCaptures'
import { cancelFamilyRefresh, confirmFamilyRefresh, fetchFamilyRefresh, fetchFamilyRefreshes, fetchRefreshItems, fetchRefreshResults, prepareFamilyRefresh, replanFamilyRefresh, resumeFamilyRefresh, useFamilyRefreshAccess, type RefreshPage, type RefreshItem, type RefreshResult, type RefreshConfirm, type RefreshControl, type RefreshCounts, type RefreshFamily, type RefreshPatch, type RefreshPrepare, type RefreshPrepared, type RefreshReplan } from '../../api/familyRefreshes'
import { buttonClasses, dialogPt, INPUT_CLASSES } from '../../lib/controls'
import { describeApiError } from '../../lib/errors'
const props = defineProps<{ refreshWorkspace: () => Promise<void> }>()
const access = useFamilyRefreshAccess()
const parent = ref(''); const selected = ref(''); const coreReport = ref(''); const historyCapture = ref('')
const families = ref<RefreshFamily[]>(['metadata', 'activity', 'history']); const family = ref<RefreshFamily>('metadata')
const parentPages = ref(['']); const corePages = ref(['']); const historyPages = ref(['']); const bundlePages = ref(['']); const rowPages = ref([''])
const mode = ref<'items' | 'results'>('items'); const outcome = ref(''); const inspected = ref(''); const patches = ref<RefreshPatch[]>([])
const timezone = ref(''); const timezoneDirty = ref(false); const acknowledged = ref(false)
const confirmFamilies = ref<RefreshFamily[]>([]); const confirmation = ref<RefreshConfirm | null>(null); const pending = ref(false); const uncertain = ref(false); const actionError = ref<unknown>(null)
const failures = ref(0); const now = ref(Date.now()); const confirmTitle = useId(); let disposed = false
const names: Record<RefreshFamily, string> = { metadata: 'Tags and custom fields', activity: 'Notes and tasks', history: 'Historical facts' }
const countNames: Record<keyof RefreshCounts, string> = { units: 'Total units', inserts: 'New records', updates: 'Updates', already_current: 'Already current', held: 'Held', excluded: 'Excluded', tag_removals: 'Tag removals', field_clears: 'Field clears', task_completions: 'Task completions', task_reopens: 'Task reopens', history_corrections: 'History corrections', source_only: 'Source-only values' }
function reading<T>(parts: () => unknown[], enabled: () => boolean, request: (signal: AbortSignal) => Promise<T>) {
  const key = computed(() => [...access.prefix.value, ...parts()])
  return useQuery({ queryKey: key, enabled: computed(() => access.enabled.value && enabled()), retry: false, gcTime: 0, refetchOnWindowFocus: false, refetchOnReconnect: false, queryFn: ({ signal }) => access.read(key.value, () => key.value, () => request(signal)) })
}
const parents = reading(() => ['parents', parentPages.value.at(-1)], () => true, signal => fetchImports(parentPages.value.at(-1) || undefined, signal))
const reports = reading(() => ['reports', parent.value, corePages.value.at(-1)], () => !!parent.value, signal => fetchCoreChangeReports(parent.value, corePages.value.at(-1) || undefined, signal))
const captures = reading(() => ['captures', parent.value, historyPages.value.at(-1)], () => !!parent.value, signal => fetchHistoryCaptures(parent.value, historyPages.value.at(-1) || undefined, signal))
const bundles = reading(() => ['bundles', parent.value, bundlePages.value.at(-1)], () => !!parent.value, signal => fetchFamilyRefreshes(parent.value, bundlePages.value.at(-1) || undefined, signal))
const detail = reading(() => ['detail', selected.value], () => !!selected.value, async signal => { try { const value = await fetchFamilyRefresh(selected.value, signal); failures.value = 0; return value } catch (e) { if (!signal.aborted) failures.value++; throw e } })
const current = computed(() => access.enabled.value ? detail.data.value : undefined)
const plan = computed(() => current.value?.families.find(p => p.family === family.value))
const rows = reading<RefreshPage<RefreshItem | RefreshResult>>(() => ['rows', selected.value, plan.value?.plan_id, current.value?.bundle.revision, plan.value?.completed_units, mode.value, outcome.value, rowPages.value.at(-1)], () => !!plan.value, signal => mode.value === 'items'
  ? fetchRefreshItems(selected.value, family.value, plan.value!.plan_id, rowPages.value.at(-1) || undefined, signal, { outcome: outcome.value || undefined })
  : fetchRefreshResults(selected.value, family.value, plan.value!.plan_id, rowPages.value.at(-1) || undefined, signal, { outcome: outcome.value || undefined }))
const dirty = computed(() => patches.value.length > 0 || timezoneDirty.value)
const canPlan = computed(() => !!current.value && !current.value.bundle.confirmed_at && !['cancelled', 'completed'].includes(current.value.bundle.state) && !pending.value && !uncertain.value)
const canConfirm = computed(() => !!current.value && current.value.bundle.state === 'ready' && !!current.value.bundle.digest && confirmFamilies.value.length > 0 && current.value.families.filter(p => confirmFamilies.value.includes(p.family)).every(p => p.state === 'ready' && !!p.digest && !!p.expires_at && Date.parse(p.expires_at) > now.value) && !dirty.value && !pending.value && !uncertain.value)
type Intent = { action: 'prepare'; body: RefreshPrepare } | { action: 'plan'; id: string; body: RefreshReplan } | { action: 'confirm'; id: string; body: RefreshConfirm } | { action: 'cancel' | 'resume'; id: string; body: RefreshControl }
const intent = ref<Intent | null>(null)
function clearReview() { confirmation.value = null; inspected.value = ''; patches.value = []; timezone.value = ''; timezoneDirty.value = false; acknowledged.value = false; rowPages.value = [''] }
function reset() { parent.value = ''; selected.value = ''; coreReport.value = ''; historyCapture.value = ''; parentPages.value = ['']; corePages.value = ['']; historyPages.value = ['']; bundlePages.value = ['']; clearReview(); intent.value = null; pending.value = false; uncertain.value = false; actionError.value = null }
watch(access.identity, reset, { flush: 'sync' })
watch(access.scope, () => { clearReview(); intent.value = null; pending.value = false; uncertain.value = false; actionError.value = null }, { flush: 'sync' })
watch(parent, () => { selected.value = ''; coreReport.value = ''; historyCapture.value = ''; corePages.value = ['']; historyPages.value = ['']; bundlePages.value = ['']; clearReview() }, { flush: 'sync' })
watch([selected, family], clearReview, { flush: 'sync' })
watch(() => current.value?.bundle.revision, () => {  confirmation.value = null; acknowledged.value = false; inspected.value = ''; rowPages.value = [''] }, { flush: 'sync' })
watch(() => JSON.stringify([current.value?.bundle.id, current.value?.bundle.revision, current.value?.families.map(p => [p.family, p.state, p.digest])]), () => { confirmFamilies.value = current.value?.families.filter(p => p.state === 'ready').map(p => p.family) ?? [] })
watch(confirmFamilies, () => { acknowledged.value = false; confirmation.value = null })
watch(mode, () => { outcome.value = ''; rowPages.value = ['']; inspected.value = '' })
watch(outcome, () => { rowPages.value = ['']; inspected.value = '' })
watch(current, value => { if (value && !value.families.some(p => p.family === family.value)) family.value = value.families[0]!.family })
onBeforeUnmount(() => { disposed = true; clearInterval(timer) })
const timer = setInterval(() => { now.value = Date.now(); if (access.enabled.value && !pending.value && !uncertain.value && !dirty.value && !confirmation.value && failures.value < 3 && ['preparing', 'queued', 'running'].includes(current.value?.bundle.state ?? '')) void detail.refetch() }, 2000)
async function refresh() { if (!access.enabled.value || pending.value || uncertain.value || dirty.value) return; clearReview(); failures.value = 0; try { await props.refreshWorkspace(); await access.client.invalidateQueries({ queryKey: access.prefix.value }) } catch (error) { actionError.value = error } }
async function send(value: Intent) {
  if (!access.enabled.value || pending.value) return
  const authority = access.scope.value
  intent.value = value; pending.value = true; actionError.value = null; uncertain.value = false
  try {
    let receipt: RefreshPrepared
    switch (value.action) {
      case 'prepare': receipt = await prepareFamilyRefresh(value.body); break
      case 'plan': receipt = await replanFamilyRefresh(value.id, value.body); break
      case 'confirm': receipt = await confirmFamilyRefresh(value.id, value.body); break
      case 'cancel': receipt = await cancelFamilyRefresh(value.id, value.body); break
      case 'resume': receipt = await resumeFamilyRefresh(value.id, value.body); break
    }
    if (disposed || authority !== access.scope.value) return
    intent.value = null; confirmation.value = null; clearReview(); selected.value = receipt.bundle_id
    await Promise.all([detail.refetch(), bundles.refetch()])
  } catch (error) {
    if (disposed || authority !== access.scope.value) return
    actionError.value = error; uncertain.value = uncertainImportError(error)
    if (!uncertain.value) { intent.value = null; confirmation.value = null; void detail.refetch() }
  } finally { if (!disposed && authority === access.scope.value) pending.value = false }
}
function prepare() {
  if (!parent.value || !families.value.length || uncertain.value) return
  void send({ action: 'prepare', body: { request_id: crypto.randomUUID(), parent_import_id: parent.value, families: [...families.value], core_report_id: families.value.some(f => f !== 'history') ? coreReport.value || null : null, history_capture_id: families.value.includes('history') ? historyCapture.value || null : null } })
}
function applyMappings() {
  if (!canPlan.value || !dirty.value) return
  void send({ action: 'plan', id: selected.value, body: { request_id: crypto.randomUUID(), expected_revision: current.value!.bundle.revision, family: family.value, patches: JSON.parse(JSON.stringify(patches.value)) as RefreshPatch[], ...(timezoneDirty.value ? { source_timezone: timezone.value.trim() || null } : {}) } })
}
function previewConfirm() {
  if (!canConfirm.value || !acknowledged.value) return
  const value = current.value!
  confirmation.value = { request_id: crypto.randomUUID(), expected_revision: value.bundle.revision, bundle_digest: value.bundle.digest!, families: value.families.filter(p => confirmFamilies.value.includes(p.family)).map(p => ({ family: p.family, plan_id: p.plan_id, plan_revision: p.revision, plan_digest: p.digest!, expected_counts: { ...p.counts } })), acknowledged_exclusions: true }
}
function control(action: 'cancel' | 'resume') {
  if (!current.value || !plan.value || pending.value || uncertain.value) return
  void send({ action, id: selected.value, body: { request_id: crypto.randomUUID(), expected_revision: current.value.bundle.revision, families: [family.value] } })
}
</script>
<template>
  <Card
    v-if="access.enabled.value"
    class="min-w-0 space-y-4"
    data-testid="family-refresh"
  >
    <h2 class="text-section font-semibold">
      Refresh imported families
    </h2>
    <p class="text-small text-text-muted">
      Review retained captures for tags, custom fields, notes, tasks and historical facts. Preparation does not change CRM records. Confirm the exact changes before execution.
    </p>
    <p
      v-if="actionError"
      role="alert"
    >
      {{ describeApiError(actionError, 'The refresh request could not be completed.') }}
    </p>
    <div
      v-if="uncertain"
      role="alert"
      class="space-y-2"
    >
      <p>The request may have completed. Retry the same request to recover its receipt.</p><button
        type="button"
        :class="buttonClasses('secondary')"
        :disabled="pending"
        @click="intent && send(intent)"
      >
        Retry same request
      </button>
    </div>
    <fieldset
      :disabled="pending || uncertain"
      class="min-w-0 space-y-3"
    >
      <legend class="font-medium">
        Retained source selection
      </legend>
      <label class="block space-y-1"><span>Original People import</span><select
        v-model="parent"
        aria-label="Original People import"
        :class="INPUT_CLASSES"
      ><option value="">Choose an import</option><option
        v-for="item in parents.data.value?.imports ?? []"
        :key="item.id"
        :value="item.id"
        :disabled="!item.confirmed_plan_id"
      >{{ item.created_at }} · {{ item.state }} · {{ item.counts.imported_people }} People</option></select></label>
      <div class="flex flex-wrap gap-2">
        <button
          type="button"
          :class="buttonClasses('ghost')"
          :disabled="parentPages.length < 2"
          @click="parentPages.pop()"
        >
          Previous imports
        </button><button
          type="button"
          :class="buttonClasses('ghost')"
          :disabled="!parents.data.value?.next_cursor"
          @click="parents.data.value?.next_cursor && parentPages.push(parents.data.value.next_cursor)"
        >
          More imports
        </button>
      </div>
      <p
        v-if="parents.error.value"
        role="alert"
      >
        {{ describeApiError(parents.error.value, 'Could not load imports.') }}
      </p>
      <template v-if="parent">
        <div class="flex flex-wrap gap-4">
          <label
            v-for="(name, key) in names"
            :key="key"
            class="inline-flex items-center gap-2"
          ><input
            v-model="families"
            type="checkbox"
            :value="key"
          >{{ name }}</label>
        </div>
        <template v-if="families.some(f => f !== 'history')">
          <label class="block space-y-1"><span>Completed core comparison</span><select
            v-model="coreReport"
            aria-label="Completed core comparison"
            :class="INPUT_CLASSES"
          ><option value="">Choose retained core evidence</option><option
            v-for="report in reports.data.value?.reports ?? []"
            :key="report.id"
            :value="report.id"
            :disabled="report.state !== 'completed'"
          >{{ report.created_at }} · {{ report.state }}</option></select></label>
          <div class="flex flex-wrap gap-2">
            <button
              type="button"
              :class="buttonClasses('ghost')"
              :disabled="corePages.length < 2"
              @click="corePages.pop()"
            >
              Previous core comparisons
            </button><button
              type="button"
              :class="buttonClasses('ghost')"
              :disabled="!reports.data.value?.next_cursor"
              @click="reports.data.value?.next_cursor && corePages.push(reports.data.value.next_cursor)"
            >
              More core comparisons
            </button>
          </div>
        </template>
        <template v-if="families.includes('history')">
          <label class="block space-y-1"><span>Retained history capture</span><select
            v-model="historyCapture"
            aria-label="Retained history capture"
            :class="INPUT_CLASSES"
          ><option value="">Choose history evidence</option><option
            v-for="capture in captures.data.value?.captures ?? []"
            :key="capture.id"
            :value="capture.id"
            :disabled="capture.state !== 'completed_with_gaps'"
          >{{ capture.created_at }} · {{ capture.state }}</option></select></label>
          <div class="flex flex-wrap gap-2">
            <button
              type="button"
              :class="buttonClasses('ghost')"
              :disabled="historyPages.length < 2"
              @click="historyPages.pop()"
            >
              Previous history captures
            </button><button
              type="button"
              :class="buttonClasses('ghost')"
              :disabled="!captures.data.value?.next_cursor"
              @click="captures.data.value?.next_cursor && historyPages.push(captures.data.value.next_cursor)"
            >
              More history captures
            </button>
          </div>
        </template>
        <p
          v-if="reports.error.value || captures.error.value"
          role="alert"
        >
          {{ describeApiError(reports.error.value ?? captures.error.value, 'Could not load retained sources.') }}
        </p>
        <button
          type="button"
          :class="buttonClasses('primary')"
          :disabled="!families.length || (families.some(f => f !== 'history') && !coreReport) || (families.includes('history') && !historyCapture)"
          @click="prepare"
        >
          Prepare family refresh
        </button>
      </template>
    </fieldset>
    <template v-if="parent">
      <label class="block space-y-1"><span>Prepared refresh</span><select
        v-model="selected"
        aria-label="Prepared refresh"
        :class="INPUT_CLASSES"
        :disabled="pending || uncertain"
      ><option value="">Choose a refresh</option><option
        v-for="bundle in bundles.data.value?.items ?? []"
        :key="bundle.id"
        :value="bundle.id"
      >{{ bundle.created_at }} · {{ bundle.state }}</option></select></label>
      <div class="flex flex-wrap gap-2">
        <button
          type="button"
          :class="buttonClasses('ghost')"
          :disabled="bundlePages.length < 2 || pending || uncertain"
          @click="bundlePages.pop()"
        >
          Previous refreshes
        </button><button
          type="button"
          :class="buttonClasses('ghost')"
          :disabled="!bundles.data.value?.next_cursor || pending || uncertain"
          @click="bundles.data.value?.next_cursor && bundlePages.push(bundles.data.value.next_cursor)"
        >
          More refreshes
        </button><button
          type="button"
          :class="buttonClasses('secondary')"
          :disabled="pending || uncertain || dirty"
          @click="refresh"
        >
          Refresh review
        </button>
      </div>
      <p
        v-if="detail.error.value || bundles.error.value"
        role="alert"
      >
        {{ describeApiError(detail.error.value ?? bundles.error.value, 'Reload the refresh review.') }}
      </p>
    </template>
    <template v-if="current">
      <p role="status">
        Refresh status: {{ current.bundle.state }}
      </p>
      <div class="grid gap-3 md:grid-cols-3">
        <section
          v-for="p in current.families"
          :key="p.plan_id"
          class="min-w-0 space-y-2 rounded border border-border p-3"
        >
          <h3 class="font-medium">
            {{ names[p.family] }}
          </h3>
          <label
            v-if="!current.bundle.confirmed_at && p.state === 'ready'"
            class="flex items-center gap-2"
          ><input
            v-model="confirmFamilies"
            type="checkbox"
            :value="p.family"
            :disabled="pending || uncertain"
          >Include in confirmation</label><p>{{ p.state }} · {{ p.phase }}</p><p
            v-if="p.pause_reason"
            class="break-all"
          >
            {{ p.pause_reason.replaceAll('_', ' ') }}
          </p>
          <dl class="space-y-1">
            <div
              v-for="(name, key) in countNames"
              :key="key"
              class="flex flex-wrap justify-between gap-2"
            >
              <dt>{{ name }}</dt><dd>{{ p.counts[key] }}</dd>
            </div>
          </dl>
          <p class="text-small">
            Settled: {{ p.completed_units }} / {{ p.counts.units }}
          </p><p class="text-small">
            {{ p.retained_bytes }} bytes retained · {{ p.reserved_bytes }} reserved · {{ p.run_byte_limit }} limit
          </p>
        </section>
      </div>
      <div
        class="flex flex-wrap gap-2"
        aria-label="Refresh family"
      >
        <button
          v-for="p in current.families"
          :key="p.family"
          type="button"
          :class="buttonClasses(family === p.family ? 'primary' : 'secondary')"
          :disabled="pending || uncertain || dirty"
          :aria-pressed="family === p.family"
          @click="family = p.family"
        >
          {{ names[p.family] }}
        </button>
      </div>
      <template v-if="plan">
        <FamilyRefreshMappings
          v-if="family !== 'history'"
          :key="plan.plan_id"
          :bundle-id="selected"
          :plan-id="plan.plan_id"
          :revision="plan.revision"
          :family="family"
          :progress-version="JSON.stringify([plan.phase, plan.state, plan.counts.units])"
          :disabled="!canPlan"
          @change="patches = $event"
        />
        <label
          v-if="family === 'activity' && canPlan"
          class="block space-y-1"
        ><span>Source timezone (IANA name, when required)</span><input
          v-model="timezone"
          :class="INPUT_CLASSES"
          placeholder="America/Los_Angeles"
          @input="timezoneDirty = true"
        ></label>
        <button
          v-if="canPlan"
          type="button"
          :class="buttonClasses('secondary')"
          :disabled="!dirty"
          @click="applyMappings"
        >
          Apply mapping choices
        </button>
        <div class="flex flex-wrap items-center gap-2">
          <button
            type="button"
            :class="buttonClasses(mode === 'items' ? 'primary' : 'secondary')"
            @click="mode = 'items'"
          >
            Preview units
          </button><button
            type="button"
            :class="buttonClasses(mode === 'results' ? 'primary' : 'secondary')"
            @click="mode = 'results'"
          >
            Committed results
          </button><label>Outcome <select
            v-model="outcome"
            :class="INPUT_CLASSES"
          ><option value="">All outcomes</option><option
            v-for="value in (mode === 'items' ? ['insert', 'update', 'already_current', 'correction', 'held', 'excluded'] : ['applied', 'already_current', 'held', 'excluded'])"
            :key="value"
            :value="value"
          >{{ value.replaceAll('_', ' ') }}</option></select></label>
        </div>
        <p
          v-if="rows.error.value"
          role="alert"
        >
          {{ describeApiError(rows.error.value, 'Reload the unit review.') }}
        </p><p
          v-if="rows.isFetching.value"
          role="status"
        >
          Loading units…
        </p>
        <ul class="space-y-2">
          <li
            v-for="row in rows.data.value?.items ?? []"
            :key="row.id"
            class="flex min-w-0 flex-wrap items-start justify-between gap-2 rounded border border-border p-3"
          >
            <span>{{ row.kind }} · {{ row.outcome.replaceAll('_', ' ') }}<span v-if="row.reason"> · {{ row.reason.replaceAll('_', ' ') }}</span></span><button
              type="button"
              :class="buttonClasses('ghost')"
              @click="inspected = 'item_id' in row ? row.item_id : row.id"
            >
              Review fields
            </button>
          </li>
        </ul>
        <div class="flex flex-wrap gap-2">
          <button
            type="button"
            :class="buttonClasses('ghost')"
            :disabled="rowPages.length < 2 || rows.isFetching.value"
            @click="inspected = ''; rowPages.pop()"
          >
            Previous units
          </button><button
            type="button"
            :class="buttonClasses('ghost')"
            :disabled="!rows.data.value?.next_cursor || rows.isFetching.value"
            @click="inspected = ''; rows.data.value?.next_cursor && rowPages.push(rows.data.value.next_cursor)"
          >
            More units
          </button>
        </div>
        <FamilyRefreshFields
          v-if="inspected"
          :key="inspected"
          :bundle-id="selected"
          :item-id="inspected"
          :revision="current.bundle.revision"
        />
        <div class="flex flex-wrap gap-2">
          <button
            v-if="plan.state === 'paused'"
            type="button"
            :class="buttonClasses('secondary')"
            :disabled="pending || uncertain"
            @click="control('resume')"
          >
            Resume {{ names[family] }}
          </button><button
            v-if="!['completed', 'cancelled'].includes(plan.state)"
            type="button"
            :class="buttonClasses('secondary')"
            :disabled="pending || uncertain"
            @click="control('cancel')"
          >
            Cancel {{ names[family] }}
          </button>
        </div>
      </template>
      <template v-if="canConfirm">
        <label class="flex items-start gap-2"><input
          v-model="acknowledged"
          type="checkbox"
        ><span>I reviewed every selected family, including removals, clears, completions, reopens, corrections, holds and exclusions. Unchecked families will be cancelled.</span></label><button
          type="button"
          :class="buttonClasses('primary')"
          :disabled="!acknowledged"
          @click="previewConfirm"
        >
          Review confirmation
        </button>
      </template>
    </template>
    <Dialog
      :visible="!!confirmation"
      modal
      :closable="false"
      :close-on-escape="!pending && !uncertain"
      :dismissable-mask="false"
      :aria-labelledby="confirmTitle"
      :pt="{ ...dialogPt(), root: { class: 'glass-panel w-full max-w-xl max-h-[calc(100dvh-2rem)] flex flex-col' }, content: { class: 'min-h-0 overflow-y-auto px-5 py-4' }, footer: { class: 'flex shrink-0 flex-wrap justify-end gap-3 px-5 pb-5 pt-2' } }"
      @update:visible="value => { if (!value && !pending && !uncertain) confirmation = null }"
    >
      <template #header>
        <h2
          :id="confirmTitle"
          class="text-section font-medium"
        >
          Confirm family refresh
        </h2>
      </template>
      <p>Apply the reviewed changes to imported CRM records. Each unit commits independently; cancellation preserves work already committed.</p>
      <div
        v-for="p in confirmation?.families ?? []"
        :key="p.family"
        class="mt-3 space-y-1"
      >
        <h3 class="font-medium">
          {{ names[p.family] }}
        </h3><p
          v-for="(name, key) in countNames"
          :key="key"
        >
          {{ name }}: {{ p.expected_counts[key] }}
        </p>
      </div>
      <p
        v-if="actionError"
        role="alert"
      >
        {{ describeApiError(actionError, 'The confirmation response could not be verified.') }}
      </p>
      <p v-if="uncertain">
        The request may have completed. Retry the same confirmation to recover its receipt.
      </p>
      <template #footer>
        <button
          type="button"
          :class="buttonClasses('secondary')"
          :disabled="pending || uncertain"
          @click="confirmation = null"
        >
          Back to review
        </button><button
          type="button"
          :class="buttonClasses('primary')"
          :disabled="pending"
          @click="confirmation && send({ action: 'confirm', id: selected, body: confirmation })"
        >
          {{ uncertain ? 'Retry same confirmation' : 'Apply reviewed refresh' }}
        </button>
      </template>
    </Dialog>
  </Card>
</template>
