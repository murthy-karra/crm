<script setup lang="ts">
// Current administrators assess source access, retain snapshots and preview
// People imports before explicitly entering workspace review.
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { useQuery, useQueryClient } from '@tanstack/vue-query'
import { KeyRound, RefreshCw, ShieldCheck, X } from 'lucide-vue-next'
import PageHeader from '../components/PageHeader.vue'
import Card from '../components/Card.vue'
import CoreSnapshotPanel from '../components/migration/CoreSnapshotPanel.vue'
import PeopleImportPanel from '../components/migration/PeopleImportPanel.vue'
import { refreshWorkspace, useWorkspacePending, useWorkspaceEpoch } from '../workspaceLifecycle'
import FormField from '../components/FormField.vue'
import { queryKeys, useAuthSessionLifetime, useMe } from '../api/queries'
import { ApiError } from '../api/client'
import {
  cancelFubAssessment,
  createFubConnection,
  disconnectFubConnection,
  fetchFubMigrationSummary,
  replaceFubCredential,
  retryFubAssessment,
  startFubAssessment,
  type FubAssessment,
  type FubAssessmentCheck,
} from '../api/migrations'
import { buttonClasses, INPUT_CLASSES } from '../lib/controls'
import { describeApiError } from '../lib/errors'

const { data: me } = useMe()
const sessionLifetime = useAuthSessionLifetime()
const workspacePending = useWorkspacePending()
const workspaceEpoch = useWorkspaceEpoch()
const queryClient = useQueryClient()
const orgId = computed(() => me.value?.organization?.id ?? '')
const actorId = computed(() => me.value?.user.id ?? '')
const scope = computed(() => [orgId.value, actorId.value, sessionLifetime.value] as const)
const migrationKey = computed(() => queryKeys.migration(orgId.value, actorId.value, sessionLifetime.value))
const canRead = computed(() => !workspacePending.value && me.value?.organization?.role === 'admin' && orgId.value !== '' && actorId.value !== '')

const { data: summary, error: summaryError, isPending, isFetching, refetch } = useQuery({
  queryKey: migrationKey,
  queryFn: ({ signal }) => fetchFubMigrationSummary(signal),
  enabled: canRead,
  retry: false,
  refetchOnWindowFocus: 'always',
  refetchOnReconnect: 'always',
  // The durable summary remains the UI authority. Failed refreshes back off,
  // while a terminal or paused assessment stops polling immediately.
  refetchInterval: (query) => {
    const state = query.state.data?.active_assessment?.state
    if (!isPollingState(state)) return false
    return Math.min(30_000, 2_000 * (2 ** Math.min(query.state.fetchFailureCount, 4)))
  },
})

const apiKey = ref('')
const credentialPending = ref(false)
const credentialError = ref<string | null>(null)
const credentialRecovery = ref(false)
const actionPending = ref<'assess' | 'retry' | 'cancel' | 'disconnect' | null>(null)
const actionError = ref<string | null>(null)
const selectedReport = ref<'current' | 'previous'>('current')
const snapshotSourceBusy = ref(false)
let operationGeneration = 0
let disposed = false

const PROFILE_CHECKS = new Set([
  'identity', 'people_excluding_trash', 'people_including_trash', 'users', 'stages', 'custom_fields',
])

const connection = computed(() => summary.value?.connection ?? null)
const currentAssessment = computed(() => summary.value?.active_assessment ?? null)
const previousReport = computed(() => summary.value?.latest_report ?? null)
const shownReport = computed(() => {
  if (currentAssessment.value && selectedReport.value === 'current') return currentAssessment.value
  return previousReport.value ?? currentAssessment.value ?? summary.value?.latest_assessment ?? null
})
const shownReportTitle = computed(() => {
  if (currentAssessment.value && selectedReport.value === 'current') return 'Current report'
  if (currentAssessment.value && previousReport.value) return 'Previous completed report'
  if (shownReport.value?.state === 'completed') return 'Latest completed report'
  return 'Latest assessment'
})
const completedChecks = computed(() => completedProfileChecks(currentAssessment.value))

