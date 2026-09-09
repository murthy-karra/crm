<script setup lang="ts">
// SLICE_003 §1, §10: the new landing route. One table card: Name (over
// primary contact), Reasons (neutral badges, wire order — §3's fixed
// order, never re-sorted), Priority (text weight, not color), Waiting
// (relative; absolute in tooltip), Recommended (Call + phone / Email +
// address), per-row secondary "Log contact". SLICE_006c §5a (D-033): a
// `low` item — the viewer's own call with no chosen outcome — reads
// "Outcome needed" / "Low" and its action is "Set outcome", which opens the
// Person page with `?outcome=<call_id>` (the Set-outcome dialog). Server
// order is the only order (§3: `rank()` preserves SQL order; `low` arrives
// last) — this view never sorts `items` itself.
import { computed, h, nextTick, onBeforeUnmount, onMounted, ref, watch, type VNode } from 'vue'
import { RouterLink, useRoute, useRouter } from 'vue-router'
import { Mail, Phone, PhoneOutgoing, Sun } from 'lucide-vue-next'
import type { ColumnDef } from '@tanstack/vue-table'
import PageHeader from '../components/PageHeader.vue'
import DataTable from '../components/DataTable.vue'
import Badge from '../components/Badge.vue'
import LogContactDialog from '../components/LogContactDialog.vue'
import {
  useAuthSessionLifetime,
  useCompleteTaskMutation,
  useDisableTodaySourceMutation,
  useMe,
  useSnoozeTaskMutation,
  useTasks,
  useToday,
  useTodayFeeds,
  useTodaySources,
} from '../api/queries'
import type { TodayItem, TodayReason } from '../api/types'
import { formatAbsoluteTime, formatRelativeTime } from '../lib/format'
import { buttonClasses } from '../lib/controls'
import { describeApiError, describeTaskError } from '../lib/errors'
import {
  MEMBER_FEED_MARKER_LABEL,
  TODAY_FEED_LABEL,
  TODAY_FEED_ORDER,
  fallbackFeedMessage,
  memberFeedMarker,
  todayFeedIssueMessage,
} from '../lib/todayFeeds'
import { TASK_KIND_LABEL, clipTitle, groupTasks, panelDueCellText, tomorrowLocalEndOfDay } from '../lib/tasks'

const route = useRoute()
const router = useRouter()
const { data: me } = useMe()
const orgId = computed(() => me.value?.organization?.id ?? '')
const actorId = computed(() => me.value?.user.id ?? '')
const authSessionLifetime = useAuthSessionLifetime()

const todayQuery = useToday(orgId, actorId)
const { data: todayData, dataUpdatedAt, isPending, isError, error } = todayQuery
const sourcesQuery = useTodaySources(orgId, actorId)
const feedsQuery = useTodayFeeds(orgId, actorId)
const disableSource = useDisableTodaySourceMutation(orgId, actorId)

// Slice 016b (docs/specs/SLICE_016.md §8): the Tasks panel's own read,
// independent of the ranked queue above (completing/snoozing a task can
// move it between tiers or off Today entirely, refetched separately).
const tasksQuery = useTasks(orgId, actorId)

// One shared Complete mutation instance for BOTH the ranked queue's
// per-row Complete button and the Tasks panel's rows (identical action,
// `POST .../tasks/{task_id}/complete`) — `completeTaskPersonId` is
// updated right before each `.mutate()` call so the hook's
// `personMutationKey` (used only for the settle/realtime-hold
// bookkeeping, never the request URL itself) always reflects whichever
// row is currently in flight, the same pattern `useSnoozeTaskMutation`
// below repeats for the panel's Snooze control.
//
// Pending state for THIS row is tracked with an explicit local ref
// (`completingTask`/`snoozingTask`), not `completeTask.isPending`/
// `.variables` directly — the `removeSource`/`removing` precedent
// earlier in this file: a plain ref set immediately before `.mutate()`
// and cleared in `onSettled` is deterministic and testable, unlike
// reading the mutation object's own reactive fields from inside a
// per-row helper.
const completeTaskPersonId = ref('')
const completeTask = useCompleteTaskMutation(orgId, completeTaskPersonId)
const snoozeTaskPersonId = ref('')
const snoozeTask = useSnoozeTaskMutation(orgId, snoozeTaskPersonId)
const taskActionError = ref<{ id: string; message: string } | null>(null)
const completingTask = ref<{ personId: string; taskId: string } | null>(null)
const snoozingTask = ref<{ personId: string; taskId: string } | null>(null)

