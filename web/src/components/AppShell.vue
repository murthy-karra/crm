<script setup lang="ts">
// D-045: compact white navigation; shared session, Operator and call ownership.
import { computed, nextTick, onBeforeUnmount, onMounted, provide, ref, watch, type Component } from 'vue'
import { RouterLink, useRoute, useRouter } from 'vue-router'
import { AtSign, Building2, Inbox, ListChecks, ListFilter, LogOut, Mail, SlidersHorizontal, Sparkles, Sun, Tag as TagIcon, UserCog, UserPlus, Users, Waypoints } from 'lucide-vue-next'
import { useAuthSessionLifetime, useLogoutMutation, useMe } from '../api/queries'
import {
  resetSessionCoordination,
  SessionCoordinationUnavailableError,
  useRouteAuthorizationReplayPending,
  useSessionBlockedByOutstandingAttempt,
  useSessionCoordinationUnavailable,
  useSessionVerificationInFlight,
} from '../sessionLifecycle'
import { initials } from '../lib/format'
import { buttonClasses } from '../lib/controls'
import { describeApiError } from '../lib/errors'
import { isOrganizationRoute, isToggleShortcut } from '../lib/operator'
import OperatorPanel from './OperatorPanel.vue'
import CallHostPanel from './CallHostPanel.vue'
import { createRealtimeClient, resolveRealtimeUrl } from '../realtime/client'
import { useRealtime } from '../realtime/useRealtime'
import { provideCallHost } from '../telephony/callHost'
import { createLiveKitRoom } from '../telephony/client'
import type { CallRoomFactory } from '../telephony/useCall'
import { OPERATOR_LAUNCHER } from '../lib/operatorLauncher'

// SLICE_006b §6: the one call session for the whole app lives here, so
// the Person page's Call button and the Ask drawer's Confirm share it and
// the docked panel (with its D-033 outcome prompt) survives navigation.
// Tests inject a fake room factory.
const props = withDefaults(
  defineProps<{
    createRoom?: CallRoomFactory
  }>(),
  { createRoom: createLiveKitRoom },
)

const router = useRouter()
const route = useRoute()

interface NavItem {
  label: string
  to: string
  icon: Component
}

interface NavGroup {
  label: string
  items: NavItem[]
}

const {
  data: me,
  error: meError,
  isError: meIsError,
  isFetching: meIsFetching,
  refetch: refetchMe,
} = useMe()
const logoutMutation = useLogoutMutation()

provideCallHost({
  orgId: () => me.value?.organization?.id ?? '',
  createRoom: props.createRoom,
})

// SLICE_004 §10: a platform-only session (`organization: null`) renders the
// `Platform` group only — no Today/People/Intake, since it has no
// Organization. An Organization session (member or admin) renders Work /
// Intake, plus `Manage` when the member's role is admin. A user who is both
// gets the Organization groups here and a `Platform` footer link below
// (spec: "A user who is both sees a Platform link in the sidebar footer").
const navGroups = computed<NavGroup[]>(() => {
  if (!me.value) return []
  if (me.value.organization === null) {
    return [
      {
        label: 'Platform',
        items: [{ label: 'Organizations', to: '/platform', icon: Building2 }],
      },
    ]
  }
  const groups: NavGroup[] = [
    {
      label: 'Work',
      items: [
        { label: 'Today', to: '/today', icon: Sun },
        { label: 'People', to: '/people', icon: Users },
        { label: 'Lists', to: '/lists', icon: ListFilter },
        // SLICE_009 §8: the agent's own credential — Work, not Manage
        // (that group is admin/tenant surface only).
        { label: 'Email capture', to: '/email-capture', icon: AtSign },
      ],
    },
    {
      label: 'Intake',
      items: [
        { label: 'New lead', to: '/intake/new', icon: UserPlus },
        { label: 'Unresolved', to: '/intake/unresolved', icon: Inbox },
      ],
    },
  ]
  // SLICE_011e §5: `Tags` is a member route (rule 1/D-051 lets a creator
  // manage their own unused tag) — visible in Manage for every Organization
  // member, unlike the three admin-only entries below it.
  const manageItems: NavItem[] = []
  if (me.value.organization.role === 'admin') {
    manageItems.push(
      { label: 'Members', to: '/manage/members', icon: UserCog },
      { label: 'Intake', to: '/manage/intake', icon: Mail },
      // SLICE_011d §6: "nav beside Intake and Members".
      { label: 'Today rules', to: '/manage/today-feeds', icon: ListChecks },
      // SLICE_019.md §7: definitions/options are admin-only (D-058 §2),
      // unlike Tags below.
      { label: 'Fields', to: '/manage/fields', icon: SlidersHorizontal },
      { label: 'Migration', to: '/manage/migration', icon: Waypoints },
    )
  }
  manageItems.push({ label: 'Tags', to: '/manage/tags', icon: TagIcon })
  groups.push({ label: 'Manage', items: manageItems })
  return groups
})

