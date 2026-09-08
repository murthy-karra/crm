// Bounded current-count scheduling for the Saved Lists index (Slice 011b).
// TanStack owns the namespace/invalidations; this small scheduler is the
// single place that starts count HTTP work, so retries, focus, navigation and
// `person.changed` cannot accidentally exceed four concurrent requests.
import { useQuery, useQueryClient } from '@tanstack/vue-query'
import { computed, onBeforeUnmount, ref, toValue, watch, type MaybeRefOrGetter } from 'vue'
import { ApiError } from './client'
import { fetchSavedListCount, queryKeys } from './queries'
import type { MeResponse, SavedListFilterError, SavedListMetadata } from './types'

const MAX_IN_FLIGHT = 4

export type SavedListCountState =
  | { kind: 'idle' | 'loading' }
  | { kind: 'ready'; count: number; truncated: boolean }
  | { kind: 'invalid'; error: SavedListFilterError }
  | { kind: 'stale' }
  | { kind: 'unavailable' }

type CountItem = Pick<SavedListMetadata, 'id' | 'revision'>

function itemKey(item: CountItem) {
  return `${item.id}:${item.revision}`
}

function isAbort(error: unknown) {
  return error instanceof DOMException && error.name === 'AbortError'
}

// A `Record<SavedListFilterError, true>` rather than a bare literal array:
// TypeScript requires every union member as a key, so adding a new
// `SavedListFilterError` code without updating this set is a compile
// error, not a silent `kind: 'unavailable'` fallback (Slice 011e e2
// review round 1, F1).
const SAVED_LIST_FILTER_ERROR_CODES: Record<SavedListFilterError, true> = {
  unsupported_filter: true,
  invalid_stage: true,
  invalid_assignee: true,
  invalid_tag: true,
}
function isSavedListFilterErrorCode(code: string): code is SavedListFilterError {
  return Object.prototype.hasOwnProperty.call(SAVED_LIST_FILTER_ERROR_CODES, code)
}

/**
 * Schedules only the `items` supplied by the current index page. The caller
 * must pass at most 25 metadata rows; this composable neither fetches nor
 * scans definitions itself.
 */
