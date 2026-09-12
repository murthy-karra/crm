<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import FormField from '../FormField.vue'
import { type HistoryBudget, type HistoryCapture } from '../../api/historyCaptures'
import { buttonClasses, INPUT_CLASSES } from '../../lib/controls'
import { bytesToGiB, formatBytes, gibToBytes } from './format'
const props = defineProps<{ capture: HistoryCapture; disabled: boolean }>()
const emit = defineEmits<{ review: [value: { body: HistoryBudget; message: string }]; change: [] }>()
const expanded = ref(false)
const runInput = ref('')
const orgInput = ref('')
const error = ref('')
const canIncrease = computed(() => BigInt(props.capture.run_byte_limit) < BigInt(props.capture.run_ceiling_bytes) || BigInt(props.capture.org_byte_limit) < BigInt(props.capture.org_ceiling_bytes))
function reset() { runInput.value = bytesToGiB(props.capture.run_byte_limit); orgInput.value = bytesToGiB(props.capture.org_byte_limit); error.value = '' }
watch(() => props.capture.id, () => { expanded.value = false; reset() }, { immediate: true })
watch([runInput, orgInput], () => emit('change'), { flush: 'sync' })
function open() { reset(); expanded.value = !expanded.value }
function review() {
  if (props.disabled || !props.capture.actions.increase_budget) return
  error.value = ''
  const run = gibToBytes(runInput.value); const org = gibToBytes(orgInput.value); const s = props.capture
  if (!run || !org) { error.value = 'Enter positive GiB amounts that represent whole bytes.'; return }
  if (BigInt(run) < BigInt(s.run_byte_limit) || BigInt(org) < BigInt(s.org_byte_limit) || run === s.run_byte_limit && org === s.org_byte_limit) { error.value = 'Increase at least one allowance. Existing allowances cannot be reduced.'; return }
  if (BigInt(run) > BigInt(s.run_ceiling_bytes) || BigInt(org) > BigInt(s.org_ceiling_bytes)) { error.value = 'These amounts exceed the deployment ceilings. An operator must raise the ceilings first.'; return }
  emit('review', { body: { request_id: crypto.randomUUID(), expected_run_revision: s.revision, expected_run_budget_revision: s.run_budget_revision, expected_org_budget_revision: s.org_budget_revision, expected_policy_revision: s.policy_revision, run_byte_limit: run, org_byte_limit: org }, message: `Historical capture ${s.id}. Run allowance: ${s.run_byte_limit} → ${run} bytes. Organization allowance: ${s.org_byte_limit} → ${org} bytes. Retained/reserved: run ${s.retained_bytes}/${s.reserved_bytes}, Organization ${s.org_retained_bytes}/${s.org_reserved_bytes} bytes. Ceilings: ${s.run_ceiling_bytes}/${s.org_ceiling_bytes} bytes. Capture revision ${s.revision}; budget revisions ${s.run_budget_revision}/${s.org_budget_revision}; policy ${s.policy_revision}. Increasing allowances starts no work. Resume remains a separate action.` })
}
</script>
<template>
  <section
    class="mt-4 border-t border-border pt-4"
    aria-label="Historical capture storage allowances"
  >
    <div class="flex flex-wrap items-center justify-between gap-2">
      <h3 class="text-body font-medium">
        Storage allowances
      </h3>
      <button
        v-if="capture.actions.increase_budget"
        type="button"
        :class="buttonClasses()"
        :disabled="disabled"
        @click="open"
      >
        {{ expanded ? 'Close historical storage review' : 'Review historical storage allowances' }}
      </button>
    </div>
    <dl class="mt-3 grid gap-3 text-small sm:grid-cols-2">
      <div>
        <dt class="text-text-muted">
          Capture · retained / reserved
        </dt><dd>{{ formatBytes(capture.retained_bytes) }} / {{ formatBytes(capture.reserved_bytes) }}</dd><dd class="text-text-muted">
          Allowance {{ formatBytes(capture.run_byte_limit) }} · ceiling {{ formatBytes(capture.run_ceiling_bytes) }}
        </dd>
      </div>
      <div>
        <dt class="text-text-muted">
          Organization · retained / reserved
        </dt><dd>{{ formatBytes(capture.org_retained_bytes) }} / {{ formatBytes(capture.org_reserved_bytes) }}</dd><dd class="text-text-muted">
          Allowance {{ formatBytes(capture.org_byte_limit) }} · ceiling {{ formatBytes(capture.org_ceiling_bytes) }}
        </dd>
      </div>
    </dl>
    <p class="mt-2 text-small text-text-muted">
      A source request needs {{ formatBytes(capture.required_reservation_bytes) }} of available allowance. Organization storage is shared with other migration work. These are retained-data allowances, not physical disk quotas.
    </p>
    <form
      v-if="expanded"
      class="mt-3 space-y-3"
      @submit.prevent="review"
    >
      <p
        v-if="!canIncrease"
        class="text-small text-text-muted"
      >
        An operator must raise the deployment ceilings before these allowances can increase.
      </p>
      <div class="grid gap-3 sm:grid-cols-2">
        <FormField
          v-slot="{ id }"
          label="Historical run allowance (GiB)"
          bare
        >
          <input
            :id="id"
            v-model="runInput"
            :class="INPUT_CLASSES"
            inputmode="decimal"
            :disabled="disabled"
            data-testid="history-run-budget"
          >
        </FormField>
        <FormField
          v-slot="{ id }"
          label="Historical Organization allowance (GiB)"
          bare
        >
          <input
            :id="id"
            v-model="orgInput"
            :class="INPUT_CLASSES"
            inputmode="decimal"
            :disabled="disabled"
            data-testid="history-org-budget"
          >
        </FormField>
      </div>
      <p class="text-small text-text-muted">
        1 GiB = 1,073,741,824 bytes. Increasing an allowance does not resume capture.
      </p>
      <button
        type="submit"
        :class="buttonClasses()"
        :disabled="disabled || !canIncrease"
      >
        Review historical allowance increase
      </button>
      <p
        v-if="error"
        role="alert"
        class="text-small text-danger"
      >
        {{ error }}
      </p>
    </form>
  </section>
</template>