/** Pessimistic (§8: "held while pending"); the row/item leaves only once
 * the natural refetch (settled via `settleTaskMutation`) stops returning
 * it — no optimistic cache write here.
 *
 * Round-2 review fix 1: `completeTask`'s `mutationKey` is
 * `computed(() => personMutationKey(orgId, completeTaskPersonId))` — a
 * REACTIVE key. Vue Query's `MutationObserver` applies a `mutationKey`
 * change through a pre-flush watcher, which runs asynchronously relative
 * to a synchronous `completeTaskPersonId.value = personId` immediately
 * followed by `.mutate()`: the key change had not yet propagated when
 * `.mutate()` fired, so the observer picked it up moments later and
 * reset, detaching the very call this function just made from its own
 * `onError`/`onSettled` — leaving `completingTask` stuck forever and
 * every later click inert. Awaiting `nextTick()` between setting the ref
 * and calling `.mutate()` lets that watcher flush first, so the mutation
 * always starts already keyed to the right Person. */
async function onCompleteTaskItem(personId: string, taskId: string) {
  if (completingTask.value) return
  taskActionError.value = null
  completingTask.value = { personId, taskId }
  completeTaskPersonId.value = personId
  await nextTick()
  completeTask.mutate(
    { personId, taskId },
    {
      onError: (err) => { taskActionError.value = { id: taskId, message: describeTaskError(err, 'Could not complete this task.') } },
      onSettled: () => { completingTask.value = null },
    },
  )
}
function isCompletingTaskItem(personId: string, taskId: string): boolean {
  return completingTask.value?.personId === personId && completingTask.value?.taskId === taskId
}

/** Snooze to tomorrow at local end of day (rule 2). A `changed: false`
 * response (the task is already due then) is not an error — the row
 * simply stays, no error copy (§8). Same `nextTick()` fix as
 * `onCompleteTaskItem` above, for the identical reactive-`mutationKey`
 * reason. */
async function onSnoozeTaskItem(personId: string, taskId: string) {
  if (snoozingTask.value) return
  taskActionError.value = null
  snoozingTask.value = { personId, taskId }
  snoozeTaskPersonId.value = personId
  await nextTick()
  snoozeTask.mutate(
    { personId, taskId, dueAt: tomorrowLocalEndOfDay() },
    {
      onError: (err) => { taskActionError.value = { id: taskId, message: describeTaskError(err, 'Could not snooze this task.') } },
      onSettled: () => { snoozingTask.value = null },
    },
  )
}
function isSnoozingTaskItem(personId: string, taskId: string): boolean {
  return snoozingTask.value?.personId === personId && snoozingTask.value?.taskId === taskId
}

type TaskReasonVariant = Extract<TodayReason, { code: 'task_overdue' | 'task_due' }>

/** The item's `task_overdue`/`task_due` reason, if any (§8: "whenever the
 * item carries a task reason, regardless of `recommended_action`"). */
function taskReason(item: TodayItem): TaskReasonVariant | null {
  const reason = item.reasons.find(
    (r): r is TaskReasonVariant => r.code === 'task_overdue' || r.code === 'task_due',
  )
  return reason ?? null
}

const generatedAt = computed(() => tasksQuery.data.value?.generated_at ?? new Date().toISOString())
// Round-2 review fix 3: `groupTasks` (lib/tasks.ts) compares with
// `Date.parse`, not raw ISO-string `<` (a whole-second `due_at` sorted
// AFTER a fractional-second `generated_at` lexically, misclassifying it).
const taskGroups = computed(() => {
  const data = tasksQuery.data.value
  if (!data) return { overdue: [], dueSoon: [] }
  return groupTasks(data.tasks, data.generated_at)
})
const overdueTasks = computed(() => taskGroups.value.overdue)
const dueSoonTasks = computed(() => taskGroups.value.dueSoon)
const orderedFeeds = computed(() => {
  const byKey = new Map((feedsQuery.data.value?.feeds ?? []).map((f) => [f.feed_key, f]))
  return TODAY_FEED_ORDER.map((key) => byKey.get(key)).filter((f): f is NonNullable<typeof f> => f !== undefined)
})
const items = computed(() => todayData.value?.items ?? [])
const showSources = ref(false)
const removing = ref<string | null>(null)
const sourceNotice = ref<string | null>(null)
const sourceManager = ref<HTMLElement | null>(null)
const removalFocus = ref<{
  listId: string
  index: number
  orgId: string
  actorId: string
  authSession: number
} | null>(null)

