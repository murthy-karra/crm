// `apiFetch`'s error-envelope handling. SLICE_006 §5 adds the one envelope
// extension (`409 {"error": "call_in_progress", "call_id": uuid}`); the
// extra field must survive into `ApiError.details` so the panel can offer
// "Hang up previous call" (§10).
import { afterEach, describe, expect, it, vi } from 'vitest'
import { ApiError, apiFetch } from './client'

const CALL_ID = '55555555-5555-5555-5555-555555555555'

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } })
}

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('apiFetch error envelopes', () => {
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
})
