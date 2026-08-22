<script setup lang="ts">
// UI_STYLE.md §7 + docs/specs/SLICE_002.md §10: header card with the
// entity's identity (name, stage Select, assignee Select — both mutating
// inline via POST .../stage and .../assignment), then Contact methods /
// Inquiries / History cards. History renders the server's per-kind
// `detail` shapes exactly as spec §5 documents them, in server order
// (occurred_at, recorded_at, kind_rank, id) — never re-sorted here.
import { computed, ref, type Component } from 'vue'
import { RouterLink } from 'vue-router'
import Select from 'primevue/select'
import { Flag, Inbox, Mail, Phone, PhoneCall, Route, UserCheck } from 'lucide-vue-next'
import Card from '../components/Card.vue'
import FormField from '../components/FormField.vue'
import Badge from '../components/Badge.vue'
import StageLabel from '../components/StageLabel.vue'
import LogContactDialog from '../components/LogContactDialog.vue'
import { useAssignPersonMutation, useChangeStageMutation, useMe, useMembers, usePerson, useStages } from '../api/queries'
import { ApiError } from '../api/client'
import type { HistoryEntry, RoutingStrategy } from '../api/types'
import { formatAbsoluteTime, formatRelativeTime } from '../lib/format'
import { buttonClasses, selectPt } from '../lib/controls'
import { describeApiError } from '../lib/errors'
import { CONTACT_CHANNEL_LABEL, CONTACT_OUTCOME_LABEL } from '../lib/labels'

const props = defineProps<{ id: string }>()

const { data: me } = useMe()
const orgId = computed(() => me.value?.organization?.id ?? '')

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
}

const logContactOpen = ref(false)

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
      const { channel, outcome } = entry.detail
      // SLICE_003 §1's walkthrough: "Contact attempted — call, no answer".
      return `Contact attempted — ${CONTACT_CHANNEL_LABEL[channel].toLowerCase()}, ${CONTACT_OUTCOME_LABEL[outcome].toLowerCase()}`
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
          <button
            type="button"
            :class="buttonClasses('secondary')"
            @click="logContactOpen = true"
          >
            Log contact
          </button>
        </div>

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
              <p class="text-body text-text">
                {{ historySummary(entry) }}
              </p>
              <p class="text-small text-text-muted">
                {{ entry.actor?.display_name ?? 'System' }} ·
                <span :title="formatAbsoluteTime(entry.occurred_at)">{{ formatRelativeTime(entry.occurred_at) }}</span>
              </p>
            </div>
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
    </div>
  </div>
</template>
