import { useMutation, useQuery, useQueryClient, type QueryClient } from '@tanstack/vue-query'
import { type MaybeRefOrGetter, computed, toValue, watch } from 'vue'
import { queryClient } from '../query-client'
import { ApiError, apiFetch } from './client'
import {
  SessionVerificationPendingError,
  beginSessionTransition,
  beginLogoutSessionTransition,
  completeSessionVerification,
  configureSessionLifecycle,
  currentSessionGeneration,
  isCurrentSessionGeneration,
  settleSessionTransition,
  useAuthSessionLifetime as useSessionLifecycleAuthLifetime,
  useSessionVerificationPending,
  waitForSessionVerification,
} from '../sessionLifecycle'
import type {
  AcceptInvitationRequest,
  AcceptInvitationResponse,
  ActorRef,
  AssignmentRequest,
  CallOutcomeCorrection,
  CallResponse,
  CaptureAddressResponse,
  CaptureUnmatchedResponse,
  ChangeMemberRoleRequest,
  ContactChannel,
  ContactOutcome,
  CreateSavedListRequest,
  CreateSavedListResponse,
  DeleteSavedListRequest,
  DeleteSavedListResponse,
  DiscardUnresolvedResponse,
  RetryUnresolvedResponse,
  SavedListCountResponse,
  SavedListDetailResponse,
  SavedListsResponse,
  UnresolvedDetailResponse,
  CreateOrganizationRequest,
  CorrectOutcomeRequest,
  CorrectOutcomeResponse,
  CreateOrganizationResponse,
  InquirySourcesResponse,
  InvitationPreviewRequest,
  InvitationPreviewResponse,
  InvitationsResponse,
  IssueInvitationRequest,
  IssueInvitationResponse,
  LinkUnmatchedRequest,
  LogContactRequest,
  LogContactResponse,
  MeResponse,
  MemberMutationResponse,
  MembershipRole,
  MembershipStatus,
  MembersResponse,
  MutatePersonResponse,
  OperatorTurnRequest,
  OperatorTurnResponse,
  PeopleResponse,
  PersonDetailResponse,
  PlatformChangeMemberRoleRequest,
  PlatformIssueInvitationRequest,
  PersonSummary,
  PlatformOrganizationDetailResponse,
  PlatformOrganizationsResponse,
  RealtimeTokenResponse,
  ReceiveInquiryRequest,
  ReceiveInquiryResponse,
  SetMemberStatusRequest,
  StageRef,
  StageRequest,
  StagesResponse,
  StartCallRequest,
  IntakeAddressResponse,
  IntakeSettingsRequest,
  IntakeSettingsResponse,
  StartCallResponse,
  CreateTagRequest,
  CreateTagResponse,
  DeleteTagResponse,
  PersonTagMutationResponse,
  RenameTagRequest,
  RenameTagResponse,
  TagRef,
  TagsResponse,
  TodayResponse,
  TodaySourcesResponse,
  EnableTodaySourceRequest,
  TodaySourceChange,
  MemberTodayFeedsResponse,
  TodayFeedsResponse,
  TodayFeedKey,
  UpdateTodayFeedRequest,
  RevertTodayFeedRequest,
  SetTodayFeedEnabledRequest,
  TodayFeedMutationResponse,
  PreviewTodayFeedRequest,
  PreviewTodayFeedResponse,
  UnresolvedResponse,
  UpdateSavedListRequest,
  UpdateSavedListResponse,
} from './types'

// Key factory (docs/specs/SLICE_002.md §10): every Organization-scoped
// resource is namespaced under ['org', orgId, ...] so a mutation can
// invalidate the whole branch at once, and so a session change (different
// user, different Organization, same tab) can never surface a stale
// cross-org cache entry under a key that looks unrelated to org.
export const queryKeys = {
  me: ['me'] as const,
  org: (orgId: string) => ['org', orgId] as const,
  // SLICE_011a §6: the filter element is OMITTED ENTIRELY when unfiltered
  // — never appended as `undefined` (TanStack would hash that to `null`,
  // orphaning the existing unfiltered cache entry and its tests, review
  // F9). Prefix matching then keeps every existing invalidation (realtime
  // `person.changed`, mutation-driven `['org', orgId]` sweeps — both call
  // `queryKeys.people(orgId)` with no filter argument) covering filtered
  // queries with zero changes to realtime/events.ts.
  // SLICE_011b_SORT.md §9: extended in the factory only. Without a sort the
  // key stays byte-identical to today (3 or 4 elements). With a normalized
  // sort token, the key is always 5 elements — `['org', orgId, 'people',
  // filter ?? '', sortToken]` — so the filter slot is forced to `''` rather
  // than omitted; `usePeople`'s queryFn already treats `''` as "no filter".
  people: (orgId: string, serializedFilter?: string, sortToken?: string) =>
    sortToken
      ? (['org', orgId, 'people', serializedFilter ?? '', sortToken] as const)
      : serializedFilter
        ? (['org', orgId, 'people', serializedFilter] as const)
        : (['org', orgId, 'people'] as const),
  person: (orgId: string, personId: string) => ['org', orgId, 'person', personId] as const,
  stages: (orgId: string) => ['org', orgId, 'stages'] as const,
  unresolved: (orgId: string) => ['org', orgId, 'unresolved'] as const,
  members: (orgId: string) => ['org', orgId, 'members'] as const,
  intakeAddress: (orgId: string) => ['org', orgId, 'intake-address'] as const,
  // SLICE_007c §10: extend the factory, never hand-write a key.
  intakeSettings: (orgId: string) => ['org', orgId, 'intake-settings'] as const,
  // Added ahead of the Today view (SLICE_003 §10 lists it alongside useToday)
  // because realtime/events.ts's invalidationsFor (Lane B step 1) already
  // needs to name this key — every key an invalidation path touches goes
  // through this factory, never hand-written (SLICE_003 Lane B task brief).
  today: (orgId: string) => ['org', orgId, 'today'] as const,
  todayForActor: (orgId: string, actorId: string) => ['org', orgId, 'today', actorId] as const,
  todaySources: (orgId: string, actorId: string) => ['org', orgId, 'today-sources', actorId] as const,
  // SLICE_011d §6: `todayFeeds(org, actor)` "under the existing Today
  // prefix" — nested under `today(orgId)` so the whole-Today invalidation
  // sweep (`queryKeys.today(org)`, used throughout this file) already
  // covers it without a separate call. The admin surface's key is
  // deliberately NOT actor-scoped (any current admin sees the same rows);
  // spec-literal key text: `['org', org, 'today-feeds-admin']`.
  todayFeeds: (orgId: string, actorId: string) => [...queryKeys.today(orgId), 'feeds', actorId] as const,
  todayFeedsAdmin: (orgId: string) => ['org', orgId, 'today-feeds-admin'] as const,
  // SLICE_004 §10: extend the factory, never hand-write a key.
  invitations: (orgId: string) => ['org', orgId, 'invitations'] as const,
  // SLICE_006 §6: `call.changed` → ['org', orgId, 'call', callId] (and the
  // person key). Under the org branch so reconnect recovery and the
  // mutations' whole-branch invalidation cover it too.
  call: (orgId: string, callId: string) => ['org', orgId, 'call', callId] as const,
  // SLICE_009 §10: extend the factory, never hand-write a key.
  captureAddress: (orgId: string) => ['org', orgId, 'capture-address'] as const,
  captureUnmatched: (orgId: string) => ['org', orgId, 'capture-unmatched'] as const,
  // Keyed by the raw token, not an org id — the public accept page has no
  // Organization context yet (that is exactly what the preview reveals).
  invitationPreview: (token: string) => ['invitation-preview', token] as const,
  // SLICE_011a §10: extend the factory, never hand-write a key.
  inquirySources: (orgId: string) => ['org', orgId, 'inquiry-sources'] as const,
  // SLICE_011b §7: saved-list cache keys include the actor wherever the
  // response can differ by ownership/capability. Count prefixes deliberately
  // sit under a separate org branch so `person.changed` can invalidate every
  // actor's visible-count cache without naming list IDs on realtime.
  savedLists: (orgId: string, actorId: string) => ['org', orgId, 'saved-lists', actorId] as const,
  savedList: (orgId: string, actorId: string, listId: string) =>
    ['org', orgId, 'saved-lists', actorId, 'detail', listId] as const,
  savedListCounts: (orgId: string) => ['org', orgId, 'saved-list-counts'] as const,
  savedListCountsForActor: (orgId: string, actorId: string) =>
    ['org', orgId, 'saved-list-counts', actorId] as const,
  savedListCountsForList: (orgId: string, actorId: string, listId: string) =>
    ['org', orgId, 'saved-list-counts', actorId, listId] as const,
  savedListCount: (orgId: string, actorId: string, listId: string, revision: number) =>
    ['org', orgId, 'saved-list-counts', actorId, listId, revision] as const,
  platformOrganizations: () => ['platform', 'organizations'] as const,
  platformOrganization: (id: string) => ['platform', 'organizations', id] as const,
  // Slice 011e §10: extend the factory, never hand-write a key. Not
  // actor-scoped: `can_manage` is a per-viewer field on each row, not a
  // separate cache — every member reads the same underlying index and gets
  // their own `can_manage` values back from the same response.
  tags: (orgId: string) => ['org', orgId, 'tags'] as const,
}

// `/me` has no public session-id field. The coordinator adds an opaque
// cross-tab generation for cookie replacement, so an A -> logout -> A login
// (including one initiated in another tab) cannot retain private state.
export function useAuthSessionLifetime() {
  return useSessionLifecycleAuthLifetime()
}

async function fetchMeForGeneration(generation: number, signal?: AbortSignal): Promise<MeResponse> {
  const data = await apiFetch<MeResponse>('/me', { signal })
  if (!isCurrentSessionGeneration(generation)) throw new SessionVerificationPendingError()
  return data
}

export async function fetchMe(signal?: AbortSignal): Promise<MeResponse> {
  const verified = await waitForSessionVerification()
  if (verified?.generation === currentSessionGeneration()) {
    if (verified.error !== undefined) throw verified.error
    if (verified.identity !== undefined) return verified.identity as MeResponse
  }
  return fetchMeForGeneration(currentSessionGeneration(), signal)
}

