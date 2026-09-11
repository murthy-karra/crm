import { assertWorkspaceEpoch, currentWorkspaceEpoch, refreshWorkspace, trackWorkspaceRequest, workspaceRequestAllowed } from '../workspaceLifecycle'
import {
  SessionVerificationPendingError,
  currentSessionGeneration,
  isCurrentSessionGeneration,
  requireVerifiedSession,
} from '../sessionLifecycle'

// Loopback dev (and a single-hostname tunnel) use the relative Vite-proxied
// path. When viewed from an "app.<domain>" tunnel hostname, the API lives
// on its own "api.<domain>" hostname and must be called directly — the API
// side must have CRM_CORS_ALLOWED_ORIGIN and CRM_SESSION_COOKIE_DOMAIN set
// to match (see .env.example). Moved here from App.vue (D-017 stack
// migration) — behavior unchanged.
export function resolveApiBaseUrl(): string {
  const { hostname, protocol } = window.location
  if (hostname.startsWith('app.')) {
    return `${protocol}//api.${hostname.slice('app.'.length)}/api`
  }
  return import.meta.env.VITE_API_BASE_URL ?? '/api'
}

const apiBaseUrl = resolveApiBaseUrl()

/**
 * Thrown by `apiFetch` for every non-2xx response. `code` is the `error`
 * field of the `{"error": "<code>"}` envelope shared by every endpoint
 * (docs/specs/SLICE_001.md §4, docs/specs/SLICE_002.md §5). `status` is 0
 * for a request that never reached the server (offline, DNS, CORS).
 */
export class ApiError extends Error {
  readonly status: number
  readonly code: string
  /** Every envelope field other than `error`. Empty for almost every
   * response; SLICE_006 §5's 409 `call_in_progress` is the one documented
   * extension (`{"error": "call_in_progress", "call_id": uuid}`) — read it
   * through `telephony/errors.ts`'s `callInProgressId`, never ad hoc. */
  readonly details: Readonly<Record<string, unknown>>

  constructor(status: number, code: string, details: Record<string, unknown> = {}) {
    super(`API error ${status}: ${code}`)
    this.name = 'ApiError'
    this.status = status
    this.code = code
    this.details = details
  }
}

interface ErrorEnvelope extends Record<string, unknown> {
  error: string
}

function isErrorEnvelope(value: unknown): value is ErrorEnvelope {
  return (
    typeof value === 'object' &&
    value !== null &&
    'error' in value &&
    typeof (value as { error: unknown }).error === 'string'
  )
}

/**
 * Fetch wrapper for every `/api/*` call. Always sends the session cookie,
 * always JSON. `path` is relative to the API root, e.g. `/people`, not
 * `/api/people`.
 */
export async function apiFetch<T>(path: string, init: RequestInit = {}): Promise<T> {
  const privateRequest = path !== '/me' && path !== '/session' && path !== '/invitations/preview' && path !== '/invitations/accept'
  if (privateRequest) requireVerifiedSession()
  if (privateRequest && !workspaceRequestAllowed(path, init.method ?? 'GET')) throw new ApiError(409, 'workspace_in_migration_review')
  const workspaceEpoch = privateRequest ? currentWorkspaceEpoch() : undefined
  const controller = privateRequest ? new AbortController() : undefined
  const release = controller ? trackWorkspaceRequest(controller) : undefined
  const signal = controller ? (init.signal ? AbortSignal.any([init.signal, controller.signal]) : controller.signal) : init.signal
  try { return await apiFetchAttempt<T>(path, { ...init, signal }, workspaceEpoch) }
  finally { release?.() }
}

async function apiFetchAttempt<T>(path: string, init: RequestInit, workspaceEpoch?: number): Promise<T> {
  // `/me` is the coordinator's authoritative verification request. Session
  // establishment endpoints must also remain usable while the old shared
  // cookie is being replaced. Every other Web API request is private and is
  // synchronously fenced before it can submit stale route state or Operator
  // history after another tab changed the cookie.
  const privateRequest =
    path !== '/me' && path !== '/session' &&
    path !== '/invitations/preview' && path !== '/invitations/accept'
  const sessionGeneration = privateRequest ? currentSessionGeneration() : undefined
  if (privateRequest) {
    requireVerifiedSession()
  }
  const assertCurrentSession = () => {
    if (sessionGeneration !== undefined && !isCurrentSessionGeneration(sessionGeneration)) {
      throw new SessionVerificationPendingError()
    }
    if (workspaceEpoch !== undefined) assertWorkspaceEpoch(workspaceEpoch)
  }
  const headers = new Headers(init.headers)
  if (init.body !== undefined && !headers.has('Content-Type')) {
    headers.set('Content-Type', 'application/json')
  }

  let response: Response
  try {
    response = await fetch(`${apiBaseUrl}${path}`, {
      ...init,
      credentials: 'include',
      headers,
    })
  } catch {
    assertCurrentSession()
    throw new ApiError(0, 'network_error')
  }

  if (!response.ok) {
    const body: unknown = await response.json().catch(() => null)
    assertCurrentSession()
    if (isErrorEnvelope(body)) {
      const { error, ...details } = body
      if (privateRequest && (error === 'workspace_in_migration_review' || response.status === 401 || response.status === 403)) void refreshWorkspace().catch(() => {})
      throw new ApiError(response.status, error, details)
    }
    throw new ApiError(response.status, 'unknown_error')
  }

  if (response.status === 204) {
    assertCurrentSession()
    return undefined as T
  }

  let body: T
  try {
    body = (await response.json()) as T
  } catch (error) {
    assertCurrentSession()
    throw error
  }
  assertCurrentSession()
  return body
}
