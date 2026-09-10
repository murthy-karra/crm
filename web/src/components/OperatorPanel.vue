<script setup lang="ts">
// The Ask drawer (docs/specs/SLICE_005.md §10). Transcript + textarea +
// Send + Clear. Hard rules (§7, §10, Lane B brief):
// - `reply` is rendered by text interpolation only — never `v-html`, never
//   markdown, never auto-linking. A UUID or an `<a href>` in a reply is
//   literal text.
// - Cards come only from `references.people`.
// - History is component state only (no localStorage, no server); the last
//   six messages travel with each turn and Clear resets it.
// - Screen context is derived from the route at send time, not open time.
import { computed, nextTick, onBeforeUnmount, reactive, ref, watch } from 'vue'
import { useRoute } from 'vue-router'
import { useQueryClient } from '@tanstack/vue-query'
import { Check, ListChecks, PhoneOutgoing, X } from 'lucide-vue-next'
import { useConfirmTaskProposal, useOperatorTurn, useReopenTaskMutation, settleTaskMutation } from '../api/queries'
import type {
  OperatorCreateTaskProposal,
  OperatorHistoryMessage,
  OperatorPersonCard,
  OperatorProposal,
  OperatorReceipt,
  OperatorStartCallProposal,
} from '../api/types'
import { ApiError } from '../api/client'
import { buttonClasses, TEXTAREA_CLASSES } from '../lib/controls'
import { describeApiError, describeTaskError } from '../lib/errors'
import {
  currentUtcOffsetMinutes,
  deriveScreenContext,
  describeOperatorError,
  describeProposalDue,
  describeReceiptDue,
  historyWindow,
  MAX_MESSAGE_CHARS,
} from '../lib/operator'
import { TASK_KIND_LABEL } from '../lib/tasks'
import OperatorPersonCardView from './OperatorPersonCard.vue'
import { useCallHost } from '../telephony/callHost'
import { isSessionVerified } from '../sessionLifecycle'

const props = defineProps<{ identityKey?: string }>()
const emit = defineEmits<{ close: [] }>()

interface TranscriptEntry {
  id: number
  role: 'user' | 'assistant'
  text: string
  cards: OperatorPersonCard[]
  /** SLICE_006b §6: the turn's `start_call` proposal — rendered from this
   * server object only, never from model prose. Narrowed from the wire's
   * `proposal` union at push time (docs/specs/SLICE_018.md §5), so the
   * template never has to re-narrow `entry.proposal.kind` itself before
   * calling `callHost.startFromProposal`. */
  startCallProposal?: OperatorStartCallProposal
  /** docs/specs/SLICE_018.md §8: the turn's `create_task` proposal, same
   * narrowing reasoning. */
  createTaskProposal?: OperatorCreateTaskProposal
  /** docs/specs/SLICE_018.md §8: the turn's `complete_task` receipt —
   * rendered from this server object only, never from model prose. */
  receipt?: OperatorReceipt
}

/** Per-proposal card state. `final` cards keep their message and never
 * re-enable (consumed/expired/executed); non-final failures (mic denied)
 * leave the proposal intact so Confirm can be clicked again. */
interface ProposalCardState {
  status: 'idle' | 'confirming' | 'started' | 'failed'
  final: boolean
  message: string | null
}

/** docs/specs/SLICE_018.md §8: the `create_task` proposal card's own
 * state, the `ProposalCardState` shape with `create_task`'s own status
 * vocabulary (no docked panel to hand off to — `confirmed` is terminal
 * here, not `started`). */
interface TaskProposalCardState {
  status: 'idle' | 'confirming' | 'confirmed' | 'failed'
  final: boolean
  message: string | null
}

/** docs/specs/SLICE_018.md §8: the receipt card's Undo state, keyed by the
 * TRANSCRIPT ENTRY's own id, not `task_id` (review round 1: two distinct
 * turns can both complete, or re-complete after an Undo, the SAME task —
 * `session-only, the SLICE_006b proposal precedent` never claimed task
 * ids are unique across a session's receipts — so keying by `task_id`
 * would let a second receipt's card silently inherit the first one's
 * already-`final`/already-`'Reopened'` Undo state). */
