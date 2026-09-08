// SLICE_006c §5/§10: `useCorrectCallOutcome` posts exactly `{"outcome"}` to
// `/calls/{id}/outcome` and, on success, invalidates the Person and Today
// queries (never the call key — the call row does not change, §6).
import { QueryClient } from '@tanstack/vue-query'
import { effectScope, ref } from 'vue'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { ApiError, apiFetch } from './client'
import {
  queryKeys,
  fetchMe,
  useAddPersonTagMutation,
  useCorrectCallOutcome,
  useCreateSavedListMutation,
  useCreateTagMutation,
  useDeleteSavedListMutation,
  useDeleteTagMutation,
  useRemovePersonTagMutation,
  useRenameTagMutation,
  useUpdateSavedListMutation,
} from './queries'
import { beginSessionTransition, settleSessionTransition } from '../sessionLifecycle'
import type {
  CorrectOutcomeResponse,
  CreateSavedListResponse,
  CreateTagResponse,
  DeleteSavedListResponse,
  MeResponse,
  PersonTagMutationResponse,
  SavedListDetailResponse,
  SavedListMetadata,
  UpdateSavedListResponse,
} from './types'

vi.mock('./client', async (importOriginal) => {
  const actual = await importOriginal<typeof import('./client')>()
  return { ...actual, apiFetch: vi.fn() }
})

const apiFetchMock = vi.mocked(apiFetch)
const ORG_ID = 'org-1'
const CALL_ID = 'call-1'
const PERSON_ID = 'person-1'
const SAVED_LIST_ID = 'saved-list-1'

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

function savedMe(orgId = ORG_ID, actorId = 'actor-a'): MeResponse {
  return {
    user: { id: actorId, email: `${actorId}@example.test`, display_name: actorId },
    organization: { id: orgId, name: 'Example', role: 'member' },
    platform_admin: false,
  }
}

function savedList(revision = 1): SavedListMetadata {
  return {
    id: SAVED_LIST_ID,
    name: 'Active buyers',
    scope: 'personal',
    revision,
    created_at: '2026-09-06T00:00:00.000Z',
    updated_at: '2026-09-06T00:00:00.000Z',
    can_edit: true,
    can_delete: true,
  }
}

function savedDetail(revision = 1): SavedListDetailResponse {
  return {
    list: savedList(revision),
    filter: { version: 1, clauses: [{ kind: 'assigned_to', assignees: ['me'] }] },
    sort: null,
    description: ['Assigned to me'],
    filter_error: null,
  }
}

function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (reason: unknown) => void
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no })
  return { promise, resolve, reject }
}

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

