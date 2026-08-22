import type { UnresolvedReason } from '../api/types'

/** Shared by UnresolvedView (queue) and NewInquiryView (inline outcome). */
export const UNRESOLVED_REASON_LABEL: Record<UnresolvedReason, string> = {
  invalid_json: 'Invalid JSON',
  not_an_object: 'Not an object',
  no_contact_method: 'No contact method',
}
