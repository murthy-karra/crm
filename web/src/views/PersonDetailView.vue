<script setup lang="ts">
// UI_STYLE.md §7 + docs/specs/SLICE_002.md §10: header card with the
// entity's identity (name, stage Select, assignee Select — both mutating
// inline via POST .../stage and .../assignment), then Contact methods /
// Inquiries / History cards. History renders the server's per-kind
// `detail` shapes exactly as spec §5 documents them, in server order
// (occurred_at, recorded_at, kind_rank, id) — never re-sorted here.
import { computed, onBeforeUnmount, ref, watch, type Component } from 'vue'
import { RouterLink, useRoute, useRouter } from 'vue-router'
import Select from 'primevue/select'
import { useQueryClient } from '@tanstack/vue-query'
import { Flag, Inbox, Mail, Phone, PhoneCall, PhoneOutgoing, Route, UserCheck } from 'lucide-vue-next'
import Card from '../components/Card.vue'
import FormField from '../components/FormField.vue'
import Badge from '../components/Badge.vue'
import StageLabel from '../components/StageLabel.vue'
import LogContactDialog from '../components/LogContactDialog.vue'

import ChangeOutcomeDialog from '../components/ChangeOutcomeDialog.vue'
import {
  queryKeys,
  useAssignPersonMutation,
  useChangeStageMutation,
  useCorrectCallOutcome,
  useMe,
  useMembers,
  usePerson,
  useStages,
} from '../api/queries'
import { ApiError } from '../api/client'
import type { ActorRef, CallOutcomeCorrection, ContactAttemptedDetail, HistoryEntry, RoutingStrategy } from '../api/types'
import { formatAbsoluteTime, formatRelativeTime } from '../lib/format'
import { buttonClasses, selectPt } from '../lib/controls'
import { describeApiError } from '../lib/errors'
import { CONTACT_CHANNEL_LABEL, CONTACT_OUTCOME_LABEL, correctedOutcomeLabel } from '../lib/labels'
import { describeOutcomeError } from '../telephony/errors'
import { callCompletedSummary, formatTalkSeconds } from '../telephony/format'
import { useCallHost } from '../telephony/callHost'

const props = defineProps<{
  id: string
}>()

const { data: me } = useMe()
const orgId = computed(() => me.value?.organization?.id ?? '')
const queryClient = useQueryClient()

const { data: detail, isPending, isFetching, isError, error } = usePerson(orgId, () => props.id)
const person = computed(() => detail.value?.person)
const contactMethods = computed(() => detail.value?.contact_methods ?? [])
const inquiries = computed(() => detail.value?.inquiries ?? [])
const history = computed(() => detail.value?.history ?? [])

const notFound = computed(() => error.value instanceof ApiError && error.value.status === 404)

const { data: stagesData, isPending: stagesPending } = useStages(orgId)
const stages = computed(() => stagesData.value?.stages ?? [])

const { data: membersData, isPending: membersPending } = useMembers(orgId)
const assigneeOptions = computed(() => [
  { id: null as string | null, display_name: 'Unassigned' },
  ...(membersData.value?.members ?? []).map((member) => ({
    id: member.user_id as string | null,
    display_name: member.display_name,
  })),
])

const { mutate: setStage, isPending: stagePending, error: stageError } = useChangeStageMutation(orgId)
const { mutate: setAssignee, isPending: assigneePending, error: assigneeError } = useAssignPersonMutation(orgId)

function onStageChange(value: unknown) {
  if (typeof value !== 'string') return
  setStage({ personId: props.id, stageId: value })
}

function onAssigneeChange(value: unknown) {
  if (typeof value !== 'string' && value !== null) return
  setAssignee({ personId: props.id, assignedUserId: value })
}

const HISTORY_ICON: Record<HistoryEntry['kind'], Component> = {
  inquiry_received: Inbox,
  routing_decision: Route,
  assignment_changed: UserCheck,
  stage_changed: Flag,
  contact_attempted: PhoneCall,
  call_completed: PhoneOutgoing,
}

const logContactOpen = ref(false)