function isPollingState(state: string | undefined): boolean {
  return state === 'queued' || state === 'running' || state === 'waiting_retry'
}

function requestId(): string {
  return crypto.randomUUID()
}

function clearCredential() {
  apiKey.value = ''
}

function invalidateSummary() {
  return queryClient.invalidateQueries({ queryKey: migrationKey.value, exact: true })
}

// Changing user, Organization, or session invalidates the old scoped branch.
// The before/after comparison around every asynchronous action below prevents
// a stale completion from resurrecting a prior tenant's cache.
watch(scope, (next, previous) => {
  operationGeneration += 1
  credentialPending.value = false
  actionPending.value = null
  clearCredential()
  credentialError.value = null
  credentialRecovery.value = false
  actionError.value = null
  if (previous && next.join(':') !== previous.join(':')) {
    queryClient.removeQueries({ queryKey: queryKeys.migration(previous[0], previous[1], previous[2]), exact: true })
  }
})
watch(canRead, allowed => {
  if (allowed) return
  operationGeneration++; clearCredential(); credentialPending.value = false; actionPending.value = null
  credentialError.value = null; credentialRecovery.value = false; actionError.value = null
}, { flush: 'sync' })
watch(currentAssessment, (assessment) => {
  if (!assessment) selectedReport.value = 'previous'
})
onBeforeUnmount(() => {
  disposed = true
  operationGeneration += 1
  clearCredential()
})

function sameScope(expected: readonly [string, string, number]) {
  return !disposed && expected[0] === orgId.value && expected[1] === actorId.value && expected[2] === sessionLifetime.value
}

function currentOperation(expected: readonly [string, string, number], generation: number) {
  return canRead.value && sameScope(expected) && generation === operationGeneration
}

async function submitCredential() {
  if (credentialPending.value || apiKey.value.trim() === '') return
  const expectedScope = scope.value
  const generation = operationGeneration
  const secret = apiKey.value
  clearCredential() // Clear before network I/O and never retain it in a mutation/cache.
  credentialPending.value = true
  credentialError.value = null
  credentialRecovery.value = false
  try {
    const current = connection.value
    if (current) {
      await replaceFubCredential(current.id, current.revision, requestId(), secret)
    } else {
      await createFubConnection(requestId(), secret)
    }
    if (currentOperation(expectedScope, generation)) await invalidateSummary()
  } catch (error) {
    if (!currentOperation(expectedScope, generation)) return
    if (error instanceof ApiError && error.status === 0) {
      credentialRecovery.value = true
      credentialError.value = 'We could not confirm the credential submission. Recover the connection status before entering the key again.'
      void refetch()
    } else {
      credentialError.value = error instanceof ApiError && error.code === 'invalid_credential'
        ? 'The API key was rejected. Enter a current Follow Up Boss API key.'
        : describeApiError(error, 'Could not save this credential.')
    }
  } finally {
    if (currentOperation(expectedScope, generation)) credentialPending.value = false
  }
}

async function assess() {
  const current = connection.value
  if (!current || actionPending.value) return
  const expectedScope = scope.value
  const generation = operationGeneration
  actionPending.value = 'assess'
  actionError.value = null
  try {
    await startFubAssessment(current.id, current.revision, requestId())
    if (currentOperation(expectedScope, generation)) await invalidateSummary()
  } catch (error) {
    if (currentOperation(expectedScope, generation)) actionError.value = describeApiError(error, 'Could not start the assessment.')
  } finally {
    if (currentOperation(expectedScope, generation)) actionPending.value = null
  }
}

