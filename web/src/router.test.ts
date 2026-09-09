import { createMemoryHistory } from 'vue-router'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { ApiError } from './api/client'
import { queryClient } from './query-client'
import type { MeResponse } from './api/types'

// Partial mock (SLICE_003 useRealtime.test.ts's pattern): keep the real
// `queryKeys` factory (router.ts's guard depends on its exact key shape),
// replace only `fetchMe` so each test controls what the "session" resolves
// to without a network call. `prefetchTodayData` is ALSO replaced here —
// SLICE_014 §4's guard calls the real one on every Today navigation, and
// the real one calls the real (unmocked in this file) `apiFetch`, which
// would otherwise issue a genuine network request to whatever origin this
// test environment resolves `/api` against every time one of the many
// existing navigation tests below lands on /today. The dedicated
// "Today prefetch" describe below restores the real implementation only
// for its own assertions.
vi.mock('./api/queries', async (importOriginal) => {
  const actual = await importOriginal<typeof import('./api/queries')>()
  return { ...actual, fetchMe: vi.fn(), prefetchTodayData: vi.fn() }
})

const { fetchMe, prefetchTodayData } = await import('./api/queries')
const { createAppRouter } = await import('./router')

function meResponse(overrides: Partial<MeResponse>): MeResponse {
  return {
    user: { id: 'u1', email: 'alice@acme.test', display_name: 'Alice' },
    organization: { id: 'org1', name: 'Acme Realty', role: 'member' },
    platform_admin: false,
    ...overrides,
  }
}

// SLICE_004 §10's three session shapes, plus the composed "both" case.
const MEMBER = meResponse({})
const ADMIN = meResponse({ organization: { id: 'org1', name: 'Acme Realty', role: 'admin' } })
const PLATFORM_ONLY = meResponse({ organization: null, platform_admin: true })
const BOTH = meResponse({ organization: { id: 'org1', name: 'Acme Realty', role: 'admin' }, platform_admin: true })
// Not one of §10's three shapes — session::verify's invariant (§3) forbids
// it — but the guard must still fail closed rather than loop (router.ts
// comment).
const BROKEN = meResponse({ organization: null, platform_admin: false })

function freshRouter() {
  return createAppRouter(createMemoryHistory())
}

beforeEach(() => {
  queryClient.clear()
  vi.mocked(fetchMe).mockReset()
  vi.mocked(prefetchTodayData).mockReset()
})

