<script setup lang="ts">
// SLICE_004 §1 steps 5-8, §10: `/manage/members`. Members table (name,
// email, role, status, joined, People assigned) with per-row Promote /
// Demote / Deactivate / Reactivate behind a confirm dialog; Invitations
// section (email, role, status, expires, invited by) with Revoke and, for
// pending invitations, Re-issue; an Invite dialog that ends in a one-time
// copy-link panel.
import { computed, h, ref } from 'vue'
import type { ColumnDef } from '@tanstack/vue-table'
import PageHeader from '../components/PageHeader.vue'
import DataTable from '../components/DataTable.vue'
import Badge from '../components/Badge.vue'
import ConfirmDialog from '../components/ConfirmDialog.vue'
import InviteDialog from '../components/InviteDialog.vue'
import OneTimeLinkDialog from '../components/OneTimeLinkDialog.vue'
import {
  useChangeMemberRoleMutation,
  useInvitations,
  useIssueInvitationMutation,
  useMe,
  useMembers,
  useRevokeInvitationMutation,
  useSetMemberStatusMutation,
} from '../api/queries'
import type { Invitation, Member, MembershipRole } from '../api/types'
import { buttonClasses } from '../lib/controls'
import { describeApiError } from '../lib/errors'
import {
  INVITATION_STATUS_LABEL,
  INVITATION_STATUS_TINT,
  MEMBERSHIP_ROLE_LABEL,
  MEMBERSHIP_STATUS_LABEL,
  MEMBERSHIP_STATUS_TINT,
} from '../lib/labels'
import { formatAbsoluteTime, formatRelativeTime } from '../lib/format'

const { data: me } = useMe()
const orgId = computed(() => me.value?.organization?.id ?? '')

// ---- Members ---------------------------------------------------------

const { data: membersData, isPending: membersPending, isError: membersIsError, error: membersError } = useMembers(orgId)
const members = computed(() => membersData.value?.members ?? [])

const roleMutation = useChangeMemberRoleMutation(orgId)
const statusMutation = useSetMemberStatusMutation(orgId)

type MemberActionKind = 'promote' | 'demote' | 'deactivate' | 'reactivate'

const pendingMemberAction = ref<{ kind: MemberActionKind; member: Member } | null>(null)

const memberActionMutation = computed(() =>
  pendingMemberAction.value?.kind === 'promote' || pendingMemberAction.value?.kind === 'demote'
    ? roleMutation
    : statusMutation,
)

const MEMBER_ACTION_COPY: Record<
  MemberActionKind,
  (member: Member) => { title: string; message: string; confirmLabel: string; variant: 'primary' | 'danger' }
> = {
  promote: (member) => ({
    title: 'Promote to admin',
    message: `Make ${member.display_name} an admin of this Organization?`,
    confirmLabel: 'Promote',
    variant: 'primary',
  }),
  demote: (member) => ({
    title: 'Demote to member',
    message: `Remove admin access for ${member.display_name}?`,
    confirmLabel: 'Demote',
    variant: 'primary',
  }),
  deactivate: (member) => ({
    title: 'Deactivate member',
    message: `${member.display_name} will lose access immediately and be signed out. Their assigned People are unaffected.`,
    confirmLabel: 'Deactivate',
    variant: 'danger',
  }),
  reactivate: (member) => ({
    title: 'Reactivate member',
    message: `Restore ${member.display_name}'s access to this Organization?`,
    confirmLabel: 'Reactivate',
    variant: 'primary',
  }),
}

const memberActionCopy = computed(() =>
  pendingMemberAction.value ? MEMBER_ACTION_COPY[pendingMemberAction.value.kind](pendingMemberAction.value.member) : null,
)

function openMemberAction(kind: MemberActionKind, member: Member) {
  roleMutation.reset()
  statusMutation.reset()
  pendingMemberAction.value = { kind, member }
}

function closeMemberAction() {
  if (memberActionMutation.value.isPending.value) return
  pendingMemberAction.value = null
}

function confirmMemberAction() {
  const action = pendingMemberAction.value
  if (!action) return
  if (action.kind === 'promote' || action.kind === 'demote') {
    roleMutation.mutate(
      { userId: action.member.user_id, role: action.kind === 'promote' ? 'admin' : 'member' },
      {
        onSuccess: () => {
          pendingMemberAction.value = null
        },
      },
    )
  } else {
    statusMutation.mutate(
      { userId: action.member.user_id, status: action.kind === 'deactivate' ? 'inactive' : 'active' },
      {
        onSuccess: () => {
          pendingMemberAction.value = null
        },
      },
    )
  }
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
    id: 'joined_at',
    header: 'Joined',
    cell: (info) => {
      const value = info.row.original.joined_at
      return h('span', { title: formatAbsoluteTime(value) }, formatRelativeTime(value))
    },
  },
  {
    id: 'assigned_people_count',
    header: 'People assigned',
    meta: { align: 'right' },
    cell: (info) => String(info.row.original.assigned_people_count),
  },
  {
    id: 'actions',
    header: '',
    cell: (info) => {
      const member = info.row.original
      const roleButton =
        member.role === 'member'
          ? h(
              'button',
              {
                type: 'button',
                class: buttonClasses('secondary'),
                disabled: member.status === 'inactive',
                onClick: (event: MouseEvent) => {
                  event.stopPropagation()
                  openMemberAction('promote', member)
                },
              },
              'Promote',
            )
          : h(
              'button',
              {
                type: 'button',
                class: buttonClasses('secondary'),
                onClick: (event: MouseEvent) => {
                  event.stopPropagation()
                  openMemberAction('demote', member)
                },
              },
              'Demote',
            )
      const statusButton =
        member.status === 'active'
          ? h(
              'button',
              {
                type: 'button',
                class: buttonClasses('secondary'),
                onClick: (event: MouseEvent) => {
                  event.stopPropagation()
                  openMemberAction('deactivate', member)
                },
              },
              'Deactivate',
            )
          : h(
              'button',
              {
                type: 'button',
                class: buttonClasses('secondary'),
                onClick: (event: MouseEvent) => {
                  event.stopPropagation()
                  openMemberAction('reactivate', member)
                },
              },
              'Reactivate',
            )
      return h('div', { class: 'flex justify-end gap-2' }, [roleButton, statusButton])
    },
  },
]

