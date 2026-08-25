<script setup lang="ts">
// SLICE_007a §6: `/manage/intake` — the Organization's email intake
// address, read-only, with Copy. The token inside it is the anti-forgery
// secret, so this page and its endpoint are org-admin only.
// SLICE_007c §6: below that, the "Unattended lead routing" card — the
// Organization's default assignee for system-actor (unattended) intake.
import { computed, ref } from 'vue'
import { Check, Copy } from 'lucide-vue-next'
import Select from 'primevue/select'
import PageHeader from '../components/PageHeader.vue'
import FormField from '../components/FormField.vue'
import {
  useIntakeAddress,
  useIntakeSettings,
  useMe,
  useMembers,
  useUpdateIntakeSettingsMutation,
} from '../api/queries'
import { buttonClasses, selectPt } from '../lib/controls'
import { describeApiError, describeMutationError } from '../lib/errors'

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

// --- Unattended lead routing (SLICE_007c §6) --------------------------------

const { data: membersData, isPending: membersPending } = useMembers(orgId)
const activeMembers = computed(() => (membersData.value?.members ?? []).filter((m) => m.status === 'active'))
const assigneeOptions = computed(() => [
  { user_id: null as string | null, display_name: 'Unassigned' },
  ...activeMembers.value.map((m) => ({ user_id: m.user_id as string | null, display_name: m.display_name })),
])

const {
  data: settingsData,
  isPending: settingsPending,
  isError: settingsError,
  error: settingsErrorObj,
} = useIntakeSettings(orgId)
const selectedAssignee = computed(() => settingsData.value?.intake_default_assignee_user_id ?? null)
const configuredMember = computed(
  () => (membersData.value?.members ?? []).find((m) => m.user_id === selectedAssignee.value) ?? null,
)
const isDeactivatedDefault = computed(() => configuredMember.value?.status === 'inactive')

const {
  mutate: saveDefaultAssignee,
  isPending: savePending,
  error: saveError,
} = useUpdateIntakeSettingsMutation(orgId)

function onAssigneeChange(value: unknown) {
  if (typeof value !== 'string' && value !== null) return
  saveDefaultAssignee({ intake_default_assignee_user_id: value })
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

    <div
      v-if="settingsError"
      class="mt-6 rounded-xl border border-border bg-surface-0 p-5 text-body text-danger"
      data-testid="intake-settings-error"
    >
      {{ describeApiError(settingsErrorObj, 'Could not load the unattended routing setting.') }}
    </div>
    <div
      v-else-if="settingsPending || membersPending"
      class="mt-6 rounded-xl border border-border bg-surface-0 p-5 text-body text-text-muted"
    >
      Loading…
    </div>
    <FormField
      v-else
      class="mt-6"
      label="Unattended lead routing"
      description="Default assignee for unattended leads."
    >
      <Select
        :model-value="selectedAssignee"
        :options="assigneeOptions"
        option-label="display_name"
        option-value="user_id"
        aria-label="Default assignee for unattended leads"
        :loading="savePending"
        :disabled="savePending"
        :pt="selectPt()"
        class="w-64"
        data-testid="intake-default-assignee"
        @update:model-value="onAssigneeChange"
      />
      <p
        v-if="selectedAssignee === null"
        class="mt-1.5 text-small text-text-muted"
        data-testid="intake-default-assignee-unset-warning"
      >
        Unattended leads will be created unassigned and appear on no one's Today.
      </p>
      <p
        v-else-if="isDeactivatedDefault"
        class="mt-1.5 text-small text-danger"
        data-testid="intake-default-assignee-deactivated-warning"
      >
        The default assignee is deactivated; unattended leads will be created unassigned.
      </p>
      <p
        v-if="saveError"
        class="mt-1.5 text-small text-danger"
      >
        {{ describeMutationError(saveError, 'Could not update the setting.') }}
      </p>
    </FormField>
  </div>
</template>