// SLICE_011e §5, §10: create-or-get, apply/remove, and rename/delete each
// invalidate the tags key (apply/remove also the Person key); rename/
// delete/apply/remove additionally refetch the tags key on a 403 or 404
// (a stale rule-1 permission or a vanished tag), never on any other error.
describe('tag mutations', () => {
  const TAG_ID = 'tag-1'

  function tagResult(): CreateTagResponse {
    return { tag: { id: TAG_ID, name: 'Investor', person_count: 0, can_manage: true }, created: true }
  }

  it('useCreateTagMutation invalidates the tags key on success only', async () => {
    apiFetchMock.mockResolvedValueOnce(tagResult())
    const queryClient = new QueryClient({ defaultOptions: { mutations: { retry: false } } })
    const invalidate = vi.spyOn(queryClient, 'invalidateQueries')
    const scope = effectScope()
    const mutation = scope.run(() => useCreateTagMutation(ORG_ID, queryClient))!
    await mutation.mutateAsync({ name: 'Investor' })
    const [path, init] = apiFetchMock.mock.calls[0]
    expect(path).toBe('/tags')
    expect(init?.method).toBe('POST')
    const keys = invalidate.mock.calls.map(([filters]) => (typeof filters === 'function' ? filters() : filters)?.queryKey)
    expect(keys).toEqual([queryKeys.tags(ORG_ID)])
    scope.stop()
  })

  it('useAddPersonTagMutation sends no body and invalidates tags + the Person key', async () => {
    const response: PersonTagMutationResponse = { tags: [{ id: TAG_ID, name: 'Investor' }], changed: true }
    apiFetchMock.mockResolvedValueOnce(response)
    const queryClient = new QueryClient({ defaultOptions: { mutations: { retry: false } } })
    const invalidate = vi.spyOn(queryClient, 'invalidateQueries')
    const scope = effectScope()
    const mutation = scope.run(() => useAddPersonTagMutation(ORG_ID, queryClient))!
    await mutation.mutateAsync({ personId: PERSON_ID, tagId: TAG_ID })
    const [path, init] = apiFetchMock.mock.calls[0]
    expect(path).toBe(`/people/${PERSON_ID}/tags/${TAG_ID}`)
    expect(init?.method).toBe('PUT')
    expect(init?.body).toBeUndefined()
    const keys = invalidate.mock.calls.map(([filters]) => (typeof filters === 'function' ? filters() : filters)?.queryKey)
    expect(keys).toEqual([queryKeys.tags(ORG_ID), queryKeys.person(ORG_ID, PERSON_ID)])
    scope.stop()
  })

  async function expectRefetchOnlyOnStaleReference(
    build: (queryClient: QueryClient) => { mutateAsync: (variables: never) => Promise<unknown> },
    variables: unknown,
  ) {
    for (const status of [403, 404] as const) {
      apiFetchMock.mockReset()
      apiFetchMock.mockRejectedValueOnce(new ApiError(status, status === 403 ? 'forbidden' : 'not_found'))
      const queryClient = new QueryClient({ defaultOptions: { mutations: { retry: false } } })
      const invalidate = vi.spyOn(queryClient, 'invalidateQueries')
      const scope = effectScope()
      const mutation = scope.run(() => build(queryClient))!
      await expect(mutation.mutateAsync(variables as never)).rejects.toThrow()
      const keys = invalidate.mock.calls.map(([filters]) => (typeof filters === 'function' ? filters() : filters)?.queryKey)
      expect(keys).toEqual([queryKeys.tags(ORG_ID)])
      scope.stop()
    }

    apiFetchMock.mockReset()
    apiFetchMock.mockRejectedValueOnce(new ApiError(409, 'conflict'))
    const queryClient = new QueryClient({ defaultOptions: { mutations: { retry: false } } })
    const invalidate = vi.spyOn(queryClient, 'invalidateQueries')
    const scope = effectScope()
    const mutation = scope.run(() => build(queryClient))!
    await expect(mutation.mutateAsync(variables as never)).rejects.toThrow()
    expect(invalidate).not.toHaveBeenCalled()
    scope.stop()
  }

  it('useRenameTagMutation refetches the tags key on 403/404, not on 409', async () => {
    await expectRefetchOnlyOnStaleReference(
      (qc) => useRenameTagMutation(ORG_ID, qc),
      { tagId: TAG_ID, body: { name: 'Investor' } },
    )
  })

  it('useDeleteTagMutation refetches the tags key on 403/404, not on 409', async () => {
    await expectRefetchOnlyOnStaleReference((qc) => useDeleteTagMutation(ORG_ID, qc), TAG_ID)
  })

  it('useAddPersonTagMutation refetches the tags key on 403/404, not on 409', async () => {
    await expectRefetchOnlyOnStaleReference(
      (qc) => useAddPersonTagMutation(ORG_ID, qc),
      { personId: PERSON_ID, tagId: TAG_ID },
    )
  })

  it('useRemovePersonTagMutation refetches the tags key on 403/404, not on 409', async () => {
    await expectRefetchOnlyOnStaleReference(
      (qc) => useRemovePersonTagMutation(ORG_ID, qc),
      { personId: PERSON_ID, tagId: TAG_ID },
    )
  })
})

describe('session lifecycle verification', () => {
  it('lets a pending cached /me reader consume the coordinator verification without a second request', async () => {
    const verification = deferred<MeResponse>()
    apiFetchMock.mockImplementation((path: string) => {
      expect(path).toBe('/me')
      return verification.promise as Promise<never>
    })
    const transition = beginSessionTransition()
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    const waiting = queryClient.fetchQuery({ queryKey: queryKeys.me, queryFn: () => fetchMe() })
    await Promise.resolve()

    settleSessionTransition(transition)
    await Promise.resolve()
    expect(apiFetchMock).toHaveBeenCalledTimes(1)

    const verified = savedMe('org-new', 'actor-new')
    verification.resolve(verified)
    await expect(waiting).resolves.toEqual(verified)
    expect(apiFetchMock).toHaveBeenCalledTimes(1)
  })
})