configureSessionLifecycle({
  discardPrivateState: () => {
    void queryClient.cancelQueries()
    queryClient.clear()
  },
  verify: (generation) => {
    // Do not use `fetchQuery(['me'])` here. A router guard may already own
    // that cached query and be waiting for this verification, which would
    // make TanStack dedupe the verifier onto its own waiter. This direct,
    // generation-fenced transport is the one authoritative recovery read;
    // `completeSessionVerification` installs its value for all observers.
    void fetchMeForGeneration(generation).then(
      (identity) => { completeSessionVerification(generation, identity) },
      (error) => { completeSessionVerification(generation, undefined, error) },
    )
  },
  installVerifiedIdentity: (identity, generation) => {
    if (!isCurrentSessionGeneration(generation)) return
    queryClient.setQueryData(queryKeys.me, identity as MeResponse)
  },
})

/**
 * Also the router's auth gate: a 401 ApiError means "go to /login" (the
 * QueryClient's default retry policy already skips retrying a 401 — see
 * query-client.ts).
 */
export function useMe() {
  const sessionVerificationPending = useSessionVerificationPending()
  return useQuery({
    queryKey: queryKeys.me,
    queryFn: ({ signal }) => fetchMe(signal),
    enabled: computed(() => !sessionVerificationPending.value),
  })
}

/**
 * `GET /api/people`, `GET /api/people?filter=<...>` and `&sort=<...>`
 * (docs/specs/SLICE_011a.md §5a, §6; docs/specs/SLICE_011b_SORT.md §6, §9).
 * `serializedFilter` is the SAME percent-encodable JSON string used for both
 * the query-key element and the URL param (`lib/filter.ts`'s
 * `serializeFilter`) — pass `undefined`/`''` for the unfiltered legacy path.
 * `sortToken` is the normalized wire token (`lib/sort.ts`'s `serializeSort`
 * after `normalizeSort`) — pass `undefined` for the default order, whose key
 * and request stay byte-identical to before this slice.
 */
export function usePeople(
  orgId: MaybeRefOrGetter<string>,
  serializedFilter?: MaybeRefOrGetter<string | undefined>,
  enabled?: MaybeRefOrGetter<boolean>,
  forceFresh?: MaybeRefOrGetter<boolean>,
  sortToken?: MaybeRefOrGetter<string | undefined>,
) {
  return useQuery({
    queryKey: computed(() => queryKeys.people(
      toValue(orgId), toValue(serializedFilter) || undefined, toValue(sortToken) || undefined,
    )),
    queryFn: ({ queryKey, signal }) => {
      // A retry belongs to the key that started it, even if the user has
      // since selected another filter/sort.
      const filter = queryKey[3]
      const sort = queryKey[4]
      const params: string[] = []
      if (filter) params.push(`filter=${encodeURIComponent(filter)}`)
      if (sort !== undefined) params.push(`sort=${encodeURIComponent(sort)}`)
      const path = params.length > 0 ? `/people?${params.join('&')}` : '/people'
      return apiFetch<PeopleResponse>(path, { signal })
    },
    // Keep the table steady between filters, but never retain another
    // Organization's rows. A saved-list detail that is missing, hidden, or
    // not yet evaluable explicitly disables this query and must not show a
    // previous list's cached People underneath its recovery state.
    placeholderData: (previousData, previousQuery) =>
      toValue(orgId) !== '' && (enabled === undefined || toValue(enabled)) &&
        previousQuery?.queryKey[1] === toValue(orgId) ? previousData : undefined,
    enabled: computed(() => toValue(orgId) !== '' && (enabled === undefined || toValue(enabled))),
    // Named definitions can contain relative-time clauses. Their workspace
    // calls this with `forceFresh` so an unchanged cached filter still gets a
    // new People result after route entry/focus; ordinary /people preserves
    // its established cache policy.
    refetchOnMount: computed(() => toValue(forceFresh) ? 'always' : true),
    refetchOnWindowFocus: computed(() => toValue(forceFresh) ? 'always' : true),
  })
}

/** `GET /api/saved-lists` — metadata only; criteria stay out of the index. */
export function useSavedLists(
  orgId: MaybeRefOrGetter<string>,
  actorId: MaybeRefOrGetter<string>,
) {
  const qc = useQueryClient()
  const query = useQuery({
    queryKey: computed(() => queryKeys.savedLists(toValue(orgId), toValue(actorId))),
    queryFn: ({ signal }) => apiFetch<SavedListsResponse>('/saved-lists', { signal }),
    enabled: computed(() => toValue(orgId) !== '' && toValue(actorId) !== ''),
    // Saved definitions may change in another browser without a new realtime
    // payload. The index intentionally revalidates on each entry/focus.
    refetchOnMount: 'always',
    refetchOnWindowFocus: 'always',
  })
  function discard(queryKey: readonly unknown[], exact = true) {
    // Abort before evicting. A test transport (and a real intermediary) can
    // resolve after AbortSignal; cancellation keeps that late value from
    // recreating a removed private cache entry.
    // Do not await cancellation: an intermediary may ignore AbortSignal and
    // never settle. The obsolete private result still has to leave the cache
    // immediately; query-core ignores a later cancelled completion.
    void qc.cancelQueries({ queryKey, exact })
    qc.removeQueries({ queryKey, exact })
  }

  watch(() => query.data.value, (data) => {
    if (!data) return
    const identity = currentSavedListIdentity(qc)
    const org = toValue(orgId)
    const actor = toValue(actorId)
    // An observer can receive a late result after its owning view has
    // switched session. Do not resurrect or remove private cache entries
    // under the newly active identity.
    if (!identity || identity.orgId !== org || identity.actorId !== actor) return

    const authoritative = new Map(data.lists.map((list) => [list.id, list]))
    // Compare every cached detail/count row, not only metadata observed in
    // this mount. Returning to Lists after a removal must evict the old
    // workspace cache just as a live index refresh would.
    for (const cached of qc.getQueryCache().findAll({ queryKey: queryKeys.savedLists(org, actor) })) {
      const key = cached.queryKey
      if (key[4] !== 'detail' || typeof key[5] !== 'string') continue
      const next = authoritative.get(key[5])
      const detail = cached.state.data as SavedListDetailResponse | undefined
      if (!next || detail?.list.revision !== next.revision) discard(key)
    }
    for (const cached of qc.getQueryCache().findAll({ queryKey: queryKeys.savedListCountsForActor(org, actor) })) {
      const key = cached.queryKey
      if (typeof key[4] !== 'string' || typeof key[5] !== 'number') continue
      const next = authoritative.get(key[4])
      if (!next || key[5] !== next.revision) discard(key)
    }
  }, { immediate: true })
  return query
}

/** `GET /api/saved-lists/{id}` — identity and actor are both in the cache key. */
export function useSavedList(
  orgId: MaybeRefOrGetter<string>,
  actorId: MaybeRefOrGetter<string>,
  listId: MaybeRefOrGetter<string>,
) {
  return useQuery({
    queryKey: computed(() => queryKeys.savedList(toValue(orgId), toValue(actorId), toValue(listId))),
    queryFn: ({ queryKey, signal }) => apiFetch<SavedListDetailResponse>(
      `/saved-lists/${encodeURIComponent(queryKey[5])}`,
      { signal },
    ),
    enabled: computed(() => toValue(orgId) !== '' && toValue(actorId) !== '' && toValue(listId) !== ''),
    refetchOnMount: 'always',
    refetchOnWindowFocus: 'always',
  })
}

/** A direct count request used only through the bounded count scheduler. */
export function fetchSavedListCount(
  listId: string,
  revision: number,
  signal?: AbortSignal,
): Promise<SavedListCountResponse> {
  return apiFetch<SavedListCountResponse>(
    `/saved-lists/${encodeURIComponent(listId)}/count?revision=${encodeURIComponent(String(revision))}`,
    { signal },
  )
}

/** Single active workspace count. Index rows instead use the four-wide scheduler. */
export function useSavedListCount(
  orgId: MaybeRefOrGetter<string>,
  actorId: MaybeRefOrGetter<string>,
  listId: MaybeRefOrGetter<string>,
  revision: MaybeRefOrGetter<number>,
  enabled: MaybeRefOrGetter<boolean>,
) {
  return useQuery({
    queryKey: computed(() => queryKeys.savedListCount(
      toValue(orgId), toValue(actorId), toValue(listId), toValue(revision),
    )),
    queryFn: ({ queryKey, signal }) => fetchSavedListCount(queryKey[4], queryKey[5], signal),
    enabled: computed(() =>
      toValue(orgId) !== '' && toValue(actorId) !== '' && toValue(listId) !== '' && toValue(enabled),
    ),
    retry: false,
    refetchOnMount: 'always',
    refetchOnWindowFocus: 'always',
  })
}

interface SavedListMutationIdentity {
  orgId: string
  actorId: string
  // Object identity is a small in-memory session generation: A → logout → A
  // gets a fresh `/me` object even though its public IDs are identical.
  // That prevents a late old-session mutation from repopulating a cache
  // cleared at logout.
  session: MeResponse | undefined
}

function currentSavedListIdentity(queryClient: QueryClient): SavedListMutationIdentity | undefined {
  const me = queryClient.getQueryData<MeResponse>(queryKeys.me)
  if (!me?.organization) return undefined
  return { orgId: me.organization.id, actorId: me.user.id, session: me }
}

function hasCurrentSavedListIdentity(queryClient: QueryClient, submitted: SavedListMutationIdentity | undefined) {
  const current = currentSavedListIdentity(queryClient)
  return submitted !== undefined && current?.orgId === submitted.orgId &&
    current.actorId === submitted.actorId && current.session === submitted.session
}

function invalidateSavedListCaches(queryClient: QueryClient, orgId: string, actorId: string, listId?: string) {
  void queryClient.invalidateQueries({ queryKey: queryKeys.savedLists(orgId, actorId) })
  void queryClient.invalidateQueries({ queryKey: queryKeys.savedListCountsForActor(orgId, actorId) })
  if (listId) void queryClient.invalidateQueries({ queryKey: queryKeys.savedList(orgId, actorId, listId) })
  invalidateTodaySourceCaches(queryClient, orgId, actorId)
}

/** Create/Save as/Duplicate share the same explicit no-retry mutation path. */
export function useCreateSavedListMutation(
  orgId: MaybeRefOrGetter<string>,
  actorId: MaybeRefOrGetter<string>,
  providedQueryClient?: QueryClient,
) {
  const qc = providedQueryClient ?? useQueryClient()
  return useMutation({
    mutationFn: (body: CreateSavedListRequest) => apiFetch<CreateSavedListResponse>('/saved-lists', {
      method: 'POST', body: JSON.stringify(body),
    }),
    retry: false,
    // Capture identity at submission. A late response after logout or a
    // same-org actor switch must not repopulate a private cache that was
    // deliberately cleared, nor invalidate the newly active actor's data.
    onMutate: (): SavedListMutationIdentity => currentSavedListIdentity(qc) ?? {
      orgId: toValue(orgId), actorId: toValue(actorId), session: undefined,
    },
    onSuccess: (result, _variables, submitted) => {
      if (!hasCurrentSavedListIdentity(qc, submitted)) return
      invalidateSavedListCaches(qc, submitted.orgId, submitted.actorId, result.list.id)
    },
  }, providedQueryClient)
}