interface ReceiptCardState {
  status: 'idle' | 'undoing' | 'done' | 'failed'
  final: boolean
  message: string | null
}

const route = useRoute()
const turn = useOperatorTurn()
const host = useCallHost()
const qc = useQueryClient()

// docs/specs/SLICE_018.md §8: `identityKey` is `${orgId}:${actorId}:
// ${sessionLifetime}` (AppShell.vue's `operatorIdentityKey`) — the org id
// is always its first segment. Parsed here rather than adding a new prop,
// since every other Slice 018 Web mutation this panel needs (task confirm,
// Undo) is keyed by Organization.
const orgId = computed(() => props.identityKey?.split(':')[0] ?? '')

// `personId` is a ref updated immediately before each `mutate()` call —
// one panel instance may confirm/undo across different People in one
// session, but these hooks are constructed once at setup. It feeds only
// `mutationKey` (the `isMutating`-gated settle system every task mutation
// in `api/queries.ts` shares); the actual settle target is read from the
// call's own `variables.personId` instead (`useReopenTaskMutation`'s own
// shape, `useConfirmTaskProposal`'s own doc comment) — never this ref —
// so two confirms racing for different People each settle their own.
const taskConfirmPersonId = ref('')
const confirmTask = useConfirmTaskProposal(orgId, taskConfirmPersonId)
const undoPersonId = ref('')
const reopenTask = useReopenTaskMutation(orgId, undoPersonId)

const proposalStates = reactive(new Map<string, ProposalCardState>())
const taskProposalStates = reactive(new Map<string, TaskProposalCardState>())
const receiptStates = reactive(new Map<number, ReceiptCardState>())

// A coarse clock so a pending card disables itself at `expires_at`
// (SLICE_006b §1) without a per-card timer.
const now = ref(Date.now())
const clock = setInterval(() => {
  now.value = Date.now()
}, 5000)
onBeforeUnmount(() => clearInterval(clock))

function proposalState(id: string): ProposalCardState {
  let state = proposalStates.get(id)
  if (!state) {
    state = { status: 'idle', final: false, message: null }
    proposalStates.set(id, state)
  }
  return state
}

function proposalExpired(proposal: OperatorProposal): boolean {
  return now.value >= Date.parse(proposal.expires_at)
}

/** Non-final = the proposal was not consumed; Confirm may be retried
 * (mic denial, and the two local pre-checks that never POST). */
const RETRYABLE_CODES = new Set(['microphone_denied', 'call_in_progress', 'outcome_pending'])

/** Copy for the local pre-checks, where `host.call.error` is null. */
const LOCAL_CODE_MESSAGES: Record<string, string> = {
  call_in_progress: 'You already have a call in progress — hang up first.',
  outcome_pending: "Save the previous call's outcome first.",
}

async function confirmProposal(proposal: OperatorStartCallProposal) {
  if (!isSessionVerified()) return
  const proposalLifetime = lifetime
  const state = proposalState(proposal.id)
  if (state.status === 'confirming' || state.final) return
  if (proposalExpired(proposal)) {
    state.status = 'failed'
    state.final = true
    state.message = 'This suggestion expired — ask again.'
    return
  }
  state.status = 'confirming'
  state.message = null
  const code = await host.startFromProposal(proposal)
  if (!live || proposalLifetime !== lifetime) return
  if (code === null) {
    state.status = 'started'
    state.final = true
    state.message = null
    return
  }
  state.status = 'failed'
  state.final = !RETRYABLE_CODES.has(code)
  state.message =
    LOCAL_CODE_MESSAGES[code] ?? host.call.error.value?.message ?? 'Could not place the call.'
  if (!state.final) state.status = 'idle'
}

function dismissProposal(proposal: OperatorStartCallProposal) {
  const proposalLifetime = lifetime
  if (!live || proposalLifetime !== lifetime) return
  // Purely local (SLICE_006b §6): the row expires inert server-side.
  const state = proposalState(proposal.id)
  if (state.status === 'confirming') return
  state.status = 'failed'
  state.final = true
  state.message = 'Dismissed.'
}