// ---- Invitations -------------------------------------------------------

const {
  data: invitationsData,
  isPending: invitationsPending,
  isError: invitationsIsError,
  error: invitationsError,
} = useInvitations(orgId)
const invitations = computed(() => invitationsData.value?.invitations ?? [])

const issueMutation = useIssueInvitationMutation(orgId)
const revokeMutation = useRevokeInvitationMutation(orgId)

const inviteDialogOpen = ref(false)
const oneTimeLink = ref<{ email: string; acceptPath: string } | null>(null)

function openInviteDialog() {
  issueMutation.reset()
  inviteDialogOpen.value = true
}

function onInviteSubmit(payload: { email: string; role: MembershipRole }) {
  issueMutation.mutate(payload, {
    onSuccess: (data) => {
      inviteDialogOpen.value = false
      oneTimeLink.value = { email: payload.email, acceptPath: data.accept_path }
    },
  })
}

function closeOneTimeLink(value: boolean) {
  if (!value) oneTimeLink.value = null
}

function reissue(invitation: Invitation) {
  issueMutation.mutate(
    { email: invitation.email, role: invitation.role },
    {
      onSuccess: (data) => {
        oneTimeLink.value = { email: invitation.email, acceptPath: data.accept_path }
      },
    },
  )
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
    id: 'role',
    header: 'Role',
    cell: (info) => h(Badge, { tint: 'neutral' }, () => MEMBERSHIP_ROLE_LABEL[info.row.original.role]),
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
      const buttons = []
      if (invitation.status === 'pending') {
        buttons.push(
          h(
            'button',
            {
              type: 'button',
              class: buttonClasses('secondary'),
              disabled: issueMutation.isPending.value,
              onClick: (event: MouseEvent) => {
                event.stopPropagation()
                reissue(invitation)
              },
            },
            'Re-issue',
          ),
        )
      }
      if (invitation.status === 'pending' || invitation.status === 'expired') {
        buttons.push(
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
        )
      }
      return h('div', { class: 'flex justify-end gap-2' }, buttons)
    },
  },
]
</script>

<template>
  <div>
    <PageHeader
      title="Members"
      :subtitle="me?.organization ? `Who can access ${me.organization.name}.` : undefined"
    />

    <div
      v-if="membersIsError"
      class="mb-4 rounded-xl border border-border bg-surface-0 p-5 text-body text-danger"
    >
      {{ describeApiError(membersError, 'Could not load members.') }}
    </div>
    <div
      v-else-if="membersPending"
      class="mb-4 rounded-xl border border-border bg-surface-0 p-5 text-body text-text-muted"
    >
      Loading…
    </div>
    <DataTable
      v-else
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
        Invite
      </button>
    </div>

    <div
      v-if="invitationsIsError"
      class="rounded-xl border border-border bg-surface-0 p-5 text-body text-danger"
    >
      {{ describeApiError(invitationsError, 'Could not load invitations.') }}
    </div>
    <div
      v-else-if="invitationsPending"
      class="rounded-xl border border-border bg-surface-0 p-5 text-body text-text-muted"
    >
      Loading…
    </div>
    <DataTable
      v-else
      :data="invitations"
      :columns="invitationColumns"
      :row-key="(invitation) => invitation.id"
      count-noun="invitations"
      count-noun-singular="invitation"
      empty-message="No invitations yet."
    />

    <ConfirmDialog
      :visible="pendingMemberAction !== null"
      :title="memberActionCopy?.title ?? ''"
      :message="memberActionCopy?.message ?? ''"
      :confirm-label="memberActionCopy?.confirmLabel ?? 'Confirm'"
      :confirm-variant="memberActionCopy?.variant"
      :is-pending="memberActionMutation.isPending.value"
      :error="memberActionMutation.error.value"
      @update:visible="(value) => !value && closeMemberAction()"
      @confirm="confirmMemberAction"
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
      :role-options="[
        { id: 'member', label: 'Member' },
        { id: 'admin', label: 'Admin' },
      ]"
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
  </div>
</template>
