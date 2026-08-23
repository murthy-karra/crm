<script setup lang="ts">
// UI_STYLE.md §7 + docs/specs/SLICE_002.md §10: header card with the
// entity's identity (name, stage Select, assignee Select — both mutating
// inline via POST .../stage and .../assignment), then Contact methods /
// Inquiries / History cards. History renders the server's per-kind
// `detail` shapes exactly as spec §5 documents them, in server order
// (occurred_at, recorded_at, kind_rank, id) — never re-sorted here.
import { computed, onBeforeUnmount, ref, watch, type Component } from 'vue'
import { RouterLink, onBeforeRouteLeave } from 'vue-router'
import Select from 'primevue/select'
import { useQueryClient } from '@tanstack/vue-query'
import { Flag, Inbox, Mail, Phone, PhoneCall, PhoneOutgoing, Route, UserCheck } from 'lucide-vue-next'
import Card from '../components/Card.vue'
import FormField from '../components/FormField.vue'
import Badge from '../components/Badge.vue'
import StageLabel from '../components/StageLabel.vue'
import LogContactDialog from '../components/LogContactDialog.vue'
import CallPanel from '../components/CallPanel.vue'
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
import type { CallOutcomeCorrection, HistoryEntry, RoutingStrategy } from '../api/types'
import { formatAbsoluteTime, formatRelativeTime } from '../lib/format'
import { buttonClasses, selectPt } from '../lib/controls'
import { describeApiError } from '../lib/errors'
import { CONTACT_CHANNEL_LABEL, CONTACT_OUTCOME_LABEL, correctedOutcomeLabel } from '../lib/labels'
import { describeOutcomeError } from '../telephony/errors'
import { callCompletedSummary, showsOutcomePrompt } from '../telephony/format'
import { useCall, type CallRoomFactory } from '../telephony/useCall'
import { createLiveKitRoom } from '../telephony/client'

const props = withDefaults(
  defineProps<{
    id: string
    /** SLICE_006 §10: the LiveKit client factory is injected so tests mount
     * this view with a fake room (no SDK, no WebRTC). */
    createRoom?: CallRoomFactory
  }>(),
  // A Function-typed prop's default is the value itself, not a factory.
  { createRoom: createLiveKitRoom },
)

const { data: me } = useMe()
const orgId = computed(() => me.value?.organization?.id ?? '')
const queryClient = useQueryClient()

const { data: detail, isPending, isError, error } = usePerson(orgId, () => props.id)
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

// ---- Calling (SLICE_006 §10) ----------------------------------------------
// One call per view, owned here as component state (never persisted). The
// header's Call button is the view's primary action until a call is
// active, when the panel's Hang up takes over (UI_STYLE §5: one primary).
const call = useCall({ orgId, createRoom: props.createRoom })

const phones = computed(() => contactMethods.value.filter((cm) => cm.kind === 'phone'))
const callDisabled = computed(() => phones.value.length === 0 || call.active.value)

// ---- Call outcome correction (SLICE_006c §10) ------------------------------
// Two instances of the one mutation: the panel's post-call prompt and the
// History "Change outcome" dialog each own their pending/error state.
const panelOutcome = useCorrectCallOutcome(orgId)
const panelOutcomeSaved = ref<CallOutcomeCorrection | null>(null)
const panelOutcomeError = ref<string | null>(null)

// Computed once and passed to CallPanel: while the prompt is open, Save
// outcome is the view's one primary, so the header's Call button steps down
// to secondary and the History "Change outcome" action is disabled.
const outcomePromptOpen = computed(() =>
  showsOutcomePrompt(call.phase.value, call.error.value !== null, call.call.value, panelOutcomeSaved.value !== null),
)
const callPrimary = computed(() => !call.active.value && !outcomePromptOpen.value)

function resetPanelOutcome() {
  panelOutcomeSaved.value = null
  panelOutcomeError.value = null
  panelOutcome.reset()
}

function onSaveOutcome(outcome: CallOutcomeCorrection) {
  if (panelOutcome.isPending.value) return
  const callId = call.callId.value
  const personId = call.personId.value
  if (callId === '' || personId === '') return
  panelOutcomeError.value = null
  panelOutcome.mutate(
    { callId, personId, outcome },
    {
      onSuccess: (data) => {
        // §1 step 4: the outcome already recorded → nothing written; close.
        if (!data.changed) {
          dismissCall()
          return
        }
        panelOutcomeSaved.value = outcome
      },
      onError: (failure) => {
        panelOutcomeError.value = describeOutcomeError(failure)
        if (failure instanceof ApiError && failure.code === 'correction_conflict') {
          void queryClient.invalidateQueries({ queryKey: queryKeys.person(orgId.value, personId) })
        }
      },
    },
  )
}

function dismissCall() {
  call.dismiss()
  resetPanelOutcome()
}

// History "Change outcome" (§1 step 7). Only the caller's own call-derived,
// non-superseded attempt rows offer it — decided from the row's detail and
// actor, never from its position in the list.
const changeOutcomeOpen = ref(false)
const changeOutcomeTarget = ref<{ callId: string; outcome: CallOutcomeCorrection } | null>(null)
const historyOutcome = useCorrectCallOutcome(orgId)
const historyOutcomeError = ref<string | null>(null)

function changeableOutcome(entry: HistoryEntry): { callId: string; outcome: CallOutcomeCorrection } | null {
  if (entry.kind !== 'contact_attempted') return null
  const { call_id, superseded, outcome } = entry.detail
  if (call_id === null || superseded || outcome === 'sent') return null
  if (entry.actor === null || entry.actor.id !== me.value?.user.id) return null
  return { callId: call_id, outcome }
}