export function useUpdateSavedListMutation(
  orgId: MaybeRefOrGetter<string>,
  actorId: MaybeRefOrGetter<string>,
  providedQueryClient?: QueryClient,
) {
  const qc = providedQueryClient ?? useQueryClient()
  return useMutation({
    mutationFn: ({ listId, body }: { listId: string; body: UpdateSavedListRequest }) =>
      apiFetch<UpdateSavedListResponse>(`/saved-lists/${encodeURIComponent(listId)}`, {
        method: 'PUT', body: JSON.stringify(body),
    }),
    retry: false,
    onMutate: (): SavedListMutationIdentity => currentSavedListIdentity(qc) ?? {
      orgId: toValue(orgId), actorId: toValue(actorId), session: undefined,
    },
    onSuccess: (result, variables, submitted) => {
      if (!hasCurrentSavedListIdentity(qc, submitted)) return
      invalidateSavedListCaches(qc, submitted.orgId, submitted.actorId, variables.listId)
      qc.setQueryData<SavedListDetailResponse>(
        queryKeys.savedList(submitted.orgId, submitted.actorId, variables.listId),
        (previous) => previous
          ? { ...previous, list: result.list, filter: variables.body.filter, sort: variables.body.sort ?? null, filter_error: null }
          : previous,
      )
    },
  }, providedQueryClient)
}

export function useDeleteSavedListMutation(
  orgId: MaybeRefOrGetter<string>,
  actorId: MaybeRefOrGetter<string>,
  providedQueryClient?: QueryClient,
) {
  const qc = providedQueryClient ?? useQueryClient()
  return useMutation({
    mutationFn: ({ listId, body }: { listId: string; body: DeleteSavedListRequest }) =>
      apiFetch<DeleteSavedListResponse>(`/saved-lists/${encodeURIComponent(listId)}`, {
        method: 'DELETE', body: JSON.stringify(body),
    }),
    retry: false,
    onMutate: (): SavedListMutationIdentity => currentSavedListIdentity(qc) ?? {
      orgId: toValue(orgId), actorId: toValue(actorId), session: undefined,
    },
    onSuccess: (_result, variables, submitted) => {
      if (!hasCurrentSavedListIdentity(qc, submitted)) return
      invalidateSavedListCaches(qc, submitted.orgId, submitted.actorId, variables.listId)
      qc.removeQueries({ queryKey: queryKeys.savedList(submitted.orgId, submitted.actorId, variables.listId) })
    },
  }, providedQueryClient)
}

/** `GET /api/inquiry-sources` (docs/specs/SLICE_011a.md §5b) — feeds the
 *  FilterBar's Source picker. Any authenticated member. */
export function useInquirySources(orgId: MaybeRefOrGetter<string>) {
  return useQuery({
    queryKey: computed(() => queryKeys.inquirySources(toValue(orgId))),
    queryFn: () => apiFetch<InquirySourcesResponse>('/inquiry-sources'),
    enabled: computed(() => toValue(orgId) !== ''),
  })
}

/** `GET /api/people/{id}` — shared by `usePerson`'s queryFn and
 * PeopleView's hover/focus prefetch (SLICE_014 §4), so both forward the
 * same abort signal and hit the exact same request shape rather than one
 * of the two silently drifting. */
export function fetchPerson(personId: string, signal?: AbortSignal): Promise<PersonDetailResponse> {
  return apiFetch<PersonDetailResponse>(`/people/${personId}`, { signal })
}

export function usePerson(orgId: MaybeRefOrGetter<string>, personId: MaybeRefOrGetter<string>) {
  return useQuery({
    queryKey: computed(() => queryKeys.person(toValue(orgId), toValue(personId))),
    // SLICE_014 §3: the signal is forwarded so a cancelQueries (an
    // optimistic mutation's onMutate, or a route/identity change) actually
    // aborts the in-flight request instead of leaving it to resolve unused.
    queryFn: ({ signal }) => fetchPerson(toValue(personId), signal),
    enabled: computed(() => toValue(orgId) !== '' && toValue(personId) !== ''),
  })
}

export function useStages(orgId: MaybeRefOrGetter<string>) {
  return useQuery({
    queryKey: computed(() => queryKeys.stages(toValue(orgId))),
    queryFn: () => apiFetch<StagesResponse>('/stages'),
    enabled: computed(() => toValue(orgId) !== ''),
  })
}

export function useUnresolved(orgId: MaybeRefOrGetter<string>) {
  return useQuery({
    queryKey: computed(() => queryKeys.unresolved(toValue(orgId))),
    queryFn: () => apiFetch<UnresolvedResponse>('/intake/unresolved'),
    enabled: computed(() => toValue(orgId) !== ''),
  })
}

/** `GET /api/organization/intake-address` (SLICE_007a §5) — org admins only. */
export function useIntakeAddress(orgId: MaybeRefOrGetter<string>) {
  return useQuery({
    queryKey: computed(() => queryKeys.intakeAddress(toValue(orgId))),
    queryFn: () => apiFetch<IntakeAddressResponse>('/organization/intake-address'),
    enabled: computed(() => toValue(orgId) !== ''),
  })
}

/** `POST /api/organization/intake-address/rotate` (SLICE_007g §5):
 *  break-glass rotation; returns the NEW address in the GET shape. */
export function useRotateIntakeAddressMutation(orgId: MaybeRefOrGetter<string>) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: () =>
      apiFetch<IntakeAddressResponse>('/organization/intake-address/rotate', {
        method: 'POST',
      }),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.intakeAddress(toValue(orgId)) })
    },
  })
}

/** `GET /api/organization/intake-settings` (SLICE_007c §5) — org admins only. */
export function useIntakeSettings(orgId: MaybeRefOrGetter<string>) {
  return useQuery({
    queryKey: computed(() => queryKeys.intakeSettings(toValue(orgId))),
    queryFn: () => apiFetch<IntakeSettingsResponse>('/organization/intake-settings'),
    enabled: computed(() => toValue(orgId) !== ''),
  })
}

/** `PUT /api/organization/intake-settings` (SLICE_007c §5). */
export function useUpdateIntakeSettingsMutation(orgId: MaybeRefOrGetter<string>) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (body: IntakeSettingsRequest) =>
      apiFetch<IntakeSettingsResponse>('/organization/intake-settings', {
        method: 'PUT',
        body: JSON.stringify(body),
      }),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.intakeSettings(toValue(orgId)) })
    },
  })
}

/** `GET /api/intake/unresolved/{id}` (SLICE_007e §5) — admin-only,
 *  decrypt-on-demand. Deliberately NOT a useQuery: content is fetched
 *  imperatively when the dialog opens and never cached (§7). */
export function fetchUnresolvedDetail(id: string): Promise<UnresolvedDetailResponse> {
  return apiFetch<UnresolvedDetailResponse>(`/intake/unresolved/${id}`)
}

/** `POST /api/intake/unresolved/{id}/retry` (SLICE_007e §5). A resolved
 *  retry also touches people/today, so invalidate those alongside the
 *  queue. */
export function useRetryUnresolvedMutation(orgId: MaybeRefOrGetter<string>) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: string) =>
      apiFetch<RetryUnresolvedResponse>(`/intake/unresolved/${id}/retry`, { method: 'POST' }),
    onSuccess: (outcome) => {
      const org = toValue(orgId)
      void qc.invalidateQueries({ queryKey: queryKeys.unresolved(org) })
      if (outcome.status === 'resolved') {
        void qc.invalidateQueries({ queryKey: queryKeys.people(org) })
        void qc.invalidateQueries({ queryKey: queryKeys.today(org) })
      }
    },
    // A failed retry may still have committed the reset-to-pending
    // (SLICE_007e §4) — refetch so the row's changed state shows.
    onError: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.unresolved(toValue(orgId)) })
    },
  })
}

/** `POST /api/intake/unresolved/{id}/discard` (SLICE_007e §5). */
export function useDiscardUnresolvedMutation(orgId: MaybeRefOrGetter<string>) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: string) =>
      apiFetch<DiscardUnresolvedResponse>(`/intake/unresolved/${id}/discard`, { method: 'POST' }),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.unresolved(toValue(orgId)) })
    },
  })
}

export function useMembers(orgId: MaybeRefOrGetter<string>) {
  return useQuery({
    queryKey: computed(() => queryKeys.members(toValue(orgId))),
    queryFn: () => apiFetch<MembersResponse>('/organization/members'),
    enabled: computed(() => toValue(orgId) !== ''),
  })
}

// SLICE_014 §4: named fetchers, factored out of the three Today queries'
// inline queryFns, so the router guard's `prefetchTodayData` (below) shares
// the exact same request shape rather than re-deriving it.
function fetchToday(signal?: AbortSignal): Promise<TodayResponse> {
  return apiFetch<TodayResponse>('/today', { signal })
}
function fetchTodaySources(signal?: AbortSignal): Promise<TodaySourcesResponse> {
  return apiFetch<TodaySourcesResponse>('/today/sources', { signal })
}
function fetchTodayFeeds(signal?: AbortSignal): Promise<MemberTodayFeedsResponse> {
  return apiFetch<MemberTodayFeedsResponse>('/today/feeds', { signal })
}

/**
 * SLICE_003 §10: `refetchInterval: 60_000` is one of the two backstops
 * (with window-focus refetch, a TanStack default) that keep Today correct
 * even if a realtime event is missed entirely — D-011, §9 "Missed events".
 * TanStack pauses the interval while the tab is backgrounded.
 */
export function useToday(orgId: MaybeRefOrGetter<string>, actorId: MaybeRefOrGetter<string>) {
  return useQuery({
    queryKey: computed(() => queryKeys.todayForActor(toValue(orgId), toValue(actorId))),
    queryFn: ({ signal }) => fetchToday(signal),
    enabled: computed(() => toValue(orgId) !== '' && toValue(actorId) !== ''),
    refetchInterval: 60_000,
    refetchOnMount: 'always',
    refetchOnWindowFocus: 'always',
  })
}