// --- Slice 018: create_task proposal card (docs/specs/SLICE_018.md §8) ----

function taskProposalState(id: string): TaskProposalCardState {
  let state = taskProposalStates.get(id)
  if (!state) {
    state = { status: 'idle', final: false, message: null }
    taskProposalStates.set(id, state)
  }
  return state
}

async function confirmTaskProposal(proposal: OperatorCreateTaskProposal) {
  if (!isSessionVerified()) return
  const proposalLifetime = lifetime
  const state = taskProposalState(proposal.id)
  if (state.status === 'confirming' || state.final) return
  if (proposalExpired(proposal)) {
    state.status = 'failed'
    state.final = true
    state.message = 'This suggestion expired — ask again.'
    return
  }
  state.status = 'confirming'
  state.message = null
  taskConfirmPersonId.value = proposal.person.id
  try {
    await confirmTask.mutateAsync({ proposalId: proposal.id, personId: proposal.person.id })
    if (!live || proposalLifetime !== lifetime) return
    state.status = 'confirmed'
    state.final = true
    state.message = 'Task added.'
  } catch (err) {
    if (!live || proposalLifetime !== lifetime) return
    // "the telephony copy says 'call'" (docs/specs/SLICE_018.md §8): this
    // card's own consumed/expired copy, not `telephony/errors.ts`'s.
    if (err instanceof ApiError && err.code === 'proposal_expired') {
      state.status = 'failed'
      state.final = true
      state.message = 'This suggestion expired — ask again.'
      return
    }
    if (err instanceof ApiError && err.code === 'proposal_consumed') {
      state.status = 'failed'
      state.final = true
      // `task_id` is non-null only when the race's WINNER was this same
      // create_task proposal (confirmed, a task now exists); null means a
      // start_call-shaped consumption (claimed-then-crashed, or this
      // proposal failed before ever producing a task) — never "already
      // added" for a task that was never actually created.
      state.message =
        err.details.task_id != null
          ? 'This task was already added.'
          : 'This suggestion can no longer be used — ask again.'
      return
    }
    if (err instanceof ApiError && err.status !== 0) {
      // CONTRACT (docs/specs/SLICE_018.md §5, §10): every pass-through
      // task error (404/403/422/503/…) means the confirm route already
      // finalized the proposal row `failed` server-side — never
      // retryable; a fresh ask is required either way. Only a genuine
      // network error (status 0, the request never reached the server)
      // leaves the proposal itself untouched and stays retryable.
      state.status = 'failed'
      state.final = true
      state.message = `${describeTaskError(err, 'This suggestion can no longer be used')} — ask again.`
      return
    }
    state.status = 'failed'
    state.final = false
    state.message = describeTaskError(err, 'Something went wrong. Try again.')
  }
}

function dismissTaskProposal(proposal: OperatorCreateTaskProposal) {
  const proposalLifetime = lifetime
  if (!live || proposalLifetime !== lifetime) return
  // Purely local, the start_call Dismiss precedent: the row expires inert
  // server-side.
  const state = taskProposalState(proposal.id)
  if (state.status === 'confirming') return
  state.status = 'failed'
  state.final = true
  state.message = 'Dismissed.'
}

// --- Slice 018: complete_task receipt card / Undo (docs/specs/
// SLICE_018.md §8) ----------------------------------------------------

function receiptState(entryId: number): ReceiptCardState {
  let state = receiptStates.get(entryId)
  if (!state) {
    state = { status: 'idle', final: false, message: null }
    receiptStates.set(entryId, state)
  }
  return state
}

