<script setup lang="ts">
// SLICE_006 §10: the docked call panel — status line, elapsed timer, mute,
// Hang up (the view's primary while a call is active), and the post-call
// line "Logged as contact attempt — call, reached / no answer". A floating
// surface (UI_STYLE §2: white, 12 px radius, hairline plus the one soft
// shadow) docked bottom-right of the content column. Purely presentational:
// every value comes from `useCall` via props and every action is an emit,
// so PersonDetailView.vue owns the one call and this component holds no
// call state of its own.
import { computed } from 'vue'
import { Mic, MicOff, PhoneOff } from 'lucide-vue-next'
import type { CallView } from '../api/types'
import { buttonClasses } from '../lib/controls'
import { postCallLine, statusLine } from '../telephony/format'
import type { CallError, CallPhase } from '../telephony/useCall'

const props = defineProps<{
  phase: CallPhase
  personName: string
  elapsedSeconds: number
  muted: boolean
  error: CallError | null
  call: CallView | undefined
}>()

const emit = defineEmits<{
  hangup: []
  'toggle-mute': []
  'hangup-previous': []
  dismiss: []
}>()

const active = computed(() => !['idle', 'ended', 'failed'].includes(props.phase))
const status = computed(() => statusLine(props.phase, props.elapsedSeconds, props.call))
const logged = computed(() => postCallLine(props.call))
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
      v-else-if="!active && logged"
      class="mt-2 text-small text-text-muted"
      data-testid="call-logged"
    >
      {{ logged }}
    </p>

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
