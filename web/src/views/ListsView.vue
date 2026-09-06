<script setup lang="ts">
// Saved-list index: metadata arrives once, then the current page's dynamic
// counts are admitted through the four-wide scheduler.
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { ChevronLeft, ChevronRight, ListFilter, Plus, RefreshCw } from 'lucide-vue-next'
import { RouterLink } from 'vue-router'
import PageHeader from '../components/PageHeader.vue'
import { useMe, useSavedLists } from '../api/queries'
import { useSavedListCountScheduler } from '../api/savedListCounts'
import type { SavedListMetadata } from '../api/types'
import { buttonClasses } from '../lib/controls'
import { describeApiError } from '../lib/errors'

const PAGE_SIZE = 25

const { data: me } = useMe()
const orgId = computed(() => me.value?.organization?.id ?? '')
const actorId = computed(() => me.value?.user.id ?? '')
const listsQuery = useSavedLists(orgId, actorId)
const lists = computed(() => listsQuery.data.value?.lists ?? [])

const personalLists = computed(() => lists.value.filter((list) => list.scope === 'personal'))
const sharedLists = computed(() => lists.value.filter((list) => list.scope === 'shared'))
const orderedLists = computed(() => [...personalLists.value, ...sharedLists.value])
const page = ref(0)
const pageCount = computed(() => Math.max(1, Math.ceil(orderedLists.value.length / PAGE_SIZE)))
const pageItems = computed(() => orderedLists.value.slice(page.value * PAGE_SIZE, (page.value + 1) * PAGE_SIZE))
const pagePersonal = computed(() => pageItems.value.filter((list) => list.scope === 'personal'))
const pageShared = computed(() => pageItems.value.filter((list) => list.scope === 'shared'))

watch(pageCount, (count) => {
  if (page.value >= count) page.value = count - 1
})

const counts = useSavedListCountScheduler(orgId, actorId, pageItems, me)

function countLabel(list: SavedListMetadata) {
  const state = counts.stateFor(list)
  if (state.kind === 'ready') return state.truncated ? '500+' : String(state.count)
  if (state.kind === 'invalid') return 'Invalid definition'
  if (state.kind === 'stale') return 'Refresh needed'
  if (state.kind === 'unavailable') return 'Unavailable'
  return 'Loading…'
}

function canRetryCount(list: SavedListMetadata) {
  const state = counts.stateFor(list)
  return state.kind === 'unavailable' || state.kind === 'stale'
}

async function refresh() {
  await listsQuery.refetch()
  counts.refresh()
}

async function retryCount(list: SavedListMetadata) {
  if (counts.stateFor(list).kind === 'stale') {
    // A stale revision can never succeed by retrying the same URL. Fetch the
    // authoritative index first; its page refresh goes through the scheduler.
    await refresh()
    return
  }
  counts.retry(list)
}

function refreshOnWindowFocus() {
  void refresh()
}

onMounted(() => {
  // query-core observes visibility changes, but switching between visible
  // desktop windows emits only focus. Keep this recovery path explicit.
  window.addEventListener('focus', refreshOnWindowFocus)
})
onBeforeUnmount(() => window.removeEventListener('focus', refreshOnWindowFocus))
</script>