const subtitle = computed(() => {
  if (isPending.value || isError.value) return undefined
  const count = items.value.length
  return `Updated ${formatRelativeTime(dataUpdatedAt.value)} · ${count} ${count === 1 ? 'item' : 'items'} of work`
})

function reasonLabel(reason: TodayReason): string {
  switch (reason.code) {
    case 'new_inquiry':
      return 'New inquiry'
    case 'no_contact_attempt':
      return 'No contact attempt'
    case 'repeat_inquiry':
      return `Inquired again (${reason.inquiry_count})`
    case 'call_outcome_needed':
      return 'Outcome needed'
    case 'client_replied':
      return 'Client replied'
    case 'list_member':
      return reason.name
    // Slice 016b (docs/specs/SLICE_016.md §8): the badge shows the
    // CLIPPED title (the `list_member` chip precedent — full text there
    // because list names are short; a task title can run to 500 chars).
    case 'task_overdue':
    case 'task_due':
      return clipTitle(reason.title)
    // Forward-compatibility default (an older bundle encountering a
    // future additive reason code): never an empty badge or `undefined`,
    // the `HistoryEntry` generic-fallback precedent.
    default:
      return 'Work item'
  }
}

const PRIORITY_LABEL: Record<TodayItem['priority'], string> = { high: 'High', normal: 'Normal', list: 'From your lists', low: 'Low' }

function sourceError(error: string | null) {
  if (error === 'unsupported_filter') return 'This list definition is no longer supported.'
  if (error === 'invalid_stage') return 'This list refers to a stage that no longer exists.'
  if (error === 'invalid_assignee') return 'This list refers to a member who no longer exists.'
  if (error === 'invalid_tag') return 'This list refers to a tag that no longer exists.'
  return null
}

function removeSource(listId: string) {
  if (removing.value === listId) return
  const identity = {
    orgId: orgId.value,
    actorId: actorId.value,
    authSession: authSessionLifetime.value,
  }
  const stillCurrent = () => orgId.value === identity.orgId && actorId.value === identity.actorId &&
    authSessionLifetime.value === identity.authSession
  const index = sourcesQuery.data.value?.sources.findIndex((source) => source.list_id === listId) ?? -1
  removalFocus.value = {
    listId,
    index: Math.max(0, index),
    ...identity,
  }
  removing.value = listId
  sourceNotice.value = null
  disableSource.mutate(listId, {
    onError: async () => {
      removalFocus.value = null
      if (!stillCurrent()) return
      const refreshed = await sourcesQuery.refetch()
      if (!stillCurrent()) return
      const stillEnabled = refreshed.data?.sources.some((source) => source.list_id === listId) ?? true
      sourceNotice.value = stillEnabled
        ? 'Could not confirm removal. Try again.'
        : 'Removal completed, but the response was lost.'
    },
    onSettled: () => { if (stillCurrent() && removing.value === listId) removing.value = null },
  })
}

watch(
  () => sourcesQuery.data.value?.sources.map((source) => source.list_id),
  async (sourceIds) => {
    const pending = removalFocus.value
    if (!pending) return
    const stillCurrent = orgId.value === pending.orgId && actorId.value === pending.actorId &&
      authSessionLifetime.value === pending.authSession
    if (!stillCurrent || sourceIds?.includes(pending.listId)) {
      if (!stillCurrent) removalFocus.value = null
      return
    }
    removalFocus.value = null
    await nextTick()
    const removeButtons = sourceManager.value?.querySelectorAll<HTMLButtonElement>('[data-source-remove]')
    const nextButton = removeButtons?.item(Math.min(pending.index, (removeButtons.length ?? 1) - 1))
    ;(nextButton ?? sourceManager.value?.querySelector<HTMLElement>('a[href="/lists"]'))?.focus()
  },
)