// ---- Calling (SLICE_006 §10; SLICE_006b §6) --------------------------------
// The call session and docked panel live in the app-level call host
// (AppShell provides it; CallHostPanel renders it) so the Ask drawer's
// Confirm shares them and the panel survives navigation. This view keeps
// its Call button, number picker, and History outcome dialog.
const host = useCallHost()
const { call } = host

const phones = computed(() => contactMethods.value.filter((cm) => cm.kind === 'phone'))
const callDisabled = computed(() => phones.value.length === 0 || call.active.value)

// ---- Call outcome (SLICE_006c §10, §5a) -------------------------------------
// The panel's post-call prompt moved to the call host (SLICE_006b §6);
// this view keeps the History Set/Change-outcome dialog and the
// `?outcome=` deep link.
// While the prompt is open, Save outcome is the app's one primary, so the
// header's Call button steps down to secondary and the History "Change
// outcome" action is disabled (UI_STYLE §5: one primary).
const outcomePromptOpen = host.outcomePromptOpen
const callPrimary = computed(() => !call.active.value && !outcomePromptOpen.value)

// History "Set outcome" / "Change outcome" (§1 step 7, §5a). Offered on the
// call row when the caller is me and the call has an effective attempt —
// decided from the folded row (below), never from its position in the list.
// `outcome` is the agent's current choice, or null while the call is still
// incomplete (the dialog then opens with nothing selected).
interface OutcomeTarget {
  callId: string
  outcome: CallOutcomeCorrection | null
}
const changeOutcomeOpen = ref(false)
const changeOutcomeTarget = ref<OutcomeTarget | null>(null)
const historyOutcome = useCorrectCallOutcome(orgId)
const historyOutcomeError = ref<string | null>(null)
const historyOutcomeSaving = ref(false)

function openChangeOutcome(target: OutcomeTarget | null) {
  if (!target || outcomePromptOpen.value) return
  changeOutcomeTarget.value = target
  historyOutcomeError.value = null
  historyOutcomeSaving.value = false
  historyOutcome.reset()
  changeOutcomeOpen.value = true
}

function onChangeOutcomeSave(outcome: CallOutcomeCorrection) {
  const target = changeOutcomeTarget.value
  if (!target || historyOutcomeSaving.value || historyOutcome.isPending.value) return
  historyOutcomeSaving.value = true
  historyOutcomeError.value = null
  historyOutcome.mutate(
    { callId: target.callId, personId: props.id, outcome },
    {
      onSuccess: () => {
        closeChangeOutcome()
      },
      onError: (failure) => {
        historyOutcomeError.value = describeOutcomeError(failure)
        if (failure instanceof ApiError && failure.code === 'correction_conflict') {
          void queryClient.invalidateQueries({ queryKey: queryKeys.person(orgId.value, props.id) })
        }
      },
      onSettled: () => {
        historyOutcomeSaving.value = false
      },
    },
  )
}

function closeChangeOutcome() {
  changeOutcomeOpen.value = false
  clearOutcomeQuery()
}

// Today → Person (§5a): `/people/{id}?outcome=<call_id>` opens the dialog
// for that call once History has loaded — with nothing selected while the
// call is incomplete, or as "Change outcome" pre-selected with the current
// choice; the param is cleared when the dialog closes (Save or Cancel) and
// when a *settled* fetch shows no row I can act on (not mine, or gone) —
// never while a refetch is still in flight.
const route = useRoute()
const router = useRouter()
const outcomeQuery = computed(() => {
  const value = route.query.outcome
  return typeof value === 'string' && value !== '' ? value : null
})

function clearOutcomeQuery() {
  if (outcomeQuery.value === null) return
  const query = { ...route.query }
  delete query.outcome
  void router.replace({ query })
}
// The call id the param has already opened the dialog for: the param is
// cleared asynchronously (router.replace), so a refetch landing in between
// must not reopen the dialog just closed.
const outcomeQueryHandled = ref<string | null>(null)

