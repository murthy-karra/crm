<script setup lang="ts">
// SLICE_011d.md §4, §6: "Preview evaluates the candidate definition for a
// chosen active member of the Organization, defaulting to the admin ...
// Preview shows the §4 result in the Today row rendering with a member
// picker defaulting to the admin, with an honest empty state and the 200
// cap notice." Opened both from a feed card (previewing its current
// effective definition) and from inside the editor (previewing the unsaved
// draft) — TodayFeedsView.vue passes whichever `filter`/`freshWithinHours`
// applies. Never persists anything (§4: "a read, admin-only, never
// persisted").
import { computed, h, onBeforeUnmount, ref, watch } from 'vue'
import Dialog from 'primevue/dialog'
import Select from 'primevue/select'
import { Mail, Phone, PhoneOutgoing } from 'lucide-vue-next'
import type { ColumnDef } from '@tanstack/vue-table'
import DataTable from './DataTable.vue'
import Badge from './Badge.vue'
import { useMe, usePreviewTodayFeedMutation } from '../api/queries'
import type { FilterDefinition, MeResponse, Member, PreviewTodayFeedResponse, TodayFeedKey, TodayItem, TodayReason } from '../api/types'
import { buttonClasses, dialogPt, selectPt } from '../lib/controls'
import { describeMutationError } from '../lib/errors'
import { formatAbsoluteTime, formatRelativeTime } from '../lib/format'
import { TODAY_FEED_LABEL } from '../lib/todayFeeds'

const props = defineProps<{
  visible: boolean
  orgId: string
  feedKey: TodayFeedKey
  filter: FilterDefinition
  freshWithinHours: number | null
  members: Member[]
  defaultSubjectId: string
}>()
const emit = defineEmits<{ 'update:visible': [value: boolean] }>()

const subjectId = ref(props.defaultSubjectId)
const memberOptions = computed(() =>
  props.members
    .filter((member) => member.status === 'active')
    .map((member) => ({ user_id: member.user_id, display_name: member.display_name })),
)

const preview = usePreviewTodayFeedMutation(() => props.orgId)

// Session-identity fence (the 011b/011c pattern reused by
// `queries.ts`'s saved-list/Today-source mutations): a preview request that
// outlives the actor or Organization it was started under must never paint
// its result. Local state (not the mutation's own reactive `data`/`error`)
// so a late, identity-mismatched settlement can be silently discarded
// instead of overwriting what the current identity is looking at.
const { data: me } = useMe()
let mounted = true
onBeforeUnmount(() => { mounted = false })

interface RequestIdentity {
  orgId: string
  actorId: string
  session: MeResponse | undefined
}
function captureIdentity(): RequestIdentity {
  return { orgId: me.value?.organization?.id ?? '', actorId: me.value?.user.id ?? '', session: me.value }
}
function identityMatches(identity: RequestIdentity) {
  return mounted && me.value === identity.session &&
    me.value?.organization?.id === identity.orgId && me.value?.user.id === identity.actorId
}

const items = ref<PreviewTodayFeedResponse | null>(null)
const previewPending = ref(false)
const previewErrorValue = ref<unknown>(null)

function run() {
  const identity = captureIdentity()
  items.value = null
  previewErrorValue.value = null
  previewPending.value = true
  preview.mutate(
    {
      feedKey: props.feedKey,
      body: { filter: props.filter, fresh_within_hours: props.freshWithinHours, subject_user_id: subjectId.value },
    },
    {
      onSuccess: (data) => {
        if (!identityMatches(identity)) return
        items.value = data
      },
      onError: (error) => {
        if (!identityMatches(identity)) return
        previewErrorValue.value = error
      },
      onSettled: () => {
        if (!identityMatches(identity)) return
        previewPending.value = false
      },
    },
  )
}

// `immediate: true`: the parent only mounts this component once it has a
// candidate to preview (`v-if="previewState"` alongside `:visible="previewState
// !== null"`), so `visible` is already `true` at the very first setup pass —
// a non-immediate watcher would never observe that as a "change" and the
// initial preview would never run.
watch(() => props.visible, (open) => {
  if (!open) return
  subjectId.value = props.defaultSubjectId
  run()
}, { immediate: true })

