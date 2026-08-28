<script setup lang="ts">
// SLICE_009 §8: the agent's OWN capture address — deliberately not on
// IntakeSettingsView (admin/tenant surface); this is the member's own
// credential. Address card + copy + connect instructions + reply-all
// etiquette/signature snippet (O-014 mitigation 1) + Rotate behind a
// consequence-stating ConfirmDialog (mirrors IntakeSettingsView's own
// Rotate flow) + the unmatched held queue with link/dismiss.
import { computed, ref } from 'vue'
import { Check, Copy, Mail, RefreshCw } from 'lucide-vue-next'
import Select from 'primevue/select'
import ConfirmDialog from '../components/ConfirmDialog.vue'
import Card from '../components/Card.vue'
import Badge from '../components/Badge.vue'
import PageHeader from '../components/PageHeader.vue'
import {
  useCaptureAddress,
  useCaptureUnmatched,
  useDismissUnmatchedMutation,
  useLinkUnmatchedMutation,
  useMe,
  usePeople,
  useRotateCaptureAddressMutation,
} from '../api/queries'
import type { CaptureUnmatchedItem } from '../api/types'
import { buttonClasses, selectPt } from '../lib/controls'
import { describeApiError, describeMutationError } from '../lib/errors'
import { formatAbsoluteTime, formatRelativeTime } from '../lib/format'

const { data: me } = useMe()
const orgId = computed(() => me.value?.organization?.id ?? '')

// ---- Address card ----------------------------------------------------

const { data: addressData, isPending, isError, error } = useCaptureAddress(orgId)
const address = computed(() => addressData.value?.address ?? '')

const copied = ref(false)
let copiedTimer: ReturnType<typeof setTimeout> | null = null

async function copy() {
  if (address.value === '') return
  try {
    await navigator.clipboard.writeText(address.value)
    copied.value = true
    if (copiedTimer !== null) clearTimeout(copiedTimer)
    copiedTimer = setTimeout(() => {
      copied.value = false
    }, 1500)
  } catch {
    // Clipboard unavailable: the address stays selectable text.
  }
}

const signatureCopied = ref(false)
let signatureCopiedTimer: ReturnType<typeof setTimeout> | null = null
const signatureSnippet = computed(() =>
  address.value === '' ? '' : `Please reply-all so our team stays in the loop (${address.value} is CC'd).`,
)

async function copySignature() {
  if (signatureSnippet.value === '') return
  try {
    await navigator.clipboard.writeText(signatureSnippet.value)
    signatureCopied.value = true
    if (signatureCopiedTimer !== null) clearTimeout(signatureCopiedTimer)
    signatureCopiedTimer = setTimeout(() => {
      signatureCopied.value = false
    }, 1500)
  } catch {
    // Clipboard unavailable: the snippet stays selectable text.
  }
}

const rotateConfirmOpen = ref(false)
const rotateMutation = useRotateCaptureAddressMutation(orgId)

async function rotate() {
  try {
    await rotateMutation.mutateAsync()
    rotateConfirmOpen.value = false
  } catch {
    // Error rendered below the card via rotateMutation.error.
    rotateConfirmOpen.value = false
  }
}

// ---- Unmatched held queue ----------------------------------------------

const {
  data: unmatchedData,
  isPending: unmatchedPending,
  isError: unmatchedIsError,
  error: unmatchedError,
} = useCaptureUnmatched(orgId)
const items = computed(() => unmatchedData.value?.items ?? [])

const { data: peopleData } = usePeople(orgId)
const peopleOptions = computed(() =>
  (peopleData.value?.people ?? []).map((p) => ({ id: p.id, display_name: p.display_name })),
)

const linkOpenId = ref<string | null>(null)
const linkPersonId = ref<string | null>(null)
const linkAddContactMethod = ref(true)
const linkMutation = useLinkUnmatchedMutation(orgId)
const dismissMutation = useDismissUnmatchedMutation(orgId)
const rowError = ref<{ id: string; message: string } | null>(null)

function openLink(item: CaptureUnmatchedItem) {
  linkOpenId.value = item.id
  linkPersonId.value = null
  linkAddContactMethod.value = true
  rowError.value = null
}