const pickerOpen = ref(false)
const pickerRoot = ref<HTMLElement | null>(null)
function startCall(contactMethodId: string) {
  pickerOpen.value = false
  if (!person.value) return
  host.startFromPerson(person.value.id, person.value.display_name, contactMethodId)
}

function onCallClick() {
  if (callDisabled.value || phones.value.length === 0) return
  if (phones.value.length === 1) {
    startCall(phones.value[0].id)
    return
  }
  // Several phones: the number picker (§10 "only with several").
  pickerOpen.value = !pickerOpen.value
}

function onDocumentClick(event: MouseEvent) {
  if (!pickerOpen.value) return
  const target = event.target
  if (target instanceof Node && pickerRoot.value?.contains(target)) return
  pickerOpen.value = false
}

function onDocumentKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape' && pickerOpen.value) pickerOpen.value = false
}

watch(pickerOpen, (open) => {
  if (open) {
    document.addEventListener('click', onDocumentClick, true)
    document.addEventListener('keydown', onDocumentKeydown)
  } else {
    document.removeEventListener('click', onDocumentClick, true)
    document.removeEventListener('keydown', onDocumentKeydown)
  }
})
onBeforeUnmount(() => {
  document.removeEventListener('click', onDocumentClick, true)
  document.removeEventListener('keydown', onDocumentKeydown)
})

// A different Person (route param change while this view stays mounted):
// close this view's own popovers. The panel is app-level now (SLICE_006b
// §6): a call — or an unanswered D-033 outcome prompt — survives
// navigation, still naming the original callee.
watch(
  () => props.id,
  () => {
    pickerOpen.value = false
    changeOutcomeOpen.value = false
  },
)

const ROUTING_STRATEGY_LABEL: Record<RoutingStrategy, string> = {
  explicit: 'an explicit choice',
  actor_default: 'the default assignee',
  kept_existing: 'the existing assignee',
  organization_default: 'the organization default',
  unassigned: 'no default assignee',
  round_robin: 'round-robin',
}

function historySummary(entry: HistoryEntry): string {
  switch (entry.kind) {
    case 'inquiry_received': {
      const { source, person_created, matched_by } = entry.detail
      return person_created
        ? `Inquiry received via ${source} — new person created`
        : `Inquiry received via ${source} — matched by ${matched_by ?? 'existing contact'}`
    }
    case 'routing_decision': {
      const { assignee, strategy } = entry.detail
      return assignee
        ? `Routed to ${assignee.display_name} (${ROUTING_STRATEGY_LABEL[strategy]})`
        : `Routing decided (${ROUTING_STRATEGY_LABEL[strategy]}) — left unassigned`
    }
    case 'assignment_changed': {
      const { from, to } = entry.detail
      if (to) return from ? `Reassigned from ${from.display_name} to ${to.display_name}` : `Assigned to ${to.display_name}`
      return from ? `Unassigned (previously ${from.display_name})` : 'Unassigned'
    }
    case 'stage_changed': {
      const { from_stage, to_stage } = entry.detail
      return from_stage
        ? `Stage changed from ${from_stage.name} to ${to_stage.name}`
        : `Stage set to ${to_stage.name}`
    }
    case 'contact_attempted': {
      const { channel, outcome } = entry.detail
      // SLICE_003 §1's walkthrough: "Contact attempted — call, no answer".
      return `Contact attempted — ${CONTACT_CHANNEL_LABEL[channel].toLowerCase()}, ${CONTACT_OUTCOME_LABEL[outcome].toLowerCase()}`
    }
    case 'call_completed': {
      // SLICE_006 §1 steps 4–5: "Call — reached, 1 min 12 s" / "Call — no answer".
      const { outcome, talk_seconds } = entry.detail
      return callCompletedSummary(outcome, talk_seconds)
    }
  }
}

