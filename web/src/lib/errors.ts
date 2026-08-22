import { ApiError } from '../api/client'

/**
 * Human-readable message for a failed read query. Covers the failure modes
 * a list/detail screen can actually hit per docs/specs/SLICE_002.md §9
 * (database down → 503 `unavailable`).
 *
 * A 401 is normally intercepted before a view has to display this for long:
 * the router guard (router.ts) catches it on navigation, and the
 * QueryCache's global `onError` handler (query-client.ts) catches it for
 * everything else — a background refetch finding the session already dead
 * while the user sits on a page without navigating (e.g. TanStack Query's
 * default `refetchOnWindowFocus`) — and both redirect to /login. The case
 * below is the accurate message for the brief window before that redirect
 * lands, not the primary handling path.
 */
export function describeApiError(err: unknown, fallback: string): string {
  if (err instanceof ApiError) {
    if (err.status === 0) return 'Could not reach the server. Check your connection and try again.'
    if (err.status === 401) return 'Your session has expired. Redirecting to sign in…'
    if (err.code === 'unavailable') return 'The server is temporarily unavailable. Try again shortly.'
  }
  return fallback
}

/**
 * SLICE_004 §5's new `ApiError` codes, mapped to the human-readable text the
 * administration views (MembersView, InviteView, PlatformOrganizations{,
 * Organization}View) show inline next to the action that failed. `last_admin`
 * uses the exact wording spec §1 step 5 and §10 specify — do not reword it.
 */
const ADMIN_CODE_MESSAGES: Record<string, string> = {
  malformed_request: 'That request was not valid.',
  forbidden: 'You do not have permission to do that.',
  not_found: 'Not found.',
  invalid_email: 'Enter a valid email address.',
  weak_password: 'Password must be between 12 and 256 characters.',
  last_admin: 'You are the last active admin. Promote someone else first.',
  invitation_used: 'This invitation has already been used.',
  invitation_expired: 'This invitation has expired.',
  invitation_not_acceptable: 'This invitation cannot be accepted.',
  organization_name_taken: 'An Organization with this name already exists.',
  already_member: 'This person is already a member of this Organization.',
}

/** Same shape as `describeApiError`, extended with SLICE_004's mutation error codes. */
export function describeMutationError(err: unknown, fallback: string): string {
  if (err instanceof ApiError) {
    if (err.status === 0) return 'Could not reach the server. Check your connection and try again.'
    if (err.status === 401) return 'Your session has expired. Redirecting to sign in…'
    if (err.code === 'unavailable') return 'The server is temporarily unavailable. Try again shortly.'
    const message = ADMIN_CODE_MESSAGES[err.code]
    if (message) return message
  }
  return fallback
}
