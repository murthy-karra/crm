import {
  createRouter,
  createWebHistory,
  type Router,
  type RouteRecordRaw,
  type RouterHistory,
} from 'vue-router'
import { fetchMe, queryKeys } from './api/queries'
import { ApiError } from './api/client'
import { queryClient, setUnauthorizedHandler } from './query-client'
import type { MeResponse } from './api/types'

declare module 'vue-router' {
  interface RouteMeta {
    /** Skips the auth gate below and redirects an already-signed-in visitor away. */
    public?: boolean
    /** SLICE_004 §10: the public route's one exception — a signed-in visitor
     * is not redirected away (they may be accepting a second invitation in
     * a private window). Only meaningful when `public` is also set. */
    allowAuthenticated?: boolean
    /** SLICE_004 §10: `/manage/*` — requires `organization.role === 'admin'`. */
    requiresOrgAdmin?: boolean
    /** SLICE_004 §10: `/platform/*` — requires `platform_admin === true`. */
    requiresPlatformAdmin?: boolean
  }
}

function routes(): RouteRecordRaw[] {
  return [
    {
      path: '/login',
      name: 'login',
      component: () => import('./views/LoginView.vue'),
      meta: { public: true },
    },
    {
      // SLICE_004 §5/§10: the token is the credential, never a query string
      // (it would leak into logs); it travels in the JSON body of the
      // preview/accept calls, read here only from `route.params`.
      path: '/invite/:token',
      name: 'invite',
      component: () => import('./views/InviteView.vue'),
      props: true,
      meta: { public: true, allowAuthenticated: true },
    },
    {
      // SLICE_003 §14: Today is the landing route after login.
      path: '/today',
      name: 'today',
      component: () => import('./views/TodayView.vue'),
    },
    {
      path: '/people',
      name: 'people',
      component: () => import('./views/PeopleView.vue'),
    },
    {
      path: '/people/:id',
      name: 'person-detail',
      component: () => import('./views/PersonDetailView.vue'),
      props: true,
    },
    {
      path: '/intake/new',
      name: 'new-inquiry',
      component: () => import('./views/NewInquiryView.vue'),
    },
    {
      path: '/intake/unresolved',
      name: 'unresolved',
      component: () => import('./views/UnresolvedView.vue'),
    },
    {
      path: '/manage/members',
      name: 'manage-members',
      component: () => import('./views/MembersView.vue'),
      meta: { requiresOrgAdmin: true },
    },
    {
      path: '/platform',
      name: 'platform-organizations',
      component: () => import('./views/PlatformOrganizationsView.vue'),
      meta: { requiresPlatformAdmin: true },
    },
    {
      path: '/platform/organizations/:id',
      name: 'platform-organization',
      component: () => import('./views/PlatformOrganizationView.vue'),
      props: true,
      meta: { requiresPlatformAdmin: true },
    },
    { path: '/', redirect: '/today' },
    { path: '/:pathMatch(.*)*', redirect: '/today' },
  ]
}

/**
 * Builds one configured router instance. Factored out of the `router`
 * singleton below (SLICE_004) so router.test.ts can create isolated
 * instances over `createMemoryHistory()` — the singleton's own
 * `initialNavigationSettled`/`redirectingToLogin` state and its
 * `setUnauthorizedHandler` registration must not leak between tests, and a
 * real `createWebHistory()` is awkward to drive from Vitest.
 */