// Guarded so a stale/removed id (e.g. a race with deactivation elsewhere)
// can never reach the server — the picker's own options are already
// filtered to active members, so this also fails closed against anything
// driving the Select outside a normal user click.
function onSubjectChange(value: unknown) {
  if (typeof value !== 'string' || value === subjectId.value) return
  if (!memberOptions.value.some((member) => member.user_id === value)) return
  subjectId.value = value
  run()
}

function close() {
  emit('update:visible', false)
}

// Preview evaluates exactly this one feed alone (§4) — no list sources and
// no other feeds, so only this feed's own canonical reason codes can ever
// appear (§3's "Reasons it can emit" column). `list_member` cannot occur
// here; kept for type exhaustiveness only.
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
  }
}

const PRIORITY_LABEL: Record<TodayItem['priority'], string> = { high: 'High', normal: 'Normal', list: 'From your lists', low: 'Low' }

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
        info.row.original.reasons.map((reason) => h(Badge, { key: reason.code, tint: 'neutral' }, () => reasonLabel(reason))),
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
      const value = info.row.original.waiting_since
      return value
        ? h('span', { title: formatAbsoluteTime(value) }, formatRelativeTime(value))
        : h('span', { class: 'text-text-muted' }, 'Never contacted')
    },
  },
  {
    id: 'recommended',
    header: 'Recommended',
    cell: (info) => {
      const item = info.row.original
      if (item.recommended_action === 'set_outcome') {
        return h('div', { class: 'flex items-center gap-1.5' }, [
          h(PhoneOutgoing, { class: 'h-4 w-4 shrink-0 text-text-muted', 'stroke-width': 1.5 }),
          h('span', { class: 'text-text' }, 'Set outcome'),
        ])
      }
      if (item.recommended_action === 'review_person') {
        return h('span', { class: 'text-text' }, 'Review person')
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
]
</script>

<template>
  <Dialog
    :visible="visible"
    modal
    :closable="true"
    :pt="{ ...dialogPt(), root: { class: 'glass-panel w-full max-w-3xl' } }"
    @update:visible="(value: boolean) => !value && close()"
  >
    <template #header>
      <h2 class="text-section font-semibold text-text">
        Preview: {{ TODAY_FEED_LABEL[feedKey] }}
      </h2>
    </template>

    <label class="mb-4 block max-w-xs">
      <span class="mb-1.5 block text-small font-medium text-text">Preview for</span>
      <Select
        :model-value="subjectId"
        :options="memberOptions"
        option-label="display_name"
        option-value="user_id"
        aria-label="Preview for member"
        :pt="selectPt()"
        data-testid="preview-subject"
        @update:model-value="onSubjectChange"
      />
    </label>

    <div
      v-if="previewPending"
      class="rounded-xl border border-border bg-surface-0 p-5 text-body text-text-muted"
      role="status"
    >
      Loading preview…
    </div>
    <div
      v-else-if="previewErrorValue"
      class="rounded-xl border border-border bg-surface-0 p-5 text-body text-danger"
    >
      {{ describeMutationError(previewErrorValue, 'Could not load this preview.') }}
      <button
        type="button"
        :class="buttonClasses('secondary')"
        class="mt-3"
        @click="run"
      >
        Retry
      </button>
    </div>
    <template v-else-if="items">
      <DataTable
        :data="items.items"
        :columns="columns"
        :row-key="(item) => item.person.id"
        :row-to="(item) => `/people/${item.person.id}`"
        count-noun="matches"
        count-noun-singular="match"
        :truncated="items.truncated"
        empty-title="No matches"
        :empty-message="`No people currently match this rule for ${items.subject.display_name}.`"
      />
    </template>

    <template #footer>
      <button
        type="button"
        :class="buttonClasses('secondary')"
        @click="close"
      >
        Close
      </button>
    </template>
  </Dialog>
</template>
