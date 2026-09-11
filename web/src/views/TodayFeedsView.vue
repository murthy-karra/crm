<script setup lang="ts">
// SLICE_011d.md §6 "Manage → Today rules" (`/manage/today-feeds`,
// admin-only): one card per system feed in fixed order (§1), showing the
// effective rule as chips, freshness for the two person-state feeds,
// status, and Edit / Preview / Revert / Turn off-on. Edit opens FilterBar in
// its locked-clause mode (the anchor clause and, for the person-state
// feeds, the `me` assignee are visible but not removable/negatable — §1
// rules 3-4). Save is a PUT with the loaded revision; a 409 reloads the
// feed for review and requires a new explicit Save click (no auto-retry).
// Revert and Turn off confirm; turning off `unanswered_inquiry` requires
// typing the feed's display name (§1 rule 2).
import { computed, ref } from 'vue'
import PageHeader from '../components/PageHeader.vue'
import Card from '../components/Card.vue'
import Badge from '../components/Badge.vue'
import ConfirmDialog from '../components/ConfirmDialog.vue'
import TypedConfirmDialog from '../components/TypedConfirmDialog.vue'
import PreviewTodayFeedDialog from '../components/PreviewTodayFeedDialog.vue'
import FilterBar from '../components/FilterBar.vue'
import { ApiError } from '../api/client'
import {
  useCustomFieldsQuery,
  useInquirySources,
  useMe,
  useMembers,
  useRevertTodayFeedMutation,
  useSetTodayFeedEnabledMutation,
  useStages,
  useTagsQuery,
  useTodayFeedsAdmin,
  useUpdateTodayFeedMutation,
} from '../api/queries'
import type { Feed, FilterClause, MeResponse, TodayFeedKey } from '../api/types'
import { buttonClasses, INPUT_CLASSES, LABEL_CLASSES } from '../lib/controls'
import { describeApiError, describeMutationError } from '../lib/errors'
import { formatAbsoluteTime } from '../lib/format'
import {
  TODAY_FEED_ANCHOR_KIND,
  TODAY_FEED_LABEL,
  TODAY_FEED_ORDER,
  adminFeedStatus,
  requiresAssigneeMe,
  usesFreshWindow,
} from '../lib/todayFeeds'

const { data: me } = useMe()
const orgId = computed(() => me.value?.organization?.id ?? '')
const actorId = computed(() => me.value?.user.id ?? '')

const feedsQuery = useTodayFeedsAdmin(orgId)
const stagesQuery = useStages(orgId)
const membersQuery = useMembers(orgId)
const sourcesQuery = useInquirySources(orgId)
const tagsQuery = useTagsQuery(orgId)
const customFieldsQuery = useCustomFieldsQuery(orgId)

function retryCustomFields() {
  void customFieldsQuery.refetch()
}

const updateMutation = useUpdateTodayFeedMutation(orgId, actorId)
const revertMutation = useRevertTodayFeedMutation(orgId, actorId)
const toggleMutation = useSetTodayFeedEnabledMutation(orgId, actorId)

function feedByKey(key: TodayFeedKey): Feed | undefined {
  return feedsQuery.data.value?.feeds.find((f) => f.feed_key === key)
}
function cloneClauses(clauses: FilterClause[]): FilterClause[] {
  return JSON.parse(JSON.stringify(clauses)) as FilterClause[]
}
function draftFromFeed(feed: Feed) {
  return {
    feedKey: feed.feed_key,
    revision: feed.revision,
    clauses: cloneClauses(feed.filter?.clauses ?? feed.default.filter.clauses),
    freshHours: String(feed.fresh_within_hours ?? feed.default.fresh_within_hours ?? 24),
  }
}

// ---- Session-identity fence (011b/011c pattern): a background completion
// after the org, actor, or auth-session lifetime changed must not apply UI
// state built for the old identity. -----------------------------------
interface ViewIdentity {
  orgId: string
  actorId: string
  session: MeResponse | undefined
}
function captureIdentity(): ViewIdentity {
  return { orgId: orgId.value, actorId: actorId.value, session: me.value }
}
function identityMatches(identity: ViewIdentity) {
  return orgId.value === identity.orgId && actorId.value === identity.actorId && me.value === identity.session
}

