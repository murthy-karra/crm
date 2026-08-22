import { useMutation, useQuery, useQueryClient } from '@tanstack/vue-query'
import { type MaybeRefOrGetter, computed, toValue } from 'vue'
import { queryClient } from '../query-client'
import { apiFetch } from './client'
import type {
  AcceptInvitationRequest,
  AcceptInvitationResponse,
  AssignmentRequest,
  ChangeMemberRoleRequest,
  ContactChannel,
  ContactOutcome,
  CreateOrganizationRequest,
  CreateOrganizationResponse,
  InvitationPreviewRequest,
  InvitationPreviewResponse,
  InvitationsResponse,
  IssueInvitationRequest,
  IssueInvitationResponse,
  LogContactRequest,
  LogContactResponse,
  MeResponse,
  MemberMutationResponse,
  MembershipRole,
  MembershipStatus,
  MembersResponse,
  MutatePersonResponse,
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
  people: (orgId: string) => ['org', orgId, 'people'] as const,
  person: (orgId: string, personId: string) => ['org', orgId, 'person', personId] as const,
  stages: (orgId: string) => ['org', orgId, 'stages'] as const,
  unresolved: (orgId: string) => ['org', orgId, 'unresolved'] as const,
  members: (orgId: string) => ['org', orgId, 'members'] as const,
  // Added ahead of the Today view (SLICE_003 §10 lists it alongside useToday)
  // because realtime/events.ts's invalidationsFor (Lane B step 1) already
  // needs to name this key — every key an invalidation path touches goes
  // through this factory, never hand-written (SLICE_003 Lane B task brief).
  today: (orgId: string) => ['org', orgId, 'today'] as const,
  // SLICE_004 §10: extend the factory, never hand-write a key.
  invitations: (orgId: string) => ['org', orgId, 'invitations'] as const,
  // Keyed by the raw token, not an org id — the public accept page has no
  // Organization context yet (that is exactly what the preview reveals).
  invitationPreview: (token: string) => ['invitation-preview', token] as const,
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

export function usePeople(orgId: MaybeRefOrGetter<string>) {
  return useQuery({
    queryKey: computed(() => queryKeys.people(toValue(orgId))),
    queryFn: () => apiFetch<PeopleResponse>('/people'),
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
