<script setup lang="ts">
// SLICE_003 §10: "PrimeVue Dialog (unstyled pt; UI_STYLE §2 floating
// surface): Channel + Outcome Selects with per-channel default outcome;
// primary "Log contact"; button disabled while pending." Shared by
// TodayView.vue (per-row) and PersonDetailView.vue (header card) — the one
// place LogContactAttempt's request/response shape and the per-channel
// default-outcome behavior live, so the two callers cannot drift.
import { ref, watch } from 'vue'
import Dialog from 'primevue/dialog'
import Select from 'primevue/select'
import FormField from './FormField.vue'
import { useLogContactMutation } from '../api/queries'
import type { ContactChannel, ContactOutcome } from '../api/types'
import { buttonClasses, dialogPt, selectPt } from '../lib/controls'
import { describeApiError } from '../lib/errors'
import { CONTACT_CHANNEL_LABEL, CONTACT_OUTCOME_LABEL, DEFAULT_OUTCOME_FOR_CHANNEL } from '../lib/labels'

const props = defineProps<{
  visible: boolean
  orgId: string
  personId: string
  personName: string
}>()

const emit = defineEmits<{ 'update:visible': [value: boolean] }>()

const CHANNEL_OPTIONS = (Object.keys(CONTACT_CHANNEL_LABEL) as ContactChannel[]).map((id) => ({
  id,
  label: CONTACT_CHANNEL_LABEL[id],
}))
const OUTCOME_OPTIONS = (Object.keys(CONTACT_OUTCOME_LABEL) as ContactOutcome[]).map((id) => ({
  id,
  label: CONTACT_OUTCOME_LABEL[id],
}))

const channel = ref<ContactChannel>('call')
const outcome = ref<ContactOutcome>(DEFAULT_OUTCOME_FOR_CHANNEL.call)

const mutation = useLogContactMutation(() => props.orgId)

// §1's walkthrough opens the dialog on "Channel: Call, Outcome: No answer"
// every time — reset on each open rather than leaking the previous row's
// pick or a stale error from a prior submission.
watch(
  () => props.visible,
  (visible) => {
    if (!visible) return
    channel.value = 'call'
    outcome.value = DEFAULT_OUTCOME_FOR_CHANNEL.call
    mutation.reset()
  },
)

function onChannelChange(value: unknown) {
  if (typeof value !== 'string') return
  channel.value = value as ContactChannel
  outcome.value = DEFAULT_OUTCOME_FOR_CHANNEL[channel.value]
}

function onOutcomeChange(value: unknown) {
  if (typeof value !== 'string') return
  outcome.value = value as ContactOutcome
}

function close() {
  if (mutation.isPending.value) return
  emit('update:visible', false)
}

function submit() {
  // Defense in depth beyond the button's `:disabled` binding (which relies
  // on Vue having already patched the DOM before a second click/submit
  // fires) — a fast synthetic double-fire must not log two facts for one
  // click (LogContactAttempt is not idempotent by design, SLICE_003 §4).
  if (mutation.isPending.value) return
  mutation.mutate(
    { personId: props.personId, channel: channel.value, outcome: outcome.value },
    { onSuccess: () => emit('update:visible', false) },
  )
}
</script>

<template>
  <Dialog
    :visible="visible"
    modal
    :closable="false"
    :close-on-escape="!mutation.isPending.value"
    :dismissable-mask="!mutation.isPending.value"
    :pt="dialogPt()"
    @update:visible="(value: boolean) => !value && close()"
  >
    <template #header>
      <h2 class="text-section font-semibold text-text">
        Log contact
      </h2>
    </template>

    <p class="mb-4 text-body text-text-muted">
      {{ personName }}
    </p>

    <div class="space-y-4">
      <FormField
        label="Channel"
        bare
      >
        <Select
          :model-value="channel"
          :options="CHANNEL_OPTIONS"
          option-label="label"
          option-value="id"
          aria-label="Channel"
          :disabled="mutation.isPending.value"
          :pt="selectPt()"
          class="w-full"
          @update:model-value="onChannelChange"
        />
      </FormField>

      <FormField
        label="Outcome"
        bare
      >
        <Select
          :model-value="outcome"
          :options="OUTCOME_OPTIONS"
          option-label="label"
          option-value="id"
          aria-label="Outcome"
          :disabled="mutation.isPending.value"
          :pt="selectPt()"
          class="w-full"
          @update:model-value="onOutcomeChange"
        />
      </FormField>

      <p
        v-if="mutation.error.value"
        role="alert"
        class="text-small text-danger"
      >
        {{ describeApiError(mutation.error.value, 'Could not log the contact attempt.') }}
      </p>
    </div>

    <template #footer>
      <button
        type="button"
        :class="buttonClasses('secondary')"
        :disabled="mutation.isPending.value"
        @click="close"
      >
        Cancel
      </button>
      <button
        type="button"
        :class="buttonClasses('primary')"
        :disabled="mutation.isPending.value"
        @click="submit"
      >
        {{ mutation.isPending.value ? 'Logging…' : 'Log contact' }}
      </button>
    </template>
  </Dialog>
</template>