// SLICE_004 §10: "A user who is both sees a Platform link in the sidebar
// footer." A platform-only session already gets the `Platform` group above
// as its primary nav, so this is only for the composed case (an
// Organization member/admin who is also a platform admin).
const showPlatformFooterLink = computed(() => me.value?.organization !== null && me.value?.platform_admin === true)

// Every authenticated view derives its `orgId` from `me` and keeps its own
// queries `enabled: false` until that resolves — and a disabled TanStack
// query reports `isPending` forever. So a `me` failure that the router
// guard deliberately lets through (router.ts: anything that is not a 401 —
// a 503 `unavailable`, an unreachable API, a cross-origin response the
// browser discards) used to leave every screen on a permanent "Loading…"
// with nothing to click. Surface it here, once, instead of in each view.
//
// Guarded on `me` having no data at all: a background refetch (TanStack's
// `refetchOnWindowFocus`) that fails while the cached session is still good
// must not replace a working screen with an error.
const sessionCoordinationUnavailable = useSessionCoordinationUnavailable()
// F1/F3: distinguish "blocked by another tab's unfinished attempt" (a
// recovery control applies) from "this generation's own /me round trip is
// in flight" (an ordinary, non-alarming wait — no control).
const sessionBlockedByOutstandingAttempt = useSessionBlockedByOutstandingAttempt()
const sessionVerificationInFlight = useSessionVerificationInFlight()
const routeAuthorizationReplayPending = useRouteAuthorizationReplayPending()
const sessionUnavailable = computed(() =>
  sessionCoordinationUnavailable.value || sessionBlockedByOutstandingAttempt.value ||
  sessionVerificationInFlight.value || routeAuthorizationReplayPending.value ||
  (meIsError.value && me.value === undefined),
)

function retrySession() {
  void refetchMe()
}

// F1: user-driven recovery, never a timer — only this click reaches
// `resetSessionCoordination()`. On `SessionCoordinationUnavailableError` the
// reactive `sessionCoordinationUnavailable` ref (set by the same call) takes
// over the box on the next render with its own existing copy/control.
function resetSession() {
  try {
    resetSessionCoordination()
  } catch (error) {
    if (!(error instanceof SessionCoordinationUnavailableError)) throw error
  }
}

// AppShell mounts once for every non-public route (App.vue) and stays
// mounted across navigations between them — the single owner of the
// realtime connection SLICE_003 §7 describes: it tears down on unmount and
// whenever `orgId` goes empty (logout navigates to /login, a public route,
// unmounting AppShell), and on an Organization change it tears down and
// reconnects rather than resubscribing (D-023 §1's channel is fixed per
// connection).
const orgId = computed(() => me.value?.organization?.id ?? '')
const actorId = computed(() => me.value?.user.id ?? '')
const authSessionLifetime = useAuthSessionLifetime()
const operatorIdentityKey = computed(() => `${orgId.value}:${actorId.value}:${authSessionLifetime.value}`)
const { status: realtimeStatus } = useRealtime({
  orgId,
  createClient: createRealtimeClient,
  resolveUrl: () => resolveRealtimeUrl(window.location),
})

const navItemClass =
  'flex h-10 items-center gap-2 rounded-lg px-3 text-body text-text-muted transition-colors duration-150 ease-out hover:bg-surface-2/60'
const navItemActiveClass = 'glass-control font-medium text-text'

// A platform-only session has no Organization to name; label it instead so
// the footer identity row is never blank.
const orgLabel = computed(() => me.value?.organization?.name ?? (me.value?.platform_admin ? 'Platform admin' : ''))

// SLICE_005 §10: the Ask drawer. Owned here, not by a route, so it persists
// across navigations while open (a card click navigates and leaves it
// open). Available on Organization routes only — hidden on /platform/** and
// /invite/** and for a platform-only session. `⌘K`/`Ctrl+K` toggles, `Esc`
// closes. The transcript is OperatorPanel's own state and is discarded when
// the drawer closes (v-if), matching "local history ... component state
// only".
const askAvailable = computed(() =>
  !sessionUnavailable.value && me.value?.organization != null && isOrganizationRoute(route.path),
)
const askOpen = ref(false)
const operatorPanel = ref<InstanceType<typeof OperatorPanel> | null>(null)

