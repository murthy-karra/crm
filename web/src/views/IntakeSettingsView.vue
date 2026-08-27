<script setup lang="ts">
// SLICE_007a §6: `/manage/intake` — the Organization's email intake
// address, read-only, with Copy. The token inside it is the anti-forgery
// secret, so this page and its endpoint are org-admin only.
// SLICE_008 §5 (D-041, supersedes SLICE_007c §6): below that, the
// "Unattended lead routing" card — a three-mode picker (default assignee /
// round-robin / unassigned) for how the Organization's unattended leads
// route.
import { computed, ref, watch } from 'vue'
import { Check, Copy, RefreshCw } from 'lucide-vue-next'
import ConfirmDialog from '../components/ConfirmDialog.vue'
import Select from 'primevue/select'
import PageHeader from '../components/PageHeader.vue'
import FormField from '../components/FormField.vue'
import {
  useIntakeAddress,
  useRotateIntakeAddressMutation,
  useIntakeSettings,
  useMe,
  useMembers,
  useUpdateIntakeSettingsMutation,
} from '../api/queries'
import type { IntakeRoutingMode } from '../api/types'
import { buttonClasses, selectPt } from '../lib/controls'
import { describeApiError, describeMutationError } from '../lib/errors'

const { data: me } = useMe()
const orgId = computed(() => me.value?.organization?.id ?? '')
const { data, isPending, isError, error } = useIntakeAddress(orgId)

const address = computed(() => data.value?.address ?? '')

// SLICE_007g §8: break-glass rotation behind a confirm that states the
// immediate-invalidation consequence.
const rotateConfirmOpen = ref(false)
const rotateMutation = useRotateIntakeAddressMutation(orgId)

async function rotate() {
  try {
    await rotateMutation.mutateAsync()
    rotateConfirmOpen.value = false
  } catch {
    // Error rendered below the card via rotateMutation.error.
    rotateConfirmOpen.value = false
  }
}
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

// --- Unattended lead routing (SLICE_008 §5, D-041) -------------------------

const MODE_OPTIONS: { value: IntakeRoutingMode; label: string }[] = [
  { value: 'default_assignee', label: 'Default assignee' },
  { value: 'round_robin', label: 'Round-robin' },
  { value: 'unassigned', label: 'Unassigned' },
]

const { data: membersData, isPending: membersPending } = useMembers(orgId)
const activeMembers = computed(() => (membersData.value?.members ?? []).filter((m) => m.status === 'active'))
// D-041: the dropdown's old "Unassigned" entry moved up to become its own
// mode above — this list is active members only now.
const assigneeOptions = computed(() =>
  activeMembers.value.map((m) => ({ user_id: m.user_id, display_name: m.display_name })),
)

const {
  data: settingsData,
  isPending: settingsPending,
  isError: settingsError,
  error: settingsErrorObj,
} = useIntakeSettings(orgId)
const serverMode = computed<IntakeRoutingMode>(() => settingsData.value?.intake_routing_mode ?? 'unassigned')
// Reviewer F1: picking "Default assignee" when the stored assignee is
// null/inactive must NOT PUT immediately (the server would 422 per §5's
// S1 rule and the picker would revert before the dropdown ever
// rendered — a lockout). Hold the choice locally, render the dropdown,
// and defer the single both-fields PUT until a member is chosen.
const pendingMode = ref<IntakeRoutingMode | null>(null)
const selectedMode = computed<IntakeRoutingMode>(() => pendingMode.value ?? serverMode.value)
const selectedAssignee = computed(() => settingsData.value?.intake_default_assignee_user_id ?? null)
const configuredMember = computed(
  () => (membersData.value?.members ?? []).find((m) => m.user_id === selectedAssignee.value) ?? null,
)
const isDeactivatedDefault = computed(() => configuredMember.value?.status === 'inactive')

const {
  mutate: saveSettings,
  isPending: savePending,
  error: saveError,
} = useUpdateIntakeSettingsMutation(orgId)

// One mutation PUTs both fields on any change (§5): each handler supplies
// the OTHER field's current value alongside the one that actually changed.
function onModeChange(value: unknown) {
  if (typeof value !== 'string') return
  const mode = value as IntakeRoutingMode
  const storedAssigneeSatisfiesDefaultMode =
    configuredMember.value !== null && configuredMember.value.status === 'active'
  if (mode === 'default_assignee' && !storedAssigneeSatisfiesDefaultMode) {
    pendingMode.value = mode
    return
  }
  pendingMode.value = null
  saveSettings({
    intake_routing_mode: mode,
    intake_default_assignee_user_id: selectedAssignee.value,
  })
}

function onAssigneeChange(value: unknown) {
  if (typeof value !== 'string') return
  saveSettings({ intake_routing_mode: selectedMode.value, intake_default_assignee_user_id: value })
}

// The deferred choice completes (or is abandoned) once the server state
// catches up or the admin navigates the picker elsewhere.
watch(serverMode, (mode) => {
  if (pendingMode.value === mode) pendingMode.value = null
})
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
        <button
          type="button"
          :class="buttonClasses('danger')"
          data-testid="rotate-address"
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
      description="How unattended leads route when nobody explicitly assigns them."
    >
      <Select
        :model-value="selectedMode"
        :options="MODE_OPTIONS"
        option-label="label"
        option-value="value"
        aria-label="Unattended lead routing mode"
        :loading="savePending"
        :disabled="savePending"
        :pt="selectPt()"
        class="w-64"
        data-testid="intake-routing-mode"
        @update:model-value="onModeChange"
      />

      <template v-if="selectedMode === 'default_assignee'">
        <Select
          :model-value="selectedAssignee"
          :options="assigneeOptions"
          option-label="display_name"
          option-value="user_id"
          aria-label="Default assignee for unattended leads"
          placeholder="Choose a member"
          :loading="savePending"
          :disabled="savePending"
          :pt="selectPt()"
          class="mt-3 w-64"
          data-testid="intake-default-assignee"
          @update:model-value="onAssigneeChange"
        />
        <p
          v-if="isDeactivatedDefault"
          class="mt-1.5 text-small text-danger"
          data-testid="intake-default-assignee-deactivated-warning"
        >
          The default assignee is deactivated; unattended leads will be created unassigned.
        </p>
      </template>

      <p
        v-else-if="selectedMode === 'round_robin'"
        class="mt-3 text-small text-text-muted"
        data-testid="intake-round-robin-description"
      >
        Rotates across all active members in join order.
      </p>

      <p
        v-else
        class="mt-3 text-small text-text-muted"
        data-testid="intake-unassigned-warning"
      >
        Unattended leads will be created unassigned and appear on no one's Today.
      </p>

      <p
        v-if="saveError"
        class="mt-1.5 text-small text-danger"
      >
        {{ describeMutationError(saveError, 'Could not update the setting.') }}
      </p>
    </FormField>
  </div>

  <ConfirmDialog
    v-model:visible="rotateConfirmOpen"
    title="Rotate the intake address?"
    message="The current address stops working immediately — mail sent to it will be silently discarded. Update every forwarding rule to the new address after rotating."
    confirm-label="Rotate"
    confirm-variant="danger"
    :is-pending="rotateMutation.isPending.value"
    @confirm="rotate"
  />
</template>
