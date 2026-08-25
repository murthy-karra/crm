<script setup lang="ts">
// SLICE_007e §7: the admin-only Unresolved workbench dialog. Content is
// fetched imperatively when the dialog opens (never prefetched, never
// cached beyond the dialog) and cleared on close. Two actions: Try again
// (primary) and Discard (danger, behind ConfirmDialog).
import { ref, watch } from 'vue'
import Dialog from 'primevue/dialog'
import ConfirmDialog from './ConfirmDialog.vue'
import {
  fetchUnresolvedDetail,
  useDiscardUnresolvedMutation,
  useRetryUnresolvedMutation,
} from '../api/queries'
import type { RetryUnresolvedResponse, UnresolvedDetailResponse } from '../api/types'
import { buttonClasses, dialogPt } from '../lib/controls'
import { ApiError } from '../api/client'
import { describeApiError } from '../lib/errors'
import { UNRESOLVED_REASON_LABEL } from '../lib/labels'
import { formatAbsoluteTime, formatBytes } from '../lib/format'

const props = defineProps<{
  visible: boolean
  orgId: string
  rawPayloadId: string
}>()

const emit = defineEmits<{ 'update:visible': [value: boolean] }>()

const detail = ref<UnresolvedDetailResponse | null>(null)
const loadError = ref<string | null>(null)
const loading = ref(false)
const retryOutcome = ref<RetryUnresolvedResponse | null>(null)
const confirmDiscardOpen = ref(false)

const retryMutation = useRetryUnresolvedMutation(() => props.orgId)
const discardMutation = useDiscardUnresolvedMutation(() => props.orgId)

// `immediate`: the dialog is created fresh (v-if) with visible already
// true, so a lazy watch would never fire and content would never load.
watch(
  () => props.visible,
  (visible) => {
    if (visible) {
      void load()
    } else {
      // Content never outlives the dialog (SLICE_007e §7).
      detail.value = null
      loadError.value = null
      retryOutcome.value = null
      retryMutation.reset()
      discardMutation.reset()
    }
  },
  { immediate: true },
)

async function load() {
  loading.value = true
  loadError.value = null
  try {
    detail.value = await fetchUnresolvedDetail(props.rawPayloadId)
  } catch (error) {
    // A 500 on a pending row is the corrupted-ciphertext case (§4):
    // Try again will not help; Discard is the remedy.
    loadError.value =
      error instanceof ApiError && error.code === 'internal_error'
        ? 'This entry cannot be decrypted. Try again will not help — Discard is the remedy.'
        : describeApiError(error, 'Could not load the entry.')
  } finally {
    loading.value = false
  }
}

async function retry() {
  retryOutcome.value = null
  try {
    retryOutcome.value = await retryMutation.mutateAsync(props.rawPayloadId)
    if (retryOutcome.value.status === 'unresolved') {
      // Still unresolved — refresh the shown metadata (reason may have
      // been re-recorded).
      void load()
    }
  } catch {
    // Rendered from retryMutation.error below.
  }
}

async function discard() {
  confirmDiscardOpen.value = false
  try {
    await discardMutation.mutateAsync(props.rawPayloadId)
    emit('update:visible', false)
  } catch {
    // Rendered from discardMutation.error below.
  }
}
</script>

