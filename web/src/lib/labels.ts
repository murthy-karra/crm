import type { ContactChannel, ContactOutcome, UnresolvedReason } from '../api/types'

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