<template>
  <main class="mx-auto w-full max-w-6xl px-4 py-6 md:px-8 md:py-8">
    <PageHeader
      title="Lists"
      subtitle="Saved People criteria update as your CRM changes."
    >
      <template #action>
        <RouterLink
          to="/people"
          :class="buttonClasses('primary')"
        >
          <Plus
            class="h-4 w-4"
            stroke-width="1.5"
          />
          Create a list
        </RouterLink>
      </template>
    </PageHeader>

    <section
      class="mb-6 grid gap-3 sm:grid-cols-2"
      aria-label="List limits"
    >
      <div class="rounded-xl border border-border bg-surface-0 px-4 py-3">
        <p class="text-small text-text-muted">
          My lists
        </p>
        <p class="mt-1 text-section font-semibold text-text">
          {{ personalLists.length }} of 50
        </p>
      </div>
      <div class="rounded-xl border border-border bg-surface-0 px-4 py-3">
        <p class="text-small text-text-muted">
          Shared lists
        </p>
        <p class="mt-1 text-section font-semibold text-text">
          {{ sharedLists.length }} of 200
        </p>
      </div>
    </section>

    <div class="mb-4 flex items-center justify-between gap-3">
      <p class="text-small text-text-muted">
        {{ orderedLists.length }} {{ orderedLists.length === 1 ? 'saved list' : 'saved lists' }}
      </p>
      <button
        type="button"
        :class="buttonClasses('secondary')"
        :disabled="listsQuery.isFetching.value || counts.isRefreshing.value"
        @click="refresh"
      >
        <RefreshCw
          class="h-4 w-4"
          stroke-width="1.5"
        />
        Refresh
      </button>
    </div>

    <div
      v-if="listsQuery.isError.value"
      role="alert"
      class="rounded-xl border border-border bg-surface-0 p-5 text-body text-danger"
    >
      <p>{{ describeApiError(listsQuery.error.value, 'Could not load saved lists.') }}</p>
      <button
        type="button"
        :class="[buttonClasses('secondary'), 'mt-3']"
        @click="listsQuery.refetch()"
      >
        Try again
      </button>
    </div>

    <div
      v-else-if="listsQuery.isPending.value && !listsQuery.data.value"
      class="rounded-xl border border-border bg-surface-0 p-5 text-body text-text-muted"
    >
      Loading saved lists…
    </div>

    <div
      v-else-if="orderedLists.length === 0"
      class="rounded-xl border border-border bg-surface-0 p-8 text-center"
    >
      <ListFilter
        class="mx-auto h-8 w-8 text-text-muted"
        stroke-width="1.4"
      />
      <h2 class="mt-3 text-section font-semibold text-text">
        No saved lists yet
      </h2>
      <p class="mt-1 text-body text-text-muted">
        Build criteria in People, then save them here for reuse.
      </p>
      <RouterLink
        to="/people"
        :class="[buttonClasses('primary'), 'mt-4 inline-flex']"
      >
        Create a list
      </RouterLink>
    </div>

    <div
      v-else
      class="space-y-6"
    >
      <section
        v-if="pagePersonal.length"
        aria-labelledby="my-lists-heading"
      >
        <h2
          id="my-lists-heading"
          class="mb-2 text-small font-semibold text-text-muted"
        >
          My lists
        </h2>
        <div class="overflow-hidden rounded-xl border border-border bg-surface-0">
          <div
            v-for="list in pagePersonal"
            :key="list.id"
            class="flex items-center gap-3 border-b border-border px-4 py-3 last:border-b-0 hover:bg-surface-1"
          >
            <RouterLink
              :to="`/lists/${list.id}`"
              class="flex min-w-0 flex-1 items-center gap-3"
            >
              <ListFilter
                class="h-4 w-4 shrink-0 text-text-muted"
                stroke-width="1.5"
              />
              <span class="min-w-0 flex-1 truncate text-body font-medium text-text">{{ list.name }}</span>
            </RouterLink>
            <span
              class="text-small text-text-muted"
              :aria-label="`${countLabel(list)} matches`"
            >{{ countLabel(list) }}</span>
            <button
              v-if="canRetryCount(list)"
              type="button"
              class="text-small font-medium text-accent hover:underline"
              @click.prevent="retryCount(list)"
            >
              Retry
            </button>
          </div>
        </div>
      </section>

      <section
        v-if="pageShared.length"
        aria-labelledby="shared-lists-heading"
      >
        <h2
          id="shared-lists-heading"
          class="mb-2 text-small font-semibold text-text-muted"
        >
          Shared lists
        </h2>
        <div class="overflow-hidden rounded-xl border border-border bg-surface-0">
          <div
            v-for="list in pageShared"
            :key="list.id"
            class="flex items-center gap-3 border-b border-border px-4 py-3 last:border-b-0 hover:bg-surface-1"
          >
            <RouterLink
              :to="`/lists/${list.id}`"
              class="flex min-w-0 flex-1 items-center gap-3"
            >
              <ListFilter
                class="h-4 w-4 shrink-0 text-text-muted"
                stroke-width="1.5"
              />
              <span class="min-w-0 flex-1 truncate text-body font-medium text-text">{{ list.name }}</span>
            </RouterLink>
            <span
              class="text-small text-text-muted"
              :aria-label="`${countLabel(list)} matches`"
            >{{ countLabel(list) }}</span>
            <button
              v-if="canRetryCount(list)"
              type="button"
              class="text-small font-medium text-accent hover:underline"
              @click.prevent="retryCount(list)"
            >
              Retry
            </button>
          </div>
        </div>
      </section>

      <nav
        v-if="pageCount > 1"
        class="flex items-center justify-between border-t border-border pt-4"
        aria-label="Saved list pages"
      >
        <button
          type="button"
          :class="buttonClasses('secondary')"
          :disabled="page === 0"
          @click="page--"
        >
          <ChevronLeft
            class="h-4 w-4"
            stroke-width="1.5"
          />
          Previous
        </button>
        <span class="text-small text-text-muted">Page {{ page + 1 }} of {{ pageCount }}</span>
        <button
          type="button"
          :class="buttonClasses('secondary')"
          :disabled="page + 1 >= pageCount"
          @click="page++"
        >
          Next
          <ChevronRight
            class="h-4 w-4"
            stroke-width="1.5"
          />
        </button>
      </nav>
    </div>
  </main>
</template>