export function createAppRouter(history: RouterHistory): Router {
  const router = createRouter({ history, routes: routes() })

  // query-client.ts's global QueryCache handler (any 401 outside of
  // navigation — e.g. a background refetch after the session died while the
  // user sat on a page) also redirects to /login. Two safeguards keep it from
  // fighting the beforeEach guard below rather than complementing it:
  //
  // - `initialNavigationSettled`: during the app's very first navigation,
  //   `currentRoute` is still the router's pre-navigation placeholder, not
  //   the real destination, so a redirect built from it would lose the
  //   intended `redirect` target. The guard below already handles that first
  //   navigation correctly (it redirects from `to`, the actual target), so
  //   the global handler stays out of the way until it settles.
  // - `redirectingToLogin`: a mutex so multiple queries 401ing at once (e.g.
  //   several views' queries all invalidated by one focus event) only ever
  //   produce one in-flight navigation, never a pile-up.
  let initialNavigationSettled = false
  void router.isReady().finally(() => {
    initialNavigationSettled = true
  })

  let redirectingToLogin = false
  setUnauthorizedHandler(() => {
    if (!initialNavigationSettled || redirectingToLogin) return
    redirectingToLogin = true
    queryClient.clear()
    const current = router.currentRoute.value
    const redirect = current.path === '/login' ? undefined : current.fullPath
    router
      .push({ path: '/login', query: redirect ? { redirect } : undefined })
      .catch(() => {})
      .finally(() => {
        redirectingToLogin = false
      })
  })

  // Gate every route on the `me` query. `ensureQueryData` returns the cached
  // value without a network round-trip once a session is known-good, and
  // de-dupes concurrent navigations. A 401 ApiError means "not authenticated"
  // (`me = undefined` below); any other error (503 `unavailable`, network) is
  // left for the view itself to surface, since redirecting to /login on a
  // database outage would just fail again there.
  //
  // SLICE_004 §10 session shapes this covers: an authenticated member or
  // admin has `organization != null`; a platform-only session has
  // `organization: null, platform_admin: true`; a user can be both. When a
  // guard branch below returns a route location, Vue Router re-runs this
  // function for that new target — so, e.g., a platform-only visitor
  // bounced off `/login` (public branch, below) lands on `/today` only
  // fleetingly before the `organization === null` branch bounces them again
  // to `/platform`; no branch needs to duplicate another branch's landing
  // logic.
  router.beforeEach(async (to) => {
    let me: MeResponse | undefined
    try {
      me = await queryClient.ensureQueryData({ queryKey: queryKeys.me, queryFn: fetchMe })
    } catch (error) {
      if (error instanceof ApiError && error.status === 401) {
        me = undefined
      } else if (error instanceof ApiError) {
        return true
      } else {
        throw error
      }
    }

    // session::verify's invariant (§3) guarantees `organization === null`
    // implies `platform_admin === true` — the only two valid shapes are
    // "has an Organization" and "platform-only". A `me` that violates it
    // (organization null AND platform_admin false) is not one of §10's
    // three session shapes and cannot come from a spec-conformant backend;
    // folding validity into `authenticated` here, rather than special-casing
    // it further down, means every branch below fails closed to /login the
    // same way an absent session does, instead of the /login and /today
    // redirect targets bouncing off each other forever.
    const authenticated = me !== undefined && (me.organization !== null || me.platform_admin)

    if (to.meta.public) {
      // Already signed in and heading to a public route (e.g. /login) —
      // send them where a signed-in visitor belongs, unless this route
      // explicitly allows a signed-in visitor to stay (the invite page: they
      // may be accepting a second account in a private window).
      if (authenticated && !to.meta.allowAuthenticated) {
        return { path: '/today' }
      }
      return true
    }

    if (!authenticated) {
      return { path: '/login', query: to.fullPath !== '/' ? { redirect: to.fullPath } : undefined }
    }

    // `authenticated` narrows `me` to a defined, valid MeResponse from here on.
    const session = me as MeResponse

    if (session.organization === null) {
      return to.meta.requiresPlatformAdmin ? true : { path: '/platform' }
    }

    if (to.meta.requiresOrgAdmin && session.organization.role !== 'admin') {
      return { path: '/today' }
    }

    if (to.meta.requiresPlatformAdmin && !session.platform_admin) {
      return { path: '/today' }
    }

    return true
  })

  return router
}

export const router = createAppRouter(createWebHistory())
