// SLICE_006c §5/§10: `useCorrectCallOutcome` posts exactly `{"outcome"}` to
// `/calls/{id}/outcome` and, on success, invalidates the Person and Today
// queries (never the call key — the call row does not change, §6).
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import { defineComponent, effectScope, ref } from 'vue'
import { flushPromises, mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { ApiError, apiFetch } from './client'
import {
  queryKeys,
  fetchMe,
  prefetchTodayData,
  useAddPersonTagMutation,
  useAssignPersonMutation,
  useChangeStageMutation,
  useCorrectCallOutcome,
  useCreateSavedListMutation,
  useCreateTagMutation,
  useDeleteSavedListMutation,
  useDeleteTagMutation,
  usePerson,
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
  MembersResponse,
  MutatePersonResponse,
  PeopleResponse,
  PersonDetailResponse,
  PersonSummary,
  PersonTagMutationResponse,
  SavedListDetailResponse,
  SavedListMetadata,
  StagesResponse,
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

// ---- SLICE_014 §3 optimistic-mutation fixtures -----------------------------
const STAGE_LEAD = { id: 'stage-lead', name: 'Lead' }
const STAGE_HOT = { id: 'stage-hot', name: 'Hot Prospect' }
const MEMBER_ALICE = { id: 'user-alice', display_name: 'Alice' }
const MEMBER_BOB = { id: 'user-bob', display_name: 'Bob' }

function personSummary(overrides: Partial<PersonSummary> = {}): PersonSummary {
  return {
    id: PERSON_ID,
    first_name: 'Jamie',
    last_name: 'Doe',
    display_name: 'Jamie Doe',
    stage: STAGE_LEAD,
    assigned_user: null,
    primary_email: 'jamie@example.test',
    primary_phone: null,
    inquiry_count: 1,
    last_inquiry_at: '2026-09-01T00:00:00.000Z',
    created_at: '2026-08-01T00:00:00.000Z',
    ...overrides,
  }
}
function personDetail(overrides: Partial<PersonSummary> = {}): PersonDetailResponse {
  return {
    person: personSummary(overrides),
    contact_methods: [],
    inquiries: [],
    history: [],
    tags: [],
  }
}
function peopleResponse(people: PersonSummary[]): PeopleResponse {
  return { people, truncated: false }
}
function stagesResponse(): StagesResponse {
  return { stages: [{ ...STAGE_LEAD, position: 1 }, { ...STAGE_HOT, position: 2 }] }
}
function membersResponse(): MembersResponse {
  return {
    members: [
      { user_id: MEMBER_ALICE.id, display_name: MEMBER_ALICE.display_name, email: 'alice@example.test', role: 'member', status: 'active', joined_at: '2026-01-01T00:00:00Z', assigned_people_count: 0 },
      { user_id: MEMBER_BOB.id, display_name: MEMBER_BOB.display_name, email: 'bob@example.test', role: 'member', status: 'active', joined_at: '2026-01-01T00:00:00Z', assigned_people_count: 0 },
    ],
  }
}
function mutatePersonResponse(person: PersonSummary, changed = true): MutatePersonResponse {
  return { person, changed }
}

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

  // Person-tag routes carry no 403 case (any member may apply/remove), so
  // unlike the tag-level mutations above, only 404 triggers a refetch here
  // — and it refetches BOTH the tags index and the Person detail: the
  // vanished tag can still be sitting in this tab's cached Person `tags`
  // array, and only invalidating the Person key clears that stale chip
  // (reviewer F1 / tester F1).
  async function expectPersonTagRefetchOnly404(
    build: (queryClient: QueryClient) => { mutateAsync: (variables: never) => Promise<unknown> },
  ) {
    const variables = { personId: PERSON_ID, tagId: TAG_ID }

    apiFetchMock.mockReset()
    apiFetchMock.mockRejectedValueOnce(new ApiError(404, 'not_found'))
    let queryClient = new QueryClient({ defaultOptions: { mutations: { retry: false } } })
    let invalidate = vi.spyOn(queryClient, 'invalidateQueries')
    let scope = effectScope()
    let mutation = scope.run(() => build(queryClient))!
    await expect(mutation.mutateAsync(variables as never)).rejects.toThrow()
    const keys = invalidate.mock.calls.map(([filters]) => (typeof filters === 'function' ? filters() : filters)?.queryKey)
    expect(keys).toEqual([queryKeys.tags(ORG_ID), queryKeys.person(ORG_ID, PERSON_ID)])
    scope.stop()

    for (const [status, code] of [[403, 'forbidden'], [409, 'conflict']] as const) {
      apiFetchMock.mockReset()
      apiFetchMock.mockRejectedValueOnce(new ApiError(status, code))
      queryClient = new QueryClient({ defaultOptions: { mutations: { retry: false } } })
      invalidate = vi.spyOn(queryClient, 'invalidateQueries')
      scope = effectScope()
      mutation = scope.run(() => build(queryClient))!
      await expect(mutation.mutateAsync(variables as never)).rejects.toThrow()
      expect(invalidate).not.toHaveBeenCalled()
      scope.stop()
    }
  }

  it('useAddPersonTagMutation refetches tags + the Person key on 404 only', async () => {
    await expectPersonTagRefetchOnly404((qc) => useAddPersonTagMutation(ORG_ID, qc))
  })

  it('useRemovePersonTagMutation refetches tags + the Person key on 404 only', async () => {
    await expectPersonTagRefetchOnly404((qc) => useRemovePersonTagMutation(ORG_ID, qc))
  })

  // SLICE_014 §3, §8.4: apply/remove predict their result from the tags
  // cache and roll back losslessly; the 404/409 refetch tests above already
  // cover the rollback's error-path invalidation, so these only add the
  // optimistic write and its rollback.
  it('useAddPersonTagMutation shows the tag in the detail before the response resolves, in lower-cased-name order, and rolls back on 409', async () => {
    const existing = { id: 'tag-existing', name: 'zzz-existing' }
    const newTag = { id: 'tag-1', name: 'Investor' }
    const queryClient = new QueryClient({ defaultOptions: { mutations: { retry: false } } })
    queryClient.setQueryData(queryKeys.tags(ORG_ID), { tags: [{ ...newTag, person_count: 0, can_manage: true }] })
    queryClient.setQueryData(queryKeys.person(ORG_ID, PERSON_ID), { ...personDetail(), tags: [existing] })
    const pending = deferred<PersonTagMutationResponse>()
    apiFetchMock.mockReturnValueOnce(pending.promise)
    const scope = effectScope()
    const mutation = scope.run(() => useAddPersonTagMutation(ORG_ID, queryClient))!
    const call = mutation.mutateAsync({ personId: PERSON_ID, tagId: newTag.id })
    await flushPromises()

    const duringMutation = queryClient.getQueryData<PersonDetailResponse>(queryKeys.person(ORG_ID, PERSON_ID))
    // Lower-cased name order: "Investor" sorts before "zzz-existing".
    expect(duringMutation?.tags).toEqual([newTag, existing])

    pending.reject(new ApiError(409, 'person_tag_limit_reached'))
    await expect(call).rejects.toThrow()
    const afterRollback = queryClient.getQueryData<PersonDetailResponse>(queryKeys.person(ORG_ID, PERSON_ID))
    expect(afterRollback?.tags).toEqual([existing])
    scope.stop()
  })

  it('useRemovePersonTagMutation removes the tag from the detail before the response resolves, and rolls back on 404', async () => {
    const tag = { id: 'tag-1', name: 'Investor' }
    const queryClient = new QueryClient({ defaultOptions: { mutations: { retry: false } } })
    queryClient.setQueryData(queryKeys.person(ORG_ID, PERSON_ID), { ...personDetail(), tags: [tag] })
    const pending = deferred<PersonTagMutationResponse>()
    apiFetchMock.mockReturnValueOnce(pending.promise)
    const scope = effectScope()
    const mutation = scope.run(() => useRemovePersonTagMutation(ORG_ID, queryClient))!
    const call = mutation.mutateAsync({ personId: PERSON_ID, tagId: tag.id })
    await flushPromises()

    expect(queryClient.getQueryData<PersonDetailResponse>(queryKeys.person(ORG_ID, PERSON_ID))?.tags).toEqual([])

    pending.reject(new ApiError(404, 'not_found'))
    await expect(call).rejects.toThrow()
    expect(queryClient.getQueryData<PersonDetailResponse>(queryKeys.person(ORG_ID, PERSON_ID))?.tags).toEqual([tag])
    scope.stop()
  })
})