<template>
  <Dialog
    :visible="visible"
    modal
    header="Unresolved entry"
    :style="{ width: '40rem' }"
    :pt="dialogPt()"
    @update:visible="emit('update:visible', $event)"
  >
    <div
      v-if="loading"
      class="p-1 text-body text-text-muted"
    >
      Loading…
    </div>
    <div
      v-else-if="loadError"
      class="p-1 text-body text-danger"
    >
      {{ loadError }}
    </div>
    <div
      v-else-if="detail"
      class="flex flex-col gap-4"
    >
      <dl class="grid grid-cols-2 gap-x-6 gap-y-1 text-body">
        <div>
          <dt class="text-caption text-text-muted">
            Source
          </dt>
          <dd class="font-medium text-text">
            {{ detail.source }}
          </dd>
        </div>
        <div>
          <dt class="text-caption text-text-muted">
            Received
          </dt>
          <dd class="text-text">
            {{ formatAbsoluteTime(detail.received_at) }}
          </dd>
        </div>
        <div>
          <dt class="text-caption text-text-muted">
            Reason
          </dt>
          <dd class="text-text">
            {{ detail.reason ? UNRESOLVED_REASON_LABEL[detail.reason] : '—' }}
          </dd>
        </div>
        <div>
          <dt class="text-caption text-text-muted">
            Size
          </dt>
          <dd class="text-text">
            {{ formatBytes(detail.byte_len) }}
          </dd>
        </div>
      </dl>

      <div
        v-if="detail.content.kind === 'email'"
        class="rounded-lg border border-border bg-surface-1 p-4 text-body"
      >
        <div class="mb-2 flex flex-col gap-0.5">
          <div class="font-medium text-text">
            {{ detail.content.subject ?? '(no subject)' }}
          </div>
          <div class="text-caption text-text-muted">
            From:
            <template v-if="detail.content.from_display">
              {{ detail.content.from_display }}
              &lt;{{ detail.content.from_addr ?? '?' }}&gt;
            </template>
            <template v-else>
              {{ detail.content.from_addr ?? '—' }}
            </template>
            <template v-if="detail.content.date">
              · {{ formatAbsoluteTime(detail.content.date) }}
            </template>
          </div>
        </div>
        <pre
          class="max-h-80 overflow-auto whitespace-pre-wrap break-words text-body text-text"
        >{{ detail.content.text ?? '(no text body)' }}</pre>
      </div>
      <div
        v-else
        class="rounded-lg border border-border bg-surface-1 p-4"
      >
        <pre
          class="max-h-80 overflow-auto whitespace-pre-wrap break-words text-body text-text"
        >{{ detail.content.text }}</pre>
      </div>
      <p
        v-if="detail.content.truncated"
        class="text-caption text-text-muted"
      >
        Content truncated for display — the full original is preserved.
      </p>

      <div
        v-if="retryOutcome?.status === 'resolved'"
        class="rounded-lg border border-border bg-surface-1 p-3 text-body text-text"
      >
        Lead created —
        <RouterLink
          :to="`/people/${retryOutcome.person_id}`"
          class="font-medium text-accent hover:underline"
          @click="emit('update:visible', false)"
        >
          open the person
        </RouterLink>.
      </div>
      <div
        v-else-if="retryOutcome?.status === 'unresolved'"
        class="rounded-lg border border-border bg-surface-1 p-3 text-body text-text-muted"
      >
        Still unresolved — {{ UNRESOLVED_REASON_LABEL[retryOutcome.reason] }}.
      </div>
      <div
        v-if="retryMutation.error.value"
        class="text-body text-danger"
      >
        {{
          describeApiError(
            retryMutation.error.value,
            'Could not retry this entry.',
          )
        }}
      </div>
      <div
        v-if="discardMutation.error.value"
        class="text-body text-danger"
      >
        {{
          describeApiError(
            discardMutation.error.value,
            'Could not discard this entry.',
          )
        }}
      </div>
    </div>

    <template #footer>
      <div class="flex w-full items-center justify-between">
        <button
          type="button"
          :class="buttonClasses('danger')"
          :disabled="loading || discardMutation.isPending.value"
          @click="confirmDiscardOpen = true"
        >
          Discard
        </button>
        <div class="flex gap-2">
          <button
            type="button"
            :class="buttonClasses('secondary')"
            @click="emit('update:visible', false)"
          >
            Close
          </button>
          <button
            type="button"
            :class="buttonClasses('primary')"
            :disabled="loading || !!loadError || retryMutation.isPending.value"
            @click="retry"
          >
            {{ retryMutation.isPending.value ? 'Retrying…' : 'Try again' }}
          </button>
        </div>
      </div>
    </template>
  </Dialog>

  <ConfirmDialog
    v-model:visible="confirmDiscardOpen"
    title="Discard this entry?"
    message="It will leave the queue for everyone. The original content is retained but no longer shown anywhere."
    confirm-label="Discard"
    confirm-variant="danger"
    :is-pending="discardMutation.isPending.value"
    @confirm="discard"
  />
</template>
