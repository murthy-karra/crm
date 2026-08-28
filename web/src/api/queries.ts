import { useMutation, useQuery, useQueryClient, type QueryClient } from '@tanstack/vue-query'
import { type MaybeRefOrGetter, computed, toValue } from 'vue'
import { queryClient } from '../query-client'
import { apiFetch } from './client'
import type {
  AcceptInvitationRequest,
  AcceptInvitationResponse,
  AssignmentRequest,
  CallOutcomeCorrection,
  CallResponse,
  CaptureAddressResponse,
  CaptureUnmatchedResponse,
  ChangeMemberRoleRequest,
  ContactChannel,
  ContactOutcome,
  DiscardUnresolvedResponse,
  RetryUnresolvedResponse,
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
  PlatformOrganizationDetailResponse,
  PlatformOrganizationsResponse,
  RealtimeTokenResponse,
  ReceiveInquiryRequest,
  ReceiveInquiryResponse,
  SetMemberStatusRequest,
  StageRequest,
  StagesResponse,
  StartCallRequest,
  IntakeAddressResponse,
  IntakeSettingsRequest,
  IntakeSettingsResponse,
  StartCallResponse,
  TodayResponse,
  UnresolvedResponse,
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
  people: (orgId: string, serializedFilter?: string) =>
    serializedFilter ? (['org', orgId, 'people', serializedFilter] as const) : (['org', orgId, 'people'] as const),
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
  platformOrganizations: () => ['platform', 'organizations'] as const,
  platformOrganization: (id: string) => ['platform', 'organizations', id] as const,
}

export function fetchMe(): Promise<MeResponse> {
  return apiFetch<MeResponse>('/me')
}

/**
 * Also the router's auth gate: a 401 ApiError means "go to /login" (the
 * QueryClient's default retry policy already skips retrying a 401 — see
 * query-client.ts).
 */
export function useMe() {
  return useQuery({
    queryKey: queryKeys.me,
    queryFn: fetchMe,
  })
}

/**
 * `GET /api/people` and `GET /api/people?filter=<...>` (docs/specs/SLICE_011a.md
 * §5a, §6). `serializedFilter` is the SAME percent-encodable JSON string
 * used for both the query-key element and the URL param
 * (`lib/filter.ts`'s `serializeFilter`) — pass `undefined`/`''` for the
 * unfiltered legacy path, whose key stays byte-identical to before this
 * slice.
 */
export function usePeople(orgId: MaybeRefOrGetter<string>, serializedFilter?: MaybeRefOrGetter<string | undefined>) {
  return useQuery({
    queryKey: computed(() => queryKeys.people(toValue(orgId), toValue(serializedFilter) || undefined)),
    queryFn: () => {
      const filter = toValue(serializedFilter)
      const path = filter ? `/people?filter=${encodeURIComponent(filter)}` : '/people'
      return apiFetch<PeopleResponse>(path)
    },
    enabled: computed(() => toValue(orgId) !== ''),
  })
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

export function usePerson(orgId: MaybeRefOrGetter<string>, personId: MaybeRefOrGetter<string>) {
  return useQuery({
    queryKey: computed(() => queryKeys.person(toValue(orgId), toValue(personId))),
    queryFn: () => apiFetch<PersonDetailResponse>(`/people/${toValue(personId)}`),
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

/**
 * SLICE_003 §10: `refetchInterval: 60_000` is one of the two backstops
 * (with window-focus refetch, a TanStack default) that keep Today correct
 * even if a realtime event is missed entirely — D-011, §9 "Missed events".
 * TanStack pauses the interval while the tab is backgrounded.
 */
export function useToday(orgId: MaybeRefOrGetter<string>) {
  return useQuery({
    queryKey: computed(() => queryKeys.today(toValue(orgId))),
    queryFn: () => apiFetch<TodayResponse>('/today'),
    enabled: computed(() => toValue(orgId) !== ''),
    refetchInterval: 60_000,
  })
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
    onSuccess: (data) => {
      qc.setQueryData(queryKeys.me, data)
    },
  })
}

export function useLogoutMutation() {
  return useMutation({
    mutationFn: () => apiFetch<void>('/session', { method: 'DELETE' }),
    onSuccess: () => {
      // Full reset, not just the me query: the next login may be a
      // different user in a different Organization, and every org-scoped
      // key is namespaced off the org id (see queryKeys above) so stale
      // entries would otherwise sit in the cache unreachable but present.
      queryClient.clear()
    },
  })
}

export function useAssignPersonMutation(orgId: MaybeRefOrGetter<string>) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ personId, assignedUserId }: { personId: string; assignedUserId: string | null }) =>
      apiFetch<MutatePersonResponse>(`/people/${personId}/assignment`, {
        method: 'POST',
        body: JSON.stringify({ assigned_user_id: assignedUserId } satisfies AssignmentRequest),
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

export function useChangeStageMutation(orgId: MaybeRefOrGetter<string>) {
  const qc = useQueryClient()
  return useMutation({
    mutationFn: ({ personId, stageId }: { personId: string; stageId: string }) =>
      apiFetch<MutatePersonResponse>(`/people/${personId}/stage`, {
        method: 'POST',
        body: JSON.stringify({ stage_id: stageId } satisfies StageRequest),
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
    onSuccess: (data) => {
      qc.setQueryData(queryKeys.me, data)
    },
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