export function useTodaySources(orgId: MaybeRefOrGetter<string>, actorId: MaybeRefOrGetter<string>) {
  return useQuery({
    queryKey: computed(() => queryKeys.todaySources(toValue(orgId), toValue(actorId))),
    queryFn: ({ signal }) => fetchTodaySources(signal),
    enabled: computed(() => toValue(orgId) !== '' && toValue(actorId) !== ''),
    refetchInterval: 60_000,
    refetchOnMount: 'always',
    refetchOnWindowFocus: 'always',
  })
}

/**
 * SLICE_014 §4: called from the router guard once identity resolves for a
 * Today target with an Organization — before the Today route chunk even
 * finishes importing (`preloadTodayView` in `../preload.ts` races it from
 * the other direction, starting at login). `prefetchQuery` never throws and
 * never overwrites a fresher cache entry; a failed prefetch is silent (§7)
 * — the click/mount path just fetches normally, exactly like any other
 * failed mount fetch already does. Keys only from the factory (SLICE_002
 * §10). Nothing for a session with no Organization (platform-only) — the
 * caller (router.ts) only reaches this after confirming one.
 */
export function prefetchTodayData(qc: QueryClient, session: MeResponse): void {
  const orgId = session.organization?.id
  if (!orgId) return
  const actorId = session.user.id
  void qc.prefetchQuery({ queryKey: queryKeys.todayForActor(orgId, actorId), queryFn: ({ signal }) => fetchToday(signal) })
  void qc.prefetchQuery({ queryKey: queryKeys.todaySources(orgId, actorId), queryFn: ({ signal }) => fetchTodaySources(signal) })
  void qc.prefetchQuery({ queryKey: queryKeys.todayFeeds(orgId, actorId), queryFn: ({ signal }) => fetchTodayFeeds(signal) })
}

function invalidateTodaySourceCaches(qc: QueryClient, orgId: string, actorId: string) {
  void qc.invalidateQueries({ queryKey: queryKeys.today(orgId) })
  void qc.invalidateQueries({ queryKey: queryKeys.todaySources(orgId, actorId) })
}

// --- Slice 011d: Today system feeds (docs/specs/SLICE_011d.md §4, §6) -----
// `currentSavedListIdentity`/`hasCurrentSavedListIdentity`/
// `SavedListMutationIdentity` above are already a generic actor/Organization/
// auth-session fence (011b/011c pattern) — `useEnableTodaySourceMutation`
// already reuses them for a non-list mutation, so these do too rather than
// duplicating the fence.

/** `GET /api/today/feeds` — any active member; the effective rule only. */
export function useTodayFeeds(orgId: MaybeRefOrGetter<string>, actorId: MaybeRefOrGetter<string>) {
  return useQuery({
    queryKey: computed(() => queryKeys.todayFeeds(toValue(orgId), toValue(actorId))),
    queryFn: ({ signal }) => fetchTodayFeeds(signal),
    enabled: computed(() => toValue(orgId) !== '' && toValue(actorId) !== ''),
    refetchOnMount: 'always',
    refetchOnWindowFocus: 'always',
  })
}

/** `GET /api/organization/today-feeds` — org-admin only. */
export function useTodayFeedsAdmin(orgId: MaybeRefOrGetter<string>) {
  return useQuery({
    queryKey: computed(() => queryKeys.todayFeedsAdmin(toValue(orgId))),
    queryFn: ({ signal }) => apiFetch<TodayFeedsResponse>('/organization/today-feeds', { signal }),
    enabled: computed(() => toValue(orgId) !== ''),
    refetchOnMount: 'always',
    refetchOnWindowFocus: 'always',
  })
}

/** §6: "Mutations invalidate both and Today" — the admin list, the member
 * summary and Today itself (built-in items/reasons can change). */
function invalidateTodayFeedCaches(qc: QueryClient, orgId: string, actorId: string) {
  void qc.invalidateQueries({ queryKey: queryKeys.todayFeedsAdmin(orgId) })
  void qc.invalidateQueries({ queryKey: queryKeys.today(orgId) })
  // todayFeeds(orgId, actorId) already sits under today(orgId)'s prefix
  // (queryKeys.todayFeeds), so the invalidation above already covers it for
  // this actor; other actors' cached member summaries are covered the same
  // way the existing Today-prefix sweep covers every other actor-scoped key.
  void qc.invalidateQueries({ queryKey: queryKeys.todayFeeds(orgId, actorId) })
}

/** `PUT /api/organization/today-feeds/{feed_key}` (§4, §6). No auto-retry;
 * an uncertain outcome (network/5xx) still triggers the caller's own
 * refetch-and-reconcile flow (TodayFeedsView.vue), matching 011b's Save. */
export function useUpdateTodayFeedMutation(
  orgId: MaybeRefOrGetter<string>, actorId: MaybeRefOrGetter<string>, providedQueryClient?: QueryClient,
) {
  const qc = providedQueryClient ?? useQueryClient()
  return useMutation({
    mutationFn: ({ feedKey, body }: { feedKey: TodayFeedKey; body: UpdateTodayFeedRequest }) =>
      apiFetch<TodayFeedMutationResponse>(`/organization/today-feeds/${encodeURIComponent(feedKey)}`, {
        method: 'PUT', body: JSON.stringify(body),
      }),
    retry: false,
    onMutate: (): SavedListMutationIdentity => currentSavedListIdentity(qc) ?? {
      orgId: toValue(orgId), actorId: toValue(actorId), session: undefined,
    },
    onSuccess: (_result, _variables, identity) => {
      if (hasCurrentSavedListIdentity(qc, identity)) invalidateTodayFeedCaches(qc, identity.orgId, identity.actorId)
    },
    // §6 "uncertain mutations refetch": a network/5xx failure may still have
    // committed (as 011b/011c's Today-source mutations already handle) — an
    // uncertain outcome refetches the admin list so the card and the 409
    // reload flow both read the true current row afterward.
    onSettled: async (_result, _error, _variables, identity) => {
      if (!identity || !hasCurrentSavedListIdentity(qc, identity)) return
      invalidateTodayFeedCaches(qc, identity.orgId, identity.actorId)
      await qc.refetchQueries({ queryKey: queryKeys.todayFeedsAdmin(identity.orgId), type: 'active' })
    },
  }, providedQueryClient)
}

/** `POST /api/organization/today-feeds/{feed_key}/revert` (§4, §6). */
export function useRevertTodayFeedMutation(
  orgId: MaybeRefOrGetter<string>, actorId: MaybeRefOrGetter<string>, providedQueryClient?: QueryClient,
) {
  const qc = providedQueryClient ?? useQueryClient()
  return useMutation({
    mutationFn: ({ feedKey, body }: { feedKey: TodayFeedKey; body: RevertTodayFeedRequest }) =>
      apiFetch<TodayFeedMutationResponse>(`/organization/today-feeds/${encodeURIComponent(feedKey)}/revert`, {
        method: 'POST', body: JSON.stringify(body),
      }),
    retry: false,
    onMutate: (): SavedListMutationIdentity => currentSavedListIdentity(qc) ?? {
      orgId: toValue(orgId), actorId: toValue(actorId), session: undefined,
    },
    onSuccess: (_result, _variables, identity) => {
      if (hasCurrentSavedListIdentity(qc, identity)) invalidateTodayFeedCaches(qc, identity.orgId, identity.actorId)
    },
    onSettled: async (_result, _error, _variables, identity) => {
      if (!identity || !hasCurrentSavedListIdentity(qc, identity)) return
      invalidateTodayFeedCaches(qc, identity.orgId, identity.actorId)
      await qc.refetchQueries({ queryKey: queryKeys.todayFeedsAdmin(identity.orgId), type: 'active' })
    },
  }, providedQueryClient)
}

/** `PUT /api/organization/today-feeds/{feed_key}/enabled` (§4, §6) — Turn
 * off/Turn on. */
export function useSetTodayFeedEnabledMutation(
  orgId: MaybeRefOrGetter<string>, actorId: MaybeRefOrGetter<string>, providedQueryClient?: QueryClient,
) {
  const qc = providedQueryClient ?? useQueryClient()
  return useMutation({
    mutationFn: ({ feedKey, body }: { feedKey: TodayFeedKey; body: SetTodayFeedEnabledRequest }) =>
      apiFetch<TodayFeedMutationResponse>(`/organization/today-feeds/${encodeURIComponent(feedKey)}/enabled`, {
        method: 'PUT', body: JSON.stringify(body),
      }),
    retry: false,
    onMutate: (): SavedListMutationIdentity => currentSavedListIdentity(qc) ?? {
      orgId: toValue(orgId), actorId: toValue(actorId), session: undefined,
    },
    onSuccess: (_result, _variables, identity) => {
      if (hasCurrentSavedListIdentity(qc, identity)) invalidateTodayFeedCaches(qc, identity.orgId, identity.actorId)
    },
    onSettled: async (_result, _error, _variables, identity) => {
      if (!identity || !hasCurrentSavedListIdentity(qc, identity)) return
      invalidateTodayFeedCaches(qc, identity.orgId, identity.actorId)
      await qc.refetchQueries({ queryKey: queryKeys.todayFeedsAdmin(identity.orgId), type: 'active' })
    },
  }, providedQueryClient)
}

/** `POST /api/organization/today-feeds/{feed_key}/preview` (§4, §6) — a
 * read, never persisted; no cache to invalidate or seed. `_orgId` is kept
 * for signature symmetry with the other feed mutations (matches
 * `useDialCall`'s `_orgId` convention above). */
export function usePreviewTodayFeedMutation(_orgId: MaybeRefOrGetter<string>, providedQueryClient?: QueryClient) {
  return useMutation(
    {
      mutationFn: ({ feedKey, body }: { feedKey: TodayFeedKey; body: PreviewTodayFeedRequest }) =>
        apiFetch<PreviewTodayFeedResponse>(`/organization/today-feeds/${encodeURIComponent(feedKey)}/preview`, {
          method: 'POST', body: JSON.stringify(body),
        }),
      retry: false,
    },
    providedQueryClient,
  )
}

