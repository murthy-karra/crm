// `apiFetch`'s error-envelope handling. SLICE_006 §5 adds the one envelope
// extension (`409 {"error": "call_in_progress", "call_id": uuid}`); the
// extra field must survive into `ApiError.details` so the panel can offer
// "Hang up previous call" (§10).
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ApiError, apiFetch } from './client'
import { observeWorkspace, resetWorkspace } from '../workspaceLifecycle'
import {
  SessionVerificationPendingError,
  beginSessionTransition,
  completeSessionVerification,
  configureSessionLifecycle,
  currentSessionGeneration,
  settleSessionTransition,
} from '../sessionLifecycle'

const CALL_ID = '55555555-5555-5555-5555-555555555555'

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } })
}

function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((done) => { resolve = done })
  return { promise, resolve }
}

beforeEach(() => {
  observeWorkspace({ user: { id: 'actor', email: 'actor@example.test', display_name: 'Actor' }, organization: { id: 'org', name: 'Org', role: 'admin', workspace_mode: 'operational', workspace_revision: '1' }, platform_admin: false })
})
afterEach(() => {
  resetWorkspace()
  vi.unstubAllGlobals()
})

describe('apiFetch error envelopes', () => {
  it('does not dispatch a private request while another tab has announced a session replacement', async () => {
    const fetchMock = vi.fn()
    vi.stubGlobal('fetch', fetchMock)
    const transition = beginSessionTransition()

    await expect(apiFetch('/today')).rejects.toBeInstanceOf(SessionVerificationPendingError)
    expect(fetchMock).not.toHaveBeenCalled()

    // A seeded identity stands in for the authoritative successful `/me`
    // verification and leaves following tests outside the pending boundary.
    settleSessionTransition(transition, {})
    completeSessionVerification(currentSessionGeneration(), {})
  })

  it('drops a private response that resolves after the shared session generation changes', async () => {
    const response = deferred<Response>()
    const fetchMock = vi.fn(() => response.promise)
    vi.stubGlobal('fetch', fetchMock)

    const pending = apiFetch<{ private: string }>('/today')
    await Promise.resolve()
    expect(fetchMock).toHaveBeenCalledTimes(1)

    const transition = beginSessionTransition()
    response.resolve(jsonResponse(200, { private: 'old-session payload' }))

    await expect(pending).rejects.toBeInstanceOf(SessionVerificationPendingError)
    settleSessionTransition(transition, {})
    completeSessionVerification(currentSessionGeneration(), {})
  })

  it('keeps envelope fields beyond `error` in ApiError.details', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => jsonResponse(409, { error: 'call_in_progress', call_id: CALL_ID })),
    )
    const err = await apiFetch('/people/x/calls', { method: 'POST', body: '{}' }).catch((e: unknown) => e)
    expect(err).toBeInstanceOf(ApiError)
    const apiError = err as ApiError
    expect(apiError.status).toBe(409)
    expect(apiError.code).toBe('call_in_progress')
    expect(apiError.details).toEqual({ call_id: CALL_ID })
  })

  it('has empty details for the plain envelope', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => jsonResponse(503, { error: 'telephony_disabled' })))
    const err = (await apiFetch('/calls/x/dial', { method: 'POST' }).catch((e: unknown) => e)) as ApiError
    expect(err.code).toBe('telephony_disabled')
    expect(err.details).toEqual({})
  })

  it('reports unknown_error for a non-envelope body', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response('nope', { status: 502 })))
    const err = (await apiFetch('/calls/x').catch((e: unknown) => e)) as ApiError
    expect(err.status).toBe(502)
    expect(err.code).toBe('unknown_error')
    expect(err.details).toEqual({})
  })

  it('drops a deferred private body and clears private state if durable marker writes fail mid-response', async () => {
    const originalStorage = Object.getOwnPropertyDescriptor(window, 'localStorage')
    const values = new Map<string, string>()
    let writesAllowed = true
    Object.defineProperty(window, 'localStorage', {
      configurable: true,
      value: {
        get length() { return values.size },
        key: (index: number) => [...values.keys()][index] ?? null,
        getItem: (key: string) => values.get(key) ?? null,
        setItem: (key: string, value: string) => {
          if (!writesAllowed) throw new Error('quota exceeded')
          values.set(key, value)
        },
        removeItem: (key: string) => {
          if (!writesAllowed) throw new Error('quota exceeded')
          values.delete(key)
        },
      },
    })
    let discarded = 0
    configureSessionLifecycle({
      discardPrivateState: () => { discarded += 1 },
      verify: () => undefined,
      installVerifiedIdentity: () => undefined,
    })
    const response = deferred<Response>()
    vi.stubGlobal('fetch', vi.fn(() => response.promise))

    try {
      const pending = apiFetch<{ private: string }>('/today')
      await Promise.resolve()
      writesAllowed = false
      response.resolve(jsonResponse(200, { private: 'old body' }))

      await expect(pending).rejects.toBeInstanceOf(SessionVerificationPendingError)
      expect(discarded).toBe(1)
    } finally {
      if (originalStorage) Object.defineProperty(window, 'localStorage', originalStorage)
      else delete (window as { localStorage?: unknown }).localStorage
    }
  })
})
