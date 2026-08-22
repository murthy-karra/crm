import { QueryCache, QueryClient } from '@tanstack/vue-query'
import { ApiError } from './api/client'

// Registered by router.ts once the router instance exists. query-client.ts
// intentionally never imports router.ts directly — router.ts already
// imports `queryClient` from here, so a static import back would be a
// circular module dependency. This setter is the decoupling point.
let unauthorizedHandler: (() => void) | null = null
export function setUnauthorizedHandler(handler: () => void): void {
  unauthorizedHandler = handler
}

// A 401 means "not authenticated" — retrying it delays the redirect to
// /login for no benefit. Anything else (503 unavailable, network) gets one
// retry before the query surfaces as an error.
function shouldRetry(failureCount: number, error: unknown): boolean {
  if (error instanceof ApiError && error.status === 401) {
    return false
  }
  return failureCount < 1
}

// Global catch for a 401 a *query* hits outside of navigation. The router
// guard (router.ts) only re-checks auth when the user actually navigates;
// it does not cover a session that is revoked or expires while the user
// sits on a page — the next background refetch (TanStack Query v5 default:
// refetchOnWindowFocus) would otherwise just leave a stale, broken-looking
// screen with no redirect. Scoped to queries, not mutations: a failed
// login attempt is its own expected 401 that LoginView already handles,
// and including it here would mean clearing the cache mid-login-error.
function handleQueryError(error: unknown): void {
  if (error instanceof ApiError && error.status === 401) {
    unauthorizedHandler?.()
  }
}

// Shared instance: main.ts installs it as the VueQueryPlugin, router.ts
// reads it in the navigation guard (useMe) and registers the handler
// above, and api/queries.ts mutations invalidate/clear it.
export const queryClient = new QueryClient({
  queryCache: new QueryCache({ onError: handleQueryError }),
  defaultOptions: {
    queries: {
      staleTime: 30_000,
      retry: shouldRetry,
    },
  },
})