function retrySources() {
  sourceNotice.value = null
  void sourcesQuery.refetch()
}

function refreshToday() {
  sourceNotice.value = null
  // Round-2 review fix 5: the Tasks panel's own read was missing from the
  // Refresh button's join — a stale panel could persist indefinitely
  // after a Refresh that otherwise looked complete.
  void Promise.all([todayQuery.refetch(), sourcesQuery.refetch(), feedsQuery.refetch(), tasksQuery.refetch()])
}

const emptyTitle = computed(() =>
  todayData.value?.sources.status === 'complete' ? "You're all caught up" : 'No available work to show',
)
const emptyMessage = computed(() =>
  todayData.value?.sources.status === 'complete'
    ? 'No work matches your current Today rules or sources.'
    : 'Some sources could not load. Retry when they are available.',
)

watch([orgId, actorId, authSessionLifetime], () => {
  sourceNotice.value = null
  removing.value = null
  removalFocus.value = null
  showSources.value = false
  // Round-2 review fix 5: an actor/org/session change (account switch,
  // re-auth) must not leave a stale task action's guard/error stuck for
  // the new identity.
  completingTask.value = null
  snoozingTask.value = null
  taskActionError.value = null
})

// The cap notice may link directly to this panel. This is view-local route
// state only: ordinary Today navigation and the normal Manager button keep
// their existing behavior.
watch(
  () => route.query.sources,
  (value) => {
    if (value === '1') showSources.value = true
  },
  { immediate: true },
)

function refreshOnWindowFocus() {
  refreshToday()
}
onMounted(() => window.addEventListener('focus', refreshOnWindowFocus))
onBeforeUnmount(() => window.removeEventListener('focus', refreshOnWindowFocus))

/** The item's `call_outcome_needed` reason, if any (§5a). */
function outcomeNeededReason(item: TodayItem): { call_id: string; ended_at: string } | null {
  const reason = item.reasons.find((r) => r.code === 'call_outcome_needed')
  return reason && reason.code === 'call_outcome_needed' ? reason : null
}

/** The `call_outcome_needed` reason on a `set_outcome` item (§5a: present
 * whenever the action is `set_outcome`; null otherwise). */
function outcomeNeeded(item: TodayItem): { call_id: string; ended_at: string } | null {
  if (item.recommended_action !== 'set_outcome') return null
  return outcomeNeededReason(item)
}

/** A Person qualifying both ways (§5a): the Inquiry tier wins and the
 * action stays Call/Email, but the viewer's own call still needs an
 * outcome — offered as a secondary "Set outcome" beside Log contact. */
function outcomeNeededAside(item: TodayItem): { call_id: string; ended_at: string } | null {
  if (item.recommended_action === 'set_outcome') return null
  return outcomeNeededReason(item)
}

function setOutcomeButton(item: TodayItem, callId: string, testId: string) {
  return h(
    'button',
    {
      type: 'button',
      class: buttonClasses('secondary'),
      'data-testid': testId,
      onClick: (event: MouseEvent) => {
        event.stopPropagation()
        void router.push(setOutcomeTo(item, callId))
      },
    },
    'Set outcome',
  )
}

function setOutcomeTo(item: TodayItem, callId: string): string {
  return `/people/${item.person.id}?outcome=${encodeURIComponent(callId)}`
}

const logContactTarget = ref<{ personId: string; personName: string } | null>(null)

function openLogContact(item: TodayItem) {
  logContactTarget.value = { personId: item.person.id, personName: item.person.display_name }
}

function onDialogVisibleChange(value: boolean) {
  if (!value) logContactTarget.value = null
}