// SLICE_014 §3, §8.3, §8.5: stage and assignment predict their result from
// the stages/members cache, write it into the Person detail AND the
// matching row of every cached People variant (never adding/removing a
// row — rule 2), roll back every snapshot on failure, write the server's
// authoritative value on success, and invalidate the whole org branch
// exactly once on settle regardless of outcome.
describe('optimistic stage and assignment mutations (SLICE_014 §3)', () => {
  const OTHER_PERSON_ID = 'person-2'
  const FILTERED_KEY = queryKeys.people(ORG_ID, JSON.stringify({ version: 1, clauses: [{ kind: 'stage', stage_ids: [STAGE_LEAD.id] }] }))

  function seedPersonAndPeopleCaches(qc: QueryClient) {
    qc.setQueryData(queryKeys.stages(ORG_ID), stagesResponse())
    qc.setQueryData(queryKeys.members(ORG_ID), membersResponse())
    qc.setQueryData(queryKeys.person(ORG_ID, PERSON_ID), personDetail({ stage: STAGE_LEAD, assigned_user: null }))
    const row = personSummary({ stage: STAGE_LEAD, assigned_user: null })
    const otherRow = personSummary({ id: OTHER_PERSON_ID, stage: STAGE_HOT, assigned_user: null })
    qc.setQueryData(queryKeys.people(ORG_ID), peopleResponse([row, otherRow]))
    // A filtered variant containing only the row this test mutates — its
    // filter (stage = Lead) will stop matching once the row's stage
    // changes, but rule 2 says it stays until the settled refetch removes
    // it, never disappearing from an optimistic write alone.
    qc.setQueryData(FILTERED_KEY, peopleResponse([row]))
  }

  it('writes the predicted stage into the detail and into a filtered and unfiltered People row, changing only the matching row and leaving the now-non-matching filtered row in place', async () => {
    const queryClient = new QueryClient({ defaultOptions: { mutations: { retry: false } } })
    seedPersonAndPeopleCaches(queryClient)
    const pending = deferred<MutatePersonResponse>()
    apiFetchMock.mockReturnValueOnce(pending.promise)
    const scope = effectScope()
    const mutation = scope.run(() => useChangeStageMutation(ORG_ID, queryClient))!
    const call = mutation.mutateAsync({ personId: PERSON_ID, stageId: STAGE_HOT.id })
    await flushPromises()

    const detail = queryClient.getQueryData<PersonDetailResponse>(queryKeys.person(ORG_ID, PERSON_ID))
    expect(detail?.person.stage).toEqual(STAGE_HOT)

    const unfiltered = queryClient.getQueryData<PeopleResponse>(queryKeys.people(ORG_ID))
    expect(unfiltered?.people.find((p) => p.id === PERSON_ID)?.stage).toEqual(STAGE_HOT)
    // The other row (never targeted) is untouched.
    expect(unfiltered?.people.find((p) => p.id === OTHER_PERSON_ID)?.stage).toEqual(STAGE_HOT)

    // Rule 2: the filtered variant still contains the row (membership never
    // moves optimistically), even though its new stage no longer matches
    // that filter's own criteria.
    const filtered = queryClient.getQueryData<PeopleResponse>(FILTERED_KEY)
    expect(filtered?.people.map((p) => p.id)).toEqual([PERSON_ID])
    expect(filtered?.people[0]?.stage).toEqual(STAGE_HOT)

    pending.resolve(mutatePersonResponse(personSummary({ stage: STAGE_HOT })))
    await call
    scope.stop()
  })

  it('restores every snapshot on a rejected response, so the existing error text renders the true state', async () => {
    const queryClient = new QueryClient({ defaultOptions: { mutations: { retry: false } } })
    seedPersonAndPeopleCaches(queryClient)
    const pending = deferred<MutatePersonResponse>()
    apiFetchMock.mockReturnValueOnce(pending.promise)
    const scope = effectScope()
    const mutation = scope.run(() => useChangeStageMutation(ORG_ID, queryClient))!
    const call = mutation.mutateAsync({ personId: PERSON_ID, stageId: STAGE_HOT.id })
    await flushPromises()
    expect(queryClient.getQueryData<PersonDetailResponse>(queryKeys.person(ORG_ID, PERSON_ID))?.person.stage).toEqual(STAGE_HOT)

    pending.reject(new ApiError(409, 'conflict'))
    await expect(call).rejects.toThrow()

    expect(queryClient.getQueryData<PersonDetailResponse>(queryKeys.person(ORG_ID, PERSON_ID))?.person.stage).toEqual(STAGE_LEAD)
    expect(queryClient.getQueryData<PeopleResponse>(queryKeys.people(ORG_ID))?.people.find((p) => p.id === PERSON_ID)?.stage).toEqual(STAGE_LEAD)
    expect(queryClient.getQueryData<PeopleResponse>(FILTERED_KEY)?.people[0]?.stage).toEqual(STAGE_LEAD)
    scope.stop()
  })

  it('writes the server person on success and invalidates the org branch exactly once on settle', async () => {
    const queryClient = new QueryClient({ defaultOptions: { mutations: { retry: false } } })
    seedPersonAndPeopleCaches(queryClient)
    const invalidate = vi.spyOn(queryClient, 'invalidateQueries')
    apiFetchMock.mockResolvedValueOnce(mutatePersonResponse(personSummary({ stage: STAGE_HOT })))
    const scope = effectScope()
    const mutation = scope.run(() => useChangeStageMutation(ORG_ID, queryClient))!
    await mutation.mutateAsync({ personId: PERSON_ID, stageId: STAGE_HOT.id })

    expect(queryClient.getQueryData<PersonDetailResponse>(queryKeys.person(ORG_ID, PERSON_ID))?.person.stage).toEqual(STAGE_HOT)
    expect(invalidate).toHaveBeenCalledTimes(1)
    expect(invalidate.mock.calls[0]?.[0]).toMatchObject({ queryKey: queryKeys.org(ORG_ID) })
    scope.stop()
  })

  it('also invalidates exactly once on settle after a rejected response (an uncertain outcome may still have committed)', async () => {
    const queryClient = new QueryClient({ defaultOptions: { mutations: { retry: false } } })
    seedPersonAndPeopleCaches(queryClient)
    const invalidate = vi.spyOn(queryClient, 'invalidateQueries')
    apiFetchMock.mockRejectedValueOnce(new ApiError(409, 'conflict'))
    const scope = effectScope()
    const mutation = scope.run(() => useChangeStageMutation(ORG_ID, queryClient))!
    await expect(mutation.mutateAsync({ personId: PERSON_ID, stageId: STAGE_HOT.id })).rejects.toThrow()

    expect(invalidate).toHaveBeenCalledTimes(1)
    expect(invalidate.mock.calls[0]?.[0]).toMatchObject({ queryKey: queryKeys.org(ORG_ID) })
    scope.stop()
  })

  it('skips the optimistic write with an empty stages cache, but the mutation still succeeds and invalidates', async () => {
    const queryClient = new QueryClient({ defaultOptions: { mutations: { retry: false } } })
    // No stages cache seeded at all (rule 3's fallback).
    queryClient.setQueryData(queryKeys.person(ORG_ID, PERSON_ID), personDetail({ stage: STAGE_LEAD }))
    const invalidate = vi.spyOn(queryClient, 'invalidateQueries')
    const pending = deferred<MutatePersonResponse>()
    apiFetchMock.mockReturnValueOnce(pending.promise)
    const scope = effectScope()
    const mutation = scope.run(() => useChangeStageMutation(ORG_ID, queryClient))!
    const pendingWrite = mutation.mutateAsync({ personId: PERSON_ID, stageId: STAGE_HOT.id })
    await flushPromises()
    // Still the pre-mutation stage: nothing was invented from an empty cache.
    expect(queryClient.getQueryData<PersonDetailResponse>(queryKeys.person(ORG_ID, PERSON_ID))?.person.stage).toEqual(STAGE_LEAD)

    pending.resolve(mutatePersonResponse(personSummary({ stage: STAGE_HOT })))
    await pendingWrite
    expect(queryClient.getQueryData<PersonDetailResponse>(queryKeys.person(ORG_ID, PERSON_ID))?.person.stage).toEqual(STAGE_HOT)
    expect(invalidate).toHaveBeenCalledTimes(1)
    scope.stop()
  })

  it('skips the optimistic assignment write with an empty members cache, but Unassigned (null) is always predictable', async () => {
    const queryClient = new QueryClient({ defaultOptions: { mutations: { retry: false } } })
    // No members cache seeded.
    queryClient.setQueryData(queryKeys.person(ORG_ID, PERSON_ID), personDetail({ assigned_user: MEMBER_ALICE }))
    const scope = effectScope()
    const mutation = scope.run(() => useAssignPersonMutation(ORG_ID, queryClient))!

    // A specific user: no members cache to resolve the name from, so no
    // optimistic write.
    const named = deferred<MutatePersonResponse>()
    apiFetchMock.mockReturnValueOnce(named.promise)
    const namedCall = mutation.mutateAsync({ personId: PERSON_ID, assignedUserId: MEMBER_BOB.id })
    await flushPromises()
    expect(queryClient.getQueryData<PersonDetailResponse>(queryKeys.person(ORG_ID, PERSON_ID))?.person.assigned_user).toEqual(MEMBER_ALICE)
    named.resolve(mutatePersonResponse(personSummary({ assigned_user: MEMBER_BOB })))
    await namedCall

    // Unassigned: always predictable, no cache dependency.
    const unassign = deferred<MutatePersonResponse>()
    apiFetchMock.mockReturnValueOnce(unassign.promise)
    const unassignCall = mutation.mutateAsync({ personId: PERSON_ID, assignedUserId: null })
    await flushPromises()
    expect(queryClient.getQueryData<PersonDetailResponse>(queryKeys.person(ORG_ID, PERSON_ID))?.person.assigned_user).toBeNull()
    unassign.resolve(mutatePersonResponse(personSummary({ assigned_user: null })))
    await unassignCall
    scope.stop()
  })

  // §8.5: a person.changed invalidation (a realtime refetch landing with
  // stale pre-commit data) racing a pending mutation must never leave the
  // old value showing once the mutation itself has resolved — the
  // mutation's own onSuccess write is the final word for its own request.
  it('never shows the old stage after the mutation resolves, even if a racing invalidation lands stale data mid-flight', async () => {
    const queryClient = new QueryClient({ defaultOptions: { mutations: { retry: false } } })
    seedPersonAndPeopleCaches(queryClient)
    const pending = deferred<MutatePersonResponse>()
    apiFetchMock.mockReturnValueOnce(pending.promise)
    const scope = effectScope()
    const mutation = scope.run(() => useChangeStageMutation(ORG_ID, queryClient))!
    const call = mutation.mutateAsync({ personId: PERSON_ID, stageId: STAGE_HOT.id })
    await flushPromises()
    expect(queryClient.getQueryData<PersonDetailResponse>(queryKeys.person(ORG_ID, PERSON_ID))?.person.stage).toEqual(STAGE_HOT)

    // A racing person.changed-triggered refetch lands with the PRE-commit
    // stage (it was in flight before the server committed this mutation).
    queryClient.setQueryData(queryKeys.person(ORG_ID, PERSON_ID), personDetail({ stage: STAGE_LEAD }))
    expect(queryClient.getQueryData<PersonDetailResponse>(queryKeys.person(ORG_ID, PERSON_ID))?.person.stage).toEqual(STAGE_LEAD)

    pending.resolve(mutatePersonResponse(personSummary({ stage: STAGE_HOT })))
    await call

    expect(queryClient.getQueryData<PersonDetailResponse>(queryKeys.person(ORG_ID, PERSON_ID))?.person.stage).toEqual(STAGE_HOT)
    scope.stop()
  })
})