async function retry() {
  if (!currentAssessment.value || actionPending.value) return
  const expectedScope = scope.value
  const generation = operationGeneration
  actionPending.value = 'retry'
  actionError.value = null
  try {
    await retryFubAssessment(currentAssessment.value.id)
    if (currentOperation(expectedScope, generation)) await invalidateSummary()
  } catch (error) {
    if (currentOperation(expectedScope, generation)) actionError.value = describeApiError(error, 'Could not retry this assessment.')
  } finally {
    if (currentOperation(expectedScope, generation)) actionPending.value = null
  }
}

async function cancel() {
  if (!currentAssessment.value || actionPending.value) return
  const expectedScope = scope.value
  const generation = operationGeneration
  actionPending.value = 'cancel'
  actionError.value = null
  try {
    await cancelFubAssessment(currentAssessment.value.id)
    if (currentOperation(expectedScope, generation)) await invalidateSummary()
  } catch (error) {
    if (currentOperation(expectedScope, generation)) actionError.value = describeApiError(error, 'Could not cancel this assessment.')
  } finally {
    if (currentOperation(expectedScope, generation)) actionPending.value = null
  }
}

async function disconnect() {
  const current = connection.value
  if (!current || actionPending.value) return
  const expectedScope = scope.value
  const generation = operationGeneration
  actionPending.value = 'disconnect'
  actionError.value = null
  try {
    await disconnectFubConnection(current.id)
    if (currentOperation(expectedScope, generation)) await invalidateSummary()
  } catch (error) {
    if (currentOperation(expectedScope, generation)) actionError.value = describeApiError(error, 'Could not disconnect this source account.')
  } finally {
    if (currentOperation(expectedScope, generation)) actionPending.value = null
  }
}

function formattedTime(value: string | null | undefined) {
  if (!value) return 'Unknown'
  const date = new Date(value)
  return Number.isNaN(date.valueOf()) ? 'Unknown' : date.toLocaleString()
}

function checkLabel(key: string) {
  return key.replaceAll('_', ' ').replace(/\b\w/g, (letter) => letter.toUpperCase())
}

function statusLabel(state: string) {
  return state.replaceAll('_', ' ')
}

function summaryFor(report: FubAssessment | null) {
  if (!report) return null
  return `${completedProfileChecks(report)} of ${PROFILE_CHECKS.size} source checks completed`
}

function completedProfileChecks(report: FubAssessment | null) {
  return report?.checks.filter((check) => PROFILE_CHECKS.has(check.check_key) && check.state === 'completed').length ?? 0
}

function reasonLabel(code: string) {
  const labels: Record<string, string> = {
    bounded_first_page_only: 'Only the first source page was read',
    not_checked_by_profile: 'Not checked in this assessment',
    source_total_unknown_or_inconsistent: 'Source total is unknown or inconsistent',
    destination_model_available_mapping_pending: 'Destination model exists; mapping still needs review',
    destination_semantics_require_review: 'Destination meaning needs review',
    destination_capability_not_available: 'Destination capability is not available yet',
    retry_exhausted: 'Retries were exhausted',
    source_unavailable: 'The source is temporarily unavailable',
    initiator_not_authorized: 'The initiating administrator no longer has access',
  }
  return labels[code] ?? statusLabel(code)
}

function pauseLabel(code: string) {
  return reasonLabel(code)
}

function destinationLabel(report: FubAssessment) {
  if (report.destination_organization_id === orgId.value && me.value?.organization) {
    return me.value.organization.name
  }
  return 'Unknown Organization'
}

function nextActionLabel(action: string) {
  const labels: Record<string, string> = {
    'Review the check failure and retry after resolving access or source availability.': 'Resolve the source access issue, then retry this assessment.',
    'Assess this family in the later inventory slice.': 'Assess this family during detailed inventory.',
    'Review full inventory and mappings in the next migration slice.': 'Review detailed inventory and destination mappings before migration.',
  }
  return labels[action] ?? action
}