export function useEnableTodaySourceMutation(
  orgId: MaybeRefOrGetter<string>, actorId: MaybeRefOrGetter<string>, providedQueryClient?: QueryClient,
) {
  const qc = providedQueryClient ?? useQueryClient()
  return useMutation({
    mutationFn: ({ listId, body }: { listId: string; body: EnableTodaySourceRequest }) =>
      apiFetch<TodaySourceChange>(`/today/sources/${encodeURIComponent(listId)}`, { method: 'PUT', body: JSON.stringify(body) }),
    retry: false,
    onMutate: (): SavedListMutationIdentity => currentSavedListIdentity(qc) ?? { orgId: toValue(orgId), actorId: toValue(actorId), session: undefined },
    onSuccess: (_result, _variables, identity) => {
      if (hasCurrentSavedListIdentity(qc, identity)) invalidateTodaySourceCaches(qc, identity.orgId, identity.actorId)
    },
    onSettled: async (_result, _error, _variables, identity) => {
      if (!identity || !hasCurrentSavedListIdentity(qc, identity)) return
      invalidateTodaySourceCaches(qc, identity.orgId, identity.actorId)
      await qc.refetchQueries({ queryKey: queryKeys.todaySources(identity.orgId, identity.actorId), type: 'active' })
    },
  }, providedQueryClient)
}

export function useDisableTodaySourceMutation(
  orgId: MaybeRefOrGetter<string>, actorId: MaybeRefOrGetter<string>, providedQueryClient?: QueryClient,
) {
  const qc = providedQueryClient ?? useQueryClient()
  return useMutation({
    mutationFn: (listId: string) => apiFetch<TodaySourceChange>(`/today/sources/${encodeURIComponent(listId)}`, { method: 'DELETE' }),
    retry: false,
    onMutate: (): SavedListMutationIdentity => currentSavedListIdentity(qc) ?? { orgId: toValue(orgId), actorId: toValue(actorId), session: undefined },
    onSuccess: (_result, _listId, identity) => {
      if (hasCurrentSavedListIdentity(qc, identity)) invalidateTodaySourceCaches(qc, identity.orgId, identity.actorId)
    },
    onSettled: async (_result, _error, _listId, identity) => {
      if (!identity || !hasCurrentSavedListIdentity(qc, identity)) return
      invalidateTodaySourceCaches(qc, identity.orgId, identity.actorId)
      await qc.refetchQueries({ queryKey: queryKeys.todaySources(identity.orgId, identity.actorId), type: 'active' })
    },
  }, providedQueryClient)
}

/**
 * `POST /api/realtime/token` (§5, §6). Used by realtime/useRealtime.ts's
 * `getToken` — a plain function, not a `useQuery`/`useMutation` hook, since
 * the Centrifuge SDK calls it directly on its own schedule, not through
 * TanStack Query's cache.
 */
export function fetchRealtimeToken(): Promise<RealtimeTokenResponse> {
  return apiFetch<RealtimeTokenResponse>('/realtime/token', { method: 'POST' })
}

export function useLoginMutation() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (credentials: { email: string; password: string }) =>
      apiFetch<MeResponse>('/session', { method: 'POST', body: JSON.stringify(credentials) }),
    onMutate: () => {
      const epoch = beginSessionTransition()
      qc.clear()
      return epoch
    },
    onSuccess: (_data, _credentials, epoch) => {
      // A response callback can run after a different tab's later Set-Cookie
      // has replaced its browser session. Never seed this body: completion
      // always verifies the actual shared cookie through `/me`.
      if (epoch) settleSessionTransition(epoch)
      qc.clear()
    },
    onError: (_error, _credentials, epoch) => { if (epoch) settleSessionTransition(epoch) },
  })
}

export function useLogoutMutation() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: () => apiFetch<void>('/session', { method: 'DELETE' }),
    onMutate: () => {
      const epoch = beginLogoutSessionTransition()
      qc.clear()
      return epoch
    },
    onSuccess: (_data, _variables, epoch) => { if (epoch) settleSessionTransition(epoch) },
    onError: (_error, _variables, epoch) => { if (epoch) settleSessionTransition(epoch) },
  })
}

// ---- Optimistic Person mutations (SLICE_014 §3) ---------------------------
// Stage, assignment, and tag apply/remove pay the tunnel's ≥0.5-1s POST
// penalty (docs/design/perceived-latency-2026-09-07.md §1) even though the
// server settles in tens of milliseconds, so each predicts its result from
// an already-cached reference (the stages/members/tags query) and rolls
// back losslessly on failure.
//
// Rule 2 (§1): list membership never moves optimistically. A matching row is
// rewritten in place in every cached People variant (filtered, sorted and
// unfiltered alike, all sharing the ['org', orgId, 'people'] prefix — a
// prefix no other factory key shares: person/stages/members/tags all use a
// different third element), never added or removed; a row that stops
// matching its filter leaves only when the settled invalidate's refetch
// says so.
//
// Rule 3 (§1): with no cached reference to predict from (an empty stages/
// members/tags cache, or the specific id missing from it), the optimistic
// write is skipped entirely — never invent a name — and the mutation still
// completes pessimistically (the UI just waits for the response like today).

interface PersonDetailSnapshot {
  personKey: ReturnType<typeof queryKeys.person>
  person: PersonDetailResponse | undefined
}

interface PersonMutationSnapshot extends PersonDetailSnapshot {
  peopleEntries: Array<[readonly unknown[], PeopleResponse | undefined]>
}

async function snapshotPersonDetail(qc: QueryClient, orgId: string, personId: string): Promise<PersonDetailSnapshot> {
  const personKey = queryKeys.person(orgId, personId)
  await qc.cancelQueries({ queryKey: personKey })
  return { personKey, person: qc.getQueryData<PersonDetailResponse>(personKey) }
}

/** Also snapshots and cancels every cached People variant (the prefix match
 * documented above) — used by stage/assignment, whose optimistic write
 * touches list rows as well as the detail; tag apply/remove use
 * `snapshotPersonDetail` alone since `PersonSummary` (the People row shape)
 * carries no `tags` field. */
async function snapshotPersonForOptimism(qc: QueryClient, orgId: string, personId: string): Promise<PersonMutationSnapshot> {
  const peoplePrefix = queryKeys.people(orgId)
  const [detail] = await Promise.all([
    snapshotPersonDetail(qc, orgId, personId),
    qc.cancelQueries({ queryKey: peoplePrefix }),
  ])
  return { ...detail, peopleEntries: qc.getQueriesData<PeopleResponse>({ queryKey: peoplePrefix }) }
}

function restorePersonDetailSnapshot(qc: QueryClient, snapshot: PersonDetailSnapshot | undefined) {
  if (!snapshot) return
  qc.setQueryData(snapshot.personKey, snapshot.person)
}

function restorePersonMutationSnapshot(qc: QueryClient, snapshot: PersonMutationSnapshot | undefined) {
  if (!snapshot) return
  restorePersonDetailSnapshot(qc, snapshot)
  for (const [key, data] of snapshot.peopleEntries) qc.setQueryData(key, data)
}

/** Writes `update` into the Person detail (if cached) and into the matching
 * row (by id) of every cached People variant — never touching row count. */
function writeOptimisticPersonSummary(
  qc: QueryClient,
  orgId: string,
  personId: string,
  update: (summary: PersonSummary) => PersonSummary,
) {
  qc.setQueryData<PersonDetailResponse>(queryKeys.person(orgId, personId), (old) =>
    old ? { ...old, person: update(old.person) } : old,
  )
  qc.setQueriesData<PeopleResponse>({ queryKey: queryKeys.people(orgId) }, (old) => {
    if (!old) return old
    let changed = false
    const people = old.people.map((row) => {
      if (row.id !== personId) return row
      changed = true
      return update(row)
    })
    return changed ? { ...old, people } : old
  })
}

function stageRefFromCache(qc: QueryClient, orgId: string, stageId: string): StageRef | undefined {
  // `Stage` (the cache's row shape) also carries `position`, which
  // `PersonSummary.stage` (a `StageRef`) never does — narrow explicitly
  // rather than passing the wider object through.
  const stage = qc.getQueryData<StagesResponse>(queryKeys.stages(orgId))?.stages.find((s) => s.id === stageId)
  return stage ? { id: stage.id, name: stage.name } : undefined
}

function actorRefFromCache(qc: QueryClient, orgId: string, userId: string): ActorRef | undefined {
  const member = qc.getQueryData<MembersResponse>(queryKeys.members(orgId))?.members.find((m) => m.user_id === userId)
  return member ? { id: member.user_id, display_name: member.display_name } : undefined
}

function tagRefFromCache(qc: QueryClient, orgId: string, tagId: string): TagRef | undefined {
  const tag = qc.getQueryData<TagsResponse>(queryKeys.tags(orgId))?.tags.find((t) => t.id === tagId)
  return tag ? { id: tag.id, name: tag.name } : undefined
}

/** §3 table: "sorted by lower-cased name then id" — the server's
 * `lower(name), id` order is authoritative on success; this is only the
 * optimistic prediction of it. */
function sortTagRefs(tags: TagRef[]): TagRef[] {
  return [...tags].sort((a, b) => {
    const an = a.name.toLowerCase()
    const bn = b.name.toLowerCase()
    if (an !== bn) return an < bn ? -1 : 1
    return a.id < b.id ? -1 : a.id > b.id ? 1 : 0
  })
}

// --- Item 5 (014 LATER): isMutating guards on the four Person mutations ---
//
// Each of the four Person mutations (assign/stage/apply-tag/remove-tag)
// gets a `mutationKey` that includes the Person id, so a settle-invalidate
// can ask "is another mutation for this same Person still in flight?"
// before clobbering its optimistic value — the last one to settle is the
// one that actually invalidates. The bare prefix (no orgId/personId) also
// lets the realtime path (realtime/useRealtime.ts) ask "is any Person
// mutation pending at all?": `isMutating`/`invalidateQueries` match
// `mutationKey` by prefix, the same partial-match semantics `queryKey`
// filters already use, so `{ mutationKey: [PERSON_MUTATION_KEY_PREFIX] }`
// matches every Person mutation regardless of which Person or Organization.
export const PERSON_MUTATION_KEY_PREFIX = 'person-mutation'

export function personMutationKey(orgId: string, personId: string) {
  return [PERSON_MUTATION_KEY_PREFIX, orgId, personId] as const
}

/** Invalidates `queryKey` unless another mutation for this same Person is
 * still pending. Called from `onSuccess`/`onError`/`onSettled`: TanStack
 * Query's `Mutation#execute` (query-core's mutation.ts) runs these
 * callbacks and only THEN dispatches the `'success'`/`'error'` state
 * change, so the calling mutation itself is still counted as `isMutating`
 * at this point — `isMutating` for this Person's key is therefore always
 * >= 1 (itself); `> 1` is what detects a genuinely different, still-
 * in-flight sibling mutation for the same Person. */
function invalidateUnlessPersonMutationPending(
  qc: QueryClient,
  orgId: string,
  personId: string,
  queryKey: readonly unknown[],
) {
  if (qc.isMutating({ mutationKey: personMutationKey(orgId, personId) }) > 1) return
  void qc.invalidateQueries({ queryKey })
}

