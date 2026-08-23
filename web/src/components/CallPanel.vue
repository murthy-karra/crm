<script setup lang="ts">
// SLICE_006 §10: the docked call panel — status line, elapsed timer, mute,
// Hang up (the view's primary while a call is active). SLICE_006c §10
// replaces SLICE_006's post-call line ("Logged as contact attempt — …"):
// whenever an automatic attempt was written (`attemptOutcome(call) !==
// null`) the post-call block is the "How did it go?" prompt — the five-choice
// picker pre-selected from what the system observed, Save outcome (the one
// primary; enabled only once the *server* says the call is terminal) and
// Skip (ghost, replaces Done; sends nothing). A floating
// surface (UI_STYLE §2: white, 12 px radius, hairline plus the one soft
// shadow) docked bottom-right of the content column. Purely presentational:
// every value comes from `useCall` via props and every action is an emit,
// so PersonDetailView.vue owns the one call and this component holds no
// call state of its own.
import { computed, ref, watch } from 'vue'
import { Mic, MicOff, PhoneOff } from 'lucide-vue-next'
import type { CallOutcomeCorrection, CallView } from '../api/types'
import { buttonClasses } from '../lib/controls'
import { correctedOutcomeLabel } from '../lib/labels'
import { attemptOutcome, statusLine } from '../telephony/format'
import type { CallError, CallPhase } from '../telephony/useCall'
import OutcomePicker from './OutcomePicker.vue'

const props = defineProps<{
  phase: CallPhase
  personName: string
  elapsedSeconds: number
  muted: boolean
  error: CallError | null
  call: CallView | undefined
  /** SLICE_006c: the owner decides (`showsOutcomePrompt`) whether the
   * post-call block is the "How did it go?" prompt — it also gates the
   * header's primary and the History action, so it is computed once. */
  outcomePrompt?: boolean
  /** The owner's `useCorrectCallOutcome` state — pending, the outcome the
   * server just recorded (`changed: true`), and §10 error copy. */
  outcomeSaving?: boolean
  outcomeSaved?: CallOutcomeCorrection | null
  outcomeError?: string | null
}>()

const emit = defineEmits<{
  hangup: []
  'toggle-mute': []
  'hangup-previous': []
  dismiss: []
  'save-outcome': [outcome: CallOutcomeCorrection]
  skip: []
}>()

const active = computed(() => !['idle', 'ended', 'failed'].includes(props.phase))
const status = computed(() => statusLine(props.phase, props.elapsedSeconds, props.call))

// ---- "How did it go?" (SLICE_006c §10) --------------------------------------
// Observed = the automatic attempt D-031 wrote (reached / no_answer); null
// when nothing reached the callee — then there is nothing to correct and the
// owner never sets `outcomePrompt`.
const observed = computed(() => attemptOutcome(props.call))
const prompt = computed(() => props.outcomePrompt === true && !active.value && !props.outcomeSaved)
const savedLine = computed(() => (props.outcomeSaved ? `Outcome saved — ${correctedOutcomeLabel(props.outcomeSaved)}` : null))
// The server's word, not the client phase: `phase` turns `ended` before the
// hangup request completes, and `call` is refetched on `call.changed`.
const terminal = computed(() => props.call?.status === 'ended' || props.call?.status === 'failed')
const saveEnabled = computed(() => terminal.value && !props.outcomeSaving)

const selected = ref<CallOutcomeCorrection>('reached')
// Pre-select from the observed outcome: once per call (keyed on the call
// id), and again if the observation changes before the user picks (a
// ring-out settling after the panel first rendered).
const touched = ref(false)
watch(
  () => [props.call?.id, observed.value] as const,
  ([id, value], previous) => {
    if (id !== previous?.[0]) touched.value = false
    if (value !== null && !touched.value) selected.value = value
  },
  { immediate: true },
)

function pick(value: CallOutcomeCorrection) {
  touched.value = true
  selected.value = value
}

function save() {
  if (!saveEnabled.value) return
  emit('save-outcome', selected.value)
}
</script>

<template>
  <section
    v-if="phase !== 'idle'"
    class="fixed bottom-6 right-10 z-40 w-80 rounded-xl border border-border bg-surface-0 p-5 shadow-floating"
    role="status"
    aria-live="polite"
    data-testid="call-panel"
  >
    <p class="text-body font-medium text-text">
      {{ personName }}
    </p>
    <p
      class="mt-1 text-body text-text-muted tabular-nums"
      data-testid="call-status"
    >
      {{ status }}
    </p>

    <p
      v-if="error"
      role="alert"
      class="mt-2 text-small text-danger"
      data-testid="call-error"
    >
      {{ error.message }}
    </p>
    <p
      v-else-if="savedLine"
      class="mt-2 text-small text-text-muted"
      data-testid="call-outcome-saved"
    >
      {{ savedLine }}
    </p>
    <template v-else-if="prompt">
      <p
        class="mt-3 text-body font-medium text-text"
        data-testid="call-outcome-prompt"
      >
        How did it go?
      </p>
      <OutcomePicker
        class="mt-2"
        :model-value="selected"
        :disabled="outcomeSaving"
        @update:model-value="pick"
      />
      <p
        v-if="outcomeError"
        role="alert"
        class="mt-2 text-small text-danger"
        data-testid="call-outcome-error"
      >
        {{ outcomeError }}
      </p>
      <p
        v-else-if="!terminal"
        class="mt-2 text-small text-text-muted"
        data-testid="call-outcome-finishing"
      >
        Finishing up…
      </p>
    </template>

    <div class="mt-4 flex items-center justify-end gap-3">
      <template v-if="active">
        <button
          type="button"
          :class="buttonClasses('secondary')"
          :aria-pressed="muted"
          data-testid="call-mute"
          @click="emit('toggle-mute')"
        >
          <MicOff
            v-if="muted"
            class="h-[18px] w-[18px]"
            stroke-width="1.5"
          />
          <Mic
            v-else
            class="h-[18px] w-[18px]"
            stroke-width="1.5"
          />
          {{ muted ? 'Unmute' : 'Mute' }}
        </button>
        <button
          type="button"
          :class="buttonClasses('primary')"
          data-testid="call-hangup"
          @click="emit('hangup')"
        >
          <PhoneOff
            class="h-[18px] w-[18px]"
            stroke-width="1.5"
          />
          Hang up
        </button>
      </template>
      <template v-else-if="prompt">
        <button
          type="button"
          :class="buttonClasses('ghost')"
          :disabled="outcomeSaving"
          data-testid="call-outcome-skip"
          @click="emit('skip')"
        >
          Skip
        </button>
        <button
          type="button"
          :class="buttonClasses('primary')"
          :disabled="!saveEnabled"
          data-testid="call-outcome-save"
          @click="save"
        >
          {{ outcomeSaving ? 'Saving…' : 'Save outcome' }}
        </button>
      </template>
      <template v-else>
        <button
          v-if="error?.previousCallId"
          type="button"
          :class="buttonClasses('secondary')"
          data-testid="call-hangup-previous"
          @click="emit('hangup-previous')"
        >
          Hang up previous call
        </button>
        <button
          type="button"
          :class="buttonClasses('ghost')"
          data-testid="call-dismiss"
          @click="emit('dismiss')"
        >
          Done
        </button>
      </template>
    </div>
  </section>
</template>
