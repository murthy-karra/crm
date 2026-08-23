// SLICE_006c §5/§10: `useCorrectCallOutcome` posts exactly `{"outcome"}` to
// `/calls/{id}/outcome` and, on success, invalidates the Person and Today
// queries (never the call key — the call row does not change, §6).
import { QueryClient } from '@tanstack/vue-query'
import { effectScope } from 'vue'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { apiFetch } from './client'
import { queryKeys, useCorrectCallOutcome } from './queries'
import type { CorrectOutcomeResponse } from './types'

vi.mock('./client', async (importOriginal) => {
  const actual = await importOriginal<typeof import('./client')>()
  return { ...actual, apiFetch: vi.fn() }
})

const apiFetchMock = vi.mocked(apiFetch)
const ORG_ID = 'org-1'
const CALL_ID = 'call-1'
const PERSON_ID = 'person-1'

function response(changed: boolean): CorrectOutcomeResponse {
  return {
    attempt: {
      id: 'att-2',
      channel: 'call',
      outcome: 'left_message',
      occurred_at: '2026-08-22T10:00:00.000Z',
      recorded_at: '2026-08-22T10:05:00.000Z',
      corrects_id: 'att-1',
    },
    changed,
  }
}

beforeEach(() => {
  apiFetchMock.mockReset()
})

describe('useCorrectCallOutcome', () => {
  it('posts the exact body and invalidates person + today on success', async () => {
    apiFetchMock.mockResolvedValueOnce(response(true))
    const queryClient = new QueryClient({ defaultOptions: { mutations: { retry: false } } })
    const invalidate = vi.spyOn(queryClient, 'invalidateQueries')
    const scope = effectScope()
    const mutation = scope.run(() => useCorrectCallOutcome(ORG_ID, queryClient))!
    const result = await mutation.mutateAsync({ callId: CALL_ID, personId: PERSON_ID, outcome: 'left_message' })
    expect(result.changed).toBe(true)
    expect(apiFetchMock).toHaveBeenCalledTimes(1)
    const [path, init] = apiFetchMock.mock.calls[0]
    expect(path).toBe(`/calls/${CALL_ID}/outcome`)
    expect(init?.method).toBe('POST')
    expect(JSON.parse(String(init?.body))).toEqual({ outcome: 'left_message' })
    const keys = invalidate.mock.calls.map(([filters]) => (typeof filters === 'function' ? filters() : filters)?.queryKey)
    expect(keys).toEqual([queryKeys.person(ORG_ID, PERSON_ID), queryKeys.today(ORG_ID)])
    scope.stop()
  })

  it('does not invalidate on failure', async () => {
    apiFetchMock.mockRejectedValueOnce(new Error('nope'))
    const queryClient = new QueryClient({ defaultOptions: { mutations: { retry: false } } })
    const invalidate = vi.spyOn(queryClient, 'invalidateQueries')
    const scope = effectScope()
    const mutation = scope.run(() => useCorrectCallOutcome(ORG_ID, queryClient))!
    await expect(mutation.mutateAsync({ callId: CALL_ID, personId: PERSON_ID, outcome: 'busy' })).rejects.toThrow('nope')
    expect(invalidate).not.toHaveBeenCalled()
    scope.stop()
  })
})
