<script setup lang="ts">
// The one create/copy dialog for Save as and Duplicate. It intentionally
// exposes scope as an explicit choice; copying never publishes a private
// definition implicitly.
import { ref, watch } from 'vue'
import Dialog from 'primevue/dialog'
import type { SavedListScope } from '../api/types'
import { ApiError } from '../api/client'
import { buttonClasses, dialogPt, INPUT_CLASSES, LABEL_CLASSES } from '../lib/controls'
import { describeMutationError } from '../lib/errors'

const props = withDefaults(defineProps<{
  visible: boolean
  title: string
  submitLabel: string
  initialName: string
  description?: string
  initialScope?: SavedListScope
  allowShared: boolean
  isPending: boolean
  freezePayload?: boolean
  retryUnavailable?: boolean
  error?: unknown
}>(), {
  initialScope: 'personal',
  description: undefined,
  error: undefined,
})

const emit = defineEmits<{
  'update:visible': [value: boolean]
  submit: [payload: { name: string; scope: SavedListScope }]
}>()

const name = ref('')
const scope = ref<SavedListScope>('personal')
const submittedScope = ref<SavedListScope | null>(null)
const localError = ref('')

watch(() => props.visible, (open) => {
  if (!open) return
  name.value = props.initialName
  scope.value = props.allowShared && props.initialScope === 'shared' ? 'shared' : 'personal'
  submittedScope.value = null
  localError.value = ''
})

function close() {
  if (props.isPending) return
  emit('update:visible', false)
}

function submit() {
  if (props.isPending || props.retryUnavailable) return
  const trimmed = name.value.trim()
  if (trimmed === '') {
    localError.value = 'Enter a name for this list.'
    return
  }
  if (Array.from(trimmed).length > 80) {
    localError.value = 'List names can be at most 80 characters.'
    return
  }
  localError.value = ''
  submittedScope.value = scope.value
  emit('submit', { name: trimmed, scope: scope.value })
}

function errorMessage(error: unknown) {
  if (props.retryUnavailable) {
    if (error instanceof ApiError && error.code === 'saved_list_deleted') {
      return 'This saved request was deleted before its response could be confirmed.'
    }
    if (error instanceof ApiError && error.code === 'saved_list_request_conflict') {
      return 'This saved request conflicts with a different saved request.'
    }
    return 'This saved request cannot be retried.'
  }
  if (error instanceof ApiError && error.code === 'saved_list_limit_reached') {
    const failedScope = submittedScope.value ?? scope.value
    return failedScope === 'shared'
      ? 'This Organization already has 200 shared lists. Delete a shared list, or choose Only me and submit a new list.'
      : 'You already have 50 personal lists. Delete one of your lists, or choose Shared lists if an administrator can publish it.'
  }
  return describeMutationError(error, 'Could not save this list. Try again.')
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

    <form @submit.prevent="submit">
      <p
        v-if="description"
        class="mb-4 text-small text-text-muted"
      >
        {{ description }}
      </p>
      <label class="block">
        <span :class="LABEL_CLASSES">List name</span>
        <input
          v-model="name"
          :class="INPUT_CLASSES"
          autocomplete="off"
          :disabled="isPending || freezePayload || retryUnavailable"
          data-testid="saved-list-name"
        >
      </label>

      <fieldset
        v-if="allowShared"
        class="mt-4"
      >
        <legend :class="LABEL_CLASSES">
          Who can use it?
        </legend>
        <label class="flex cursor-pointer items-start gap-2 rounded-lg border border-border px-3 py-2 text-body text-text">
          <input
            v-model="scope"
            type="radio"
            value="personal"
            :disabled="isPending || freezePayload || retryUnavailable"
          >
          <span><strong class="font-medium">Only me</strong><br><span class="text-small text-text-muted">Keep this list private.</span></span>
        </label>
        <label class="mt-2 flex cursor-pointer items-start gap-2 rounded-lg border border-border px-3 py-2 text-body text-text">
          <input
            v-model="scope"
            type="radio"
            value="shared"
            :disabled="isPending || freezePayload || retryUnavailable"
          >
          <span><strong class="font-medium">Shared lists</strong><br><span class="text-small text-text-muted">Everyone in this Organization can use it.</span></span>
        </label>
      </fieldset>
      <p
        v-else
        class="mt-4 text-small text-text-muted"
      >
        This copy will be visible only to you.
      </p>

      <p
        v-if="localError"
        role="alert"
        class="mt-3 text-small text-danger"
      >
        {{ localError }}
      </p>
      <p
        v-else-if="error"
        role="alert"
        class="mt-3 text-small text-danger"
      >
        {{ errorMessage(error) }}
      </p>
      <p
        v-if="retryUnavailable"
        class="mt-3 text-small text-text-muted"
      >
        This saved request cannot be retried. Close this dialog before deliberately creating another list.
      </p>

      <button
        class="sr-only"
        type="submit"
      >
        {{ submitLabel }}
      </button>
    </form>

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
        :class="buttonClasses('primary')"
        :disabled="isPending || retryUnavailable"
        @click="submit"
      >
        {{ isPending ? 'Saving…' : retryUnavailable ? 'Retry unavailable' : freezePayload ? 'Retry saved request' : submitLabel }}
      </button>
    </template>
  </Dialog>
</template>
