<script setup lang="ts">
// SLICE_004 §1 step 3, §10: `/platform/organizations/:id` — the
// Organization's members (Promote action only, enabled for active members)
// and invitations (Invite admin / Revoke), the same one-time link panel as
// MembersView.vue. A `needs_attention` Organization leads with a hint that
// both recovery actions (promote or invite) are always available (D-026 §3).
import { computed, h, ref } from 'vue'
import { RouterLink } from 'vue-router'
import type { ColumnDef } from '@tanstack/vue-table'
import PageHeader from '../components/PageHeader.vue'
import DataTable from '../components/DataTable.vue'
import Badge from '../components/Badge.vue'
import ConfirmDialog from '../components/ConfirmDialog.vue'
import InviteDialog from '../components/InviteDialog.vue'
import OneTimeLinkDialog from '../components/OneTimeLinkDialog.vue'
import {
  usePlatformIssueInvitationMutation,
  usePlatformOrganization,
  usePlatformPromoteMutation,
  usePlatformRevokeInvitationMutation,
} from '../api/queries'
import { ApiError } from '../api/client'
import type { Invitation, Member, MembershipRole } from '../api/types'
import { buttonClasses } from '../lib/controls'
import { describeApiError } from '../lib/errors'
import {
  INVITATION_STATUS_LABEL,
  INVITATION_STATUS_TINT,
  MEMBERSHIP_ROLE_LABEL,
  MEMBERSHIP_STATUS_LABEL,
  MEMBERSHIP_STATUS_TINT,
  ORGANIZATION_STATE_LABEL,
  ORGANIZATION_STATE_TINT,
} from '../lib/labels'
import { formatAbsoluteTime, formatRelativeTime } from '../lib/format'

const props = defineProps<{ id: string }>()

const { data, isPending, isError, error } = usePlatformOrganization(() => props.id)
const organization = computed(() => data.value?.organization)
const members = computed(() => data.value?.members ?? [])
const invitations = computed(() => data.value?.invitations ?? [])

const notFound = computed(() => error.value instanceof ApiError && error.value.status === 404)

// ---- Members: Promote only, active rows only ------------------------

const promoteMutation = usePlatformPromoteMutation(() => props.id)
const pendingPromote = ref<Member | null>(null)

function openPromote(member: Member) {
  promoteMutation.reset()
  pendingPromote.value = member
}

function closePromote() {
  if (promoteMutation.isPending.value) return
  pendingPromote.value = null
}

function confirmPromote() {
  const member = pendingPromote.value
  if (!member) return
  promoteMutation.mutate(member.user_id, {
    onSuccess: () => {
      pendingPromote.value = null
    },
  })
}

const memberColumns: ColumnDef<Member>[] = [
  {
    id: 'name',
    header: 'Name',
    cell: (info) => {
      const member = info.row.original
      return h('div', [
        h('p', { class: 'text-body font-medium text-text' }, member.display_name),
        h('p', { class: 'text-small text-text-muted' }, member.email),
      ])
    },
  },
  {
    id: 'role',
    header: 'Role',
    cell: (info) => h(Badge, { tint: 'neutral' }, () => MEMBERSHIP_ROLE_LABEL[info.row.original.role]),
  },
  {
    id: 'status',
    header: 'Status',
    cell: (info) => {
      const status = info.row.original.status
      return h(Badge, { tint: MEMBERSHIP_STATUS_TINT[status] }, () => MEMBERSHIP_STATUS_LABEL[status])
    },
  },
  {
    id: 'actions',
    header: '',
    cell: (info) => {
      const member = info.row.original
      if (member.role !== 'member' || member.status !== 'active') return null
      return h('div', { class: 'flex justify-end' }, [
        h(
          'button',
          {
            type: 'button',
            class: buttonClasses('secondary'),
            onClick: (event: MouseEvent) => {
              event.stopPropagation()
              openPromote(member)
            },
          },
          'Promote',
        ),
      ])
    },
  },
]

// ---- Invitations: Invite admin / Revoke -----------------------------

const issueMutation = usePlatformIssueInvitationMutation(() => props.id)
const revokeMutation = usePlatformRevokeInvitationMutation(() => props.id)

const inviteDialogOpen = ref(false)
const oneTimeLink = ref<{ email: string; acceptPath: string } | null>(null)

function openInviteDialog() {
  issueMutation.reset()
  inviteDialogOpen.value = true
}

function onInviteSubmit(payload: { email: string; role: MembershipRole }) {
  issueMutation.mutate(
    { email: payload.email, role: 'admin' },
    {
      onSuccess: (data) => {
        inviteDialogOpen.value = false
        oneTimeLink.value = { email: payload.email, acceptPath: data.accept_path }
      },
    },
  )
}

function closeOneTimeLink(value: boolean) {
  if (!value) oneTimeLink.value = null
}

const pendingRevoke = ref<Invitation | null>(null)

function openRevoke(invitation: Invitation) {
  revokeMutation.reset()
  pendingRevoke.value = invitation
}

function closeRevoke() {
  if (revokeMutation.isPending.value) return
  pendingRevoke.value = null
}