const columns: ColumnDef<TodayItem>[] = [
  {
    id: 'name',
    header: 'Name',
    cell: (info) => {
      const person = info.row.original.person
      const contact = person.primary_email ?? person.primary_phone
      return h('div', [
        h('p', { class: 'text-body font-medium text-text' }, person.display_name),
        contact ? h('p', { class: 'text-small text-text-muted' }, contact) : null,
      ])
    },
  },
  {
    id: 'reasons',
    header: 'Reasons',
    cell: (info) =>
      h(
        'div',
        { class: 'flex flex-wrap gap-1.5' },
        info.row.original.reasons.map((reason) => reason.code === 'list_member'
          ? h(RouterLink, { key: `${reason.code}-${reason.list_id}`, to: `/lists/${reason.list_id}`, class: 'inline-flex' }, () => h(Badge, { tint: 'neutral' }, () => reasonLabel(reason)))
          : h(Badge, { key: reason.code, tint: 'neutral' }, () => reasonLabel(reason)),
        ),
      ),
  },
  {
    id: 'priority',
    header: 'Priority',
    cell: (info) => {
      const priority = info.row.original.priority
      return h('span', { class: priority === 'high' ? 'font-semibold text-text' : 'text-text-muted' }, PRIORITY_LABEL[priority])
    },
  },
  {
    id: 'waiting_since',
    header: 'Waiting',
    cell: (info) => {
      const item = info.row.original
      const value = item.priority === 'list'
        ? item.last_contact_attempt?.occurred_at ?? null
        : item.waiting_since
      if (value) {
        return h('span', { title: formatAbsoluteTime(value) }, formatRelativeTime(value))
      }
      // Slice 016b (docs/specs/SLICE_016.md §8): "the Waiting cell shows
      // 'Due <relative>' from the task reason when `waiting_since` is
      // null" — a task-only item's `waiting_since` is always null (§5);
      // a retained item with a real `waiting_since` never reaches here.
      const task = item.priority !== 'list' ? taskReason(item) : null
      if (task) {
        return h('span', { title: formatAbsoluteTime(task.due_at) }, `Due ${formatRelativeTime(task.due_at)}`)
      }
      return h('span', { class: 'text-text-muted' }, 'Never contacted')
    },
  },
  {
    id: 'recommended_action',
    header: 'Recommended',
    cell: (info) => {
      const item = info.row.original
      const needed = outcomeNeeded(item)
      if (needed) {
        return h('div', { class: 'flex items-center gap-1.5', 'data-testid': 'today-set-outcome' }, [
          h(PhoneOutgoing, { class: 'h-4 w-4 shrink-0 text-text-muted', 'stroke-width': 1.5 }),
          h('span', { class: 'text-text' }, 'Set outcome'),
          h('span', { class: 'text-text-muted', title: formatAbsoluteTime(needed.ended_at) }, `Call ${formatRelativeTime(needed.ended_at)} has no outcome yet`),
        ])
      }
      if (item.recommended_action === 'review_person') {
        return h('div', { class: 'flex items-center gap-1.5' }, [
          h('span', { class: 'text-text' }, 'Review person'),
        ])
      }
      const isCall = item.recommended_action === 'call'
      const detail = isCall ? item.person.primary_phone : item.person.primary_email
      return h('div', { class: 'flex items-center gap-1.5' }, [
        h(isCall ? Phone : Mail, { class: 'h-4 w-4 shrink-0 text-text-muted', 'stroke-width': 1.5 }),
        h('span', { class: 'text-text' }, isCall ? 'Call' : 'Email'),
        detail ? h('span', { class: 'text-text-muted' }, detail) : null,
      ])
    },
  },
  {
    id: 'actions',
    header: '',
    cell: (info) => {
      const item = info.row.original
      const needed = outcomeNeeded(item)
      const buttons: VNode[] = []
      if (needed) {
        buttons.push(setOutcomeButton(item, needed.call_id, 'today-set-outcome-action'))
      } else {
        buttons.push(
          h(
            'button',
            {
              type: 'button',
              class: buttonClasses('secondary'),
              onClick: (event: MouseEvent) => {
                // Rows are links (DataTable.vue) — stop the click from also
                // bubbling into the row's own navigate() (SLICE_003 §10).
                event.stopPropagation()
                openLogContact(item)
              },
            },
            'Log contact',
          ),
        )
        const aside = outcomeNeededAside(item)
        if (aside) buttons.push(setOutcomeButton(item, aside.call_id, 'today-set-outcome-aside'))
      }
      // Slice 016b (docs/specs/SLICE_016.md §8): "whenever the item
      // carries a task reason, regardless of `recommended_action`" — the
      // `set_outcome` control precedent: an additional row action, never
      // replacing the ones above.
      const task = taskReason(item)
      if (task) {
        const pending = isCompletingTaskItem(item.person.id, task.task_id)
        buttons.push(
          h(
            'button',
            {
              type: 'button',
              class: buttonClasses('secondary'),
              disabled: pending,
              'data-testid': 'today-task-complete',
              onClick: (event: MouseEvent) => {
                event.stopPropagation()
                onCompleteTaskItem(item.person.id, task.task_id)
              },
            },
            pending ? 'Completing…' : 'Complete',
          ),
        )
      }
      if (buttons.length === 1) return buttons[0]
      return h('div', { class: 'flex items-center justify-end gap-2' }, buttons)
    },
  },
]
</script>

