import type {
  ContactChannel,
  ContactOutcome,
  InvitationStatus,
  MembershipRole,
  MembershipStatus,
  OrganizationState,
  UnresolvedReason,
} from '../api/types'
import type { BadgeTint } from './controls'

/** Shared by UnresolvedView (queue) and NewInquiryView (inline outcome). */
export const UNRESOLVED_REASON_LABEL: Record<UnresolvedReason, string> = {
  invalid_json: 'Invalid JSON',
  not_an_object: 'Not an object',
  no_contact_method: 'No contact method',
}

/** Shared by LogContactDialog.vue's Channel Select and PersonDetailView's
 * history summary (lowercased there — SLICE_003 §1's "Contact attempted —
 * call, no answer" phrasing). */
export const CONTACT_CHANNEL_LABEL: Record<ContactChannel, string> = {
  call: 'Call',
  text: 'Text',
  email: 'Email',
  other: 'Other',
}

/** Shared by LogContactDialog.vue's Outcome Select and PersonDetailView's
 * history summary. */
export const CONTACT_OUTCOME_LABEL: Record<ContactOutcome, string> = {
  reached: 'Reached',
  no_answer: 'No answer',
  left_message: 'Left message',
  sent: 'Sent',
}

/** LogContactDialog.vue's per-channel default outcome (SLICE_003 §10: "per-
 * channel default outcome"). §1's walkthrough pins Call → No answer as the
 * dialog's opening selection; the rest are reasonable defaults for the
 * ContactOutcome vocabulary (§2 "no channel/outcome cross-validation" — the
 * UI, not the server, picks sensible defaults). */
export const DEFAULT_OUTCOME_FOR_CHANNEL: Record<ContactChannel, ContactOutcome> = {
  call: 'no_answer',
  text: 'sent',
  email: 'sent',
  other: 'reached',
}

// ---- SLICE_004 (MembersView, InviteView, PlatformOrganizations{,Organization}View) ----

export const MEMBERSHIP_ROLE_LABEL: Record<MembershipRole, string> = {
  admin: 'Admin',
  member: 'Member',
}

export const MEMBERSHIP_STATUS_LABEL: Record<MembershipStatus, string> = {
  active: 'Active',
  inactive: 'Inactive',
}

/** `inactive` gets the "needs attention" red tint (UI_STYLE §3 "unresolved /
 * error") since it is the exceptional state for a membership; `active` is
 * the unremarkable default. */
export const MEMBERSHIP_STATUS_TINT: Record<MembershipStatus, BadgeTint> = {
  active: 'neutral',
  inactive: 'red',
}

export const INVITATION_STATUS_LABEL: Record<InvitationStatus, string> = {
  pending: 'Pending',
  accepted: 'Accepted',
  expired: 'Expired',
  revoked: 'Revoked',
}

export const INVITATION_STATUS_TINT: Record<InvitationStatus, BadgeTint> = {
  pending: 'neutral',
  accepted: 'green',
  expired: 'red',
  revoked: 'red',
}

/** D-026 §5 naming; the platform Organizations table's state badge. */
export const ORGANIZATION_STATE_LABEL: Record<OrganizationState, string> = {
  ok: 'OK',
  pending_first_admin: 'Pending first admin',
  needs_attention: 'Needs attention',
}

export const ORGANIZATION_STATE_TINT: Record<OrganizationState, BadgeTint> = {
  ok: 'green',
  pending_first_admin: 'neutral',
  needs_attention: 'red',
}
