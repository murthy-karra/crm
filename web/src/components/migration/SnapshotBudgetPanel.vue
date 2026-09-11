<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import ConfirmDialog from '../ConfirmDialog.vue'
import FormField from '../FormField.vue'
import { ApiError } from '../../api/client'
import { increaseSnapshotBudget, type CoreSnapshot, type SnapshotBudgetRequest } from '../../api/snapshots'
import { buttonClasses, INPUT_CLASSES } from '../../lib/controls'
import { describeApiError } from '../../lib/errors'
import { bytesToGiB, formatBytes, gibToBytes } from './format'

const props = defineProps<{ snapshot: CoreSnapshot }>()
const emit = defineEmits<{ refresh: []; accessDenied: [] }>()
const expanded = ref(false)
const runInput = ref('')
const orgInput = ref('')
const review = ref<{ body: SnapshotBudgetRequest; message: string } | null>(null)
const pending = ref(false)
const error = ref<string | null>(null)
const receipt = ref<string | null>(null)
let disposed = false
onBeforeUnmount(() => { disposed = true })
const canIncrease = computed(() => BigInt(props.snapshot.run_byte_limit) < BigInt(props.snapshot.run_ceiling_bytes)
  || BigInt(props.snapshot.org_byte_limit) < BigInt(props.snapshot.org_ceiling_bytes))
function resetInputs() {
  runInput.value = bytesToGiB(props.snapshot.run_byte_limit)
  orgInput.value = bytesToGiB(props.snapshot.org_byte_limit)
}
watch(() => props.snapshot.id, () => { resetInputs(); review.value = null; expanded.value = false; error.value = null; receipt.value = null }, { immediate: true })
function open() { resetInputs(); expanded.value = !expanded.value; error.value = null; receipt.value = null }
function prepareReview() {
  error.value = null
  const run = gibToBytes(runInput.value)
  const org = gibToBytes(orgInput.value)
  const s = props.snapshot
  if (!run || !org) { error.value = 'Enter positive GiB amounts that represent whole bytes.'; return }
  if (BigInt(run) < BigInt(s.run_byte_limit) || BigInt(org) < BigInt(s.org_byte_limit)
    || (run === s.run_byte_limit && org === s.org_byte_limit)) {
    error.value = 'Increase at least one allowance. Existing allowances cannot be reduced here.'; return
  }
  if (BigInt(run) > BigInt(s.run_ceiling_bytes) || BigInt(org) > BigInt(s.org_ceiling_bytes)) {
    error.value = 'These amounts exceed the deployment ceilings. An operator must raise the configuration ceilings first.'; return
  }
  // Freeze exactly what the administrator reviews, including all revisions.
  review.value = {
    body: { request_id: crypto.randomUUID(), expected_run_budget_revision: s.run_budget_revision,
      expected_org_budget_revision: s.org_budget_revision, expected_policy_revision: s.policy_revision,
      run_byte_limit: run, org_byte_limit: org },
    message: `Run allowance: ${s.run_byte_limit} → ${run} bytes (${bytesToGiB(run)} GiB). Organization allowance: ${s.org_byte_limit} → ${org} bytes (${bytesToGiB(org)} GiB). Run retained/reserved: ${s.retained_bytes}/${s.reserved_bytes} bytes; Organization retained/reserved: ${s.org_retained_bytes}/${s.org_reserved_bytes} bytes. Deployment ceilings: run ${s.run_ceiling_bytes}, Organization ${s.org_ceiling_bytes} bytes. Run revision ${s.run_budget_revision}; Organization revision ${s.org_budget_revision}; deployment policy ${s.policy_revision}. Increasing these allowances does not resume capture or preview.`,
  }
}
async function confirm() {
  if (!review.value || pending.value) return
  const id = props.snapshot.id
  const reviewed = review.value
  pending.value = true; error.value = null
  try {
    const result = await increaseSnapshotBudget(id, reviewed.body)
    if (disposed || props.snapshot.id !== id) return
    receipt.value = `Allowances updated to ${formatBytes(result.budget.run_byte_limit)} per run and ${formatBytes(result.budget.org_byte_limit)} for this Organization. This change started no work.`
    review.value = null; expanded.value = false; emit('refresh')
  } catch (cause) {
    if (disposed || props.snapshot.id !== id) return
    if (cause instanceof ApiError && [401, 403].includes(cause.status)) { review.value = null; emit('accessDenied'); return }
    if (cause instanceof ApiError && cause.status === 409) {
      review.value = null
      error.value = 'The allowances or deployment policy changed. Refresh the current values and review a new increase.'
      emit('refresh')
    } else error.value = describeApiError(cause, 'Could not confirm the increase. Retry this reviewed request or refresh the current values.')
  } finally { if (!disposed && props.snapshot.id === id) pending.value = false }
}
</script>