<template>
  <div>
    <PageHeader
      title="Today"
      :subtitle="subtitle"
      stack-action-on-narrow
    >
      <template #action>
        <div class="flex flex-wrap items-center gap-2">
          <button
            type="button"
            :class="buttonClasses('secondary')"
            @click="refreshToday"
          >
            Refresh
          </button>
          <button
            type="button"
            :class="buttonClasses('secondary')"
            @click="showSources = !showSources"
          >
            Manage sources
          </button>
        </div>
      </template>
    </PageHeader>

    <!-- Slice 016b (docs/specs/SLICE_016.md §8): the Tasks panel, below
         the queue header — Overdue / Due soon grouped client-side against
         the response's own `generated_at`, never the browser clock. -->
    <section
      class="mb-5 rounded-xl border border-border bg-surface-0 p-4"
      aria-label="Tasks"
      data-testid="tasks-panel"
    >
      <h2 class="text-body font-semibold text-text">
        Tasks
      </h2>
      <div
        v-if="tasksQuery.isError.value"
        class="mt-3 text-small text-danger"
        role="status"
      >
        Could not load your tasks.
      </div>
      <p
        v-else-if="tasksQuery.isPending.value && !tasksQuery.data.value"
        class="mt-3 text-small text-text-muted"
      >
        Loading tasks…
      </p>
      <template v-else-if="tasksQuery.data.value">
        <div
          v-for="group in [
            { key: 'overdue', label: 'Overdue', rows: overdueTasks },
            { key: 'due_soon', label: 'Due soon', rows: dueSoonTasks },
          ]"
          :key="group.key"
          class="mt-3"
          :data-testid="`task-panel-group-${group.key}`"
        >
          <h3 class="text-small font-semibold text-text-muted">
            {{ group.label }}
          </h3>
          <p
            v-if="group.rows.length === 0"
            class="mt-1 text-small text-text-muted"
          >
            Nothing due
          </p>
          <ul
            v-else
            class="mt-1 divide-y divide-border rounded-lg border border-border"
          >
            <li
              v-for="task in group.rows"
              :key="task.id"
              class="flex flex-wrap items-center gap-3 px-3 py-2"
              :data-testid="`task-panel-row-${task.id}`"
            >
              <RouterLink
                :to="`/people/${task.person.id}`"
                class="min-w-0 flex-1 truncate text-body font-medium text-text hover:underline"
              >
                {{ task.person.display_name }}
              </RouterLink>
              <Badge tint="neutral">
                {{ TASK_KIND_LABEL[task.kind] }}
              </Badge>
              <span class="min-w-0 flex-[2] truncate text-body text-text">{{ task.title }}</span>
              <span
                class="shrink-0 text-small text-text-muted"
                :title="task.due_at ? formatAbsoluteTime(task.due_at) : undefined"
              >{{ task.due_at ? panelDueCellText(task.due_at, generatedAt) : '' }}</span>
              <button
                type="button"
                :class="buttonClasses('secondary')"
                :disabled="isCompletingTaskItem(task.person.id, task.id)"
                :data-testid="`task-panel-complete-${task.id}`"
                @click="onCompleteTaskItem(task.person.id, task.id)"
              >
                {{ isCompletingTaskItem(task.person.id, task.id) ? 'Completing…' : 'Complete' }}
              </button>
              <button
                type="button"
                :class="buttonClasses('ghost')"
                :disabled="isSnoozingTaskItem(task.person.id, task.id)"
                :data-testid="`task-panel-snooze-${task.id}`"
                @click="onSnoozeTaskItem(task.person.id, task.id)"
              >
                Tomorrow
              </button>
            </li>
          </ul>
        </div>
        <p
          v-if="tasksQuery.data.value.truncated"
          class="mt-3 text-small text-text-muted"
        >
          Showing the first 200
        </p>
      </template>
    </section>

    <section
      v-if="showSources"
      ref="sourceManager"
      class="mb-5 rounded-xl border border-border bg-surface-0 p-4"
      aria-label="Today sources"
    >
      <div class="flex items-start justify-between gap-4">
        <div>
          <h2 class="text-body font-semibold text-text">
            Today sources
          </h2>
          <p class="mt-1 text-small text-text-muted">
            Your saved lists can add current matches to Today, including colleagues’ or unassigned People. Broad lists stay active after contact unless their criteria no longer match. Urgent and normal work stay first; list work then shows never-contacted People before the oldest contact. Built-in work keeps its place when Today is full.
          </p>
        </div>
        <RouterLink
          to="/lists"
          class="shrink-0 text-small font-medium text-accent hover:underline"
        >
          Lists
        </RouterLink>
      </div>
      <p
        v-if="sourcesQuery.data.value"
        class="mt-3 text-small text-text-muted"
      >
        {{ sourcesQuery.data.value.sources.length }} of {{ sourcesQuery.data.value.limit }} sources
      </p>
      <div
        v-if="sourcesQuery.isError.value"
        class="mt-3 text-small text-danger"
        role="status"
      >
        Could not load your source settings.
        <button
          type="button"
          class="font-medium text-accent hover:underline"
          @click="retrySources"
        >
          Try again
        </button>
      </div>
      <p
        v-if="sourceNotice"
        class="mt-3 text-small text-danger"
        role="status"
      >
        {{ sourceNotice }}
      </p>
      <p
        v-if="sourcesQuery.isPending.value && !sourcesQuery.data.value"
        class="mt-3 text-small text-text-muted"
      >
        Loading sources…
      </p>
      <ul
        v-else-if="sourcesQuery.data.value"
        class="mt-3 divide-y divide-border rounded-lg border border-border"
      >
        <li
          v-for="source in sourcesQuery.data.value.sources"
          :key="source.list_id"
          class="flex items-center gap-3 px-3 py-2"
        >
          <RouterLink
            :to="`/lists/${source.list_id}`"
            class="min-w-0 flex-1 truncate text-body font-medium text-text"
          >
            {{ source.name }}
          </RouterLink>
          <span
            v-if="sourceError(source.filter_error)"
            class="text-small text-danger"
          >{{ sourceError(source.filter_error) }}</span>
          <button
            type="button"
            :class="buttonClasses('secondary')"
            :disabled="removing === source.list_id"
            :aria-label="`Remove ${source.name} source`"
            data-source-remove
            @click="removeSource(source.list_id)"
          >
            Remove
          </button>
        </li>
        <li
          v-if="sourcesQuery.data.value.sources.length === 0"
          class="px-3 py-3 text-small text-text-muted"
        >
          No saved lists are feeding Today.
        </li>
      </ul>

      <div class="mt-5 border-t border-border pt-4">
        <div class="flex items-center justify-between gap-4">
          <h2 class="text-body font-semibold text-text">
            Rules
          </h2>
          <RouterLink
            to="/manage/today-feeds"
            class="shrink-0 text-small font-medium text-accent hover:underline"
          >
            Manage
          </RouterLink>
        </div>
        <p class="mt-1 text-small text-text-muted">
          The three built-in rules that put People on your Today. An admin can adjust, preview, disable or restore each one.
        </p>
        <div
          v-if="feedsQuery.isError.value"
          class="mt-3 text-small text-danger"
          role="status"
        >
          Could not load Today rules.
        </div>
        <p
          v-else-if="feedsQuery.isPending.value && !feedsQuery.data.value"
          class="mt-3 text-small text-text-muted"
        >
          Loading rules…
        </p>
        <ul
          v-else
          class="mt-3 divide-y divide-border rounded-lg border border-border"
        >
          <li
            v-for="feed in orderedFeeds"
            :key="feed.feed_key"
            class="px-3 py-2"
            :data-testid="`today-rules-${feed.feed_key}`"
          >
            <div class="flex items-center justify-between gap-3">
              <span class="text-body font-medium text-text">{{ TODAY_FEED_LABEL[feed.feed_key] }}</span>
              <span class="shrink-0 text-small text-text-muted">{{ MEMBER_FEED_MARKER_LABEL[memberFeedMarker(feed)] }}</span>
            </div>
            <p
              v-if="feed.description.length"
              class="mt-1 text-small text-text-muted"
            >
              {{ feed.description.join(' · ') }}
            </p>
          </li>
        </ul>
      </div>
    </section>

    <div
      v-if="todayData && todayData.sources.status !== 'complete'"
      class="mb-5 rounded-xl border border-border bg-surface-0 p-4 text-body text-text-muted"
      role="status"
    >
      Some Today rules or sources could not load. Available work is shown.
      <ul
        v-if="todayData.sources.issues.length > 0 || todayData.sources.system_feed_issues.length > 0"
        class="mt-2 list-disc space-y-1 pl-5 text-small"
      >
        <li
          v-for="issue in todayData.sources.system_feed_issues"
          :key="`feed-${issue.feed_key}`"
        >
          <template v-if="issue.fallback">
            {{ fallbackFeedMessage(issue) }}
          </template>
          <template v-else>
            {{ todayFeedIssueMessage(issue.feed_key) }}
          </template>
        </li>
        <li
          v-for="issue in todayData.sources.issues"
          :key="issue.list_id"
        >
          <RouterLink
            :to="`/lists/${issue.list_id}`"
            class="font-medium text-accent hover:underline"
          >
            {{ issue.name }}
          </RouterLink>
          <span> could not load.</span>
        </li>
      </ul>
      <div class="mt-3 flex items-center gap-2">
        <button
          type="button"
          :class="buttonClasses('secondary')"
          @click="refreshToday"
        >
          Retry
        </button>
        <button
          type="button"
          :class="buttonClasses('secondary')"
          @click="showSources = true"
        >
          Manage sources
        </button>
      </div>
    </div>

    <!-- Round-2 review fix 2: a page-level, dismissible alert for a task
         action's error, keyed by task id and independent of any row — the
         016a "deleted elsewhere" banner pattern (PersonDetailView.vue).
         Neither the ranked queue's Complete button nor a panel row whose
         own refetch just removed it had anywhere to show this before. -->
    <div
      v-if="taskActionError"
      role="alert"
      class="mb-4 flex items-center justify-between gap-3 rounded-xl border border-border bg-surface-0 p-3 text-body text-danger"
      data-testid="task-action-error-banner"
    >
      <span>{{ taskActionError.message }}</span>
      <button
        type="button"
        :class="buttonClasses('ghost')"
        data-testid="task-action-error-dismiss"
        @click="taskActionError = null"
      >
        Dismiss
      </button>
    </div>

    <div
      v-if="isError"
      class="rounded-xl border border-border bg-surface-0 p-5 text-body text-danger"
    >
      {{ describeApiError(error, 'Could not load Today.') }}
    </div>
    <div
      v-else-if="isPending"
      class="rounded-xl border border-border bg-surface-0 p-5 text-body text-text-muted"
    >
      Loading…
    </div>
    <DataTable
      v-else
      :data="items"
      :columns="columns"
      :row-key="(item) => item.person.id"
      :row-to="(item) => `/people/${item.person.id}`"
      count-noun="items"
      count-noun-singular="item"
      :truncated="todayData?.truncated ?? false"
      :empty-title="emptyTitle"
      :empty-message="emptyMessage"
      :empty-icon="Sun"
    />

    <LogContactDialog
      v-if="logContactTarget"
      :visible="logContactTarget !== null"
      :org-id="orgId"
      :person-id="logContactTarget.personId"
      :person-name="logContactTarget.personName"
      @update:visible="onDialogVisibleChange"
    />
  </div>
</template>
