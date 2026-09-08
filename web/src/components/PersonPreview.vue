<script setup lang="ts">
import { computed, inject, onBeforeUnmount, onMounted, ref } from 'vue'
import { RouterLink } from 'vue-router'
import { ArrowUpRight, Flag, Inbox, Mail, Phone, Route, Sparkles, UserCheck, X } from 'lucide-vue-next'
import { usePerson } from '../api/queries'
import { ApiError } from '../api/client'
import type { HistoryEntry, PersonSummary } from '../api/types'
import { buttonClasses } from '../lib/controls'
import { describeApiError } from '../lib/errors'
import { formatAbsoluteTime, formatRelativeTime, initials } from '../lib/format'
import { CONTACT_CHANNEL_LABEL, CONTACT_OUTCOME_LABEL } from '../lib/labels'
import { OPERATOR_LAUNCHER } from '../lib/operatorLauncher'
import { CALL_HOST_KEY } from '../telephony/callHost'
import StageLabel from './StageLabel.vue'

const props = defineProps<{ orgId: string; summary: PersonSummary }>()
const emit = defineEmits<{ close: [] }>()
const { data, isPending, isError, error, refetch } = usePerson(() => props.orgId, () => props.summary.id)
const person = computed(() => data.value?.person ?? props.summary)
const notFound = computed(() => error.value instanceof ApiError && [403, 404].includes(error.value.status))
const host = inject(CALL_HOST_KEY, null)
const launchOperator = inject(OPERATOR_LAUNCHER, null)
const root = ref<HTMLElement | null>(null)
const tab = ref<'activity' | 'contact'>('activity')
const phonePicker = ref(false)
const phones = computed(() => data.value?.contact_methods.filter((method) => method.kind === 'phone') ?? [])
const emails = computed(() => data.value?.contact_methods.filter((method) => method.kind === 'email') ?? [])
const callDisabled = computed(() => !phones.value.length || !host || host.call.active.value || host.outcomePromptOpen.value)
const profilePath = computed(() => `/people/${encodeURIComponent(props.summary.id)}`)
const latestSource = computed(() => data.value?.inquiries[0]?.source ?? '—')

function startCall(contactMethodId: string) {
  if (callDisabled.value || !data.value || isError.value) return
  phonePicker.value = false
  host?.startFromPerson(person.value.id, person.value.display_name, contactMethodId)
}
function callPerson() {
  if (callDisabled.value) return
  if (phones.value.length === 1) startCall(phones.value[0]!.id)
  else phonePicker.value = !phonePicker.value
}
function onKeydown(event: KeyboardEvent) {
  if (event.key !== 'Escape' || event.defaultPrevented) return
  const operator = document.querySelector<HTMLElement>('[data-testid="operator-panel"]')
  if (operator && operator.style.display !== 'none') return
  // A higher-level modal/popover owns its Escape first.
  if (document.querySelector('[aria-modal="true"], [role="listbox"]')) return
  if (phonePicker.value) { phonePicker.value = false; return }
  emit('close')
}
onMounted(() => document.addEventListener('keydown', onKeydown))
onBeforeUnmount(() => document.removeEventListener('keydown', onKeydown))
defineExpose({ focus: () => root.value?.focus() })

