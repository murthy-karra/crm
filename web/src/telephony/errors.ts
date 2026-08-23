// SLICE_006 §10 error copy, exact — do not reword. Everything not named
// here falls through to the generic pattern (lib/errors.ts), so the
// network/401/`unavailable` wording stays identical across the app.
import { ApiError } from '../api/client'
import { describeMutationError } from '../lib/errors'

const CALL_CODE_MESSAGES: Record<string, string> = {
  telephony_disabled: 'Calling is not configured on this server.',
  telephony_unavailable: 'Calling is temporarily unavailable — try again in a moment.',
  call_in_progress: 'You already have a call in progress.',
  invalid_contact_method: "That number can't be called.",
  invalid_call_state: 'This call can no longer be dialed.',
  forbidden: 'Only the caller can control this call.',
  not_found: 'This call no longer exists.',
}

/** Client-side failures that are not `ApiError`s: the composable raises
 * these with a stable code so the panel's copy is testable per code. */
export class CallClientError extends Error {
  readonly code: 'microphone_denied' | 'join_failed'

  constructor(code: CallClientError['code'], cause?: unknown) {
    super(code, cause === undefined ? undefined : { cause })
    this.name = 'CallClientError'
    this.code = code
  }
}

const CLIENT_CODE_MESSAGES: Record<CallClientError['code'], string> = {
  microphone_denied: 'Microphone access was denied. Allow the microphone and try again.',
  join_failed: 'Could not connect to the call server. Try again in a moment.',
}

export function describeCallError(err: unknown): string {
  if (err instanceof CallClientError) return CLIENT_CODE_MESSAGES[err.code]
  if (err instanceof ApiError) {
    const message = CALL_CODE_MESSAGES[err.code]
    if (message) return message
  }
  return describeMutationError(err, 'Could not place the call.')
}

/** The `call_id` from SLICE_006 §5's one envelope extension
 * (`409 {"error": "call_in_progress", "call_id"}`), or null. */
export function callInProgressId(err: unknown): string | null {
  if (!(err instanceof ApiError) || err.code !== 'call_in_progress') return null
  const id = err.details.call_id
  return typeof id === 'string' && id !== '' ? id : null
}

// SLICE_006c §10 error copy for `POST /api/calls/{id}/outcome`, exact.
const OUTCOME_CODE_MESSAGES: Record<string, string> = {
  invalid_call_state: "The call hasn't finished yet.",
  no_contact_attempt: "There's no contact attempt to correct.",
  correction_conflict: 'This outcome was just changed — refreshed.',
  forbidden: 'Only the caller can change this outcome.',
  not_found: 'This call no longer exists.',
}

export function describeOutcomeError(err: unknown): string {
  if (err instanceof ApiError) {
    const message = OUTCOME_CODE_MESSAGES[err.code]
    if (message) return message
  }
  return describeMutationError(err, 'Could not save the outcome.')
}