function cancelLink() {
  linkOpenId.value = null
}

async function confirmLink(id: string) {
  if (linkPersonId.value === null) return
  rowError.value = null
  try {
    await linkMutation.mutateAsync({
      id,
      personId: linkPersonId.value,
      addContactMethod: linkAddContactMethod.value,
    })
    linkOpenId.value = null
  } catch (err) {
    rowError.value = { id, message: describeMutationError(err, 'Could not link this message.') }
  }
}

async function dismiss(id: string) {
  rowError.value = null
  try {
    await dismissMutation.mutateAsync(id)
  } catch (err) {
    rowError.value = { id, message: describeMutationError(err, 'Could not dismiss this message.') }
  }
}

function directionLabel(hint: CaptureUnmatchedItem['direction_hint']): string {
  if (hint === 'inbound') return 'Presumed inbound'
  if (hint === 'outbound') return 'Presumed outbound'
  return 'Unknown'
}
</script>

<template>
  <div class="mx-auto max-w-3xl px-8 py-8">
    <PageHeader title="Email capture" />

    <div
      v-if="isError"
      class="rounded-xl border border-border bg-surface-0 p-5 text-body text-danger"
      data-testid="capture-address-error"
    >
      {{ describeApiError(error, 'Could not load your capture address.') }}
    </div>
    <div
      v-else-if="isPending"
      class="rounded-xl border border-border bg-surface-0 p-5 text-body text-text-muted"
    >
      Loading…
    </div>
    <Card
      v-else
      data-testid="capture-address-card"
    >
      <p class="text-body text-text">
        CC or BCC this address on lead and client email threads. Every captured message appears on that
        Person's timeline — no content is ever stored, only who and when.
      </p>
      <div class="mt-3 flex flex-wrap items-center gap-3">
        <code
          class="select-all rounded-lg bg-surface-2 px-3 py-2 font-mono text-body text-text"
          data-testid="capture-address"
        >{{ address }}</code>
        <button
          type="button"
          :class="buttonClasses('secondary')"
          data-testid="capture-copy"
          @click="copy"
        >
          <component
            :is="copied ? Check : Copy"
            class="mr-1.5 inline h-4 w-4"
            stroke-width="1.75"
          />
          {{ copied ? 'Copied' : 'Copy' }}
        </button>
        <button
          type="button"
          :class="buttonClasses('danger')"
          data-testid="rotate-capture-address"
          @click="rotateConfirmOpen = true"
        >
          <component
            :is="RefreshCw"
            class="mr-1.5 inline h-4 w-4 align-text-bottom"
          />
          Rotate
        </button>
      </div>
      <div
        v-if="rotateMutation.error.value"
        class="mt-3 text-body text-danger"
      >
        {{ describeApiError(rotateMutation.error.value, 'Could not rotate the address.') }}
      </div>

      <div class="mt-5 border-t border-border pt-5">
        <p class="text-body font-medium text-text">
          Get replies captured too
        </p>
        <p class="mt-1 text-small text-text-muted">
          CC catches your own messages; ask clients to "reply-all" so their replies come back to this
          address as well. A line like this in your signature helps:
        </p>
        <div class="mt-2 flex flex-wrap items-center gap-3">
          <code
            class="select-all rounded-lg bg-surface-2 px-3 py-2 text-small text-text"
            data-testid="capture-signature-snippet"
          >{{ signatureSnippet }}</code>
          <button
            type="button"
            :class="buttonClasses('secondary')"
            data-testid="capture-copy-signature"
            @click="copySignature"
          >
            <component
              :is="signatureCopied ? Check : Copy"
              class="mr-1.5 inline h-4 w-4"
              stroke-width="1.75"
            />
            {{ signatureCopied ? 'Copied' : 'Copy' }}
          </button>
        </div>
      </div>

      <p class="mt-5 text-small text-text-muted">
        Missed a thread? Forward the old emails to this same address — they land on the timeline at their
        original date, correctly placed among everything else.
      </p>
    </Card>

    <h2 class="mt-8 mb-3 text-section font-semibold text-text">
      Unmatched
    </h2>
    <p class="mb-3 text-small text-text-muted">
      Captured mail whose sender isn't a known Person yet. Only you can see this list.
    </p>

    <div
      v-if="unmatchedIsError"
      class="rounded-xl border border-border bg-surface-0 p-5 text-body text-danger"
      data-testid="capture-unmatched-error"
    >
      {{ describeApiError(unmatchedError, 'Could not load the unmatched list.') }}
    </div>
    <div
      v-else-if="unmatchedPending"
      class="rounded-xl border border-border bg-surface-0 p-5 text-body text-text-muted"
    >
      Loading…
    </div>
    <div
      v-else-if="items.length === 0"
      class="rounded-xl border border-border bg-surface-0 p-5 text-body text-text-muted"
      data-testid="capture-unmatched-empty"
    >
      Nothing unmatched right now.
    </div>
    <div
      v-else
      class="space-y-3"
      data-testid="capture-unmatched-list"
    >
      <Card
        v-for="item in items"
        :key="item.id"
        :data-testid="`capture-unmatched-row-${item.id}`"
      >
        <div class="flex flex-wrap items-center justify-between gap-3">
          <div class="flex min-w-0 items-center gap-3">
            <component
              :is="Mail"
              class="h-4 w-4 shrink-0 text-text-muted"
              stroke-width="1.75"
            />
            <span class="truncate text-body text-text">{{ item.counterparty_email ?? '(unknown address)' }}</span>
            <Badge tint="neutral">
              {{ directionLabel(item.direction_hint) }}
            </Badge>
          </div>
          <div class="flex shrink-0 items-center gap-3">
            <span
              class="text-small text-text-muted"
              :title="formatAbsoluteTime(item.captured_at)"
            >{{ formatRelativeTime(item.captured_at) }}</span>
            <button
              type="button"
              :class="buttonClasses('secondary')"
              :data-testid="`link-${item.id}`"
              @click="openLink(item)"
            >
              Link
            </button>
            <button
              type="button"
              :class="buttonClasses('secondary')"
              :disabled="dismissMutation.isPending.value"
              :data-testid="`dismiss-${item.id}`"
              @click="dismiss(item.id)"
            >
              Dismiss
            </button>
          </div>
        </div>

        <div
          v-if="linkOpenId === item.id"
          class="mt-4 border-t border-border pt-4"
        >
          <label class="mb-1.5 block text-small font-medium text-text">Link to</label>
          <Select
            v-model="linkPersonId"
            :options="peopleOptions"
            option-label="display_name"
            option-value="id"
            filter
            filter-placeholder="Search people…"
            placeholder="Choose a person"
            :pt="selectPt()"
            class="w-full"
            :data-testid="`link-person-select-${item.id}`"
          />
          <label class="mt-3 flex items-center gap-2 text-body text-text">
            <input
              v-model="linkAddContactMethod"
              type="checkbox"
              class="h-4 w-4 rounded border-border text-text accent-text"
              :data-testid="`link-add-contact-method-${item.id}`"
            >
            Add {{ item.counterparty_email ?? 'this address' }} as a contact method
          </label>
          <div class="mt-3 flex items-center gap-3">
            <button
              type="button"
              :class="buttonClasses('primary')"
              :disabled="linkPersonId === null || linkMutation.isPending.value"
              :data-testid="`confirm-link-${item.id}`"
              @click="confirmLink(item.id)"
            >
              {{ linkMutation.isPending.value ? 'Linking…' : 'Link' }}
            </button>
            <button
              type="button"
              :class="buttonClasses('secondary')"
              @click="cancelLink"
            >
              Cancel
            </button>
          </div>
        </div>

        <p
          v-if="rowError && rowError.id === item.id"
          class="mt-3 text-small text-danger"
        >
          {{ rowError.message }}
        </p>
      </Card>
    </div>
  </div>

  <ConfirmDialog
    v-model:visible="rotateConfirmOpen"
    title="Rotate your capture address?"
    message="Your current address stops working immediately — mail sent to it will be silently discarded. Update every thread you're CC'd on with the new address after rotating."
    confirm-label="Rotate"
    confirm-variant="danger"
    :is-pending="rotateMutation.isPending.value"
    @confirm="rotate"
  />
</template>
