import { workspaceOperational } from './workspaceLifecycle'
import { watch } from 'vue'
import {
  START_LOCATION,
  createRouter,
  createWebHistory,
  type Router,
  type RouteRecordRaw,
  type RouterHistory,
} from 'vue-router'
import { fetchMe, prefetchTodayData, queryKeys } from './api/queries'
import { ApiError } from './api/client'
import {
  isSessionVerified,
  SessionCoordinationUnavailableError,
  SessionVerificationPendingError,
  setRouteAuthorizationReplayPending,
  useSessionCoordinationUnavailable,
  useSessionVerificationPending,
} from './sessionLifecycle'
import { queryClient, setUnauthorizedHandler } from './query-client'
import { preloadTodayView } from './preload'
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
    /** Document title, rendered as "<title> · Elysium CRM". */
    title?: string
  }
}

function routes(): RouteRecordRaw[] {
  return [
    { path: '/workspace-review', name: 'workspace-review', component: () => import('./views/WorkspaceReviewView.vue'), meta: { title: 'Workspace under review' } },
    {
      path: '/login',
      name: 'login',
      component: () => import('./views/LoginView.vue'),
      meta: { public: true, title: 'Sign in' },
    },
    {
      // SLICE_004 §5/§10: the token is the credential, never a query string
      // (it would leak into logs); it travels in the JSON body of the
      // preview/accept calls, read here only from `route.params`.
      path: '/invite/:token',
      name: 'invite',
      component: () => import('./views/InviteView.vue'),
      props: true,
      meta: { public: true, allowAuthenticated: true, title: 'Invitation' },
    },
    {
      // SLICE_003 §14: Today is the landing route after login.
      path: '/today',
      name: 'today',
      // SLICE_014 §4: shared with LoginView's onMounted preload — the same
      // dynamic import, so whichever call comes second just resolves from
      // Vite's module cache instead of importing twice.
      component: preloadTodayView,
      meta: { title: 'Today' },
    },
    {
      path: '/people',
      name: 'people',
      component: () => import('./views/PeopleView.vue'),
      meta: { title: 'People' },
    },
    {
      path: '/lists',
      name: 'saved-lists',
      component: () => import('./views/ListsView.vue'),
      meta: { title: 'Lists' },
    },
    {
      path: '/lists/:savedListId',
      name: 'saved-list',
      component: () => import('./views/PeopleView.vue'),
      props: (route) => ({ savedListId: String(route.params.savedListId) }),
      meta: { title: 'Saved list' },
    },
    {
      path: '/people/:id',
      name: 'person-detail',
      component: () => import('./views/PersonDetailView.vue'),
      props: true,
      meta: { title: 'Person' },
    },
    {
      path: '/intake/new',
      name: 'new-inquiry',
      component: () => import('./views/NewInquiryView.vue'),
      meta: { title: 'New lead' },
    },
    {
      // SLICE_009 §8: the agent's own capture address + unmatched queue —
      // member-self, deliberately NOT under /manage (admin surface).
      path: '/email-capture',
      name: 'email-capture',
      component: () => import('./views/EmailCaptureView.vue'),
      meta: { title: 'Email capture' },
    },
    {
      path: '/intake/unresolved',
      name: 'unresolved',
      component: () => import('./views/UnresolvedView.vue'),
      meta: { title: 'Unresolved' },
    },
    {
      path: '/manage/members',
      name: 'manage-members',
      component: () => import('./views/MembersView.vue'),
      meta: { requiresOrgAdmin: true, title: 'Members' },
    },
    {
      path: '/manage/intake',
      name: 'manage-intake',
      component: () => import('./views/IntakeSettingsView.vue'),
      meta: { requiresOrgAdmin: true, title: 'Intake' },
    },
    {
      // SLICE_011d §6: "Manage → Today rules ... admin-only route meta,
      // nav beside Intake and Members" — same `requiresOrgAdmin` pattern as
      // the two routes above.
      path: '/manage/today-feeds',
      name: 'manage-today-feeds',
      component: () => import('./views/TodayFeedsView.vue'),
      meta: { requiresOrgAdmin: true, title: 'Today rules' },
    },
    {
      // SLICE_011e §5: a MEMBER route — deliberately NO `requiresOrgAdmin`.
      // Rule 1 (D-051) lets any member create/apply/remove tags and manage
      // their own unused ones; the page itself decides per-row controls
      // from each tag's `can_manage`.
      path: '/manage/tags',
      name: 'manage-tags',
      component: () => import('./views/TagsView.vue'),
      meta: { title: 'Tags' },
    },
    {
      // SLICE_019.md §7: definitions/options are Organization-admin only
      // (D-058 §2) — unlike Tags above, this IS `requiresOrgAdmin`, the
      // Members/Intake pattern.
      path: '/manage/fields',
      name: 'manage-fields',
      component: () => import('./views/FieldsView.vue'),
      meta: { requiresOrgAdmin: true, title: 'Fields' },
    },
    {
      path: '/manage/migration',
      name: 'manage-migration',
      component: () => import('./views/MigrationView.vue'),
      meta: { requiresOrgAdmin: true, title: 'Migration' },
    },
    {
      path: '/platform',
      name: 'platform-organizations',
      component: () => import('./views/PlatformOrganizationsView.vue'),
      meta: { requiresPlatformAdmin: true, title: 'Organizations' },
    },
    {
      path: '/platform/organizations/:id',
      name: 'platform-organization',
      component: () => import('./views/PlatformOrganizationView.vue'),
      props: true,
      meta: { requiresPlatformAdmin: true, title: 'Organization' },
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

  // A protected route may deliberately reach AppShell while a cross-tab
  // session replacement is unresolved, so its fail-closed recovery copy can
  // render instead of leaving initial navigation blank. Once verification
  // settles, force this same route back through the normal authorization
  // guard; otherwise a member paused on `/manage/*` could mount its route
  // component after recovery without the role redirect below running again.
  let replayingAfterSessionRecovery = false
  // F4: a second session boundary (e.g. another tab's login) can complete
  // while this replay's own role-authorization request is still in flight.
  // Dropping it silently would let the first replay's guard admit the
  // route on stale authorization. Remember it here and re-run once the
  // in-flight replay finishes, keeping the flag raised continuously across
  // both so AppShell's paused state never blips open in between.
  let replayRequested = false
  const lifecyclePending = useSessionVerificationPending()
  const lifecycleUnavailable = useSessionCoordinationUnavailable()
  function replayCurrentRoute() {
    replayingAfterSessionRecovery = true
    setRouteAuthorizationReplayPending(true)
    // Verification can settle while the very first navigation is still in
    // flight (a direct load of a protected URL). `currentRoute` is then still
    // the start location, and replaying it would follow the `/` redirect to
    // Today instead of re-authorizing the URL the user actually opened. Wait
    // for the initial navigation, then replay whatever it resolved to; if it
    // never resolved there is nothing rendered to re-authorize.
    void router.isReady().catch(() => {}).then(() => {
      const current = router.currentRoute.value
      if (current === START_LOCATION) return undefined
      return router.replace({
        path: current.path,
        query: current.query,
        hash: current.hash,
        force: true,
      })
    }).catch(() => {}).finally(() => {
      if (replayRequested) {
        replayRequested = false
        replayCurrentRoute()
        return
      }
      replayingAfterSessionRecovery = false
      setRouteAuthorizationReplayPending(false)
    })
  }
  watch(
    [lifecyclePending, lifecycleUnavailable],
    ([pending, unavailable], [wasPending, wasUnavailable]) => {
      if (pending || unavailable || (!wasPending && !wasUnavailable)) return
      if (replayingAfterSessionRecovery) {
        replayRequested = true
        return
      }
      replayCurrentRoute()
    },
    { flush: 'sync' },
  )

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
    // F5: synchronize storage before EITHER branch below reads pending/
    // unavailable state. A `changing` marker durably written by another
    // tab can be observable here before that tab's own `storage` event
    // listener fires in this one; reading a stale ref first let a public
    // route (e.g. /login) fall through to `fetchMe` and park forever.
    isSessionVerified()
    // A public sign-in/recovery route remains reachable while a different
    // tab's opaque auth attempt is unfinished. Private routes below stay
    // paused until that marker actually settles.
    if (to.meta.public && useSessionVerificationPending().value) return true
    // `main.ts` waits for the first navigation before mounting AppShell. If
    // another tab left a durable auth attempt unfinished, waiting for `/me`
    // here would keep a direct protected navigation permanently blank. Let
    // AppShell mount its fail-closed recovery shell instead.
    if (!to.meta.public && (useSessionVerificationPending().value || useSessionCoordinationUnavailable().value)) {
      return true
    }
    let me: MeResponse | undefined
    try {
      me = await queryClient.ensureQueryData({ queryKey: queryKeys.me, queryFn: ({ signal }) => fetchMe(signal) })
    } catch (error) {
      if (error instanceof ApiError && error.status === 401) {
        me = undefined
      } else if (error instanceof SessionVerificationPendingError || error instanceof SessionCoordinationUnavailableError) {
        return true
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

    if (session.organization.workspace_mode === 'migration_review' && !to.meta.requiresPlatformAdmin) {
      if (session.organization.role !== 'admin') return to.name === 'workspace-review' ? true : { path: '/workspace-review' }
      const allowed = ['people', 'person-detail', 'manage-migration', 'manage-members']
      if (!allowed.includes(String(to.name))) return { path: '/manage/migration' }
    } else if (to.name === 'workspace-review') return { path: '/today' }

    if (to.meta.requiresOrgAdmin && session.organization.role !== 'admin') {
      return { path: '/today' }
    }

    if (to.meta.requiresPlatformAdmin && !session.platform_admin) {
      return { path: '/today' }
    }

    // SLICE_014 §4: as soon as identity is known for a Today target — well
    // before the Today route chunk (preloaded from login, above) has
    // necessarily finished importing — start Today's three data requests.
    // Every earlier return above (public routes, the pending/unavailable
    // early returns, unauthenticated, platform-only) has already exited
    // before this point, so nothing is prefetched for any of those cases.
    if (to.name === 'today' && workspaceOperational(session)) {
      prefetchTodayData(queryClient, session)
    }

    return true
  })

  router.afterEach((to) => {
    if (typeof document !== 'undefined') {
      document.title = to.meta.title ? `${to.meta.title} · Elysium CRM` : 'Elysium CRM'
    }
  })

  return router
}

export const router = createAppRouter(createWebHistory())
