<script setup lang="ts">
// UI_STYLE.md §1: fixed 280px sidebar, white, 1px hairline separator (no
// shadow); nav groups Work / Intake (§10). The icon rail shown in the
// reference screens is deliberately not shipped this slice (§1) — the
// sidebar's internal layout is left so a rail can be added later without
// moving anything.
import { computed, type Component } from 'vue'
import { RouterLink, useRouter } from 'vue-router'
import { Inbox, LogOut, UserPlus, Users } from 'lucide-vue-next'
import { useLogoutMutation, useMe } from '../api/queries'
import { initials } from '../lib/format'

const router = useRouter()

interface NavItem {
  label: string
  to: string
  icon: Component
}

interface NavGroup {
  label: string
  items: NavItem[]
}

const navGroups: NavGroup[] = [
  { label: 'Work', items: [{ label: 'People', to: '/people', icon: Users }] },
  {
    label: 'Intake',
    items: [
      { label: 'New lead', to: '/intake/new', icon: UserPlus },
      { label: 'Unresolved', to: '/intake/unresolved', icon: Inbox },
    ],
  },
]

const { data: me } = useMe()
const logoutMutation = useLogoutMutation()

const navItemClass =
  'flex h-10 items-center gap-2 rounded-lg px-3 text-body text-text-muted transition-colors duration-150 ease-out hover:bg-surface-2/60'
const navItemActiveClass = 'bg-surface-2 font-semibold text-text hover:bg-surface-2'

const orgLabel = computed(() => me.value?.organization.name ?? '')

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

    <div class="min-w-0 flex-1 overflow-y-auto">
      <div class="mx-auto max-w-[1280px] px-10 py-10">
        <slot />
      </div>
    </div>
  </div>
</template>