export function useAssignPersonMutation(
  orgId: MaybeRefOrGetter<string>,
  personId: MaybeRefOrGetter<string>,
  providedQueryClient?: QueryClient,
) {
  const qc = providedQueryClient ?? useQueryClient()
  return useMutation({
    mutationKey: computed(() => personMutationKey(toValue(orgId), toValue(personId))),
    mutationFn: ({ personId, assignedUserId }: { personId: string; assignedUserId: string | null }) =>
      apiFetch<MutatePersonResponse>(`/people/${personId}/assignment`, {
        method: 'POST',
        body: JSON.stringify({ assigned_user_id: assignedUserId } satisfies AssignmentRequest),
      }),
    onMutate: async ({ personId, assignedUserId }) => {
      const id = toValue(orgId)
      const snapshot = await snapshotPersonForOptimism(qc, id, personId)
      // Unassigned (`null`) needs no cache lookup and is always predictable;
      // a specific user needs the members cache (rule 3's fallback).
      const assignee = assignedUserId === null ? null : actorRefFromCache(qc, id, assignedUserId)
      if (assignedUserId === null || assignee !== undefined) {
        writeOptimisticPersonSummary(qc, id, personId, (person) => ({ ...person, assigned_user: assignee ?? null }))
      }
      return { id, snapshot }
    },
    onError: (_error, _variables, context) => {
      restorePersonMutationSnapshot(qc, context?.snapshot)
    },
    // Field-only (014 LATER item 4): a rapid stage-then-assignee pair must
    // not let this response's stale `stage` snapshot (as of when the
    // assignment command ran) overwrite a still-in-flight or already-
    // settled stage change — write only the field this mutation owns, into
    // the detail and every cached People row (writeOptimisticPersonSummary
    // already does both).
    onSuccess: (data, variables, context) => {
      const id = context?.id ?? toValue(orgId)
      writeOptimisticPersonSummary(qc, id, variables.personId, (person) => ({
        ...person,
        assigned_user: data.person.assigned_user,
      }))
    },
    // Fires whether the mutation resolved or rejected — an uncertain
    // network failure may still have committed server-side, and a real
    // `person.changed` invalidation racing a pending mutation only ever
    // arrives after that mutation's own commit (§3 "Realtime"), so this
    // final invalidate's refetch is guaranteed to see the true state.
    // Item 5 (014 LATER): skipped while another mutation for the same
    // Person is still pending — that one invalidates when it settles.
    onSettled: (_data, _error, variables, context) => {
      const id = context?.id ?? toValue(orgId)
      invalidateUnlessPersonMutationPending(qc, id, variables.personId, queryKeys.org(id))
    },
  }, providedQueryClient)
}

export function useChangeStageMutation(
  orgId: MaybeRefOrGetter<string>,
  personId: MaybeRefOrGetter<string>,
  providedQueryClient?: QueryClient,
) {
  const qc = providedQueryClient ?? useQueryClient()
  return useMutation({
    mutationKey: computed(() => personMutationKey(toValue(orgId), toValue(personId))),
    mutationFn: ({ personId, stageId }: { personId: string; stageId: string }) =>
      apiFetch<MutatePersonResponse>(`/people/${personId}/stage`, {
        method: 'POST',
        body: JSON.stringify({ stage_id: stageId } satisfies StageRequest),
      }),
    onMutate: async ({ personId, stageId }) => {
      const id = toValue(orgId)
      const snapshot = await snapshotPersonForOptimism(qc, id, personId)
      const stage = stageRefFromCache(qc, id, stageId)
      if (stage) writeOptimisticPersonSummary(qc, id, personId, (person) => ({ ...person, stage }))
      return { id, snapshot }
    },
    onError: (_error, _variables, context) => {
      restorePersonMutationSnapshot(qc, context?.snapshot)
    },
    // Field-only (014 LATER item 4): see useAssignPersonMutation's
    // onSuccess above — same reasoning, this mutation's own field is
    // `stage`.
    onSuccess: (data, variables, context) => {
      const id = context?.id ?? toValue(orgId)
      writeOptimisticPersonSummary(qc, id, variables.personId, (person) => ({
        ...person,
        stage: data.person.stage,
      }))
    },
    // Item 5 (014 LATER): skipped while another mutation for the same
    // Person is still pending — that one invalidates when it settles.
    onSettled: (_data, _error, variables, context) => {
      const id = context?.id ?? toValue(orgId)
      invalidateUnlessPersonMutationPending(qc, id, variables.personId, queryKeys.org(id))
    },
  }, providedQueryClient)
}

/** `POST /api/people/{id}/contact-attempts` (§5, D-022). Same
 * setQueryData-plus-invalidate pattern as assign/stage above: the mutation's
 * own response updates the Person query immediately (this tab does not
 * need to wait for a realtime round-trip), and invalidating the whole
 * `['org', orgId]` branch covers Today (the row leaves) and any other open
 * view without hand-picking keys. */
export function useLogContactMutation(orgId: MaybeRefOrGetter<string>) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ personId, channel, outcome }: { personId: string; channel: ContactChannel; outcome: ContactOutcome }) =>
      apiFetch<LogContactResponse>(`/people/${personId}/contact-attempts`, {
        method: 'POST',
        body: JSON.stringify({ channel, outcome } satisfies LogContactRequest),
      }),
    onSuccess: (data, variables) => {
      const id = toValue(orgId)
      qc.setQueryData(queryKeys.person(id, variables.personId), (old: PersonDetailResponse | undefined) =>
        old ? { ...old, person: data.person } : old,
      )
      void qc.invalidateQueries({ queryKey: queryKeys.org(id) })
    },
  })
}

export function useCreateInquiryMutation(orgId: MaybeRefOrGetter<string>) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (request: ReceiveInquiryRequest) =>
      apiFetch<ReceiveInquiryResponse>('/inquiries', {
        method: 'POST',
        body: JSON.stringify(request),
      }),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.org(toValue(orgId)) })
    },
  })
}

// ---- Administration (SLICE_004 §5, §10) -----------------------------------
// Task brief: "After any mutation invalidate `me` and ['org', orgId,
// 'members'] (plus the invitations and platform keys you add)."

export function useInvitations(orgId: MaybeRefOrGetter<string>) {
  return useQuery({
    queryKey: computed(() => queryKeys.invitations(toValue(orgId))),
    queryFn: () => apiFetch<InvitationsResponse>('/organization/invitations'),
    enabled: computed(() => toValue(orgId) !== ''),
  })
}

export function useIssueInvitationMutation(orgId: MaybeRefOrGetter<string>) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (request: IssueInvitationRequest) =>
      apiFetch<IssueInvitationResponse>('/organization/invitations', {
        method: 'POST',
        body: JSON.stringify(request),
      }),
    onSuccess: () => {
      const id = toValue(orgId)
      void qc.invalidateQueries({ queryKey: queryKeys.invitations(id) })
      void qc.invalidateQueries({ queryKey: queryKeys.members(id) })
    },
  })
}

export function useRevokeInvitationMutation(orgId: MaybeRefOrGetter<string>) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (invitationId: string) =>
      apiFetch<void>(`/organization/invitations/${invitationId}`, { method: 'DELETE' }),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.invitations(toValue(orgId)) })
    },
  })
}

export function useChangeMemberRoleMutation(orgId: MaybeRefOrGetter<string>) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ userId, role }: { userId: string; role: MembershipRole }) =>
      apiFetch<MemberMutationResponse>(`/organization/members/${userId}/role`, {
        method: 'PUT',
        body: JSON.stringify({ role } satisfies ChangeMemberRoleRequest),
      }),
    onSuccess: () => {
      const id = toValue(orgId)
      void qc.invalidateQueries({ queryKey: queryKeys.members(id) })
      void qc.invalidateQueries({ queryKey: queryKeys.me })
    },
  })
}

export function useSetMemberStatusMutation(orgId: MaybeRefOrGetter<string>) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ userId, status }: { userId: string; status: MembershipStatus }) =>
      apiFetch<MemberMutationResponse>(`/organization/members/${userId}/status`, {
        method: 'PUT',
        body: JSON.stringify({ status } satisfies SetMemberStatusRequest),
      }),
    onSuccess: () => {
      const id = toValue(orgId)
      void qc.invalidateQueries({ queryKey: queryKeys.members(id) })
      void qc.invalidateQueries({ queryKey: queryKeys.me })
    },
  })
}

export function usePlatformOrganizations() {
  return useQuery({
    queryKey: queryKeys.platformOrganizations(),
    queryFn: () => apiFetch<PlatformOrganizationsResponse>('/platform/organizations'),
  })
}

export function useCreateOrganizationMutation() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (request: CreateOrganizationRequest) =>
      apiFetch<CreateOrganizationResponse>('/platform/organizations', {
        method: 'POST',
        body: JSON.stringify(request),
      }),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.platformOrganizations() })
    },
  })
}

export function usePlatformOrganization(id: MaybeRefOrGetter<string>) {
  return useQuery({
    queryKey: computed(() => queryKeys.platformOrganization(toValue(id))),
    queryFn: () => apiFetch<PlatformOrganizationDetailResponse>(`/platform/organizations/${toValue(id)}`),
    enabled: computed(() => toValue(id) !== ''),
  })
}

/** PUT /api/platform/organizations/{id}/members/{user_id}/role — always `admin` (D-026 §4); the route rejects `member` before the domain. */
export function usePlatformPromoteMutation(organizationId: MaybeRefOrGetter<string>) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (userId: string) =>
      apiFetch<MemberMutationResponse>(`/platform/organizations/${toValue(organizationId)}/members/${userId}/role`, {
        method: 'PUT',
        body: JSON.stringify({ role: 'admin' } satisfies PlatformChangeMemberRoleRequest),
      }),
    onSuccess: () => {
      const id = toValue(organizationId)
      void qc.invalidateQueries({ queryKey: queryKeys.platformOrganization(id) })
      void qc.invalidateQueries({ queryKey: queryKeys.platformOrganizations() })
      void qc.invalidateQueries({ queryKey: queryKeys.me })
    },
  })
}

