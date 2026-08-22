<script setup lang="ts">
// SLICE_004 §1 steps 2-3, §10: `/platform` — Organizations table with the
// state badge, ordered exactly as the API returns it (needs_attention,
// pending_first_admin, ok, then name — §5), and a New Organization dialog.
import { computed, h, ref } from 'vue'
import type { ColumnDef } from '@tanstack/vue-table'
import Dialog from 'primevue/dialog'
import PageHeader from '../components/PageHeader.vue'
import DataTable from '../components/DataTable.vue'
import Badge from '../components/Badge.vue'
import FormField from '../components/FormField.vue'
import { useCreateOrganizationMutation, usePlatformOrganizations } from '../api/queries'
import type { PlatformOrganizationSummary } from '../api/types'
import { buttonClasses, dialogPt, INPUT_CLASSES } from '../lib/controls'
import { describeApiError, describeMutationError } from '../lib/errors'
import { ORGANIZATION_STATE_LABEL, ORGANIZATION_STATE_TINT } from '../lib/labels'

const { data, isPending, isError, error } = usePlatformOrganizations()
const organizations = computed(() => data.value?.organizations ?? [])

const columns: ColumnDef<PlatformOrganizationSummary>[] = [
  {
    id: 'name',
    header: 'Name',
    cell: (info) => h('span', { class: 'text-body font-medium text-text' }, info.row.original.name),
  },
  {
    id: 'state',
    header: 'State',
    cell: (info) => {
      const state = info.row.original.state
      return h(Badge, { tint: ORGANIZATION_STATE_TINT[state] }, () => ORGANIZATION_STATE_LABEL[state])
    },
  },
  {
    id: 'member_count',
    header: 'Members',
    meta: { align: 'right' },
    cell: (info) => String(info.row.original.member_count),
  },
  {
    id: 'admin_count',
    header: 'Admins',
    meta: { align: 'right' },
    cell: (info) => String(info.row.original.admin_count),
  },
  {
    id: 'pending_admin_invitations',
    header: 'Pending admin invitations',
    meta: { align: 'right' },
    cell: (info) => String(info.row.original.pending_admin_invitations),
  },
]

const createDialogOpen = ref(false)
const name = ref('')
const createMutation = useCreateOrganizationMutation()

function openCreateDialog() {
  createMutation.reset()
  name.value = ''
  createDialogOpen.value = true
}

function closeCreateDialog() {
  if (createMutation.isPending.value) return
  createDialogOpen.value = false
}

function submitCreate() {
  createMutation.mutate(
    { name: name.value.trim() },
    {
      onSuccess: () => {
        createDialogOpen.value = false
      },
    },
  )
}
</script>

<template>
  <div>
    <PageHeader title="Organizations">
      <template #action>
        <button
          type="button"
          :class="buttonClasses('primary')"
          @click="openCreateDialog"
        >
          New Organization
        </button>
      </template>
    </PageHeader>

    <div
      v-if="isError"
      class="rounded-xl border border-border bg-surface-0 p-5 text-body text-danger"
    >
      {{ describeApiError(error, 'Could not load Organizations.') }}
    </div>
    <div
      v-else-if="isPending"
      class="rounded-xl border border-border bg-surface-0 p-5 text-body text-text-muted"
    >
      Loading…
    </div>
    <DataTable
      v-else
      :data="organizations"
      :columns="columns"
      :row-key="(org) => org.id"
      :row-to="(org) => `/platform/organizations/${org.id}`"
      count-noun="Organizations"
      count-noun-singular="Organization"
      empty-message="No Organizations yet."
    />

    <Dialog
      :visible="createDialogOpen"
      modal
      :closable="false"
      :close-on-escape="!createMutation.isPending.value"
      :dismissable-mask="!createMutation.isPending.value"
      :pt="dialogPt()"
      @update:visible="(value: boolean) => !value && closeCreateDialog()"
    >
      <template #header>
        <h2 class="text-section font-semibold text-text">
          New Organization
        </h2>
      </template>

      <form
        class="space-y-4"
        @submit.prevent="submitCreate"
      >
        <FormField
          v-slot="{ id }"
          label="Name"
          bare
        >
          <input
            :id="id"
            v-model="name"
            type="text"
            required
            :disabled="createMutation.isPending.value"
            :class="INPUT_CLASSES"
          >
        </FormField>

        <p
          v-if="createMutation.error.value"
          role="alert"
          class="text-small text-danger"
        >
          {{ describeMutationError(createMutation.error.value, 'Could not create the Organization.') }}
        </p>
      </form>

      <template #footer>
        <button
          type="button"
          :class="buttonClasses('secondary')"
          :disabled="createMutation.isPending.value"
          @click="closeCreateDialog"
        >
          Cancel
        </button>
        <button
          type="button"
          :class="buttonClasses('primary')"
          :disabled="createMutation.isPending.value || !name.trim()"
          @click="submitCreate"
        >
          {{ createMutation.isPending.value ? 'Creating…' : 'Create' }}
        </button>
      </template>
    </Dialog>
  </div>
</template>