function canChangeOutcome(entry: HistoryEntry): boolean {
  return changeableOutcome(entry) !== null
}

function openChangeOutcome(entry: HistoryEntry) {
  const target = changeableOutcome(entry)
  if (!target || outcomePromptOpen.value) return
  changeOutcomeTarget.value = target
  historyOutcomeError.value = null
  historyOutcome.reset()
  changeOutcomeOpen.value = true
}

function onChangeOutcomeSave(outcome: CallOutcomeCorrection) {
  const target = changeOutcomeTarget.value
  if (!target || historyOutcome.isPending.value) return
  historyOutcomeError.value = null
  historyOutcome.mutate(
    { callId: target.callId, personId: props.id, outcome },
    {
      onSuccess: () => {
        changeOutcomeOpen.value = false
      },
      onError: (failure) => {
        historyOutcomeError.value = describeOutcomeError(failure)
        if (failure instanceof ApiError && failure.code === 'correction_conflict') {
          void queryClient.invalidateQueries({ queryKey: queryKeys.person(orgId.value, props.id) })
        }
      },
    },
  )
}
const pickerOpen = ref(false)
const pickerRoot = ref<HTMLElement | null>(null)
// The callee's name as it was when the call started — the panel keeps
// naming that Person even if the route (and `person`) changes mid-call.
const calleeName = ref('')

function startCall(contactMethodId: string) {
  pickerOpen.value = false
  if (!person.value) return
  calleeName.value = person.value.display_name
  resetPanelOutcome()
  void call.start(person.value.id, contactMethodId)
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
// a finished panel belongs to the previous Person — clear it. An active
// call is left alone; the panel still names the original callee.
watch(
  () => props.id,
  () => {
    pickerOpen.value = false
    changeOutcomeOpen.value = false
    dismissCall()
  },
)

// Leaving the page mid-call ends it (the composable hangs up on dispose),
// so ask first.
onBeforeRouteLeave(() => {
  if (!call.active.value) return true
  return window.confirm('End the call?')
})

const ROUTING_STRATEGY_LABEL: Record<RoutingStrategy, string> = {
  explicit: 'an explicit choice',
  actor_default: 'the default assignee',
  kept_existing: 'the existing assignee',
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
      const { channel, outcome, corrects_id, superseded } = entry.detail
      // SLICE_006c §10: a correction row reads "Outcome corrected — voicemail";
      // a superseded row keeps its text plus "(superseded)". Both are
      // decided from the row's own detail — never from neighbouring rows.
      const base =
        corrects_id !== null
          ? `Outcome corrected — ${correctedOutcomeLabel(outcome)}`
          : // SLICE_003 §1's walkthrough: "Contact attempted — call, no answer".
            `Contact attempted — ${CONTACT_CHANNEL_LABEL[channel].toLowerCase()}, ${CONTACT_OUTCOME_LABEL[outcome].toLowerCase()}`
      return superseded ? `${base} (superseded)` : base
    }
    case 'call_completed': {
      // SLICE_006 §1 steps 4–5: "Call — reached, 1 min 12 s" / "Call — no answer".
      const { outcome, talk_seconds } = entry.detail
      return callCompletedSummary(outcome, talk_seconds)
    }
  }
}
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
            v-for="entry in history"
            :key="entry.id"
            class="flex min-h-14 items-center gap-3 py-2 first:pt-0 last:pb-0"
          >
            <div class="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-surface-2">
              <component
                :is="HISTORY_ICON[entry.kind]"
                class="h-4 w-4 text-text-muted"
                stroke-width="1.5"
              />
            </div>
            <div class="min-w-0 flex-1">
              <p
                class="text-body"
                :class="entry.kind === 'contact_attempted' && entry.detail.superseded ? 'text-text-muted line-through' : 'text-text'"
                :data-superseded="entry.kind === 'contact_attempted' && entry.detail.superseded ? 'true' : undefined"
              >
                {{ historySummary(entry) }}
              </p>
              <p class="text-small text-text-muted">
                {{ entry.actor?.display_name ?? 'System' }} ·
                <span :title="formatAbsoluteTime(entry.occurred_at)">{{ formatRelativeTime(entry.occurred_at) }}</span>
              </p>
            </div>
            <button
              v-if="canChangeOutcome(entry)"
              type="button"
              :class="buttonClasses('ghost')"
              :disabled="outcomePromptOpen"
              data-testid="change-outcome"
              @click="openChangeOutcome(entry)"
            >
              Change outcome
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
        :current-outcome="changeOutcomeTarget?.outcome ?? 'reached'"
        :saving="historyOutcome.isPending.value"
        :error="historyOutcomeError"
        @update:visible="changeOutcomeOpen = $event"
        @save="onChangeOutcomeSave"
      />

      <CallPanel
        :phase="call.phase.value"
        :person-name="calleeName"
        :elapsed-seconds="call.elapsedSeconds.value"
        :muted="call.muted.value"
        :error="call.error.value"
        :call="call.call.value"
        :outcome-prompt="outcomePromptOpen"
        :outcome-saving="panelOutcome.isPending.value"
        :outcome-saved="panelOutcomeSaved"
        :outcome-error="panelOutcomeError"
        @hangup="call.hangup()"
        @toggle-mute="call.toggleMute()"
        @hangup-previous="call.hangupPrevious()"
        @dismiss="dismissCall()"
        @save-outcome="onSaveOutcome"
        @skip="dismissCall()"
      />
    </div>
  </div>
</template>
