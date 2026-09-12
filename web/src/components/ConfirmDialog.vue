<script setup lang="ts">
// SLICE_004 §10: "per-row actions Promote / Demote / Deactivate /
// Reactivate with a confirm dialog". Shared by MembersView.vue (role/status
// changes, invitation revoke) and PlatformOrganizationView.vue (promote,
// invitation revoke) so the one floating-surface confirm pattern (UI_STYLE
// §2) does not drift across the two screens. `error` renders inline inside
// the dialog — this is where `last_admin`'s "You are the last active admin.
// Promote someone else first." (§1 step 5, §10) surfaces.
import { useId } from 'vue'
import Dialog from 'primevue/dialog'
import { buttonClasses, dialogPt, type ButtonVariant } from '../lib/controls'
import { describeMutationError } from '../lib/errors'

const props = defineProps<{
  visible: boolean
  title: string
  message: string
  confirmLabel: string
  confirmVariant?: Extract<ButtonVariant, 'primary' | 'danger'>
  isPending: boolean
  error?: unknown
  errorFallback?: string
  scrollable?: boolean
}>()

const emit = defineEmits<{ 'update:visible': [value: boolean]; confirm: [] }>()
const titleId = useId()

function close() {
  if (props.isPending) return
  emit('update:visible', false)
}
</script>

<template>
  <Dialog
    :visible="visible"
    :aria-labelledby="titleId"
    modal
    :closable="false"
    :close-on-escape="!isPending"
    :dismissable-mask="!isPending"
    :pt="scrollable ? { ...dialogPt(), root: { class: 'glass-panel w-full max-w-md max-h-[calc(100dvh-2rem)] flex flex-col' }, content: { class: 'min-h-0 overflow-y-auto px-5 py-4' }, footer: { class: 'flex shrink-0 flex-wrap items-center justify-end gap-3 px-5 pb-5 pt-2' } } : dialogPt()"
    @update:visible="(value: boolean) => !value && close()"
  >
    <template #header>
      <h2
        :id="titleId"
        class="text-section font-semibold text-text"
      >
        {{ title }}
      </h2>
    </template>

    <p
      class="text-body text-text"
      :class="scrollable ? 'break-words' : ''"
    >
      {{ message }}
    </p>
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
        :class="buttonClasses(confirmVariant ?? 'primary')"
        :disabled="isPending"
        @click="$emit('confirm')"
      >
        {{ isPending ? 'Working…' : confirmLabel }}
      </button>
    </template>
  </Dialog>
</template>