async function undoReceipt(entryId: number, receipt: OperatorReceipt) {
  if (!isSessionVerified()) return
  const entryLifetime = lifetime
  const state = receiptState(entryId)
  if (state.status === 'undoing' || state.final) return
  state.status = 'undoing'
  state.message = null
  undoPersonId.value = receipt.person.id
  try {
    // Order: the reopen POST first, then (inside `useReopenTaskMutation`'s
    // own onSuccess) the person/today/tasks refetch — never the reverse.
    await reopenTask.mutateAsync({ personId: receipt.person.id, taskId: receipt.task_id })
    if (!live || entryLifetime !== lifetime) return
    // 200 changed:true (this click won the race) and changed:false
    // (someone reopened first) are both success — same final copy.
    state.status = 'done'
    state.final = true
    state.message = 'Reopened'
  } catch (err) {
    if (!live || entryLifetime !== lifetime) return
    if (err instanceof ApiError && err.status === 404) {
      state.status = 'failed'
      state.final = true
      state.message = 'This task no longer exists.'
      return
    }
    if (err instanceof ApiError && err.status === 403) {
      state.status = 'failed'
      state.final = true
      state.message = 'You can no longer change this task.'
      return
    }
    // A network error (and anything else) stays retryable.
    state.status = 'failed'
    state.final = false
    state.message = describeApiError(err, 'Could not reach the server. Try again.')
  }
}

const draft = ref('')
const transcript = ref<TranscriptEntry[]>([])
const errorText = ref<string | null>(null)
const scroller = ref<HTMLElement | null>(null)
const textarea = ref<HTMLTextAreaElement | null>(null)
let nextId = 1
let live = true
let lifetime = 0

const pending = computed(() => turn.isPending.value)

/** Empty-state suggestion chips (one click = one turn). */
const SUGGESTIONS = ['Who should I call next?', 'Why is she first?', 'Find …']

/** Auto-grow the textarea from 2 to 5 lines with the draft. */
const rows = computed(() => Math.min(5, Math.max(2, draft.value.split('\n').length)))

function suggest(text: string) {
  if (text.endsWith('…')) {
    draft.value = text.slice(0, -1)
    void nextTick(() => textarea.value?.focus())
    return
  }
  draft.value = text
  send()
}
const trimmed = computed(() => draft.value.trim())
const canSend = computed(() => !pending.value && trimmed.value.length > 0 && trimmed.value.length <= MAX_MESSAGE_CHARS)

/** Only user/assistant text is replayed; the cards stay local. */
const history = computed<OperatorHistoryMessage[]>(() =>
  historyWindow(transcript.value.map((entry) => ({ role: entry.role, content: entry.text }))),
)

function send() {
  if (!canSend.value || !isSessionVerified()) return
  const message = trimmed.value
  // History is what came *before* this message.
  const priorHistory = history.value
  const requestLifetime = lifetime
  transcript.value.push({ id: nextId++, role: 'user', text: message, cards: [] })
  draft.value = ''
  errorText.value = null
  turn.mutate(
    {
      message,
      history: priorHistory,
      context: deriveScreenContext(route.path),
      // docs/specs/SLICE_018.md §3, §8: sent on every turn — the
      // `create_task` due-instant composition's only source of the
      // client's time zone, and the prompt's local-time line.
      utc_offset_minutes: currentUtcOffsetMinutes(),
    },
    {
      onSuccess: (response) => {
        if (!live || requestLifetime !== lifetime) return
        const proposal = response.proposal
        if (response.receipt) {
          // docs/specs/SLICE_018.md §8: invalidate on receipt too, so a
          // missed realtime event cannot leave Today stale — the same
          // three keys `useCompleteTaskMutation`'s own success settles.
          settleTaskMutation(qc, orgId.value, response.receipt.person.id)
        }
        transcript.value.push({
          id: nextId++,
          role: 'assistant',
          text: response.reply,
          cards: response.references.people,
          startCallProposal: proposal?.kind === 'start_call' ? proposal : undefined,
          createTaskProposal: proposal?.kind === 'create_task' ? proposal : undefined,
          receipt: response.receipt ?? undefined,
        })
      },
      onError: (err) => {
        if (!live || requestLifetime !== lifetime) return
        errorText.value = describeOperatorError(err)
      },
    },
  )
}

function clear() {
  transcript.value = []
  errorText.value = null
  draft.value = ''
  turn.reset()
}