const icons = { inquiry_received: Inbox, routing_decision: Route, assignment_changed: UserCheck, stage_changed: Flag, contact_attempted: Phone, call_completed: Phone, correspondence: Mail }
function activity(entry: HistoryEntry): { title: string; description: string } {
  switch (entry.kind) {
    case 'inquiry_received': return { title: 'Inquiry received', description: entry.detail.source }
    case 'assignment_changed': return { title: 'Assignment changed', description: entry.detail.to?.display_name ?? 'Unassigned' }
    case 'routing_decision': return { title: 'Inquiry routed', description: entry.detail.assignee?.display_name ?? 'Unassigned' }
    case 'stage_changed': return { title: 'Stage changed', description: `${entry.detail.from_stage?.name ?? 'New person'} → ${entry.detail.to_stage.name}` }
    case 'contact_attempted': return { title: 'Contact attempted', description: `${CONTACT_CHANNEL_LABEL[entry.detail.channel]} · ${CONTACT_OUTCOME_LABEL[entry.detail.outcome]}` }
    // Outcome interpretation/correction stays on the full profile. Never
    // mislabel the system's observation as the agent's chosen outcome.
    case 'call_completed': return { title: 'Call ended', description: 'View outcome in full profile' }
    case 'correspondence': return { title: entry.detail.direction === 'inbound' ? 'Inbound email' : 'Outbound email', description: `${entry.detail.agent.display_name}${entry.detail.backdated ? ' · forwarded' : ''}` }
  }
}
const recent = computed(() => {
  const history = data.value?.history ?? []
  const calls = new Set(history.filter((entry) => entry.kind === 'call_completed').map((entry) => entry.detail.call_id))
  return history.filter((entry) => entry.kind !== 'contact_attempted' ||
    (!entry.detail.superseded && (entry.detail.call_id === null || !calls.has(entry.detail.call_id))))
    .slice(-3).map((entry) => ({ ...entry, ...activity(entry) }))
})
</script>

