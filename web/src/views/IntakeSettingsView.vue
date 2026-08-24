<script setup lang="ts">
// SLICE_007a §6: `/manage/intake` — the Organization's email intake
// address, read-only, with Copy. The token inside it is the anti-forgery
// secret, so this page and its endpoint are org-admin only.
import { computed, ref } from 'vue'
import { Check, Copy } from 'lucide-vue-next'
import PageHeader from '../components/PageHeader.vue'
import { useIntakeAddress, useMe } from '../api/queries'
import { buttonClasses } from '../lib/controls'
import { describeApiError } from '../lib/errors'

const { data: me } = useMe()
const orgId = computed(() => me.value?.organization?.id ?? '')
const { data, isPending, isError, error } = useIntakeAddress(orgId)

const address = computed(() => data.value?.address ?? '')
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
    // Clipboard unavailable (§8): the address stays selectable text.
  }
}
</script>

<template>
  <div class="mx-auto max-w-3xl px-8 py-8">
    <PageHeader title="Intake" />

    <div
      v-if="isError"
      class="rounded-xl border border-border bg-surface-0 p-5 text-body text-danger"
      data-testid="intake-error"
    >
      {{ describeApiError(error, 'Could not load the intake address.') }}
    </div>
    <div
      v-else-if="isPending"
      class="rounded-xl border border-border bg-surface-0 p-5 text-body text-text-muted"
    >
      Loading…
    </div>
    <div
      v-else
      class="rounded-xl border border-border bg-surface-0 p-5"
    >
      <p class="text-body text-text">
        Forward lead notifications to
      </p>
      <div class="mt-3 flex flex-wrap items-center gap-3">
        <code
          class="select-all rounded-lg bg-surface-2 px-3 py-2 font-mono text-body text-text"
          data-testid="intake-address"
        >{{ address }}</code>
        <button
          type="button"
          :class="buttonClasses('secondary')"
          data-testid="intake-copy"
          @click="copy"
        >
          <component
            :is="copied ? Check : Copy"
            class="mr-1.5 inline h-4 w-4"
            stroke-width="1.75"
          />
          {{ copied ? 'Copied' : 'Copy' }}
        </button>
      </div>
      <p class="mt-3 text-small text-text-muted">
        Emails sent here will appear as leads once email intake is enabled.
      </p>
    </div>
  </div>
</template>