provide(OPERATOR_LAUNCHER, (personId) => {
  if (!askAvailable.value) return
  const open = () => {
    askOpen.value = true
    void nextTick(() => operatorPanel.value?.focus())
  }
  if (personId) {
    const organization = orgId.value
    const actor = actorId.value
    const session = operatorIdentityKey.value
    void router.push(`/people/${encodeURIComponent(personId)}`).then(() => {
      if (organization === orgId.value && actor === actorId.value && session === operatorIdentityKey.value && askAvailable.value) open()
    }).catch(() => {})
  } else open()
})

function toggleAsk() {
  if (!askAvailable.value) return
  askOpen.value = !askOpen.value
  if (askOpen.value) {
    void nextTick(() => operatorPanel.value?.focus())
  }
}

// Leaving the Organization routes (e.g. to /platform) drops the drawer and
// its transcript; coming back starts fresh.
watch(askAvailable, (available) => {
  if (!available) askOpen.value = false
})
watch([orgId, actorId, authSessionLifetime], ([nextOrg, nextActor, nextSession], [previousOrg, previousActor, previousSession]) => {
  if (nextOrg !== previousOrg || nextActor !== previousActor || nextSession !== previousSession) askOpen.value = false
})

function closeAsk() {
  askOpen.value = false
}

function onWindowKeydown(event: KeyboardEvent) {
  if (isToggleShortcut(event)) {
    event.preventDefault()
    toggleAsk()
    return
  }
  // Esc closes the drawer only when no floating surface (PrimeVue Dialog,
  // Select menu) is open — those listen on `document` without stopping
  // propagation, and one Esc must not dismiss both.
  if (event.key === 'Escape' && askOpen.value && !document.querySelector('[role="dialog"]:not([data-testid="person-preview"]), [role="listbox"]')) {
    closeAsk()
  }
}

onMounted(() => window.addEventListener('keydown', onWindowKeydown))
onBeforeUnmount(() => window.removeEventListener('keydown', onWindowKeydown))

function logout() {
  logoutMutation.mutate(undefined, {
    onSuccess: () => {
      router.push('/login').catch(() => {})
    },
  })
}
</script>

