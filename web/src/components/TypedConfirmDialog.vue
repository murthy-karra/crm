<script setup lang="ts">
// SLICE_011d.md §1 rule 2 / §6: "Disabling the unanswered-inquiry feed
// requires typed confirmation in the Web client." Same glass-dialog shape
// as ConfirmDialog.vue, plus a required exact-text field that gates the
// confirm button — modeled on the destructive-action-needs-typed-name
// pattern (no prior instance in this codebase; kept local to this one use
// so it isn't presented as a general-purpose primitive before a second
// caller needs it).
import { ref, watch } from 'vue'
import Dialog from 'primevue/dialog'
import { buttonClasses, dialogPt, INPUT_CLASSES, LABEL_CLASSES } from '../lib/controls'
import { describeMutationError } from '../lib/errors'

const props = defineProps<{
  visible: boolean
  title: string
  message: string
  /** The exact text the admin must type to enable Confirm — the feed's
   * display name (§1 rule 2), never a raw key or id. */
  confirmText: string
  confirmLabel: string
  isPending: boolean
  error?: unknown
  errorFallback?: string
}>()

const emit = defineEmits<{ 'update:visible': [value: boolean]; confirm: [] }>()

const typed = ref('')

watch(() => props.visible, (open) => {
  if (open) typed.value = ''
})

function close() {
  if (props.isPending) return
  emit('update:visible', false)
}

function confirm() {
  if (props.isPending || typed.value !== props.confirmText) return
  emit('confirm')
}
</script>

<template>
  <Dialog
    :visible="visible"
    modal
    :closable="false"
    :close-on-escape="!isPending"
    :dismissable-mask="!isPending"
    :pt="dialogPt()"
    @update:visible="(value: boolean) => !value && close()"
  >
    <template #header>
      <h2 class="text-section font-semibold text-text">
        {{ title }}
      </h2>
    </template>

    <p class="text-body text-text">
      {{ message }}
    </p>
    <label class="mt-4 block">
      <span :class="LABEL_CLASSES">
        Type "{{ confirmText }}" to confirm
      </span>
      <input
        v-model="typed"
        :class="INPUT_CLASSES"
        autocomplete="off"
        :disabled="isPending"
        data-testid="typed-confirm-input"
        @keydown.enter.prevent="confirm"
      >
    </label>
    <p
      v-if="error"
      role="alert"
      class="mt-3 text-small text-danger"
    >
      {{ describeMutationError(error, errorFallback ?? 'Something went wrong. Try again.') }}
    </p>

    <template #footer>
      <button
        type="button"
        :class="buttonClasses('secondary')"
        :disabled="isPending"
        @click="close"
      >
        Cancel
      </button>
      <button
        type="button"
        :class="buttonClasses('danger')"
        :disabled="isPending || typed !== confirmText"
        data-testid="typed-confirm-submit"
        @click="confirm"
      >
        {{ isPending ? 'Working…' : confirmLabel }}
      </button>
    </template>
  </Dialog>
</template>
