<script setup lang="ts">
// UI_STYLE.md §1: fixed 280px sidebar, white, 1px hairline separator (no
// shadow); nav groups Work / Intake (§10). The icon rail shown in the
// reference screens is deliberately not shipped this slice (§1) — the
// sidebar's internal layout is left so a rail can be added later without
// moving anything.
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch, type Component } from 'vue'
import { RouterLink, useRoute, useRouter } from 'vue-router'
import { Building2, Inbox, LogOut, Mail, Sparkles, Sun, UserCog, UserPlus, Users } from 'lucide-vue-next'
import { useLogoutMutation, useMe } from '../api/queries'
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
  if (me.value.organization.role === 'admin') {
    groups.push({
      label: 'Manage',
      items: [
        { label: 'Members', to: '/manage/members', icon: UserCog },
        { label: 'Intake', to: '/manage/intake', icon: Mail },
      ],
    })
  }
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
const sessionUnavailable = computed(() => meIsError.value && me.value === undefined)

function retrySession() {
  void refetchMe()
}

// AppShell mounts once for every non-public route (App.vue) and stays
// mounted across navigations between them — the single owner of the
// realtime connection SLICE_003 §7 describes: it tears down on unmount and
// whenever `orgId` goes empty (logout navigates to /login, a public route,
// unmounting AppShell), and on an Organization change it tears down and
// reconnects rather than resubscribing (D-023 §1's channel is fixed per
// connection).
const orgId = computed(() => me.value?.organization?.id ?? '')
const { status: realtimeStatus } = useRealtime({
  orgId,
  createClient: createRealtimeClient,
  resolveUrl: () => resolveRealtimeUrl(window.location),
})

const navItemClass =
  'flex h-10 items-center gap-2 rounded-lg px-3 text-body text-text-muted transition-colors duration-150 ease-out hover:bg-surface-2/60'
const navItemActiveClass = 'bg-surface-2 font-semibold text-text hover:bg-surface-2'

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
const askAvailable = computed(() => me.value?.organization != null && isOrganizationRoute(route.path))
const askOpen = ref(false)
const operatorPanel = ref<InstanceType<typeof OperatorPanel> | null>(null)

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
  if (event.key === 'Escape' && askOpen.value && !document.querySelector('[role="dialog"], [role="listbox"]')) {
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
  <div class="flex min-h-screen bg-surface-1">
    <aside class="flex w-[280px] shrink-0 flex-col border-r border-border bg-surface-0">
      <div class="px-6 py-5">
        <span class="text-[17px] font-semibold text-text">CRM</span>
      </div>

      <nav class="flex-1 space-y-6 overflow-y-auto px-3 py-2">
        <div
          v-for="group in navGroups"
          :key="group.label"
        >
          <p class="mb-1 px-3 text-small font-medium text-text-muted">
            {{ group.label }}
          </p>
          <RouterLink
            v-for="item in group.items"
            :key="item.to"
            :to="item.to"
            :class="navItemClass"
            :active-class="navItemActiveClass"
          >
            <component
              :is="item.icon"
              class="h-[18px] w-[18px] shrink-0"
              stroke-width="1.5"
            />
            {{ item.label }}
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
          Platform
        </RouterLink>
      </div>

      <div
        v-if="me"
        class="border-t border-border p-3"
      >
        <div class="flex items-center gap-3 rounded-lg p-2">
          <div
            class="flex h-8 w-8 shrink-0 items-center justify-center rounded-full bg-surface-2 text-small font-medium text-text-muted"
          >
            {{ initials(me.user.display_name) }}
          </div>
          <div class="min-w-0 flex-1">
            <p class="truncate text-body font-medium text-text">
              {{ me.user.display_name }}
            </p>
            <p class="truncate text-small text-text-muted">
              {{ orgLabel }}
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
          class="mx-auto max-w-[1280px] px-10 py-10"
          :class="askAvailable ? 'pb-24' : ''"
        >
          <div
            v-if="sessionUnavailable"
            class="rounded-xl border border-border bg-surface-0 p-5"
          >
            <p class="text-body text-danger">
              {{ describeApiError(meError, 'Could not load your session.') }}
            </p>
            <button
              type="button"
              class="mt-4"
              :class="buttonClasses('secondary')"
              :disabled="meIsFetching"
              @click="retrySession"
            >
              {{ meIsFetching ? 'Retrying…' : 'Try again' }}
            </button>
          </div>
          <slot v-else />
        </div>
      </div>
      <!-- v-show while available: closing mid-turn must not discard the
           answer or the transcript (component state survives until the
           drawer stops being available, e.g. on /platform). -->
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
          class="pointer-events-none fixed bottom-6 left-[280px] right-0 z-40 flex justify-center"
        >
          <button
            type="button"
            class="pointer-events-auto inline-flex h-12 items-center gap-2.5 rounded-full bg-accent pl-4 pr-5 text-body font-medium text-white shadow-floating transition-all duration-150 ease-out hover:-translate-y-0.5 hover:bg-accent-hover hover:shadow-[0_12px_32px_rgb(0_0_0_/_0.18)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-focus focus-visible:ring-offset-2 focus-visible:ring-offset-surface-1 active:translate-y-0"
            data-testid="ask-toggle"
            @click="toggleAsk"
          >
            <Sparkles
              class="h-5 w-5 shrink-0"
              stroke-width="1.75"
            />
            Ask AI Operator
            <kbd class="ml-1 rounded-md bg-white/15 px-1.5 text-[11px] font-medium text-white/80">⌘K</kbd>
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
          ref="operatorPanel"
          class="sticky top-0 h-screen"
          @close="closeAsk"
        />
      </Transition>
      <CallHostPanel />
    </div>
  </div>
</template>