<template>
  <section
    class="mt-5 border-t border-border pt-4"
    aria-label="Snapshot storage allowances"
  >
    <div class="flex flex-wrap items-center justify-between gap-2">
      <h3 class="text-body font-medium text-text">
        Storage allowances
      </h3>
      <button
        v-if="snapshot.actions.includes('increase_budget')"
        type="button"
        :class="buttonClasses('secondary')"
        :disabled="pending"
        data-testid="snapshot-budget-open"
        @click="open"
      >
        {{ expanded ? 'Close storage review' : 'Review allowances' }}
      </button>
    </div>
    <dl class="mt-3 grid gap-3 text-small sm:grid-cols-2">
      <div>
        <dt class="text-text-muted">
          This run · retained / reserved
        </dt><dd>{{ formatBytes(snapshot.retained_bytes) }} / {{ formatBytes(snapshot.reserved_bytes) }}</dd><dd class="text-text-muted">
          Allowance {{ formatBytes(snapshot.run_byte_limit) }} · ceiling {{ formatBytes(snapshot.run_ceiling_bytes) }}
        </dd>
      </div>
      <div>
        <dt class="text-text-muted">
          Organization · retained / reserved
        </dt><dd>{{ formatBytes(snapshot.org_retained_bytes) }} / {{ formatBytes(snapshot.org_reserved_bytes) }}</dd><dd class="text-text-muted">
          Allowance {{ formatBytes(snapshot.org_byte_limit) }} · ceiling {{ formatBytes(snapshot.org_ceiling_bytes) }}
        </dd>
      </div>
    </dl>
    <p class="mt-2 text-small text-text-muted">
      Original run allowance: {{ formatBytes(snapshot.original_run_byte_limit) }}. A source page requires {{ formatBytes(snapshot.required_reservation_bytes) }} of available allowance before it starts; unused space is released. These are retained-data allowances, not physical database disk quotas.
    </p>
    <form
      v-if="expanded"
      class="mt-4 space-y-3"
      data-testid="snapshot-budget-form"
      @submit.prevent="prepareReview"
    >
      <p
        v-if="!canIncrease"
        class="text-small text-text-muted"
      >
        The current allowances cannot be increased within these deployment ceilings. An operator must raise the configuration ceilings before you can increase them here.
      </p>
      <div class="grid gap-3 sm:grid-cols-2">
        <FormField
          v-slot="{ id }"
          label="New run allowance (GiB)"
          bare
        >
          <input
            :id="id"
            v-model="runInput"
            :class="INPUT_CLASSES"
            inputmode="decimal"
            :disabled="pending"
            data-testid="snapshot-run-budget"
          >
        </FormField>
        <FormField
          v-slot="{ id }"
          label="New Organization allowance (GiB)"
          bare
        >
          <input
            :id="id"
            v-model="orgInput"
            :class="INPUT_CLASSES"
            inputmode="decimal"
            :disabled="pending"
            data-testid="snapshot-org-budget"
          >
        </FormField>
      </div>
      <p class="text-small text-text-muted">
        1 GiB = 1,073,741,824 bytes. Captures and previews share the run and Organization allowances. Raising an allowance does not resume either job.
      </p>
      <button
        type="submit"
        :class="buttonClasses('secondary')"
        :disabled="pending || !canIncrease"
        data-testid="snapshot-budget-review"
      >
        Review increase
      </button>
      <button
        type="button"
        :class="buttonClasses('ghost')"
        :disabled="pending"
        @click="emit('refresh')"
      >
        Refresh current values
      </button>
    </form>
    <p
      v-if="error"
      role="alert"
      class="mt-3 text-small text-danger"
    >
      {{ error }}
    </p>
    <p
      v-if="receipt"
      role="status"
      class="mt-3 text-small text-text"
    >
      {{ receipt }}
    </p>
    <ConfirmDialog
      :visible="review !== null"
      title="Increase snapshot allowances"
      :message="review?.message ?? ''"
      confirm-label="Confirm increase"
      :is-pending="pending"
      :error="error"
      error-fallback="Could not confirm the increase."
      @update:visible="(visible) => { if (!visible) review = null }"
      @confirm="confirm"
    />
  </section>
</template>