describe('saved-list mutation session guards', () => {
  it('does not write or invalidate after a same-Organization actor switch', async () => {
    const orgId = ref(ORG_ID)
    const actorId = ref('actor-a')
    const firstSession = savedMe(ORG_ID, 'actor-a')
    const queryClient = new QueryClient({ defaultOptions: { mutations: { retry: false } } })
    queryClient.setQueryData(queryKeys.me, firstSession)
    queryClient.setQueryData(queryKeys.savedList(ORG_ID, 'actor-a', SAVED_LIST_ID), savedDetail())
    const invalidate = vi.spyOn(queryClient, 'invalidateQueries')
    const response = deferred<UpdateSavedListResponse>()
    apiFetchMock.mockReturnValueOnce(response.promise)
    const scope = effectScope()
    const mutation = scope.run(() => useUpdateSavedListMutation(orgId, actorId, queryClient))!

    const pending = mutation.mutateAsync({
      listId: SAVED_LIST_ID,
      body: { expected_revision: 1, name: 'Renamed', filter: { version: 1, clauses: [] } },
    })
    await Promise.resolve()
    actorId.value = 'actor-b'
    queryClient.setQueryData(queryKeys.me, savedMe(ORG_ID, 'actor-b'))
    response.resolve({ list: savedList(2), changed: true })
    await pending

    expect(queryClient.getQueryData(queryKeys.savedList(ORG_ID, 'actor-b', SAVED_LIST_ID))).toBeUndefined()
    expect(invalidate).not.toHaveBeenCalled()
    scope.stop()
  })

  it('does not restore a cleared private detail after A → logout → A', async () => {
    const orgId = ref(ORG_ID)
    const actorId = ref('actor-a')
    const firstSession = savedMe()
    const queryClient = new QueryClient({ defaultOptions: { mutations: { retry: false } } })
    queryClient.setQueryData(queryKeys.me, firstSession)
    queryClient.setQueryData(queryKeys.savedList(ORG_ID, 'actor-a', SAVED_LIST_ID), savedDetail())
    const response = deferred<UpdateSavedListResponse>()
    apiFetchMock.mockReturnValueOnce(response.promise)
    const scope = effectScope()
    const mutation = scope.run(() => useUpdateSavedListMutation(orgId, actorId, queryClient))!

    const pending = mutation.mutateAsync({
      listId: SAVED_LIST_ID,
      body: { expected_revision: 1, name: 'Renamed', filter: { version: 1, clauses: [] } },
    })
    await Promise.resolve()
    queryClient.removeQueries({ queryKey: queryKeys.me, exact: true })
    queryClient.removeQueries({ queryKey: queryKeys.savedList(ORG_ID, 'actor-a', SAVED_LIST_ID), exact: true })
    // A fresh object is the in-memory session generation, despite identical
    // public IDs after the user signs back in.
    queryClient.setQueryData(queryKeys.me, savedMe())
    response.resolve({ list: savedList(2), changed: true })
    await pending

    expect(queryClient.getQueryData(queryKeys.savedList(ORG_ID, 'actor-a', SAVED_LIST_ID))).toBeUndefined()
    scope.stop()
  })

  it('keeps create and delete completion cache work in their originating session', async () => {
    const orgId = ref(ORG_ID)
    const actorId = ref('actor-a')
    const firstSession = savedMe()
    const queryClient = new QueryClient({ defaultOptions: { mutations: { retry: false } } })
    queryClient.setQueryData(queryKeys.me, firstSession)
    const invalidate = vi.spyOn(queryClient, 'invalidateQueries')
    const createResponse = deferred<CreateSavedListResponse>()
    const deleteResponse = deferred<DeleteSavedListResponse>()
    apiFetchMock.mockReturnValueOnce(createResponse.promise).mockReturnValueOnce(deleteResponse.promise)
    const scope = effectScope()
    const create = scope.run(() => useCreateSavedListMutation(orgId, actorId, queryClient))!
    const remove = scope.run(() => useDeleteSavedListMutation(orgId, actorId, queryClient))!

    const creating = create.mutateAsync({
      request_id: '11111111-1111-4111-8111-111111111111',
      scope: 'personal', name: 'One', filter: { version: 1, clauses: [] },
    })
    await Promise.resolve()
    const deleting = remove.mutateAsync({ listId: SAVED_LIST_ID, body: { expected_revision: 1 } })
    await Promise.resolve()
    orgId.value = 'org-other'
    actorId.value = 'actor-other'
    queryClient.setQueryData(queryKeys.me, savedMe('org-other', 'actor-other'))
    createResponse.resolve({ list: savedList(), created: true })
    deleteResponse.resolve({ deleted: true })
    await Promise.all([creating, deleting])

    expect(invalidate).not.toHaveBeenCalled()
    expect(queryClient.getQueryData(queryKeys.savedList(ORG_ID, 'actor-a', SAVED_LIST_ID))).toBeUndefined()
    scope.stop()
  })
})

// SLICE_011b_SORT.md §9: the factory is extended only — without a sort the
// key stays byte-identical to today (3 or 4 elements); with a normalized
// sort token the key is always 5 elements, with the filter slot forced to
// `''` rather than omitted when there is no filter.
describe('queryKeys.people', () => {
  const filter = JSON.stringify({ version: 1, clauses: [{ kind: 'has_phone', value: true }] })

  it('keeps the pre-existing 3-element shape with no filter and no sort', () => {
    expect(queryKeys.people(ORG_ID)).toEqual(['org', ORG_ID, 'people'])
  })

  it('keeps the pre-existing 4-element shape with a filter and no sort', () => {
    expect(queryKeys.people(ORG_ID, filter)).toEqual(['org', ORG_ID, 'people', filter])
  })

  it('forces an empty filter slot and appends the sort token with a sort but no filter', () => {
    expect(queryKeys.people(ORG_ID, undefined, 'name.asc')).toEqual(['org', ORG_ID, 'people', '', 'name.asc'])
  })

  it('carries both the filter and the sort token when both are present', () => {
    expect(queryKeys.people(ORG_ID, filter, 'name.asc')).toEqual(['org', ORG_ID, 'people', filter, 'name.asc'])
  })
})