export function useSavedListCountScheduler(
  orgId: MaybeRefOrGetter<string>,
  actorId: MaybeRefOrGetter<string>,
  items: MaybeRefOrGetter<readonly CountItem[]>,
  session?: MaybeRefOrGetter<MeResponse | undefined>,
) {
  const qc = useQueryClient()
  const states = ref<Record<string, SavedListCountState>>({})
  const pending = ref(0)
  const queue: CountItem[] = []
  const queued = new Set<string>()
  const active = new Map<number, AbortController>()
  let nextJobId = 0
  let generation = 0
  let disposed = false

  function currentSession() {
    return session === undefined ? qc.getQueryData<MeResponse>(queryKeys.me) : toValue(session)
  }

  // This active, zero-network query makes the standard Query invalidation
  // path observable. `person.changed` invalidates this prefix; refetch on
  // focus/reconnect then routes every actual count back through `pump`.
  const refreshMarker = useQuery({
    queryKey: computed(() => queryKeys.savedListCounts(toValue(orgId))),
    queryFn: () => Promise.resolve(Date.now()),
    enabled: computed(() => toValue(orgId) !== '' && toValue(actorId) !== ''),
    retry: false,
    staleTime: 0,
    refetchOnMount: 'always',
    refetchOnWindowFocus: 'always',
  })

  function updatePending() {
    pending.value = queue.length + active.size
  }

  function desiredItems(): CountItem[] {
    const seen = new Set<string>()
    return toValue(items).filter((item) => {
      const key = itemKey(item)
      if (seen.has(key)) return false
      seen.add(key)
      return true
    })
  }

  function setState(item: CountItem, state: SavedListCountState) {
    states.value = { ...states.value, [itemKey(item)]: state }
  }

  function queueItem(item: CountItem) {
    const key = itemKey(item)
    if (queued.has(key)) return
    queued.add(key)
    queue.push(item)
    setState(item, { kind: 'idle' })
    updatePending()
  }

  async function run(
    item: CountItem,
    requestGeneration: number,
    controller: AbortController,
    identity: { orgId: string; actorId: string; session: MeResponse | undefined },
  ) {
    const jobId = ++nextJobId
    active.set(jobId, controller)
    setState(item, { kind: 'loading' })
    updatePending()
    try {
      const result = await fetchSavedListCount(item.id, item.revision, controller.signal)
      if (
        disposed || requestGeneration !== generation ||
        toValue(orgId) !== identity.orgId || toValue(actorId) !== identity.actorId ||
        currentSession() !== identity.session
      ) return
      setState(item, { kind: 'ready', count: result.count, truncated: result.truncated })
      qc.setQueryData(queryKeys.savedListCount(
        identity.orgId, identity.actorId, item.id, item.revision,
      ), result)
    } catch (error) {
      if (
        disposed || requestGeneration !== generation || isAbort(error) ||
        toValue(orgId) !== identity.orgId || toValue(actorId) !== identity.actorId ||
        currentSession() !== identity.session
      ) return
      if (error instanceof ApiError && isSavedListFilterErrorCode(error.code)) {
        setState(item, { kind: 'invalid', error: error.code })
      } else if (error instanceof ApiError && (error.status === 404 || error.code === 'saved_list_conflict')) {
        // Metadata changed under us. The index's next revalidation replaces
        // the row; never render a stale count as zero in the meantime.
        setState(item, { kind: 'stale' })
      } else {
        setState(item, { kind: 'unavailable' })
      }
    } finally {
      active.delete(jobId)
      updatePending()
      pump()
    }
  }

  function pump() {
    // An org/actor ref can briefly retain its old values while `/me` is being
    // cleared. Never start a private count request in that session gap.
    if (disposed || toValue(orgId) === '' || toValue(actorId) === '' || currentSession() === undefined) return
    while (active.size < MAX_IN_FLIGHT && queue.length > 0) {
      const item = queue.shift()!
      queued.delete(itemKey(item))
      const controller = new AbortController()
      // IDs alone are insufficient: A → logout → A has the same public
      // identity but must not admit a completion from the dead session.
      const identity = {
        orgId: toValue(orgId),
        actorId: toValue(actorId),
        session: currentSession(),
      }
      void run(item, generation, controller, identity)
    }
    updatePending()
  }

  function refresh() {
    generation++
    for (const controller of active.values()) controller.abort()
    queue.length = 0
    queued.clear()
    const next = desiredItems()
    states.value = Object.fromEntries(next.map((item) => [itemKey(item), { kind: 'idle' }]))
    for (const item of next) queueItem(item)
    pump()
  }

  function retry(item: CountItem) {
    if (!desiredItems().some((candidate) => itemKey(candidate) === itemKey(item))) return
    queueItem(item)
    pump()
  }

  function stateFor(item: CountItem): SavedListCountState {
    return states.value[itemKey(item)] ?? { kind: 'idle' }
  }

  watch(
    () => [
      toValue(orgId),
      toValue(actorId),
      currentSession(),
      ...desiredItems().map((item) => itemKey(item)),
    ],
    refresh,
    { immediate: true },
  )
  watch(() => refreshMarker.dataUpdatedAt.value, (updatedAt, previous) => {
    if (updatedAt > 0 && updatedAt !== previous) refresh()
  })
  onBeforeUnmount(() => {
    disposed = true
    generation++
    for (const controller of active.values()) controller.abort()
    queue.length = 0
    queued.clear()
    updatePending()
  })

  return {
    states,
    isRefreshing: computed(() => pending.value > 0),
    refresh,
    retry,
    stateFor,
  }
}
