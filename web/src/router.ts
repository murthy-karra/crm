import { createRouter, createWebHistory, type RouteRecordRaw } from 'vue-router'
import { fetchMe, queryKeys } from './api/queries'
import { ApiError } from './api/client'
import { queryClient, setUnauthorizedHandler } from './query-client'

declare module 'vue-router' {
  interface RouteMeta {
    /** Skips the auth gate below and redirects an already-signed-in visitor away. */
    public?: boolean
  }
}

const routes: RouteRecordRaw[] = [
  {
    path: '/login',
    name: 'login',
    component: () => import('./views/LoginView.vue'),
    meta: { public: true },
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
  { path: '/', redirect: '/people' },
  { path: '/:pathMatch(.*)*', redirect: '/people' },
]

export const router = createRouter({
  history: createWebHistory(),
  routes,
})

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
// de-dupes concurrent navigations. A 401 ApiError sends an unauthenticated
// visitor to /login (preserving where they were headed); any other error
// (503 `unavailable`, network) is left for the view itself to surface, since
// redirecting to /login on a database outage would just fail again there.
router.beforeEach(async (to) => {
  let authenticated = true
  try {
    await queryClient.ensureQueryData({ queryKey: queryKeys.me, queryFn: fetchMe })
  } catch (error) {
    if (error instanceof ApiError && error.status === 401) {
      authenticated = false
    } else if (error instanceof ApiError) {
      // Non-auth failure (e.g. 503 unavailable): let navigation proceed so
      // the destination view's own query can show an error state.
      return true
    } else {
      throw error
    }
  }

  if (to.meta.public) {
    // Already signed in and heading to /login — send them where a signed-in
    // visitor belongs instead of showing the login form again.
    return authenticated ? { path: '/people' } : true
  }

  if (!authenticated) {
    return { path: '/login', query: to.fullPath !== '/' ? { redirect: to.fullPath } : undefined }
  }

  return true
})