describe('usePerson cancellation (SLICE_014 §3)', () => {
  it('aborts the mocked request when the query is cancelled', async () => {
    let capturedSignal: AbortSignal | null | undefined
    apiFetchMock.mockImplementation((_path: string, init?: RequestInit) => {
      capturedSignal = init?.signal
      return new Promise(() => {}) // never resolves
    })
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    const Harness = defineComponent({
      setup() {
        usePerson(ORG_ID, PERSON_ID)
        return () => null
      },
    })
    const wrapper = mount(Harness, {
      global: { plugins: [[VueQueryPlugin, { queryClient }]] },
    })
    await flushPromises()
    expect(capturedSignal).toBeDefined()
    expect(capturedSignal?.aborted).toBe(false)

    await queryClient.cancelQueries({ queryKey: queryKeys.person(ORG_ID, PERSON_ID) })
    expect(capturedSignal?.aborted).toBe(true)
    wrapper.unmount()
  })
})

describe('prefetchTodayData (SLICE_014 §4)', () => {
  it('prefetches Today, Today sources and Today feeds for the session\'s Organization and actor, using factory keys', async () => {
    apiFetchMock.mockImplementation((path: string) => {
      if (path === '/today') return Promise.resolve({ items: [] })
      if (path === '/today/sources') return Promise.resolve({ sources: [] })
      if (path === '/today/feeds') return Promise.resolve({ feeds: {} })
      return Promise.reject(new Error(`unexpected path ${path}`))
    })
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    const session = savedMe(ORG_ID, 'actor-a')
    prefetchTodayData(queryClient, session)
    await flushPromises()

    expect(apiFetchMock).toHaveBeenCalledWith('/today', expect.anything())
    expect(apiFetchMock).toHaveBeenCalledWith('/today/sources', expect.anything())
    expect(apiFetchMock).toHaveBeenCalledWith('/today/feeds', expect.anything())
    expect(queryClient.getQueryData(queryKeys.todayForActor(ORG_ID, 'actor-a'))).toEqual({ items: [] })
    expect(queryClient.getQueryData(queryKeys.todaySources(ORG_ID, 'actor-a'))).toEqual({ sources: [] })
    expect(queryClient.getQueryData(queryKeys.todayFeeds(ORG_ID, 'actor-a'))).toEqual({ feeds: {} })
  })

  it('prefetches nothing for a platform-only session (no Organization)', async () => {
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    const platformOnly: MeResponse = { user: { id: 'platform-1', email: 'p@example.test', display_name: 'Platform' }, organization: null, platform_admin: true }
    prefetchTodayData(queryClient, platformOnly)
    await Promise.resolve()
    expect(apiFetchMock).not.toHaveBeenCalled()
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