<template>
  <section
    ref="root"
    role="dialog"
    aria-label="Person preview"
    tabindex="-1"
    class="person-preview glass-panel p-5 focus:outline-none"
    data-testid="person-preview"
  >
    <div class="mb-4 flex items-center justify-end">
      <div class="flex items-center gap-1">
        <RouterLink
          :to="profilePath"
          :class="buttonClasses('ghost')"
          aria-label="Open full profile"
          title="Open full profile"
        >
          <ArrowUpRight
            class="h-4 w-4"
            aria-hidden="true"
          />
        </RouterLink>
        <button
          type="button"
          :class="buttonClasses('ghost')"
          aria-label="Close person preview"
          title="Close preview (Esc)"
          @click="emit('close')"
        >
          <X
            class="h-4 w-4"
            aria-hidden="true"
          />
        </button>
      </div>
    </div>

    <div
      v-if="isError"
      role="alert"
      class="space-y-3 py-6 text-small text-text-muted"
    >
      <p>{{ notFound ? 'This person is no longer available.' : describeApiError(error, 'Could not load this person.') }}</p>
      <button
        v-if="!notFound"
        type="button"
        :class="buttonClasses()"
        @click="refetch()"
      >
        Try again
      </button>
    </div>
    <template v-else>
      <div
        class="avatar-surface mb-4 h-12 w-12"
        aria-hidden="true"
      >
        {{ initials(person.display_name) }}
      </div>
      <h2 class="mb-3 break-words text-title font-medium text-text">
        {{ person.display_name }}
      </h2>
      <StageLabel
        :stage="person.stage"
        badge
      />

      <div
        v-if="data?.tags.length"
        class="mt-2 flex flex-wrap gap-1.5"
        aria-label="Tags"
        data-testid="person-preview-tags"
      >
        <span
          v-for="tag in data.tags"
          :key="tag.id"
          class="inline-flex items-center rounded-lg border border-border px-2 py-0.5 text-small text-text-muted"
        >{{ tag.name }}</span>
      </div>

      <p
        v-if="isPending"
        role="status"
        class="py-8 text-small text-text-muted"
      >
        Loading person…
      </p>
      <template v-else-if="data">
        <div class="preview-actions relative my-6">
          <button
            v-if="host"
            type="button"
            :class="buttonClasses()"
            :disabled="callDisabled"
            aria-label="Call person"
            :title="!phones.length ? 'No phone number' : 'Call person'"
            :aria-expanded="phonePicker"
            @click="callPerson"
          >
            <Phone
              class="h-4 w-4"
              aria-hidden="true"
            />
          </button>
          <a
            v-if="emails[0]"
            :href="`mailto:${encodeURIComponent(emails[0].value)}`"
            :class="buttonClasses()"
            aria-label="Open email app"
            title="Open email app"
          >
            <Mail
              class="h-4 w-4"
              aria-hidden="true"
            />
          </a>
          <RouterLink
            :to="profilePath"
            :class="buttonClasses()"
            aria-label="View and edit full profile"
            title="View and edit full profile"
          >
            <ArrowUpRight
              class="h-4 w-4"
              aria-hidden="true"
            />
          </RouterLink>
        </div>
        <div
          v-if="phonePicker"
          class="glass-control mb-4 rounded-xl p-2"
          aria-label="Choose a phone number"
        >
          <button
            v-for="phone in phones"
            :key="phone.id"
            type="button"
            class="block min-h-10 w-full rounded-md px-2 text-left text-small text-text-muted hover:bg-surface-2"
            @click="startCall(phone.id)"
          >
            {{ phone.value }}
          </button>
        </div>

        <dl class="space-y-3 text-small">
          <div class="flex items-start justify-between gap-4">
            <dt class="text-text-muted">
              Owner
            </dt><dd class="text-right">
              {{ person.assigned_user?.display_name ?? 'Unassigned' }}
            </dd>
          </div>
          <div class="flex items-start justify-between gap-4">
            <dt class="text-text-muted">
              Latest source
            </dt><dd class="max-w-44 break-words text-right">
              {{ latestSource }}
            </dd>
          </div>
          <div class="flex items-start justify-between gap-4">
            <dt class="text-text-muted">
              Added
            </dt><dd :title="formatAbsoluteTime(person.created_at)">
              {{ new Date(person.created_at).toLocaleDateString(undefined, { month: 'short', day: 'numeric', year: 'numeric' }) }}
            </dd>
          </div>
        </dl>

        <div
          class="people-tabs mt-7"
          aria-label="Person information"
        >
          <button
            type="button"
            class="people-tab"
            :aria-pressed="tab === 'activity'"
            @click="tab = 'activity'"
          >
            Activity
          </button>
          <button
            type="button"
            class="people-tab"
            :aria-pressed="tab === 'contact'"
            @click="tab = 'contact'"
          >
            Contact
          </button>
        </div>
        <div v-if="tab === 'activity'">
          <ol
            v-if="recent.length"
            class="preview-timeline space-y-5"
          >
            <li
              v-for="entry in recent"
              :key="entry.id"
              class="relative flex gap-3"
            >
              <span class="glass-control relative flex h-7 w-7 shrink-0 items-center justify-center rounded-lg text-text-muted"><component
                :is="icons[entry.kind]"
                class="h-3.5 w-3.5"
                stroke-width="1.5"
                aria-hidden="true"
              /></span>
              <div class="min-w-0 flex-1 pt-0.5 text-small">
                <p class="text-text">
                  {{ entry.title }}
                </p>
                <p class="mt-1 break-words text-text-muted">
                  {{ entry.description }}
                </p>
                <time
                  :datetime="entry.occurred_at"
                  :title="formatAbsoluteTime(entry.occurred_at)"
                  class="mt-1 block text-text-subtle"
                >{{ formatRelativeTime(entry.occurred_at) }}</time>
              </div>
            </li>
          </ol>
          <p
            v-else
            class="py-4 text-small text-text-muted"
          >
            No activity yet.
          </p>
          <RouterLink
            v-if="recent.length"
            :to="profilePath"
            class="mt-4 inline-flex min-h-10 items-center text-small text-text-muted hover:text-text"
          >
            Full history <ArrowUpRight
              class="ml-1 h-3.5 w-3.5"
              aria-hidden="true"
            />
          </RouterLink>
        </div>
        <ul
          v-else
          class="space-y-3 text-small"
        >
          <li
            v-for="method in data.contact_methods"
            :key="method.id"
            class="flex items-center gap-2 break-all text-text-muted"
          >
            <component
              :is="method.kind === 'email' ? Mail : Phone"
              class="h-4 w-4 shrink-0"
              aria-hidden="true"
            />{{ method.value }}
          </li>
          <li
            v-if="!data.contact_methods.length"
            class="text-text-muted"
          >
            No contact methods.
          </li>
        </ul>

        <button
          v-if="launchOperator"
          type="button"
          class="glass-control mt-6 flex min-h-12 w-full items-center gap-3 rounded-xl px-3 text-left text-small text-text-muted hover:text-text"
          @click="launchOperator(person.id)"
        >
          <Sparkles
            class="h-4 w-4"
            aria-hidden="true"
          />Ask about this person<ArrowUpRight
            class="ml-auto h-4 w-4"
            aria-hidden="true"
          />
        </button>
      </template>
    </template>
  </section>
</template>