describe('router guards (SLICE_004 §10)', () => {
  it('sends an unauthenticated visitor to /login, preserving the intended destination', async () => {
    // Persistent, not "Once": the guard re-runs (and re-fetches `me`, since
    // the query has no cached data after a 401) for the redirect target
    // (/login) within the same `router.push` — two `fetchMe` calls, not one.
    vi.mocked(fetchMe).mockRejectedValue(new ApiError(401, 'unauthenticated'))
    const router = freshRouter()
    await router.push('/today')
    expect(router.currentRoute.value.path).toBe('/login')
    expect(router.currentRoute.value.query.redirect).toBe('/today')
  })

  it('lets a non-401 failure (503 unavailable) through so the view can surface it', async () => {
    // Persistent, not "Once": query-client.ts's default retry policy gives
    // any non-401 error one retry, so the queryFn runs twice even though
    // the guard itself only runs once here (single navigation, no redirect).
    vi.mocked(fetchMe).mockRejectedValue(new ApiError(503, 'unavailable'))
    const router = freshRouter()
    await router.push('/today')
    expect(router.currentRoute.value.path).toBe('/today')
  })

  it('replays a protected route through role authorization after a paused session recovers', async () => {
    const lifecycle = await import('./sessionLifecycle')
    const settleAs = async (
      target: '/manage/members' | '/platform',
      identity: MeResponse,
      expected: string,
    ) => {
      const epoch = lifecycle.beginSessionTransition()
      const router = freshRouter()
      await router.push(target)
      expect(router.currentRoute.value.path).toBe(target)

      // The replay's real authorization guard waits for this deferred `/me`.
      // Its flag must cover that wait, so AppShell's paired component test
      // cannot mount a restricted slot between session recovery and redirect.
      let resolveAuthorization!: (identity: MeResponse) => void
      const authorization = new Promise<MeResponse>((resolve) => { resolveAuthorization = resolve })
      const callsBeforeReplay = vi.mocked(fetchMe).mock.calls.length
      vi.mocked(fetchMe).mockImplementation(() => authorization)
      window.localStorage.removeItem(`crm.session-lifecycle.v1.pending.${epoch}`)
      expect(lifecycle.completeSessionVerification(lifecycle.currentSessionGeneration())).toBe(true)
      await vi.waitFor(() => expect(vi.mocked(fetchMe).mock.calls.length).toBeGreaterThan(callsBeforeReplay))
      expect(lifecycle.useRouteAuthorizationReplayPending().value).toBe(true)
      expect(router.currentRoute.value.path).toBe(target)

      resolveAuthorization(identity)
      await vi.waitFor(() => expect(router.currentRoute.value.path).toBe(expected))
      await vi.waitFor(() => expect(lifecycle.useRouteAuthorizationReplayPending().value).toBe(false))
    }

    await settleAs('/manage/members', MEMBER, '/today')
    await settleAs('/platform', MEMBER, '/today')
    await settleAs('/manage/members', ADMIN, '/manage/members')
  })

  it('replays the URL a direct load resolved to, not the start location, when recovery settles during the initial navigation', async () => {
    // Regression: the replay read `router.currentRoute` while the very first
    // navigation was still in flight, replayed the start location `/`, and
    // its redirect sent every direct load of a protected URL to /today.
    const lifecycle = await import('./sessionLifecycle')
    const epoch = lifecycle.beginSessionTransition()
    vi.mocked(fetchMe).mockResolvedValue(MEMBER)
    const router = freshRouter()
    const initialNavigation = router.push('/people')
    // Verification completes synchronously, before the initial navigation
    // has resolved its guards.
    window.localStorage.removeItem(`crm.session-lifecycle.v1.pending.${epoch}`)
    expect(lifecycle.completeSessionVerification(lifecycle.currentSessionGeneration())).toBe(true)
    await initialNavigation
    await vi.waitFor(() => expect(lifecycle.useRouteAuthorizationReplayPending().value).toBe(false))
    expect(router.currentRoute.value.path).toBe('/people')
  })

  it('re-runs role authorization when a second session boundary completes during an in-flight replay (F4)', async () => {
    const lifecycle = await import('./sessionLifecycle')
    const epoch1 = lifecycle.beginSessionTransition()
    const router = freshRouter()
    const replaceSpy = vi.spyOn(router, 'replace')
    await router.push('/manage/members')
    expect(router.currentRoute.value.path).toBe('/manage/members')

    // Each `fetchMe` call gets its own independently resolvable promise, so
    // each replay's role-authorization request can be observed and
    // controlled separately.
    const deferredAuth: Array<{ resolve: (identity: MeResponse) => void }> = []
    vi.mocked(fetchMe).mockImplementation(() => new Promise((resolve) => { deferredAuth.push({ resolve }) }))

    window.localStorage.removeItem(`crm.session-lifecycle.v1.pending.${epoch1}`)
    expect(lifecycle.completeSessionVerification(lifecycle.currentSessionGeneration())).toBe(true)
    await vi.waitFor(() => expect(deferredAuth).toHaveLength(1))
    expect(replaceSpy).toHaveBeenCalledTimes(1)
    expect(lifecycle.useRouteAuthorizationReplayPending().value).toBe(true)

    // A second, independent boundary (e.g. another tab's login) completes
    // while the first replay's role check is still outstanding. The mutex
    // must remember it rather than silently drop it.
    const epoch2 = lifecycle.beginSessionTransition()
    window.localStorage.removeItem(`crm.session-lifecycle.v1.pending.${epoch2}`)
    expect(lifecycle.completeSessionVerification(lifecycle.currentSessionGeneration())).toBe(true)
    expect(replaceSpy).toHaveBeenCalledTimes(1) // not yet — the first replay is still in flight
    expect(lifecycle.useRouteAuthorizationReplayPending().value).toBe(true) // stays raised throughout

    // Resolving the first (now stale) role check must trigger a SECOND
    // replay whose own role guard actually runs, rather than admitting the
    // route on the first replay's stale authorization alone.
    deferredAuth[0]!.resolve(ADMIN)
    await vi.waitFor(() => expect(deferredAuth).toHaveLength(2))
    expect(replaceSpy).toHaveBeenCalledTimes(2)
    expect(lifecycle.useRouteAuthorizationReplayPending().value).toBe(true)

    deferredAuth[1]!.resolve(MEMBER)
    await vi.waitFor(() => expect(router.currentRoute.value.path).toBe('/today'))
    await vi.waitFor(() => expect(lifecycle.useRouteAuthorizationReplayPending().value).toBe(false))
  })

  it('synchronizes storage before the public-route check, so an unread changing marker does not fall through to fetchMe and park (F5)', async () => {
    const PENDING_KEY = 'crm.session-lifecycle.v1.pending.f5-foreign-epoch'
    const MARKER_KEY = 'crm.session-lifecycle.v1'
    // Written directly, exactly as another tab would durably record it —
    // deliberately NOT dispatched as a `storage` event to this window,
    // matching the real-browser fact that `storage` events never fire in
    // the tab that made the write.
    window.localStorage.setItem(PENDING_KEY, 'f5-foreign-epoch')
    window.localStorage.setItem(MARKER_KEY, JSON.stringify({
      epoch: 'f5-foreign-epoch',
      sequence: Date.now() + 1_000_000,
      phase: 'changing',
    }))

    const router = freshRouter()
    await router.push('/login')

    expect(router.currentRoute.value.path).toBe('/login')
    expect(fetchMe).not.toHaveBeenCalled()

    // Clean up so later tests do not inherit a blocked lifecycle: remove the
    // foreign pending key and directly complete verification for the
    // current generation (mirroring the replay test's own cleanup style —
    // `completeSessionVerification` only requires no pending transitions
    // remain, so no settled marker needs publishing here).
    window.localStorage.removeItem(PENDING_KEY)
    const lifecycle = await import('./sessionLifecycle')
    expect(lifecycle.completeSessionVerification(lifecycle.currentSessionGeneration())).toBe(true)
  })


  describe('member session', () => {
    it('reaches tenant routes', async () => {
      vi.mocked(fetchMe).mockResolvedValue(MEMBER)
      const router = freshRouter()
      await router.push('/today')
      expect(router.currentRoute.value.path).toBe('/today')
    })

    it('uses the Elysium CRM product name in document titles', async () => {
      vi.mocked(fetchMe).mockResolvedValue(MEMBER)
      const router = freshRouter()
      await router.push('/people')
      expect(document.title).toBe('People · Elysium CRM')
    })

    it('is bounced from /manage/members to /today (not an admin)', async () => {
      vi.mocked(fetchMe).mockResolvedValue(MEMBER)
      const router = freshRouter()
      await router.push('/manage/members')
      expect(router.currentRoute.value.path).toBe('/today')
    })

    it('is bounced from /manage/intake to /today (not an admin; SLICE_007a §6)', async () => {
      vi.mocked(fetchMe).mockResolvedValue(MEMBER)
      const router = freshRouter()
      await router.push('/manage/intake')
      expect(router.currentRoute.value.path).toBe('/today')
    })

    it('is bounced from /manage/today-feeds to /today (not an admin; SLICE_011d §6)', async () => {
      vi.mocked(fetchMe).mockResolvedValue(MEMBER)
      const router = freshRouter()
      await router.push('/manage/today-feeds')
      expect(router.currentRoute.value.path).toBe('/today')
    })

    it('is bounced from /platform to /today (not a platform admin)', async () => {
      vi.mocked(fetchMe).mockResolvedValue(MEMBER)
      const router = freshRouter()
      await router.push('/platform')
      expect(router.currentRoute.value.path).toBe('/today')
    })

    // SLICE_011e §5: /manage/tags carries NO requiresOrgAdmin meta — rule
    // 1 (D-051) lets any member create/apply/remove tags and manage their
    // own unused ones, unlike its three admin-only siblings above.
    it('reaches /manage/tags (a member route, not bounced)', async () => {
      vi.mocked(fetchMe).mockResolvedValue(MEMBER)
      const router = freshRouter()
      await router.push('/manage/tags')
      expect(router.currentRoute.value.path).toBe('/manage/tags')
    })

    it('is redirected away from /login (already signed in)', async () => {
      vi.mocked(fetchMe).mockResolvedValue(MEMBER)
      const router = freshRouter()
      await router.push('/login')
      expect(router.currentRoute.value.path).toBe('/today')
    })
  })

  describe('admin session', () => {
    it('reaches /manage/members', async () => {
      vi.mocked(fetchMe).mockResolvedValue(ADMIN)
      const router = freshRouter()
      await router.push('/manage/members')
      expect(router.currentRoute.value.path).toBe('/manage/members')
    })

    it('reaches /manage/intake (SLICE_007a §6)', async () => {
      vi.mocked(fetchMe).mockResolvedValue(ADMIN)
      const router = freshRouter()
      await router.push('/manage/intake')
      expect(router.currentRoute.value.path).toBe('/manage/intake')
    })

    it('reaches /manage/today-feeds (SLICE_011d §6)', async () => {
      vi.mocked(fetchMe).mockResolvedValue(ADMIN)
      const router = freshRouter()
      await router.push('/manage/today-feeds')
      expect(router.currentRoute.value.path).toBe('/manage/today-feeds')
    })

    it('is still bounced from /platform (not a platform admin)', async () => {
      vi.mocked(fetchMe).mockResolvedValue(ADMIN)
      const router = freshRouter()
      await router.push('/platform')
      expect(router.currentRoute.value.path).toBe('/today')
    })
  })

  describe('platform-only session (organization: null)', () => {
    it('reaches /platform', async () => {
      vi.mocked(fetchMe).mockResolvedValue(PLATFORM_ONLY)
      const router = freshRouter()
      await router.push('/platform')
      expect(router.currentRoute.value.path).toBe('/platform')
    })

    it('reaches /platform/organizations/:id', async () => {
      vi.mocked(fetchMe).mockResolvedValue(PLATFORM_ONLY)
      const router = freshRouter()
      await router.push('/platform/organizations/org2')
      expect(router.currentRoute.value.path).toBe('/platform/organizations/org2')
    })

    it('is redirected from every tenant route to /platform', async () => {
      vi.mocked(fetchMe).mockResolvedValue(PLATFORM_ONLY)
      const router = freshRouter()
      await router.push('/today')
      expect(router.currentRoute.value.path).toBe('/platform')
    })

    it('is redirected from /manage/members to /platform', async () => {
      vi.mocked(fetchMe).mockResolvedValue(PLATFORM_ONLY)
      const router = freshRouter()
      await router.push('/manage/members')
      expect(router.currentRoute.value.path).toBe('/platform')
    })

    it('lands on /platform when already signed in and visiting /login', async () => {
      vi.mocked(fetchMe).mockResolvedValue(PLATFORM_ONLY)
      const router = freshRouter()
      await router.push('/login')
      expect(router.currentRoute.value.path).toBe('/platform')
    })

    it('does not open a tenant route even via the root redirect', async () => {
      vi.mocked(fetchMe).mockResolvedValue(PLATFORM_ONLY)
      const router = freshRouter()
      await router.push('/')
      expect(router.currentRoute.value.path).toBe('/platform')
    })
  })

  describe('a user who is both an Organization admin and a platform admin', () => {
    it('reaches /manage/members', async () => {
      vi.mocked(fetchMe).mockResolvedValue(BOTH)
      const router = freshRouter()
      await router.push('/manage/members')
      expect(router.currentRoute.value.path).toBe('/manage/members')
    })

    it('reaches /platform', async () => {
      vi.mocked(fetchMe).mockResolvedValue(BOTH)
      const router = freshRouter()
      await router.push('/platform')
      expect(router.currentRoute.value.path).toBe('/platform')
    })

    it('is not bounced off /today (still has an Organization)', async () => {
      vi.mocked(fetchMe).mockResolvedValue(BOTH)
      const router = freshRouter()
      await router.push('/today')
      expect(router.currentRoute.value.path).toBe('/today')
    })
  })

  describe('the invite page', () => {
    it('is reachable while unauthenticated', async () => {
      vi.mocked(fetchMe).mockRejectedValueOnce(new ApiError(401, 'unauthenticated'))
      const router = freshRouter()
      await router.push('/invite/sometoken')
      expect(router.currentRoute.value.path).toBe('/invite/sometoken')
    })

    it('does not redirect an already-signed-in visitor away (allowAuthenticated)', async () => {
      vi.mocked(fetchMe).mockResolvedValue(MEMBER)
      const router = freshRouter()
      await router.push('/invite/sometoken')
      expect(router.currentRoute.value.path).toBe('/invite/sometoken')
    })

    it('also stays open for a platform-only visitor', async () => {
      vi.mocked(fetchMe).mockResolvedValue(PLATFORM_ONLY)
      const router = freshRouter()
      await router.push('/invite/sometoken')
      expect(router.currentRoute.value.path).toBe('/invite/sometoken')
    })
  })

  describe('a session shape outside SLICE_004 §10 (organization: null, platform_admin: false)', () => {
    it('fails closed to /login instead of looping between /today and /platform', async () => {
      vi.mocked(fetchMe).mockResolvedValue(BROKEN)
      const router = freshRouter()
      await router.push('/today')
      expect(router.currentRoute.value.path).toBe('/login')
    })
  })

  // SLICE_014 §4, §8.6: the guard calls `prefetchTodayData` as soon as
  // identity resolves for a Today target with an Organization, and only
  // then. `prefetchTodayData` itself (its request shape, its keys) is
  // covered directly in queries.test.ts; this only checks the guard calls
  // it at the right time, with the right session, and never elsewhere.
  describe('Today prefetch (SLICE_014 §4)', () => {
    it('calls prefetchTodayData with the resolved session once a member reaches /today', async () => {
      vi.mocked(fetchMe).mockResolvedValue(MEMBER)
      const router = freshRouter()
      await router.push('/today')
      expect(router.currentRoute.value.path).toBe('/today')
      expect(prefetchTodayData).toHaveBeenCalledTimes(1)
      expect(prefetchTodayData).toHaveBeenCalledWith(queryClient, MEMBER)
    })

    it('calls it again reaching /today via the root redirect', async () => {
      vi.mocked(fetchMe).mockResolvedValue(ADMIN)
      const router = freshRouter()
      await router.push('/')
      expect(router.currentRoute.value.path).toBe('/today')
      expect(prefetchTodayData).toHaveBeenCalledTimes(1)
    })

    it('does not call it for a non-Today target', async () => {
      vi.mocked(fetchMe).mockResolvedValue(MEMBER)
      const router = freshRouter()
      await router.push('/people')
      expect(router.currentRoute.value.path).toBe('/people')
      expect(prefetchTodayData).not.toHaveBeenCalled()
    })

    it('does not call it for a platform-only session redirected away from /today', async () => {
      vi.mocked(fetchMe).mockResolvedValue(PLATFORM_ONLY)
      const router = freshRouter()
      await router.push('/today')
      expect(router.currentRoute.value.path).toBe('/platform')
      expect(prefetchTodayData).not.toHaveBeenCalled()
    })

    it('does not call it for an unauthenticated visitor bounced to /login', async () => {
      vi.mocked(fetchMe).mockRejectedValue(new ApiError(401, 'unauthenticated'))
      const router = freshRouter()
      await router.push('/today')
      expect(router.currentRoute.value.path).toBe('/login')
      expect(prefetchTodayData).not.toHaveBeenCalled()
    })
  })
})