// ---- Edit -------------------------------------------------------------

const editing = ref<ReturnType<typeof draftFromFeed> | null>(null)
const editNotice = ref('')
const freshHoursError = ref(false)

function openEdit(feed: Feed) {
  updateMutation.reset()
  editNotice.value = ''
  freshHoursError.value = false
  editing.value = draftFromFeed(feed)
}
function closeEdit() {
  editing.value = null
  editNotice.value = ''
  freshHoursError.value = false
  updateMutation.reset()
}
function onEditClausesChange(next: FilterClause[]) {
  if (editing.value) editing.value.clauses = next
}

function parsedFreshHours(feedKey: TodayFeedKey, raw: string): number | null {
  if (!usesFreshWindow(feedKey)) return null
  const parsed = Number(raw)
  return Number.isInteger(parsed) && parsed >= 1 && parsed <= 8760 ? parsed : null
}

async function saveEdit() {
  const draft = editing.value
  if (!draft || updateMutation.isPending.value) return
  const freshWithinHours = parsedFreshHours(draft.feedKey, draft.freshHours)
  if (usesFreshWindow(draft.feedKey) && freshWithinHours === null) {
    freshHoursError.value = true
    return
  }
  freshHoursError.value = false
  const identity = captureIdentity()
  try {
    await updateMutation.mutateAsync({
      feedKey: draft.feedKey,
      body: { expected_revision: draft.revision, filter: { version: 1, clauses: draft.clauses }, fresh_within_hours: freshWithinHours },
    })
    if (!identityMatches(identity)) return
    editing.value = null
    editNotice.value = ''
  } catch (error) {
    if (!identityMatches(identity)) return
    if (error instanceof ApiError && error.status === 409) {
      // §6: "a 409 reloads the feed for review and needs a new click" — the
      // reload must be explicit and honest about what happened, and a
      // failed reload must never silently close the editor (that would read
      // as a discarded edit succeeding). `editing.value` is never set to
      // `null` in this branch: either the reload lands and the editor shows
      // the true current row, or it doesn't and the editor stays open on
      // the old (now-known-stale) draft with an error notice.
      const refreshed = await feedsQuery.refetch()
      if (!identityMatches(identity)) return
      const fresh = refreshed.isError ? undefined : refreshed.data?.feeds.find((f) => f.feed_key === draft.feedKey)
      if (!fresh) {
        editNotice.value = 'This rule was changed by someone else, and the latest version could not be loaded. Try Save again.'
        return
      }
      editing.value = draftFromFeed(fresh)
      editNotice.value = 'This rule was changed by someone else. The saved version has been reloaded and your draft was discarded.'
    }
    // Every other failure (422 validation, 503, network) is rendered inline
    // below from `updateMutation.error`; the draft is retained untouched.
  }
}

// ---- Preview (card-level: the effective definition; edit-mode: the draft) -

const previewState = ref<{ feedKey: TodayFeedKey; filter: { version: 1; clauses: FilterClause[] }; freshWithinHours: number | null } | null>(null)

function openPreview(feed: Feed) {
  previewState.value = {
    feedKey: feed.feed_key,
    filter: feed.filter ?? feed.default.filter,
    freshWithinHours: feed.fresh_within_hours ?? feed.default.fresh_within_hours,
  }
}
function openDraftPreview() {
  const draft = editing.value
  if (!draft) return
  previewState.value = {
    feedKey: draft.feedKey,
    filter: { version: 1, clauses: draft.clauses },
    freshWithinHours: parsedFreshHours(draft.feedKey, draft.freshHours),
  }
}
function closePreview() {
  previewState.value = null
}

// ---- Revert -------------------------------------------------------------

const revertConfirm = ref<{ feedKey: TodayFeedKey; revision: number } | null>(null)

