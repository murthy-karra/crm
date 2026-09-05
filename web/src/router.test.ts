import { createMemoryHistory } from 'vue-router'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { ApiError } from './api/client'
import { queryClient } from './query-client'
import type { MeResponse } from './api/types'

// Partial mock (SLICE_003 useRealtime.test.ts's pattern): keep the real
// `queryKeys` factory (router.ts's guard depends on its exact key shape),
// replace only `fetchMe` so each test controls what the "session" resolves
// to without a network call.
vi.mock('./api/queries', async (importOriginal) => {
  const actual = await importOriginal<typeof import('./api/queries')>()
  return { ...actual, fetchMe: vi.fn() }
})

const { fetchMe } = await import('./api/queries')
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

    it('is bounced from /platform to /today (not a platform admin)', async () => {
      vi.mocked(fetchMe).mockResolvedValue(MEMBER)
      const router = freshRouter()
      await router.push('/platform')
      expect(router.currentRoute.value.path).toBe('/today')
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
})