function confirmRevoke() {
  const invitation = pendingRevoke.value
  if (!invitation) return
  revokeMutation.mutate(invitation.id, {
    onSuccess: () => {
      pendingRevoke.value = null
    },
  })
}

const invitationColumns: ColumnDef<Invitation>[] = [
  {
    id: 'email',
    header: 'Email',
    cell: (info) => h('span', { class: 'text-body text-text' }, info.row.original.email),
  },
  {
    id: 'status',
    header: 'Status',
    cell: (info) => {
      const status = info.row.original.status
      return h(Badge, { tint: INVITATION_STATUS_TINT[status] }, () => INVITATION_STATUS_LABEL[status])
    },
  },
  {
    id: 'expires_at',
    header: 'Expires',
    cell: (info) => {
      const value = info.row.original.expires_at
      return h('span', { title: formatAbsoluteTime(value) }, formatRelativeTime(value))
    },
  },
  {
    id: 'invited_by',
    header: 'Invited by',
    cell: (info) => info.row.original.invited_by.display_name,
  },
  {
    id: 'actions',
    header: '',
    cell: (info) => {
      const invitation = info.row.original
      if (invitation.status !== 'pending' && invitation.status !== 'expired') return null
      return h('div', { class: 'flex justify-end' }, [
        h(
          'button',
          {
            type: 'button',
            class: buttonClasses('secondary'),
            onClick: (event: MouseEvent) => {
              event.stopPropagation()
              openRevoke(invitation)
            },
          },
          'Revoke',
        ),
      ])
    },
  },
]
</script>

<template>
  <div>
    <nav class="mb-2 text-small text-text-muted">
      <RouterLink
        to="/platform"
        class="hover:text-text"
      >
        Organizations
      </RouterLink>
      <span
        v-if="organization"
        class="mx-1.5"
      >/</span>
      <span
        v-if="organization"
        class="text-text"
      >{{ organization.name }}</span>
    </nav>

    <div
      v-if="notFound"
      class="rounded-xl border border-border bg-surface-0 p-5 text-body text-text-muted"
    >
      Organization not found.
    </div>
    <div
      v-else-if="isError"
      class="rounded-xl border border-border bg-surface-0 p-5 text-body text-danger"
    >
      {{ describeApiError(error, 'Could not load this Organization.') }}
    </div>
    <div
      v-else-if="isPending"
      class="rounded-xl border border-border bg-surface-0 p-5 text-body text-text-muted"
    >
      Loading…
    </div>

    <template v-else-if="organization">
      <PageHeader :title="organization.name">
        <template #action>
          <Badge :tint="ORGANIZATION_STATE_TINT[organization.state]">
            {{ ORGANIZATION_STATE_LABEL[organization.state] }}
          </Badge>
        </template>
      </PageHeader>

      <div
        v-if="organization.state === 'needs_attention'"
        class="mb-6 rounded-xl border border-border bg-surface-0 p-5 text-body text-text"
      >
        Restore admin: promote a member or invite a new admin.
      </div>

      <h2 class="mb-4 text-section font-semibold text-text">
        Members
      </h2>
      <DataTable
        :data="members"
        :columns="memberColumns"
        :row-key="(member) => member.user_id"
        count-noun="members"
        count-noun-singular="member"
        empty-message="No members yet."
      />

      <div class="mt-8 mb-4 flex items-center justify-between gap-4">
        <h2 class="text-section font-semibold text-text">
          Invitations
        </h2>
        <button
          type="button"
          :class="buttonClasses('primary')"
          @click="openInviteDialog"
        >
          Invite admin
        </button>
      </div>
      <DataTable
        :data="invitations"
        :columns="invitationColumns"
        :row-key="(invitation) => invitation.id"
        count-noun="invitations"
        count-noun-singular="invitation"
        empty-message="No invitations yet."
      />

      <ConfirmDialog
        :visible="pendingPromote !== null"
        title="Promote to admin"
        :message="pendingPromote ? `Make ${pendingPromote.display_name} an admin of ${organization.name}?` : ''"
        confirm-label="Promote"
        :is-pending="promoteMutation.isPending.value"
        :error="promoteMutation.error.value"
        @update:visible="(value) => !value && closePromote()"
        @confirm="confirmPromote"
      />

      <ConfirmDialog
        :visible="pendingRevoke !== null"
        title="Revoke invitation"
        :message="pendingRevoke ? `Revoke the invitation to ${pendingRevoke.email}? The link will stop working.` : ''"
        confirm-label="Revoke"
        confirm-variant="danger"
        :is-pending="revokeMutation.isPending.value"
        :error="revokeMutation.error.value"
        @update:visible="(value) => !value && closeRevoke()"
        @confirm="confirmRevoke"
      />

      <InviteDialog
        v-model:visible="inviteDialogOpen"
        :role-options="[{ id: 'admin', label: 'Admin' }]"
        :is-pending="issueMutation.isPending.value"
        :error="issueMutation.error.value"
        @submit="onInviteSubmit"
      />

      <OneTimeLinkDialog
        :visible="oneTimeLink !== null"
        :email="oneTimeLink?.email ?? ''"
        :accept-path="oneTimeLink?.acceptPath ?? ''"
        @update:visible="closeOneTimeLink"
      />
    </template>
  </div>
</template>