function openRevertConfirm(feed: Feed) {
  revertMutation.reset()
  revertConfirm.value = { feedKey: feed.feed_key, revision: feed.revision }
}
function closeRevertConfirm() {
  if (revertMutation.isPending.value) return
  revertConfirm.value = null
}
async function confirmRevert() {
  const intent = revertConfirm.value
  if (!intent) return
  const identity = captureIdentity()
  try {
    await revertMutation.mutateAsync({ feedKey: intent.feedKey, body: { expected_revision: intent.revision } })
    if (!identityMatches(identity)) return
    revertConfirm.value = null
  } catch {
    // Error rendered inline via revertMutation.error; dialog stays open for
    // an explicit retry (no auto-retry).
  }
}

// ---- Turn off / Turn on --------------------------------------------------

const toggleConfirm = ref<{ feedKey: TodayFeedKey; revision: number } | null>(null)
const typedToggleConfirm = ref<{ feedKey: TodayFeedKey; revision: number } | null>(null)

function openTurnOff(feed: Feed) {
  toggleMutation.reset()
  if (feed.feed_key === 'unanswered_inquiry') {
    typedToggleConfirm.value = { feedKey: feed.feed_key, revision: feed.revision }
  } else {
    toggleConfirm.value = { feedKey: feed.feed_key, revision: feed.revision }
  }
}
function closeToggleConfirm() {
  if (toggleMutation.isPending.value) return
  toggleConfirm.value = null
}
function closeTypedToggleConfirm() {
  if (toggleMutation.isPending.value) return
  typedToggleConfirm.value = null
}
async function confirmTurnOff(intent: { feedKey: TodayFeedKey; revision: number } | null) {
  if (!intent) return
  const identity = captureIdentity()
  try {
    await toggleMutation.mutateAsync({ feedKey: intent.feedKey, body: { expected_revision: intent.revision, enabled: false } })
    if (!identityMatches(identity)) return
    toggleConfirm.value = null
    typedToggleConfirm.value = null
  } catch {
    // Error rendered inline; dialog stays open.
  }
}
function turnOn(feed: Feed) {
  if (toggleMutation.isPending.value) return
  toggleMutation.mutate({ feedKey: feed.feed_key, body: { expected_revision: feed.revision, enabled: true } })
}

const feeds = computed(() => TODAY_FEED_ORDER.map((key) => {
  const feed = feedByKey(key)
  return { key, feed, status: feed ? adminFeedStatus(feed) : null }
}))
</script>