// ---- One row per call (SLICE_006c §5a, D-033) -----------------------------
// A presentation-only fold over `history` (the wire shape is unchanged): each
// `call_completed` entry absorbs the `contact_attempted` entries sharing its
// `call_id` — the automatic attempt and the agent's choices — into ONE row at
// the call's position. The effective (non-superseded) attempt decides the
// label: an agent choice (`corrects_id !== null`) → "Call — voicemail, 7 s";
// the automatic root → "Call — 7 s · outcome needed" (duration only when
// answered; "Call · outcome needed" otherwise). The system's observation is
// never rendered as the outcome. Call-derived attempts whose call row is
// missing (should not happen) fall through as ordinary rows so nothing is
// silently lost; manual attempts (`call_id === null`) are untouched.
interface HistoryRow {
  key: string
  icon: Component
  summary: string
  actor: ActorRef | null
  occurredAt: string
  /** The Set/Change-outcome target when the row's call is mine and has an
   * effective attempt; `outcome` null while the call is incomplete. */
  change: OutcomeTarget | null
}

type AttemptEntry = HistoryEntry & { kind: 'contact_attempted'; detail: ContactAttemptedDetail }

function plainRow(entry: HistoryEntry): HistoryRow {
  return {
    key: entry.id,
    icon: HISTORY_ICON[entry.kind],
    summary: historySummary(entry),
    actor: entry.actor,
    occurredAt: entry.occurred_at,
    change: null,
  }
}

const historyRows = computed<HistoryRow[]>(() => {
  const entries = history.value
  const attemptsByCall = new Map<string, AttemptEntry[]>()
  for (const entry of entries) {
    if (entry.kind !== 'contact_attempted' || entry.detail.call_id === null) continue
    const list = attemptsByCall.get(entry.detail.call_id) ?? []
    list.push(entry)
    attemptsByCall.set(entry.detail.call_id, list)
  }
  const completedCalls = new Set(entries.filter((e) => e.kind === 'call_completed').map((e) => e.detail.call_id))

  const rows: HistoryRow[] = []
  for (const entry of entries) {
    if (entry.kind === 'contact_attempted') {
      const { call_id } = entry.detail
      if (call_id !== null && completedCalls.has(call_id)) continue
      rows.push(plainRow(entry))
      continue
    }
    if (entry.kind !== 'call_completed') {
      rows.push(plainRow(entry))
      continue
    }
    const { call_id, talk_seconds } = entry.detail
    const attempts = attemptsByCall.get(call_id) ?? []
    const effective = attempts.find((a) => !a.detail.superseded) ?? null
    const actor = entry.actor ?? effective?.actor ?? null
    const chosen = effective !== null && effective.detail.corrects_id !== null

    let summary = historySummary(entry)
    if (effective && chosen) {
      const duration = talk_seconds === null ? '' : `, ${formatTalkSeconds(talk_seconds)}`
      summary = `Call — ${correctedOutcomeLabel(effective.detail.outcome)}${duration}`
    } else if (effective) {
      summary = talk_seconds === null ? 'Call · outcome needed' : `Call — ${formatTalkSeconds(talk_seconds)} · outcome needed`
    }

    const mine = actor !== null && actor.id === me.value?.user.id
    let change: OutcomeTarget | null = null
    if (mine && effective) {
      const { outcome } = effective.detail
      if (chosen && outcome !== 'sent') change = { callId: call_id, outcome }
      else if (!chosen) change = { callId: call_id, outcome: null }
    }

    rows.push({ key: entry.id, icon: HISTORY_ICON.call_completed, summary, actor, occurredAt: entry.occurred_at, change })
  }
  return rows
})

// Declared after `historyRows` — `immediate` runs it synchronously.
watch(
  [outcomeQuery, () => detail.value, () => isFetching.value, outcomePromptOpen],
  ([callId, loaded, fetching, promptOpen]) => {
    if (callId === null) {
      outcomeQueryHandled.value = null
      return
    }
    if (!loaded || changeOutcomeOpen.value || promptOpen || outcomeQueryHandled.value === callId) return
    const row = historyRows.value.find((r) => r.change?.callId === callId)
    if (row?.change) {
      outcomeQueryHandled.value = callId
      openChangeOutcome(row.change)
    } else if (!fetching) {
      clearOutcomeQuery()
    }
  },
  { immediate: true },
)
</script>