/** POST /api/platform/organizations/{id}/invitations — always role `admin` (D-021 §1, D-026 §4). Takes `{email, role}` (role ignored, always sent as 'admin') so it shares InviteDialog.vue's submit payload shape with the org-admin mutation. */
export function usePlatformIssueInvitationMutation(organizationId: MaybeRefOrGetter<string>) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ email }: { email: string; role: 'admin' }) =>
      apiFetch<IssueInvitationResponse>(`/platform/organizations/${toValue(organizationId)}/invitations`, {
        method: 'POST',
        body: JSON.stringify({ email, role: 'admin' } satisfies PlatformIssueInvitationRequest),
      }),
    onSuccess: () => {
      const id = toValue(organizationId)
      void qc.invalidateQueries({ queryKey: queryKeys.platformOrganization(id) })
      void qc.invalidateQueries({ queryKey: queryKeys.platformOrganizations() })
    },
  })
}

export function usePlatformRevokeInvitationMutation(organizationId: MaybeRefOrGetter<string>) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (invitationId: string) =>
      apiFetch<void>(`/platform/organizations/${toValue(organizationId)}/invitations/${invitationId}`, {
        method: 'DELETE',
      }),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.platformOrganization(toValue(organizationId)) })
    },
  })
}

/** POST /api/invitations/preview — public, no session (§5). Called by a `useQuery` in InviteView.vue keyed on `queryKeys.invitationPreview(token)`, not a plain fetch, so `retry: false` there controls retries the same way every other read query does. */
export function fetchInvitationPreview(token: string): Promise<InvitationPreviewResponse> {
  return apiFetch<InvitationPreviewResponse>('/invitations/preview', {
    method: 'POST',
    body: JSON.stringify({ token } satisfies InvitationPreviewRequest),
  })
}

/** POST /api/invitations/accept — public; success body is identical to `POST /api/session` (§5), so this mirrors useLoginMutation's onSuccess (seed the `me` cache directly rather than refetching). */
export function useAcceptInvitationMutation() {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (request: AcceptInvitationRequest) =>
      apiFetch<AcceptInvitationResponse>('/invitations/accept', {
        method: 'POST',
        body: JSON.stringify(request),
      }),
    onMutate: () => {
      const epoch = beginSessionTransition()
      qc.clear()
      return epoch
    },
    onSuccess: (_data, _request, epoch) => {
      if (epoch) settleSessionTransition(epoch)
      qc.clear()
    },
    onError: (_error, _request, epoch) => { if (epoch) settleSessionTransition(epoch) },
  })
}

// --- Slice 005: Operator (docs/specs/SLICE_005.md §10) ----------------------

export function postOperatorTurn(request: OperatorTurnRequest): Promise<OperatorTurnResponse> {
  return apiFetch<OperatorTurnResponse>('/operator/turns', {
    method: 'POST',
    body: JSON.stringify(request),
  })
}

/**
 * One stateless turn. No query keys and no invalidation: the Operator is
 * read-only this slice (nothing it does changes data), and the transcript
 * lives in `OperatorPanel`'s component state, never in the cache.
 */
export function useOperatorTurn() {
  return useMutation({
    mutationFn: postOperatorTurn,
  })
}

// --- Slice 006: Calling (docs/specs/SLICE_006.md §5, §6, §10) ---------------
// In-call ring/answer state comes from LiveKit itself (telephony/useCall.ts);
// these hooks are the HTTP side only. Every mutation is "caller-only" on the
// server (403 otherwise) and `hangup` is idempotent (200 on a terminal call).

/**
 * `GET /api/calls/{id}` — the authoritative fallback (D-023): `call.changed`
 * invalidates `queryKeys.call`, this query refetches, and the panel reads the
 * server's `status`/reasons from it rather than from the event. `queryClient`
 * is optional so `useCall` (the composable) can run under a bare effect scope
 * in tests without the Vue plugin; production callers leave it undefined.
 */
export function useCall(orgId: MaybeRefOrGetter<string>, callId: MaybeRefOrGetter<string>, queryClient?: QueryClient) {
  return useQuery(
    {
      queryKey: computed(() => queryKeys.call(toValue(orgId), toValue(callId))),
      queryFn: () => apiFetch<CallResponse>(`/calls/${toValue(callId)}`),
      enabled: computed(() => toValue(orgId) !== '' && toValue(callId) !== ''),
      // The start/dial/hangup responses seed this key with the settled call,
      // so a freshly seeded entry is not refetched just for mounting;
      // `invalidateQueries` (the `call.changed` path) refetches regardless of
      // staleness, which is the one refetch trigger this query needs.
      staleTime: 10_000,
    },
    queryClient,
  )
}

/** `POST /api/people/{id}/calls` → 201 `{call, join}`. The body carries only
 * the contact method's id — never a phone number (§10 hard rule). Only the
 * PII-free `call` is seeded into the query cache; the response (which holds
 * the join token) is not retained by the MutationCache: `gcTime: 0`, and
 * `useCall` resets the mutation the moment it has read `join`. */
export function useStartCall(orgId: MaybeRefOrGetter<string>, queryClient?: QueryClient) {
  const qc = queryClient ?? useQueryClient()
  return useMutation(
    {
      mutationFn: ({ personId, contactMethodId }: { personId: string; contactMethodId: string }) =>
        apiFetch<StartCallResponse>(`/people/${personId}/calls`, {
          method: 'POST',
          body: JSON.stringify({ contact_method_id: contactMethodId } satisfies StartCallRequest),
        }),
      gcTime: 0,
      onSuccess: (data) => {
        qc.setQueryData(queryKeys.call(toValue(orgId), data.call.id), { call: data.call } satisfies CallResponse)
      },
    },
    queryClient,
  )
}

/**
 * `POST /api/operator/proposals/{id}/confirm` (SLICE_006b §4): the human
 * click that executes a proposed call. Same response shape as
 * `useStartCall`; seeds the call key the same way. Model-free: works with
 * the operator unavailable.
 */
export function useConfirmProposal(orgId: MaybeRefOrGetter<string>, queryClient?: QueryClient) {
  const qc = queryClient ?? useQueryClient()
  return useMutation(
    {
      mutationFn: (proposalId: string) =>
        apiFetch<StartCallResponse>(`/operator/proposals/${proposalId}/confirm`, {
          method: 'POST',
        }),
      gcTime: 0,
      onSuccess: (data) => {
        qc.setQueryData(queryKeys.call(toValue(orgId), data.call.id), { call: data.call } satisfies CallResponse)
      },
    },
    queryClient,
  )
}

/** `POST /api/calls/{id}/dial` → 202 `{call}` (still `placing`; the dial task
 * moves it to `ringing`). 409 `invalid_call_state` on a second request. The
 * 202 body is deliberately not written to the cache: the 201 already seeded
 * `placing`, and a late 202 must not regress a newer `GET` (the
 * `call.changed` refetch) to `placing`. `orgId` is kept for signature
 * symmetry with the other call mutations. */
export function useDialCall(_orgId: MaybeRefOrGetter<string>, queryClient?: QueryClient) {
  return useMutation(
    {
      mutationFn: (callId: string) => apiFetch<CallResponse>(`/calls/${callId}/dial`, { method: 'POST' }),
    },
    queryClient,
  )
}

/** `POST /api/calls/{id}/hangup` → 200 `{call}`, idempotent. The response is
 * the settled call, so the call key is seeded directly; the Person branch is
 * invalidated for the `call_completed` / `contact_attempted` history rows —
 * this tab need not wait for the realtime round-trip. */
export function useHangupCall(orgId: MaybeRefOrGetter<string>, queryClient?: QueryClient) {
  const qc = queryClient ?? useQueryClient()
  return useMutation(
    {
      mutationFn: (callId: string) => apiFetch<CallResponse>(`/calls/${callId}/hangup`, { method: 'POST' }),
      onSuccess: (data) => {
        const id = toValue(orgId)
        qc.setQueryData(queryKeys.call(id, data.call.id), data)
        void qc.invalidateQueries({ queryKey: queryKeys.person(id, data.call.person_id) })
        void qc.invalidateQueries({ queryKey: queryKeys.today(id) })
      },
    },
    queryClient,
  )
}

// --- Slice 006c: Call outcome correction (docs/specs/SLICE_006c.md §5, §10)

/** `POST /api/calls/{id}/outcome` → 200 `{attempt, changed}`. Caller-only
 * (403), 409 `invalid_call_state` until the call is terminal, 422
 * `no_contact_attempt`, 409 `correction_conflict`. On success the Person
 * branch (history rows) and Today (`last_contact_attempt` is now the
 * effective row) are invalidated — `person.changed` covers other tabs. */
export function useCorrectCallOutcome(orgId: MaybeRefOrGetter<string>, queryClient?: QueryClient) {
  const qc = queryClient ?? useQueryClient()
  return useMutation(
    {
      // `personId` is not on the wire (the route is call-scoped); it names
      // the Person query to invalidate, so the key stays factory-built.
      mutationFn: ({ callId, outcome }: { callId: string; personId: string; outcome: CallOutcomeCorrection }) =>
        apiFetch<CorrectOutcomeResponse>(`/calls/${callId}/outcome`, {
          method: 'POST',
          body: JSON.stringify({ outcome } satisfies CorrectOutcomeRequest),
        }),
      onSuccess: (_data, variables) => {
        const id = toValue(orgId)
        void qc.invalidateQueries({ queryKey: queryKeys.person(id, variables.personId) })
        void qc.invalidateQueries({ queryKey: queryKeys.today(id) })
      },
    },
    queryClient,
  )
}

// --- Slice 009: Correspondence capture (docs/specs/SLICE_009.md §8, §10) --

/** `GET /api/capture/address` — member-self (the agent's own credential). */
export function useCaptureAddress(orgId: MaybeRefOrGetter<string>) {
  return useQuery({
    queryKey: computed(() => queryKeys.captureAddress(toValue(orgId))),
    queryFn: () => apiFetch<CaptureAddressResponse>('/capture/address'),
    enabled: computed(() => toValue(orgId) !== ''),
  })
}

/** `POST /api/capture/address/rotate` — break-glass rotation; returns the
 *  NEW address in the GET shape (mirrors `useRotateIntakeAddressMutation`). */
export function useRotateCaptureAddressMutation(orgId: MaybeRefOrGetter<string>) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: () =>
      apiFetch<CaptureAddressResponse>('/capture/address/rotate', { method: 'POST' }),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.captureAddress(toValue(orgId)) })
    },
  })
}

/** `GET /api/capture/unmatched` — the viewer's own held queue only. */
export function useCaptureUnmatched(orgId: MaybeRefOrGetter<string>) {
  return useQuery({
    queryKey: computed(() => queryKeys.captureUnmatched(toValue(orgId))),
    queryFn: () => apiFetch<CaptureUnmatchedResponse>('/capture/unmatched'),
    enabled: computed(() => toValue(orgId) !== ''),
  })
}