<template>
  <div class="mx-auto max-w-3xl px-8 py-8">
    <PageHeader
      title="Today rules"
      subtitle="Adjust the three built-in rules that put People on every member's Today."
    />

    <!-- Only blocks the whole page when there is truly nothing to show yet
         (the very first load failed). A background refetch failure — e.g.
         the reload `saveEdit()` triggers after a 409 — keeps TanStack's
         last-known-good `data` in place; gating on `isError` alone would
         replace the open editor and its error notice with this full-page
         error the instant that reload failed. -->
    <div
      v-if="feedsQuery.isError.value && !feedsQuery.data.value"
      class="rounded-xl border border-border bg-surface-0 p-5 text-body text-danger"
      data-testid="today-feeds-error"
    >
      {{ describeApiError(feedsQuery.error.value, 'Could not load Today rules.') }}
    </div>
    <div
      v-else-if="feedsQuery.isPending.value"
      class="rounded-xl border border-border bg-surface-0 p-5 text-body text-text-muted"
    >
      Loading…
    </div>

    <div
      v-else
      class="space-y-4"
    >
      <Card
        v-for="{ key, feed, status } in feeds"
        :key="key"
        :data-testid="`feed-card-${key}`"
      >
        <template v-if="!feed">
          <p class="text-body text-text-muted">
            {{ TODAY_FEED_LABEL[key] }} is unavailable.
          </p>
        </template>

        <template v-else-if="editing?.feedKey === key">
          <h2 class="text-section font-semibold text-text">
            Edit {{ TODAY_FEED_LABEL[key] }}
          </h2>
          <p
            v-if="editNotice"
            role="status"
            class="mt-2 text-small text-danger"
            :data-testid="`feed-edit-notice-${key}`"
          >
            {{ editNotice }}
          </p>
          <div class="mt-3">
            <FilterBar
              :clauses="editing!.clauses"
              :stages="stagesQuery.data.value?.stages ?? []"
              :members="membersQuery.data.value?.members ?? []"
              :sources="sourcesQuery.data.value?.sources ?? []"
              :tags="tagsQuery.data.value?.tags ?? []"
              :stages-pending="stagesQuery.isPending.value"
              :stages-error="stagesQuery.isError.value"
              :members-pending="membersQuery.isPending.value"
              :members-error="membersQuery.isError.value"
              :sources-pending="sourcesQuery.isPending.value"
              :sources-error="sourcesQuery.isError.value"
              :sources-truncated="sourcesQuery.data.value?.truncated"
              :tags-pending="tagsQuery.isPending.value"
              :tags-error="tagsQuery.isError.value"
              :custom-fields="customFieldsQuery.data.value?.fields ?? []"
              :custom-fields-pending="customFieldsQuery.isPending.value"
              :custom-fields-error="customFieldsQuery.isError.value"
              :locked-anchor-kind="TODAY_FEED_ANCHOR_KIND[key]"
              :require-assignee-me="requiresAssigneeMe(key)"
              @update:clauses="onEditClausesChange"
              @retry-options="() => {}"
              @retry-custom-fields="retryCustomFields"
            />
          </div>
          <label
            v-if="usesFreshWindow(key)"
            class="mt-3 block max-w-xs"
          >
            <span :class="LABEL_CLASSES">Fresh within (hours)</span>
            <input
              v-model="editing!.freshHours"
              type="number"
              min="1"
              max="8760"
              step="1"
              :class="INPUT_CLASSES"
              :aria-invalid="freshHoursError"
              data-testid="feed-fresh-hours"
            >
          </label>
          <p
            v-if="freshHoursError"
            role="alert"
            class="mt-1.5 text-small text-danger"
          >
            Enter a whole number of hours from 1 to 8,760.
          </p>
          <p
            v-if="updateMutation.isError.value"
            role="alert"
            class="mt-2 text-small text-danger"
          >
            {{ describeMutationError(updateMutation.error.value, 'Could not save this rule.') }}
          </p>
          <div class="mt-4 flex flex-wrap items-center gap-2">
            <button
              type="button"
              :class="buttonClasses('primary')"
              :disabled="updateMutation.isPending.value"
              :data-testid="`feed-save-${key}`"
              @click="saveEdit"
            >
              {{ updateMutation.isPending.value ? 'Saving…' : 'Save' }}
            </button>
            <button
              type="button"
              :class="buttonClasses('secondary')"
              :disabled="updateMutation.isPending.value"
              :data-testid="`feed-cancel-${key}`"
              @click="closeEdit"
            >
              Cancel
            </button>
            <button
              type="button"
              :class="buttonClasses('ghost')"
              :data-testid="`feed-preview-draft-${key}`"
              @click="openDraftPreview"
            >
              Preview
            </button>
          </div>
        </template>

        <template v-else>
          <div class="flex items-start justify-between gap-4">
            <h2 class="text-section font-semibold text-text">
              {{ TODAY_FEED_LABEL[key] }}
            </h2>
            <p
              :data-testid="`feed-status-${key}`"
              class="shrink-0 text-small text-text-muted"
            >
              <template v-if="status!.kind === 'off'">
                Off
              </template>
              <template v-else-if="status!.kind === 'invalid_fallback'">
                Using the default because the saved rule is invalid
              </template>
              <template v-else-if="status!.kind === 'default'">
                Default
              </template>
              <template v-else>
                Customized by {{ (status as { updatedBy: string }).updatedBy }}
                on {{ formatAbsoluteTime((status as { updatedAt: string }).updatedAt) }}
              </template>
            </p>
          </div>

          <div class="mt-3 flex flex-wrap gap-1.5">
            <Badge
              v-for="(line, index) in feed.description"
              :key="index"
              tint="neutral"
            >
              {{ line }}
            </Badge>
          </div>

          <p
            v-if="usesFreshWindow(key)"
            class="mt-2 text-small text-text-muted"
          >
            Fresh within {{ feed.fresh_within_hours ?? feed.default.fresh_within_hours }} hours
          </p>

          <div class="mt-4 flex flex-wrap items-center gap-2">
            <button
              type="button"
              :class="buttonClasses('secondary')"
              :data-testid="`feed-edit-${key}`"
              @click="openEdit(feed)"
            >
              Edit
            </button>
            <button
              type="button"
              :class="buttonClasses('secondary')"
              :data-testid="`feed-preview-${key}`"
              @click="openPreview(feed)"
            >
              Preview
            </button>
            <button
              type="button"
              :class="buttonClasses('ghost')"
              :data-testid="`feed-revert-${key}`"
              @click="openRevertConfirm(feed)"
            >
              Revert
            </button>
            <button
              v-if="feed.enabled"
              type="button"
              :class="buttonClasses('ghost')"
              :disabled="toggleMutation.isPending.value"
              :data-testid="`feed-turn-off-${key}`"
              @click="openTurnOff(feed)"
            >
              Turn off
            </button>
            <button
              v-else
              type="button"
              :class="buttonClasses('secondary')"
              :disabled="toggleMutation.isPending.value"
              :data-testid="`feed-turn-on-${key}`"
              @click="turnOn(feed)"
            >
              Turn on
            </button>
          </div>
        </template>
      </Card>
    </div>

    <ConfirmDialog
      :visible="revertConfirm !== null"
      title="Revert to the default rule?"
      :message="revertConfirm ? `Revert ${TODAY_FEED_LABEL[revertConfirm.feedKey]} to its default rule? This cannot be undone.` : ''"
      confirm-label="Revert"
      confirm-variant="danger"
      :is-pending="revertMutation.isPending.value"
      :error="revertMutation.error.value"
      error-fallback="Could not revert this rule."
      @update:visible="closeRevertConfirm"
      @confirm="confirmRevert"
    />

    <ConfirmDialog
      :visible="toggleConfirm !== null"
      title="Turn off this rule?"
      :message="toggleConfirm ? `Turn off ${TODAY_FEED_LABEL[toggleConfirm.feedKey]}? Every member's Today stops showing this rule's work until it is turned back on.` : ''"
      confirm-label="Turn off"
      confirm-variant="danger"
      :is-pending="toggleMutation.isPending.value"
      :error="toggleMutation.error.value"
      error-fallback="Could not turn off this rule."
      @update:visible="closeToggleConfirm"
      @confirm="confirmTurnOff(toggleConfirm)"
    />

    <TypedConfirmDialog
      :visible="typedToggleConfirm !== null"
      title="Turn off Unanswered inquiry?"
      message="This is the core Today rule that surfaces new and unanswered inquiries. Turning it off removes it from every member's Today until it is turned back on."
      :confirm-text="TODAY_FEED_LABEL.unanswered_inquiry"
      confirm-label="Turn off"
      :is-pending="toggleMutation.isPending.value"
      :error="toggleMutation.error.value"
      error-fallback="Could not turn off this rule."
      @update:visible="closeTypedToggleConfirm"
      @confirm="confirmTurnOff(typedToggleConfirm)"
    />

    <PreviewTodayFeedDialog
      v-if="previewState"
      :visible="previewState !== null"
      :org-id="orgId"
      :feed-key="previewState.feedKey"
      :filter="previewState.filter"
      :fresh-within-hours="previewState.freshWithinHours"
      :members="membersQuery.data.value?.members ?? []"
      :default-subject-id="actorId"
      @update:visible="closePreview"
    />
  </div>
</template>
