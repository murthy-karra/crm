import { useMutation, useQuery, useQueryClient } from '@tanstack/vue-query'
import { type MaybeRefOrGetter, computed, toValue } from 'vue'
import { queryClient } from '../query-client'
import { apiFetch } from './client'
import type {
  AssignmentRequest,
  MeResponse,
  MembersResponse,
  MutatePersonResponse,
  PeopleResponse,
  PersonDetailResponse,
  ReceiveInquiryRequest,
  ReceiveInquiryResponse,
  StageRequest,
  StagesResponse,
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
