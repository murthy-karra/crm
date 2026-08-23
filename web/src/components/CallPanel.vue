<script setup lang="ts">
// SLICE_006 §10: the docked call panel — status line, elapsed timer, mute,
// Hang up (the view's primary while a call is active). SLICE_006c §5a
// (D-033) replaces SLICE_006's post-call line ("Logged as contact attempt —
// …"): whenever an automatic attempt was written (`attemptOutcome(call) !==
// null`) the post-call block is the "How did it go?" prompt — the five-choice
// picker with NOTHING pre-selected (the system's observation is never
// offered as the outcome) and Save outcome (the one primary; enabled only
// once a choice is made AND the *server* says the call is terminal). There
// is no Skip: the panel stays until Save succeeds. A floating surface
// (UI_STYLE §2: white, 12 px radius, hairline plus the one soft shadow)
// docked bottom-right of the content column. Purely presentational: every
// value comes from `useCall` via props and every action is an emit, so
// PersonDetailView.vue owns the one call and this component holds no call
// state of its own.
import { computed, ref, watch } from 'vue'
import { Mic, MicOff, PhoneOff } from 'lucide-vue-next'
import type { CallOutcomeCorrection, CallView } from '../api/types'
import { buttonClasses } from '../lib/controls'
import { correctedOutcomeLabel } from '../lib/labels'
import { statusLine } from '../telephony/format'
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
}>()

const active = computed(() => !['idle', 'ended', 'failed'].includes(props.phase))
const status = computed(() => statusLine(props.phase, props.elapsedSeconds, props.call))

// ---- "How did it go?" (SLICE_006c §5a, D-033) -----------------------------
// The owner decides (`outcomePrompt`) whether an automatic attempt exists —
// null when nothing reached the callee; then there is no outcome to choose
// and the owner never sets the flag.
const prompt = computed(() => props.outcomePrompt === true && !active.value && !props.outcomeSaved)
const savedLine = computed(() => (props.outcomeSaved ? `Outcome saved — ${correctedOutcomeLabel(props.outcomeSaved)}` : null))
// The server's word, not the client phase: `phase` turns `ended` before the
// hangup request completes, and `call` is refetched on `call.changed`.
const terminal = computed(() => props.call?.status === 'ended' || props.call?.status === 'failed')

// Forced choice: starts empty for every call (keyed on the call id) and is
// never seeded from the observation.
const selected = ref<CallOutcomeCorrection | null>(null)
watch(
  () => props.call?.id,
  () => {
    selected.value = null
  },
)

const saveEnabled = computed(() => terminal.value && selected.value !== null && !props.outcomeSaving)

function pick(value: CallOutcomeCorrection) {
  selected.value = value
}

function save() {
  if (!saveEnabled.value || selected.value === null) return
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
