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
