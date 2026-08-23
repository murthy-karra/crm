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
import { computed, nextTick, ref, watch } from 'vue'
import { useRoute } from 'vue-router'
import { X } from 'lucide-vue-next'
import { useOperatorTurn } from '../api/queries'
import type { OperatorHistoryMessage, OperatorPersonCard } from '../api/types'
import { buttonClasses, TEXTAREA_CLASSES } from '../lib/controls'
import { deriveScreenContext, describeOperatorError, historyWindow, MAX_MESSAGE_CHARS } from '../lib/operator'
import OperatorPersonCardView from './OperatorPersonCard.vue'

const emit = defineEmits<{ close: [] }>()

interface TranscriptEntry {
  id: number
  role: 'user' | 'assistant'
  text: string
  cards: OperatorPersonCard[]
}

const route = useRoute()
const turn = useOperatorTurn()

const draft = ref('')
const transcript = ref<TranscriptEntry[]>([])
const errorText = ref<string | null>(null)
const scroller = ref<HTMLElement | null>(null)
const textarea = ref<HTMLTextAreaElement | null>(null)
let nextId = 1

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
  if (!canSend.value) return
  const message = trimmed.value
  // History is what came *before* this message.
  const priorHistory = history.value
  transcript.value.push({ id: nextId++, role: 'user', text: message, cards: [] })
  draft.value = ''
  errorText.value = null
  turn.mutate(
    { message, history: priorHistory, context: deriveScreenContext(route.path) },
    {
      onSuccess: (response) => {
        transcript.value.push({
          id: nextId++,
          role: 'assistant',
          text: response.reply,
          cards: response.references.people,
        })
      },
      onError: (err) => {
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
    class="flex h-full w-[420px] shrink-0 flex-col border-l border-border bg-surface-0"
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