function coverageLabel(check: FubAssessmentCheck) {
  if (check.coverage === 'not_checked') return 'Not checked by this profile'
  if (check.coverage === 'unavailable') return 'Unavailable'
  if (check.coverage === 'complete_for_query') return 'Complete for query'
  return 'Partial'
}

function checkStatusLabel(check: FubAssessmentCheck) {
  return PROFILE_CHECKS.has(check.check_key) ? statusLabel(check.state) : 'Not checked'
}
</script>

<template>
  <div>
    <PageHeader
      title="Migration"
      subtitle="Assess Follow Up Boss, capture core records and review migration evidence. Explicitly confirm a People import for admin review before later activation."
    />

    <Card class="mb-6">
      <div class="flex flex-col gap-4 sm:flex-row sm:items-start sm:justify-between">
        <div>
          <h2 class="text-section font-semibold text-text">
            Follow Up Boss connection
          </h2>
          <p class="mt-1 max-w-2xl text-body text-text-muted">
            The API key is saved encrypted for assessments and confirmed snapshots. The assessment is read-only and does not copy People or other source records into this Organization.
          </p>
        </div>
        <span
          class="rounded-full bg-surface-2 px-3 py-1 text-small font-medium text-text"
          data-testid="connection-status"
        >
          {{ connection ? statusLabel(connection.status) : 'Not connected' }}
        </span>
      </div>

      <div
        v-if="isPending"
        class="mt-5 text-body text-text-muted"
      >
        Loading connection status…
      </div>
      <div
        v-else-if="summaryError"
        class="mt-5"
      >
        <p
          class="text-body text-danger"
          role="alert"
        >
          {{ describeApiError(summaryError, 'Could not load the migration status.') }}
        </p>
        <button
          type="button"
          :class="buttonClasses('secondary')"
          class="mt-3"
          :disabled="isFetching"
          data-testid="reload-fub-summary"
          @click="() => void refetch()"
        >
          Reload status
        </button>
      </div>
      <template v-else>
        <dl
          v-if="connection"
          class="mt-5 grid gap-3 text-body sm:grid-cols-3"
        >
          <div>
            <dt class="text-text-muted">
              Source account
            </dt><dd class="font-medium text-text">
              {{ connection.source_display_name ?? 'Unknown' }}
            </dd>
          </div>
          <div>
            <dt class="text-text-muted">
              Access scope
            </dt><dd class="font-medium text-text">
              {{ connection.source_access_scope }}
            </dd>
          </div>
          <div>
            <dt class="text-text-muted">
              Updated
            </dt><dd class="font-medium text-text">
              {{ formattedTime(connection.updated_at) }}
            </dd>
          </div>
        </dl>

        <form
          class="mt-5 max-w-xl"
          @submit.prevent="submitCredential"
        >
          <FormField
            v-slot="{ id }"
            :label="connection ? 'Replace Follow Up Boss API key' : 'Follow Up Boss API key'"
            bare
          >
            <input
              :id="id"
              v-model="apiKey"
              type="password"
              autocomplete="new-password"
              :class="INPUT_CLASSES"
              :disabled="credentialPending"
              aria-describedby="credential-help"
              data-testid="fub-api-key"
            >
          </FormField>
          <p
            id="credential-help"
            class="mt-1.5 text-small text-text-muted"
          >
            The key is cleared from this page after submission and when you leave.
          </p>
          <p
            v-if="credentialError"
            class="mt-2 text-body text-danger"
            role="alert"
            data-testid="credential-error"
          >
            {{ credentialError }}
          </p>
          <div class="mt-3 flex flex-wrap gap-2">
            <button
              type="submit"
              :class="buttonClasses('primary')"
              :disabled="credentialPending || apiKey.trim() === ''"
              data-testid="save-fub-credential"
            >
              <KeyRound
                class="h-4 w-4"
                aria-hidden="true"
              /> {{ credentialPending ? 'Saving…' : (connection ? 'Replace key' : 'Connect account') }}
            </button>
            <button
              v-if="credentialRecovery"
              type="button"
              :class="buttonClasses('secondary')"
              :disabled="isFetching"
              data-testid="recover-fub-status"
              @click="() => void refetch()"
            >
              <RefreshCw
                class="h-4 w-4"
                aria-hidden="true"
              /> Recover status
            </button>
            <button
              v-if="connection"
              type="button"
              :class="buttonClasses('danger')"
              :disabled="actionPending !== null"
              data-testid="disconnect-fub"
              @click="disconnect"
            >
              <X
                class="h-4 w-4"
                aria-hidden="true"
              /> {{ actionPending === 'disconnect' ? 'Disconnecting…' : 'Disconnect' }}
            </button>
          </div>
        </form>
      </template>
    </Card>

    <PeopleImportPanel :refresh-workspace="refreshWorkspace" />

    <CoreSnapshotPanel
      v-if="canRead"
      :key="`${scope.join(':')}:${workspaceEpoch}`"
      :connection="connection"
      :assessment-busy="isPollingState(currentAssessment?.state)"
      @source-busy="snapshotSourceBusy = $event"
    />

    <Card class="mb-6">
      <div class="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <h2 class="text-section font-semibold text-text">
            Assessment
          </h2>
          <p class="mt-1 text-body text-text-muted">
            Six bounded source checks run now; thirteen later migration families remain visible as not checked.
          </p>
        </div>
        <button
          type="button"
          :class="buttonClasses('primary')"
          :disabled="!connection || connection.status !== 'connected' || currentAssessment !== null || actionPending !== null || snapshotSourceBusy"
          data-testid="assess-fub"
          @click="assess"
        >
          <ShieldCheck
            class="h-4 w-4"
            aria-hidden="true"
          /> {{ actionPending === 'assess' ? 'Starting…' : 'Assess account' }}
        </button>
      </div>
      <p
        v-if="actionError"
        class="mt-3 text-body text-danger"
        role="alert"
      >
        {{ actionError }}
      </p>

      <div
        v-if="currentAssessment"
        class="mt-5 rounded-lg border border-border bg-surface-0 p-4"
        data-testid="active-assessment"
      >
        <div class="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
          <div>
            <h3 class="font-medium text-text">
              Current assessment
            </h3>
            <p class="mt-1 text-body text-text-muted">
              {{ statusLabel(currentAssessment.state) }} · {{ completedChecks }} of {{ PROFILE_CHECKS.size }} source checks completed
            </p>
            <p
              v-if="currentAssessment.pause_reason"
              class="mt-1 text-body text-danger"
            >
              Paused: {{ pauseLabel(currentAssessment.pause_reason) }}
            </p>
          </div>
          <div class="flex gap-2">
            <button
              v-if="currentAssessment.state === 'paused'"
              type="button"
              :class="buttonClasses('secondary')"
              :disabled="actionPending !== null"
              data-testid="retry-fub"
              @click="retry"
            >
              {{ actionPending === 'retry' ? 'Retrying…' : 'Retry' }}
            </button>
            <button
              type="button"
              :class="buttonClasses('secondary')"
              :disabled="actionPending !== null"
              data-testid="cancel-fub"
              @click="cancel"
            >
              {{ actionPending === 'cancel' ? 'Cancelling…' : 'Cancel' }}
            </button>
          </div>
        </div>
        <p
          class="sr-only"
          aria-live="polite"
          data-testid="assessment-progress-live"
        >
          Assessment {{ statusLabel(currentAssessment.state) }}. {{ completedChecks }} of {{ PROFILE_CHECKS.size }} source checks completed.
        </p>
      </div>
      <p
        v-else-if="connection"
        class="mt-5 text-body text-text-muted"
      >
        No assessment is active. Start one when this Organization is ready to inspect the bounded source profile.
      </p>
    </Card>

    <Card
      v-if="previousReport && currentAssessment"
      class="mb-6"
      data-testid="previous-report"
    >
      <h2 class="text-section font-semibold text-text">
        Previous completed report
      </h2>
      <p class="mt-1 text-body text-text-muted">
        Completed {{ formattedTime(previousReport.completed_at) }} · {{ summaryFor(previousReport) }}. This remains the prior result while the current assessment runs.
      </p>
      <button
        type="button"
        :class="buttonClasses('secondary')"
        class="mt-3"
        data-testid="toggle-previous-report"
        @click="selectedReport = selectedReport === 'previous' ? 'current' : 'previous'"
      >
        {{ selectedReport === 'previous' ? 'Show current report' : 'Inspect previous report' }}
      </button>
    </Card>

    <Card
      v-if="shownReport"
      data-testid="assessment-report"
    >
      <div class="flex flex-col gap-1 sm:flex-row sm:items-start sm:justify-between">
        <div>
          <h2 class="text-section font-semibold text-text">
            {{ shownReportTitle }}
          </h2>
          <p class="mt-1 text-body text-text-muted">
            Source: {{ shownReport.source_display_name ?? 'Unknown' }} · Access scope: {{ shownReport.source_access_scope }} · Destination: {{ destinationLabel(shownReport) }} · Profile: {{ shownReport.profile_version }}
          </p>
          <p class="mt-1 text-small text-text-muted">
            State: {{ statusLabel(shownReport.state) }} · Started: {{ formattedTime(shownReport.started_at) }} · Completed: {{ formattedTime(shownReport.completed_at) }}
          </p>
        </div>
        <span class="text-small text-text-muted">{{ summaryFor(shownReport) }}</span>
      </div>
      <div class="mt-4 overflow-x-auto">
        <table class="w-full min-w-[900px] border-collapse text-left text-small">
          <thead class="border-b border-border text-text-muted">
            <tr>
              <th class="px-3 py-2 font-medium">
                Family
              </th><th class="px-3 py-2 font-medium">
                Status
              </th><th class="px-3 py-2 font-medium">
                FUB total
              </th><th class="px-3 py-2 font-medium">
                Retrieved
              </th><th class="px-3 py-2 font-medium">
                Coverage
              </th><th class="px-3 py-2 font-medium">
                Destination
              </th><th class="px-3 py-2 font-medium">
                Observed
              </th><th class="px-3 py-2 font-medium">
                Reason / next action
              </th>
            </tr>
          </thead>
          <tbody>
            <tr
              v-for="check in shownReport.checks"
              :key="check.check_key"
              class="border-b border-border/70 align-top"
            >
              <td class="px-3 py-3 font-medium text-text">
                {{ checkLabel(check.check_key) }}
              </td>
              <td class="px-3 py-3 text-text">
                {{ checkStatusLabel(check) }}
              </td>
              <td class="px-3 py-3 text-text">
                {{ check.reported_total ?? 'Unknown' }}
              </td>
              <td class="px-3 py-3 text-text">
                {{ check.retrieved_count }}
              </td>
              <td class="px-3 py-3 text-text">
                {{ coverageLabel(check) }}
              </td>
              <td class="px-3 py-3 text-text">
                {{ statusLabel(check.destination_readiness) }}
              </td>
              <td class="px-3 py-3 text-text">
                {{ formattedTime(check.observed_at) }}
              </td>
              <td class="px-3 py-3 text-text">
                <span class="block">{{ check.reason_codes.map(reasonLabel).join(', ') || 'Unknown' }}</span><span class="mt-1 block text-text-muted">{{ nextActionLabel(check.next_action) }}</span>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
      <p class="mt-4 text-small text-text-muted">
        Live source validation is unverified. Counts are source-reported per recorded query and are not a migration forecast.
      </p>
    </Card>
  </div>
</template>