watch(() => props.identityKey, () => {
  lifetime += 1
  transcript.value = []
  draft.value = ''
  errorText.value = null
  proposalStates.clear()
  taskProposalStates.clear()
  receiptStates.clear()
  turn.reset()
})

onBeforeUnmount(() => {
  live = false
  lifetime += 1
  transcript.value = []
  draft.value = ''
  errorText.value = null
  proposalStates.clear()
  taskProposalStates.clear()
  receiptStates.clear()
})

function onKeydown(event: KeyboardEvent) {
  // Enter sends; Shift+Enter inserts a newline.
  if (event.key === 'Enter' && !event.shiftKey) {
    event.preventDefault()
    send()
  }
}

watch(
  () => [transcript.value.length, pending.value, errorText.value] as const,
  async () => {
    await nextTick()
    const el = scroller.value
    if (el) el.scrollTop = el.scrollHeight
  },
)

defineExpose({ focus: () => textarea.value?.focus() })
</script>

<template>
  <aside
    class="glass-panel fixed inset-y-3 right-3 z-40 flex w-[min(420px,calc(100vw-88px))] flex-col overflow-hidden xl:sticky xl:top-3 xl:my-3 xl:mr-3 xl:h-[calc(100dvh-24px)] xl:shrink-0"
    aria-label="Ask the Operator"
    data-testid="operator-panel"
  >
    <div class="flex h-14 shrink-0 items-center justify-between border-b border-border px-4">
      <p class="text-body font-semibold text-text">
        Ask
      </p>
      <div class="flex items-center gap-1">
        <button
          type="button"
          class="h-10 rounded-lg px-3 text-small font-medium text-text-muted transition-colors duration-150 ease-out hover:bg-surface-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-focus focus-visible:ring-offset-2 disabled:opacity-50"
          :disabled="transcript.length === 0 && !errorText"
          data-testid="operator-clear"
          @click="clear"
        >
          Clear
        </button>
        <button
          type="button"
          title="Close"
          aria-label="Close"
          class="flex h-10 w-10 items-center justify-center rounded-lg text-text-muted transition-colors duration-150 ease-out hover:bg-surface-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-focus focus-visible:ring-offset-2"
          data-testid="operator-close"
          @click="emit('close')"
        >
          <X
            class="h-[18px] w-[18px]"
            stroke-width="1.5"
          />
        </button>
      </div>
    </div>

    <div
      ref="scroller"
      class="flex-1 space-y-3 overflow-y-auto px-4 py-4"
      data-testid="operator-transcript"
    >
      <div
        v-if="transcript.length === 0 && !errorText"
        class="pt-6 text-center"
      >
        <p class="text-body text-text-muted">
          Ask who to call next, why someone is first, or about any Person.
        </p>
        <div class="mt-4 flex flex-wrap justify-center gap-2">
          <button
            v-for="suggestion in SUGGESTIONS"
            :key="suggestion"
            type="button"
            class="h-10 rounded-full border border-border bg-surface-0 px-3.5 text-small font-medium text-text transition-colors duration-150 ease-out hover:bg-surface-1 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-focus focus-visible:ring-offset-2"
            data-testid="operator-suggestion"
            @click="suggest(suggestion)"
          >
            {{ suggestion }}
          </button>
        </div>
      </div>

      <div
        v-for="entry in transcript"
        :key="entry.id"
        :data-testid="`operator-${entry.role}`"
        class="flex"
        :class="entry.role === 'user' ? 'justify-end' : 'justify-start'"
      >
        <div
          class="space-y-2"
          :class="entry.role === 'user' ? 'max-w-[85%]' : 'w-full'"
        >
          <p
            v-if="entry.role === 'assistant'"
            class="text-small font-medium text-text-subtle"
          >
            Operator
          </p>
          <!-- v-text (never v-html): the reply is plain text by contract (§10). -->
          <p
            class="whitespace-pre-wrap break-words text-body text-text"
            :class="entry.role === 'user' ? 'rounded-xl bg-surface-2 px-3 py-2' : ''"
            v-text="entry.text"
          />
          <div
            v-if="entry.cards.length > 0"
            class="space-y-2"
          >
            <OperatorPersonCardView
              v-for="card in entry.cards"
              :key="card.id"
              :card="card"
            />
          </div>
          <div
            v-if="entry.startCallProposal"
            class="rounded-xl border border-border bg-surface-1 p-3"
            data-testid="operator-proposal"
          >
            <div class="flex items-center gap-2">
              <PhoneOutgoing
                class="h-4 w-4 shrink-0 text-text-muted"
                stroke-width="1.75"
              />
              <p class="text-body text-text">
                Call <span class="font-semibold">{{ entry.startCallProposal.person.display_name }}</span>
                at <span class="font-semibold">{{ entry.startCallProposal.phone }}</span>?
              </p>
            </div>
            <div
              v-if="proposalState(entry.startCallProposal.id).status === 'started'"
              class="mt-2 text-small text-text-muted"
              data-testid="operator-proposal-started"
            >
              Calling — see the call panel.
            </div>
            <template v-else>
              <div class="mt-3 flex items-center gap-2">
                <button
                  type="button"
                  :class="buttonClasses('primary')"
                  :disabled="proposalState(entry.startCallProposal.id).status === 'confirming'
                    || proposalState(entry.startCallProposal.id).final
                    || proposalExpired(entry.startCallProposal)"
                  data-testid="operator-proposal-confirm"
                  @click="confirmProposal(entry.startCallProposal)"
                >
                  {{ proposalState(entry.startCallProposal.id).status === 'confirming' ? 'Connecting…' : 'Confirm' }}
                </button>
                <button
                  type="button"
                  :class="buttonClasses('ghost')"
                  :disabled="proposalState(entry.startCallProposal.id).status === 'confirming'
                    || proposalState(entry.startCallProposal.id).final"
                  data-testid="operator-proposal-dismiss"
                  @click="dismissProposal(entry.startCallProposal)"
                >
                  Dismiss
                </button>
              </div>
              <p
                v-if="proposalState(entry.startCallProposal.id).message"
                class="mt-2 text-small text-danger"
                data-testid="operator-proposal-message"
              >
                {{ proposalState(entry.startCallProposal.id).message }}
              </p>
              <p
                v-else-if="proposalExpired(entry.startCallProposal) && !proposalState(entry.startCallProposal.id).final"
                class="mt-2 text-small text-text-muted"
                data-testid="operator-proposal-expired"
              >
                This suggestion expired — ask again.
              </p>
            </template>
          </div>

          <div
            v-if="entry.createTaskProposal"
            class="rounded-xl border border-border bg-surface-1 p-3"
            data-testid="operator-task-proposal"
          >
            <div class="flex items-center gap-2">
              <ListChecks
                class="h-4 w-4 shrink-0 text-text-muted"
                stroke-width="1.75"
              />
              <p class="min-w-0 text-body text-text">
                Add task for <span class="font-semibold">{{ entry.createTaskProposal.person.display_name }}</span>:
                <span class="break-words font-semibold">{{ entry.createTaskProposal.title }}</span>
                &middot; {{ TASK_KIND_LABEL[entry.createTaskProposal.task_kind] }}
                <template v-if="describeProposalDue(entry.createTaskProposal.due_at)">
                  &middot; {{ describeProposalDue(entry.createTaskProposal.due_at) }}
                </template>
                &middot; assigned to {{ entry.createTaskProposal.assignee.display_name }}
              </p>
            </div>
            <div class="mt-3 flex items-center gap-2">
              <button
                type="button"
                :class="buttonClasses('primary')"
                :disabled="taskProposalState(entry.createTaskProposal.id).status === 'confirming'
                  || taskProposalState(entry.createTaskProposal.id).final
                  || proposalExpired(entry.createTaskProposal)"
                data-testid="operator-task-proposal-confirm"
                @click="confirmTaskProposal(entry.createTaskProposal)"
              >
                {{ taskProposalState(entry.createTaskProposal.id).status === 'confirming' ? 'Adding…' : 'Confirm' }}
              </button>
              <button
                type="button"
                :class="buttonClasses('ghost')"
                :disabled="taskProposalState(entry.createTaskProposal.id).status === 'confirming'
                  || taskProposalState(entry.createTaskProposal.id).final"
                data-testid="operator-task-proposal-dismiss"
                @click="dismissTaskProposal(entry.createTaskProposal)"
              >
                Dismiss
              </button>
            </div>
            <p
              v-if="taskProposalState(entry.createTaskProposal.id).message"
              class="mt-2 text-small"
              :class="taskProposalState(entry.createTaskProposal.id).status === 'confirmed' ? 'text-text-muted' : 'text-danger'"
              data-testid="operator-task-proposal-message"
            >
              {{ taskProposalState(entry.createTaskProposal.id).message }}
            </p>
            <p
              v-else-if="proposalExpired(entry.createTaskProposal) && !taskProposalState(entry.createTaskProposal.id).final"
              class="mt-2 text-small text-text-muted"
              data-testid="operator-task-proposal-expired"
            >
              This suggestion expired — ask again.
            </p>
          </div>

          <div
            v-if="entry.receipt"
            class="rounded-xl border border-border bg-surface-1 p-3"
            data-testid="operator-receipt"
          >
            <div class="flex items-center gap-2">
              <Check
                class="h-4 w-4 shrink-0 text-text-muted"
                stroke-width="1.75"
              />
              <p class="min-w-0 text-body text-text">
                Completed: <span class="break-words font-semibold">{{ entry.receipt.title }}</span>
                &middot; {{ TASK_KIND_LABEL[entry.receipt.task_kind] }}
                <template v-if="describeReceiptDue(entry.receipt.due_at)">
                  &middot; {{ describeReceiptDue(entry.receipt.due_at) }}
                </template>
              </p>
            </div>
            <div class="mt-3 flex items-center gap-2">
              <button
                type="button"
                :class="buttonClasses('ghost')"
                :disabled="receiptState(entry.id).status === 'undoing' || receiptState(entry.id).final"
                data-testid="operator-receipt-undo"
                @click="undoReceipt(entry.id, entry.receipt)"
              >
                {{ receiptState(entry.id).status === 'undoing' ? 'Undoing…' : 'Undo' }}
              </button>
            </div>
            <p
              v-if="receiptState(entry.id).message"
              class="mt-2 text-small"
              :class="receiptState(entry.id).status === 'done' ? 'text-text-muted' : 'text-danger'"
              data-testid="operator-receipt-message"
            >
              {{ receiptState(entry.id).message }}
            </p>
          </div>
        </div>
      </div>

      <div
        v-if="pending"
        class="flex items-center gap-1 pl-1"
        data-testid="operator-pending"
        aria-label="Thinking"
      >
        <span class="sr-only">Thinking…</span>
        <span
          v-for="i in 3"
          :key="i"
          class="h-1.5 w-1.5 animate-pulse rounded-full bg-text-subtle"
          :style="{ animationDelay: `${(i - 1) * 150}ms` }"
        />
      </div>
      <p
        v-if="errorText"
        class="text-body text-danger"
        data-testid="operator-error"
      >
        {{ errorText }}
      </p>
    </div>

    <form
      class="shrink-0 border-t border-border p-4"
      @submit.prevent="send"
    >
      <textarea
        ref="textarea"
        v-model="draft"
        :class="TEXTAREA_CLASSES"
        class="min-h-[64px]"
        :rows="rows"
        placeholder="Who should I call next?"
        :maxlength="MAX_MESSAGE_CHARS"
        :disabled="pending"
        data-testid="operator-input"
        @keydown="onKeydown"
      />
      <div class="mt-3 flex items-center justify-end">
        <button
          type="submit"
          :class="buttonClasses('primary')"
          :disabled="!canSend"
          data-testid="operator-send"
        >
          {{ pending ? 'Sending…' : 'Send' }}
        </button>
      </div>
    </form>
  </aside>
</template>