/** `POST /api/capture/unmatched/{id}/link` — a resolved link also touches
 *  the linked Person's timeline/today, so invalidate those alongside the
 *  held queue, matching `useRetryUnresolvedMutation`'s pattern. */
export function useLinkUnmatchedMutation(orgId: MaybeRefOrGetter<string>) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ id, personId, addContactMethod }: { id: string; personId: string; addContactMethod: boolean }) =>
      apiFetch<{ status: 'linked' }>(`/capture/unmatched/${id}/link`, {
        method: 'POST',
        body: JSON.stringify({
          person_id: personId,
          add_contact_method: addContactMethod,
        } satisfies LinkUnmatchedRequest),
      }),
    onSuccess: (_data, variables) => {
      const id = toValue(orgId)
      void qc.invalidateQueries({ queryKey: queryKeys.captureUnmatched(id) })
      void qc.invalidateQueries({ queryKey: queryKeys.person(id, variables.personId) })
      void qc.invalidateQueries({ queryKey: queryKeys.today(id) })
    },
  })
}

/** `POST /api/capture/unmatched/{id}/dismiss` — idempotent. */
export function useDismissUnmatchedMutation(orgId: MaybeRefOrGetter<string>) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: (id: string) =>
      apiFetch<{ status: 'dismissed' }>(`/capture/unmatched/${id}/dismiss`, { method: 'POST' }),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.captureUnmatched(toValue(orgId)) })
    },
  })
}

// --- Slice 011e: Tags (docs/specs/SLICE_011e.md §5, §10) --------------------
// Any active member creates/applies/removes; an admin, or the creator while
// unused, renames/deletes (D-051, rule 1 — decided server-side; the client
// never re-derives it beyond the display-only `can_manage` hint). A 403
// (rule-1 permission lost between read and write) or 404 (the tag vanished)
// on rename/delete/apply/remove refetches the index so the surprise is
// explained by the next render, matching the saved-list/today-source
// uncertain-mutation convention already used above in this file.

/** `GET /api/tags` — every active member; unpaginated (≤ 200 rows). */
export function useTagsQuery(orgId: MaybeRefOrGetter<string>, providedQueryClient?: QueryClient) {
  return useQuery(
    {
      queryKey: computed(() => queryKeys.tags(toValue(orgId))),
      queryFn: ({ signal }) => apiFetch<TagsResponse>('/tags', { signal }),
      enabled: computed(() => toValue(orgId) !== ''),
    },
    providedQueryClient,
  )
}

function refetchTagsOnStaleReference(qc: QueryClient, orgId: MaybeRefOrGetter<string>, error: unknown) {
  if (error instanceof ApiError && (error.status === 403 || error.status === 404)) {
    void qc.invalidateQueries({ queryKey: queryKeys.tags(toValue(orgId)) })
  }
}

/** Apply/remove's own 404 (the tag vanished between the popover's read and
 * the write): also invalidate the Person detail, not just the tags index —
 * the vanished tag can still be sitting in this tab's cached `tags` array
 * on the Person, and only a Person-key invalidation clears that stale
 * chip. Person-tag routes carry no 403 case (any member may apply/remove),
 * so only 404 is checked here. */
function refetchOnStalePersonTagReference(
  qc: QueryClient,
  orgId: MaybeRefOrGetter<string>,
  personId: string,
  error: unknown,
) {
  if (error instanceof ApiError && error.status === 404) {
    const id = toValue(orgId)
    void qc.invalidateQueries({ queryKey: queryKeys.tags(id) })
    void qc.invalidateQueries({ queryKey: queryKeys.person(id, personId) })
  }
}

/** `POST /api/tags` — create-or-get by case-insensitive name; never 409s on
 *  a name collision (`created: false` with the first spelling instead). */
export function useCreateTagMutation(orgId: MaybeRefOrGetter<string>, providedQueryClient?: QueryClient) {
  const qc = providedQueryClient ?? useQueryClient()
  return useMutation({
    mutationFn: (body: CreateTagRequest) =>
      apiFetch<CreateTagResponse>('/tags', { method: 'POST', body: JSON.stringify(body) }),
    retry: false,
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.tags(toValue(orgId)) })
    },
  }, providedQueryClient)
}

/** `PUT /api/tags/{tag_id}` — rule-1 permission decided server-side. */
export function useRenameTagMutation(orgId: MaybeRefOrGetter<string>, providedQueryClient?: QueryClient) {
  const qc = providedQueryClient ?? useQueryClient()
  return useMutation({
    mutationFn: ({ tagId, body }: { tagId: string; body: RenameTagRequest }) =>
      apiFetch<RenameTagResponse>(`/tags/${encodeURIComponent(tagId)}`, {
        method: 'PUT',
        body: JSON.stringify(body),
      }),
    retry: false,
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.tags(toValue(orgId)) })
    },
    onError: (error) => refetchTagsOnStaleReference(qc, orgId, error),
  }, providedQueryClient)
}

/** `DELETE /api/tags/{tag_id}` — hard delete; a repeat is 404. */
export function useDeleteTagMutation(orgId: MaybeRefOrGetter<string>, providedQueryClient?: QueryClient) {
  const qc = providedQueryClient ?? useQueryClient()
  return useMutation({
    mutationFn: (tagId: string) =>
      apiFetch<DeleteTagResponse>(`/tags/${encodeURIComponent(tagId)}`, { method: 'DELETE' }),
    retry: false,
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.tags(toValue(orgId)) })
    },
    onError: (error) => refetchTagsOnStaleReference(qc, orgId, error),
  }, providedQueryClient)
}

/** `PUT /api/people/{id}/tags/{tag_id}` — target-state idempotent; no body. */
export function useAddPersonTagMutation(
  orgId: MaybeRefOrGetter<string>,
  personId: MaybeRefOrGetter<string>,
  providedQueryClient?: QueryClient,
) {
  const qc = providedQueryClient ?? useQueryClient()
  return useMutation({
    mutationKey: computed(() => personMutationKey(toValue(orgId), toValue(personId))),
    mutationFn: ({ personId, tagId }: { personId: string; tagId: string }) =>
      apiFetch<PersonTagMutationResponse>(
        `/people/${encodeURIComponent(personId)}/tags/${encodeURIComponent(tagId)}`,
        { method: 'PUT' },
      ),
    retry: false,
    // §3 table: append the TagRef from the tags cache, sorted; skipped (rule
    // 3) if the tag or an already-applied duplicate is not resolvable from
    // cache — never invent a name, never double-apply optimistically.
    onMutate: async ({ personId, tagId }) => {
      const id = toValue(orgId)
      const snapshot = await snapshotPersonDetail(qc, id, personId)
      const tag = tagRefFromCache(qc, id, tagId)
      if (tag && snapshot.person && !snapshot.person.tags.some((t) => t.id === tag.id)) {
        qc.setQueryData<PersonDetailResponse>(snapshot.personKey, (old) =>
          old ? { ...old, tags: sortTagRefs([...old.tags, tag]) } : old,
        )
      }
      return snapshot
    },
    // Round-1 review fix: ANY error (not only the 404 stale-reference case
    // above) invalidates the Person key after rollback — a timeout or
    // 5xx whose write actually committed server-side otherwise leaves a
    // rolled-back chip that contradicts the server until some unrelated
    // invalidation happens to refetch it. Item 5 (014 LATER): guarded the
    // same way as assign/stage's onSettled — skipped while another
    // mutation for this same Person is still pending.
    onError: (error, variables, snapshot) => {
      restorePersonDetailSnapshot(qc, snapshot)
      refetchOnStalePersonTagReference(qc, orgId, variables.personId, error)
      const id = toValue(orgId)
      invalidateUnlessPersonMutationPending(qc, id, variables.personId, queryKeys.person(id, variables.personId))
    },
    onSuccess: (result, variables) => {
      const id = toValue(orgId)
      qc.setQueryData<PersonDetailResponse>(queryKeys.person(id, variables.personId), (old) =>
        old ? { ...old, tags: result.tags } : old,
      )
      void qc.invalidateQueries({ queryKey: queryKeys.tags(id) })
      invalidateUnlessPersonMutationPending(qc, id, variables.personId, queryKeys.person(id, variables.personId))
    },
  }, providedQueryClient)
}

/** `DELETE /api/people/{id}/tags/{tag_id}` — target-state idempotent. */
export function useRemovePersonTagMutation(
  orgId: MaybeRefOrGetter<string>,
  personId: MaybeRefOrGetter<string>,
  providedQueryClient?: QueryClient,
) {
  const qc = providedQueryClient ?? useQueryClient()
  return useMutation({
    mutationKey: computed(() => personMutationKey(toValue(orgId), toValue(personId))),
    mutationFn: ({ personId, tagId }: { personId: string; tagId: string }) =>
      apiFetch<PersonTagMutationResponse>(
        `/people/${encodeURIComponent(personId)}/tags/${encodeURIComponent(tagId)}`,
        { method: 'DELETE' },
      ),
    retry: false,
    // §3 table: filter the TagRef out of the detail — no cache lookup is
    // needed to remove an id, so this is always optimistic when the detail
    // is cached (unlike apply, there is no "unresolvable reference" case).
    onMutate: async ({ personId, tagId }) => {
      const id = toValue(orgId)
      const snapshot = await snapshotPersonDetail(qc, id, personId)
      if (snapshot.person) {
        qc.setQueryData<PersonDetailResponse>(snapshot.personKey, (old) =>
          old ? { ...old, tags: old.tags.filter((t) => t.id !== tagId) } : old,
        )
      }
      return snapshot
    },
    // Round-1 review fix: see useAddPersonTagMutation's onError above —
    // same "any error invalidates the Person key after rollback" rule,
    // and the same item 5 isMutating guard.
    onError: (error, variables, snapshot) => {
      restorePersonDetailSnapshot(qc, snapshot)
      refetchOnStalePersonTagReference(qc, orgId, variables.personId, error)
      const id = toValue(orgId)
      invalidateUnlessPersonMutationPending(qc, id, variables.personId, queryKeys.person(id, variables.personId))
    },
    onSuccess: (result, variables) => {
      const id = toValue(orgId)
      qc.setQueryData<PersonDetailResponse>(queryKeys.person(id, variables.personId), (old) =>
        old ? { ...old, tags: result.tags } : old,
      )
      void qc.invalidateQueries({ queryKey: queryKeys.tags(id) })
      invalidateUnlessPersonMutationPending(qc, id, variables.personId, queryKeys.person(id, variables.personId))
    },
  }, providedQueryClient)
}