<template>
  <div class="flex min-h-screen bg-surface-0">
    <aside class="app-sidebar sticky top-0 flex h-dvh shrink-0 flex-col border-r border-border bg-surface-1">
      <div class="sidebar-brand px-5 py-4">
        <img
          src="/brand/elysium-lockup-horizontal-name-black.svg"
          alt="Elysium CRM"
          width="164"
          height="48"
          class="h-10 w-[140px]"
        >
      </div>

      <div class="sidebar-org mb-4 flex items-center gap-2.5 px-5 text-small text-text-muted">
        <span class="glass-control flex h-8 w-8 shrink-0 items-center justify-center rounded-lg text-text">{{ initials(orgLabel) }}</span>
        <span
          class="truncate"
          :title="orgLabel"
        >{{ orgLabel }}</span>
      </div>

      <nav
        aria-label="Main navigation"
        class="flex-1 space-y-5 overflow-y-auto px-2 py-3 md:px-3 md:py-2"
      >
        <div
          v-for="group in navGroups"
          :key="group.label"
        >
          <p class="sidebar-group-label mb-1 px-3 text-small text-text-subtle">
            {{ group.label }}
          </p>
          <RouterLink
            v-for="item in group.items"
            :key="item.to"
            :to="item.to"
            :title="item.label"
            :aria-label="item.label"
            :class="navItemClass"
            :active-class="navItemActiveClass"
          >
            <component
              :is="item.icon"
              class="h-[18px] w-[18px] shrink-0"
              stroke-width="1.5"
            />
            <span class="sidebar-label">{{ item.label }}</span>
          </RouterLink>
        </div>
      </nav>

      <div
        v-if="realtimeStatus === 'reconnecting' || realtimeStatus === 'unavailable'"
        class="border-t border-border px-4 py-2.5"
      >
        <p class="text-small text-text-muted">
          {{
            realtimeStatus === 'reconnecting'
              ? 'Realtime: reconnecting…'
              : 'Realtime unavailable — data may be delayed'
          }}
        </p>
      </div>

      <div
        v-if="showPlatformFooterLink"
        class="border-t border-border px-3 py-2"
      >
        <RouterLink
          to="/platform"
          :class="navItemClass"
          :active-class="navItemActiveClass"
        >
          <Building2
            class="h-[18px] w-[18px] shrink-0"
            stroke-width="1.5"
          />
          <span class="sidebar-label">Platform</span>
        </RouterLink>
      </div>

      <div
        v-if="me"
        class="border-t border-border p-1 md:p-3"
      >
        <div class="flex flex-wrap items-center justify-center gap-2 rounded-lg p-1 md:justify-start md:p-2">
          <div
            class="avatar-surface h-8 w-8"
          >
            {{ initials(me.user.display_name) }}
          </div>
          <div class="sidebar-user-copy min-w-0 flex-1">
            <p class="truncate text-body font-medium text-text">
              {{ me.user.display_name }}
            </p>
          </div>
          <button
            type="button"
            title="Log out"
            aria-label="Log out"
            class="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg text-text-muted transition-colors duration-150 ease-out hover:bg-surface-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-focus focus-visible:ring-offset-2 disabled:opacity-50"
            :disabled="logoutMutation.isPending.value"
            @click="logout"
          >
            <LogOut
              class="h-[18px] w-[18px]"
              stroke-width="1.5"
            />
          </button>
        </div>
      </div>
    </aside>

    <div class="flex min-w-0 flex-1">
      <div class="relative min-w-0 flex-1 overflow-y-auto">
        <div
          class="app-content mx-auto max-w-[1800px]"
          :class="askAvailable ? 'pb-24' : ''"
        >
          <div
            v-if="sessionUnavailable"
            class="rounded-xl border border-border bg-surface-0 p-5"
          >
            <p class="text-body text-danger">
              {{ sessionCoordinationUnavailable
                ? 'Browser storage is unavailable. Enable site storage, then reload.'
                : sessionBlockedByOutstandingAttempt
                  ? 'A sign-in or sign-out started in another tab has not finished. If that tab is closed or stuck, reset to continue.'
                  : sessionVerificationInFlight
                    ? 'Loading your session…'
                    : routeAuthorizationReplayPending
                      ? 'Updating access…'
                      : describeApiError(meError, 'Could not load your session.') }}
            </p>
            <button
              v-if="sessionBlockedByOutstandingAttempt"
              type="button"
              class="mt-4"
              :class="buttonClasses('secondary')"
              data-testid="session-reset"
              @click="resetSession"
            >
              Reset and continue
            </button>
            <button
              v-else-if="!sessionVerificationInFlight"
              type="button"
              class="mt-4"
              :class="buttonClasses('secondary')"
              :disabled="meIsFetching"
              @click="retrySession"
            >
              {{ meIsFetching ? 'Retrying…' : sessionCoordinationUnavailable ? 'Check again' : 'Try again' }}
            </button>
          </div>
          <slot v-else />
        </div>
      </div>
      <!-- Closing preserves same-session state; an actor, Organization, or
           auth-session lifetime change remounts and discards it. -->
      <!-- The Operator is the product's headline feature (thesis §16), so
           its trigger is a floating pill, bottom-centre of the content
           area — the one intentional exception to UI_STYLE §2/§9's
           "shadows only on floating surfaces": it *is* a floating
           surface. Hidden while the drawer is open (the drawer's own
           close button and ⌘K take over). -->
      <Transition
        enter-from-class="opacity-0 translate-y-2"
        enter-active-class="transition-all duration-150 ease-out"
        leave-active-class="transition-all duration-150 ease-out"
        leave-to-class="opacity-0 translate-y-2"
      >
        <div
          v-if="askAvailable && !askOpen"
          class="pointer-events-none fixed bottom-5 left-16 right-0 z-30 flex justify-center md:left-[220px]"
        >
          <button
            type="button"
            class="glass-control pointer-events-auto inline-flex h-11 items-center gap-2.5 rounded-full pl-4 pr-5 text-small text-text-muted shadow-floating transition-colors duration-150 hover:text-text focus-visible:ring-2 focus-visible:ring-focus"
            data-testid="ask-toggle"
            @click="toggleAsk"
          >
            <Sparkles
              class="h-5 w-5 shrink-0"
              stroke-width="1.75"
            />
            Ask AI Operator
            <kbd class="ml-1 rounded-md border border-border px-1.5 text-small text-text-subtle">⌘K</kbd>
          </button>
        </div>
      </Transition>
      <Transition
        enter-from-class="opacity-0 translate-x-2"
        enter-active-class="transition-all duration-150 ease-out"
        leave-active-class="transition-all duration-150 ease-out"
        leave-to-class="opacity-0 translate-x-2"
      >
        <OperatorPanel
          v-if="askAvailable"
          v-show="askOpen"
          :key="operatorIdentityKey"
          ref="operatorPanel"
          :identity-key="operatorIdentityKey"
          @close="closeAsk"
        />
      </Transition>
      <CallHostPanel />
    </div>
  </div>
</template>