<template>
  <div>
    <nav class="mb-2 text-small text-text-muted">
      <RouterLink
        to="/people"
        class="hover:text-text"
      >
        People
      </RouterLink>
      <span
        v-if="person"
        class="mx-1.5"
      >/</span>
      <span
        v-if="person"
        class="text-text"
      >{{ person.display_name }}</span>
    </nav>

    <div
      v-if="notFound"
      class="rounded-xl border border-border bg-surface-0 p-5 text-body text-text-muted"
    >
      Person not found.
    </div>
    <div
      v-else-if="isError"
      class="rounded-xl border border-border bg-surface-0 p-5 text-body text-danger"
    >
      {{ describeApiError(error, 'Could not load this person.') }}
    </div>
    <div
      v-else-if="isPending"
      class="rounded-xl border border-border bg-surface-0 p-5 text-body text-text-muted"
    >
      Loading…
    </div>

    <div
      v-else-if="person"
      class="space-y-4"
    >
      <Card>
        <div class="flex items-center justify-between gap-4">
          <h1 class="text-title font-semibold tracking-title text-text">
            {{ person.display_name }}
          </h1>
          <div class="flex items-center gap-3">
            <button
              type="button"
              :class="buttonClasses('secondary')"
              data-testid="log-contact"
              @click="logContactOpen = true"
            >
              Log contact
            </button>
            <div
              ref="pickerRoot"
              class="relative"
            >
              <button
                type="button"
                :class="buttonClasses(callPrimary ? 'primary' : 'secondary')"
                :disabled="callDisabled"
                :title="phones.length === 0 ? 'No phone number' : undefined"
                :aria-expanded="phones.length > 1 ? pickerOpen : undefined"
                :aria-haspopup="phones.length > 1 ? 'menu' : undefined"
                data-testid="call-button"
                @click="onCallClick"
              >
                <Phone
                  class="h-[18px] w-[18px]"
                  stroke-width="1.5"
                />
                Call
              </button>
              <div
                v-if="pickerOpen && phones.length > 1"
                role="menu"
                class="absolute right-0 top-full z-50 mt-2 min-w-56 rounded-xl border border-border bg-surface-0 py-1 shadow-floating"
                data-testid="call-number-picker"
              >
                <button
                  v-for="phone in phones"
                  :key="phone.id"
                  type="button"
                  role="menuitem"
                  class="flex h-10 w-full items-center px-3 text-left text-body text-text transition-colors duration-150 ease-out hover:bg-surface-2 focus-visible:outline-none focus-visible:bg-surface-2"
                  @click="startCall(phone.id)"
                >
                  {{ phone.value }}
                </button>
              </div>
            </div>
          </div>
        </div>
        <p
          v-if="phones.length === 0"
          class="mt-1.5 text-right text-small text-text-muted"
          data-testid="call-no-phone"
        >
          No phone number
        </p>

        <div class="mt-4 flex flex-wrap gap-6">
          <FormField
            label="Stage"
            bare
          >
            <Select
              :model-value="person.stage.id"
              :options="stages"
              option-label="name"
              option-value="id"
              aria-label="Stage"
              :loading="stagesPending"
              :disabled="stagePending"
              :pt="selectPt()"
              class="w-56"
              @update:model-value="onStageChange"
            >
              <template #value>
                <StageLabel :stage="person.stage" />
              </template>
              <template #option="{ option }">
                <StageLabel :stage="option" />
              </template>
            </Select>
            <p
              v-if="stageError"
              class="mt-1.5 text-small text-danger"
            >
              {{ describeApiError(stageError, 'Could not update the stage.') }}
            </p>
          </FormField>

          <FormField
            label="Assignee"
            bare
          >
            <Select
              :model-value="person.assigned_user?.id ?? null"
              :options="assigneeOptions"
              option-label="display_name"
              option-value="id"
              aria-label="Assignee"
              :loading="membersPending"
              :disabled="assigneePending"
              :pt="selectPt()"
              class="w-56"
              @update:model-value="onAssigneeChange"
            />
            <p
              v-if="assigneeError"
              class="mt-1.5 text-small text-danger"
            >
              {{ describeApiError(assigneeError, 'Could not update the assignee.') }}
            </p>
          </FormField>
        </div>
      </Card>

      <Card>
        <h2 class="mb-4 text-section font-semibold text-text">
          Contact methods
        </h2>
        <ul
          v-if="contactMethods.length > 0"
          class="divide-y divide-border"
        >
          <li
            v-for="cm in contactMethods"
            :key="cm.id"
            class="flex items-center gap-3 py-3 first:pt-0 last:pb-0"
          >
            <Mail
              v-if="cm.kind === 'email'"
              class="h-4 w-4 shrink-0 text-text-muted"
              stroke-width="1.5"
            />
            <Phone
              v-else
              class="h-4 w-4 shrink-0 text-text-muted"
              stroke-width="1.5"
            />
            <span class="text-body text-text">{{ cm.value }}</span>
          </li>
        </ul>
        <p
          v-else
          class="text-body text-text-muted"
        >
          No contact methods.
        </p>
      </Card>

      <Card>
        <h2 class="mb-4 text-section font-semibold text-text">
          Inquiries
        </h2>
        <ul
          v-if="inquiries.length > 0"
          class="divide-y divide-border"
        >
          <li
            v-for="inquiry in inquiries"
            :key="inquiry.id"
            class="py-3 first:pt-0 last:pb-0"
          >
            <div class="flex items-center justify-between gap-4">
              <Badge tint="warm">
                {{ inquiry.source }}
              </Badge>
              <span
                class="text-small text-text-muted"
                :title="formatAbsoluteTime(inquiry.received_at)"
              >{{ formatRelativeTime(inquiry.received_at) }}</span>
            </div>
            <p
              v-if="inquiry.message"
              class="mt-2 text-body text-text"
            >
              {{ inquiry.message }}
            </p>
          </li>
        </ul>
        <p
          v-else
          class="text-body text-text-muted"
        >
          No inquiries yet.
        </p>
      </Card>

      <Card>
        <h2 class="mb-4 text-section font-semibold text-text">
          History
        </h2>
        <ul
          v-if="history.length > 0"
          class="divide-y divide-border"
        >
          <li
            v-for="row in historyRows"
            :key="row.key"
            class="flex min-h-14 items-center gap-3 py-2 first:pt-0 last:pb-0"
          >
            <div class="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-surface-2">
              <component
                :is="row.icon"
                class="h-4 w-4 text-text-muted"
                stroke-width="1.5"
              />
            </div>
            <div class="min-w-0 flex-1">
              <p class="text-body text-text">
                {{ row.summary }}
              </p>
              <p class="text-small text-text-muted">
                {{ row.actor?.display_name ?? 'System' }} ·
                <span :title="formatAbsoluteTime(row.occurredAt)">{{ formatRelativeTime(row.occurredAt) }}</span>
              </p>
            </div>
            <button
              v-if="row.change"
              type="button"
              :class="buttonClasses('ghost')"
              :disabled="outcomePromptOpen"
              data-testid="change-outcome"
              @click="openChangeOutcome(row.change)"
            >
              {{ row.change.outcome === null ? 'Set outcome' : 'Change outcome' }}
            </button>
          </li>
        </ul>
        <p
          v-else
          class="text-body text-text-muted"
        >
          No history yet.
        </p>
      </Card>

      <LogContactDialog
        :visible="logContactOpen"
        :org-id="orgId"
        :person-id="person.id"
        :person-name="person.display_name"
        @update:visible="logContactOpen = $event"
      />

      <ChangeOutcomeDialog
        :visible="changeOutcomeOpen"
        :person-name="person.display_name"
        :current-outcome="changeOutcomeTarget?.outcome ?? null"
        :saving="historyOutcomeSaving || historyOutcome.isPending.value"
        :error="historyOutcomeError"
        @update:visible="(value: boolean) => (value ? (changeOutcomeOpen = true) : closeChangeOutcome())"
        @save="onChangeOutcomeSave"
      />
    </div>
  </div>
</template>
