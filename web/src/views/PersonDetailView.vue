<script setup lang="ts">
// UI_STYLE.md §7 + docs/specs/SLICE_002.md §10: header card with the
// entity's identity (name, stage Select, assignee Select — both mutating
// inline via POST .../stage and .../assignment), then Contact methods /
// Inquiries / History cards. History renders the server's per-kind
// `detail` shapes exactly as spec §5 documents them, in server order
// (occurred_at, recorded_at, kind_rank, id) — never re-sorted here.
import { computed, nextTick, onBeforeUnmount, ref, watch, type Component } from 'vue'
import { RouterLink, useRoute, useRouter } from 'vue-router'
import Select from 'primevue/select'
import { useQueryClient } from '@tanstack/vue-query'
import {
  Activity,
  Check,
  Flag,
  Inbox,
  ListChecks,
  Mail,
  Pencil,
  Phone,
  PhoneCall,
  PhoneOutgoing,
  Plus,
  Route,
  StickyNote,
  Trash2,
  Undo2,
  UserCheck,
  X,
} from 'lucide-vue-next'
import Card from '../components/Card.vue'
import FormField from '../components/FormField.vue'
import Badge from '../components/Badge.vue'
import StageLabel from '../components/StageLabel.vue'
import LogContactDialog from '../components/LogContactDialog.vue'

import ChangeOutcomeDialog from '../components/ChangeOutcomeDialog.vue'
import {
  queryKeys,
  useAddNoteMutation,
  useAddPersonTagMutation,
  useAssignPersonMutation,
  useChangeStageMutation,
  useCompleteTaskMutation,
  useCorrectCallOutcome,
  useCreateTagMutation,
  useCreateTaskMutation,
  useDeleteNoteMutation,
  useDeleteTaskMutation,
  useEditNoteMutation,
  useMe,
  useMembers,
  usePerson,
  useReopenTaskMutation,
  useRemovePersonTagMutation,
  useStages,
  useTagsQuery,
  useUpdateTaskMutation,
} from '../api/queries'
import { ApiError } from '../api/client'
import type {
  ActorRef,
  CallOutcomeCorrection,
  ContactAttemptedDetail,
  HistoryEntry,
  RoutingStrategy,
  Task,
  TaskKind,
  TagRef,
} from '../api/types'
import { formatAbsoluteTime, formatDateOnly, formatRelativeTime } from '../lib/format'
import { buttonClasses, INPUT_CLASSES, LABEL_CLASSES, TEXTAREA_CLASSES, selectPt } from '../lib/controls'
import ConfirmDialog from '../components/ConfirmDialog.vue'
import { describeApiError, describeNoteError, describeTaskError } from '../lib/errors'
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
const personTags = computed(() => detail.value?.tags ?? [])
const tasks = computed(() => detail.value?.tasks ?? [])

const notFound = computed(() => error.value instanceof ApiError && error.value.status === 404)

// ---- Tags (SLICE_011e §5, §9.9) --------------------------------------------
// Chips in the identity header beside Stage/Assignee. "Add tag" opens a
// keyboard-navigable popover over the Organization's tags minus those
// already applied, with a "Create '<typed>'" row when no case-insensitive
// match exists — choosing it runs POST /api/tags then PUT, in that order.
// No modal focus trap (UI_STYLE §5's ordinary popover posture, matching the
// call number picker above): Escape closes and returns focus to the button;
// an outside click closes without stealing focus.
const { data: orgTagsData } = useTagsQuery(orgId)
const orgTags = computed(() => orgTagsData.value?.tags ?? [])
const appliedTagIds = computed(() => new Set(personTags.value.map((tag) => tag.id)))
const availableTags = computed(() => orgTags.value.filter((tag) => !appliedTagIds.value.has(tag.id)))

const addPersonTag = useAddPersonTagMutation(orgId, () => props.id)
const removePersonTag = useRemovePersonTagMutation(orgId, () => props.id)
const createTag = useCreateTagMutation(orgId)
const addTagPending = computed(() => addPersonTag.isPending.value || createTag.isPending.value)

const addTagOpen = ref(false)
const addTagQuery = ref('')
const addTagActiveIndex = ref(0)
const addTagError = ref<string | null>(null)
const addTagRoot = ref<HTMLElement | null>(null)
const addTagButton = ref<HTMLButtonElement | null>(null)
const addTagInput = ref<HTMLInputElement | null>(null)

type AddTagOption = { kind: 'existing'; tag: TagRef } | { kind: 'create'; name: string }

const filteredTags = computed(() => {
  const q = addTagQuery.value.trim().toLowerCase()
  if (!q) return availableTags.value
  return availableTags.value.filter((tag) => tag.name.toLowerCase().includes(q))
})
const trimmedQuery = computed(() => addTagQuery.value.trim())
const showCreateRow = computed(() => {
  const q = trimmedQuery.value
  // Code points, not UTF-16 units — matches the server's `chars().count()`
  // (backend/crates/crm-app/src/domain/tag/commands.rs), so a 40-character
  // name with astral characters (e.g. some emoji) is not wrongly hidden.
  if (q === '' || [...q].length > 40) return false
  return !orgTags.value.some((tag) => tag.name.toLowerCase() === q.toLowerCase())
})
const addTagOptions = computed<AddTagOption[]>(() => {
  const options: AddTagOption[] = filteredTags.value.map((tag) => ({ kind: 'existing', tag }))
  if (showCreateRow.value) options.push({ kind: 'create', name: trimmedQuery.value })
  return options
})
watch(addTagOptions, () => { addTagActiveIndex.value = 0 })

const TAG_GONE_MESSAGE = 'That tag no longer exists; refreshed.'

// A 404 here means the tag (or, for remove, the applied row) vanished
// between this tab's last read and the write — the tags query and, for
// apply/remove, the Person detail have already been invalidated by the
// mutation hook's own onError (queries.ts), so this is display copy only,
// never silent (reviewer F1 / tester F1: a 404 must explain itself, not
// leave the user guessing why a chip disappeared).
function describeTagError(err: unknown, fallback: string): string {
  if (err instanceof ApiError) {
    if (err.code === 'person_tag_limit_reached') return 'This person already has 20 tags.'
    if (err.code === 'tag_limit_reached') return 'This Organization already has 200 tags.'
    if (err.status === 404) return TAG_GONE_MESSAGE
  }
  return describeApiError(err, fallback)
}

function openAddTag() {
  if (addTagOpen.value) return
  addTagQuery.value = ''
  addTagActiveIndex.value = 0
  addTagError.value = null
  addPersonTag.reset()
  createTag.reset()
  addTagOpen.value = true
  void nextTick(() => addTagInput.value?.focus())
}

function closeAddTag(returnFocus: boolean) {
  addTagOpen.value = false
  if (returnFocus) addTagButton.value?.focus()
}

function toggleAddTag() {
  if (addTagOpen.value) closeAddTag(false)
  else openAddTag()
}

function applyTag(tagId: string) {
  addPersonTag.mutate(
    { personId: props.id, tagId },
    {
      onSuccess: () => { closeAddTag(true) },
      onError: (err) => { addTagError.value = describeTagError(err, 'Could not add this tag.') },
    },
  )
}

function createAndApplyTag(name: string) {
  createTag.mutate(
    { name },
    {
      onSuccess: (result) => { applyTag(result.tag.id) },
      onError: (err) => { addTagError.value = describeTagError(err, 'Could not create this tag.') },
    },
  )
}

function chooseAddTagOption(option: AddTagOption) {
  if (addTagPending.value) return
  addTagError.value = null
  if (option.kind === 'existing') applyTag(option.tag.id)
  else createAndApplyTag(option.name)
}

function onAddTagInputKeydown(event: KeyboardEvent) {
  const options = addTagOptions.value
  if (event.key === 'ArrowDown') {
    event.preventDefault()
    if (options.length) addTagActiveIndex.value = (addTagActiveIndex.value + 1) % options.length
  } else if (event.key === 'ArrowUp') {
    event.preventDefault()
    if (options.length) addTagActiveIndex.value = (addTagActiveIndex.value - 1 + options.length) % options.length
  } else if (event.key === 'Enter') {
    event.preventDefault()
    const option = options[addTagActiveIndex.value]
    if (option) chooseAddTagOption(option)
  }
  // Escape is left to bubble to the document handler below, which also
  // returns focus to the Add tag button.
}

function onAddTagDocumentClick(event: MouseEvent) {
  if (!addTagOpen.value || addTagPending.value) return
  const target = event.target
  if (target instanceof Node && addTagRoot.value?.contains(target)) return
  closeAddTag(false)
}

function onAddTagDocumentKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape' && addTagOpen.value && !addTagPending.value) closeAddTag(true)
}

watch(addTagOpen, (open) => {
  if (open) {
    document.addEventListener('click', onAddTagDocumentClick, true)
    document.addEventListener('keydown', onAddTagDocumentKeydown)
  } else {
    document.removeEventListener('click', onAddTagDocumentClick, true)
    document.removeEventListener('keydown', onAddTagDocumentKeydown)
  }
})
onBeforeUnmount(() => {
  document.removeEventListener('click', onAddTagDocumentClick, true)
  document.removeEventListener('keydown', onAddTagDocumentKeydown)
})

const removeTagError = ref<string | null>(null)
function removeTag(tagId: string) {
  removeTagError.value = null
  removePersonTag.mutate(
    { personId: props.id, tagId },
    {
      onError: (err) => { removeTagError.value = describeTagError(err, 'Could not remove this tag.') },
    },
  )
}
function isRemovingTag(tagId: string): boolean {
  return removePersonTag.isPending.value && removePersonTag.variables.value?.tagId === tagId
}

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

const { mutate: setStage, isPending: stagePending, error: stageError } = useChangeStageMutation(orgId, () => props.id)
const { mutate: setAssignee, isPending: assigneePending, error: assigneeError } = useAssignPersonMutation(orgId, () => props.id)

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
  correspondence: Mail,
  note: StickyNote,
  task_completed: ListChecks,
}

// docs/specs/SLICE_015.md §5: a tab loaded before a deploy refetches on the
// first `person.changed` it receives, so an unrecognised future `kind`
// (one this bundle's `HistoryEntry` union does not know about) must never
// throw or render `undefined` — it renders as a generic "Activity" row
// with this fallback icon instead. `HISTORY_ICON` is indexed by the raw
// wire string (not the closed union) precisely so this lookup is defined
// for a value TypeScript believes is exhaustive but the JSON payload does
// not actually guarantee.
function historyIcon(kind: string): Component {
  return (HISTORY_ICON as Record<string, Component>)[kind] ?? Activity
}

// ---- Notes (SLICE_015 §5, §9.10) -------------------------------------------
// Composer at the top of the History card; note rows render inline with
// Edit/Delete where `detail.can_manage`. Mutations are pessimistic (§5: "a
// note body is the kind of value 014 §3 declined to invent client-side") —
// see api/queries.ts's useAdd/Edit/DeleteNoteMutation doc comments.
const NOTE_MAX_CHARS = 10_000
const NOTE_COUNTER_THRESHOLD = 9_000

function codePointLength(value: string): number {
  return Array.from(value).length
}

const addNote = useAddNoteMutation(orgId, () => props.id)
const noteDraft = ref('')
const noteAddError = ref<string | null>(null)
const noteDraftCodePoints = computed(() => codePointLength(noteDraft.value))
const noteDraftOverLimit = computed(() => noteDraftCodePoints.value > NOTE_MAX_CHARS)
const noteAddDisabled = computed(
  () => noteDraft.value.trim() === '' || noteDraftOverLimit.value || addNote.isPending.value,
)

function submitNote() {
  if (noteAddDisabled.value) return
  noteAddError.value = null
  addNote.mutate(
    { personId: props.id, body: noteDraft.value },
    {
      onSuccess: () => { noteDraft.value = '' },
      onError: (err) => { noteAddError.value = describeNoteError(err, 'Could not add this note.') },
    },
  )
}

function onNoteComposerKeydown(event: KeyboardEvent) {
  if ((event.ctrlKey || event.metaKey) && event.key === 'Enter') {
    event.preventDefault()
    submitNote()
  }
}

interface NotePayload {
  id: string
  body: string
  edited: boolean
  canManage: boolean
}

// ---- Inline edit (local state keyed by note id) ---------------------------
// Kept local rather than derived from `historyRows` so a refetch that drops
// the note out of `history` (deleted elsewhere while the viewer is typing)
// does not silently unmount the editor and lose the draft (§5).
interface EditingNote {
  id: string
  draft: string
  error: string | null
}
const editingNote = ref<EditingNote | null>(null)
const editNote = useEditNoteMutation(orgId, () => props.id)
const editButtonRefs: Record<string, HTMLButtonElement | null> = {}

function setEditButtonRef(id: string, el: unknown) {
  editButtonRefs[id] = el instanceof HTMLButtonElement ? el : null
}

// True once a refetch (any reason — realtime, focus, this mutation's own
// settle) shows the note being edited is no longer live: tombstoned or
// gone. Recomputed automatically whenever `history` changes; no manual
// watcher needed.
const editingNoteGone = computed(() => {
  const editing = editingNote.value
  if (!editing) return false
  return !history.value.some((entry) => entry.kind === 'note' && entry.id === editing.id)
})

function startEditNote(note: NotePayload) {
  if (editingNote.value?.id === note.id) return
  editingNote.value = { id: note.id, draft: note.body, error: null }
}

function cancelEditNote() {
  if (!editingNote.value || editNote.isPending.value) return
  const id = editingNote.value.id
  editingNote.value = null
  void nextTick(() => editButtonRefs[id]?.focus())
}

function onEditNoteKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape') {
    event.preventDefault()
    cancelEditNote()
  }
}

// A get/set computed rather than `v-model="editingNote.draft"` directly: the
// template's `v-if="row.note && editingNote?.id === row.note.id"` implies
// `editingNote` is non-null without a form vue-tsc's template narrowing
// recognizes, so both editor blocks below bind through this null-safe model
// instead of asserting it themselves.
const editingNoteDraftModel = computed<string>({
  get: () => editingNote.value?.draft ?? '',
  set: (value) => {
    if (editingNote.value) editingNote.value.draft = value
  },
})

const editNoteCodePoints = computed(() => (editingNote.value ? codePointLength(editingNote.value.draft) : 0))
const editNoteSaveDisabled = computed(() => {
  const editing = editingNote.value
  if (!editing) return true
  return editing.draft.trim() === '' || editNoteCodePoints.value > NOTE_MAX_CHARS || editNote.isPending.value
})

function saveEditNote() {
  const editing = editingNote.value
  if (!editing || editNoteSaveDisabled.value) return
  editing.error = null
  editNote.mutate(
    { personId: props.id, noteId: editing.id, body: editing.draft },
    {
      onSuccess: () => {
        if (editingNote.value?.id === editing.id) editingNote.value = null
      },
      onError: (err) => {
        const current = editingNote.value
        if (current?.id === editing.id) {
          current.error = describeNoteError(err, 'Could not save this note.')
        }
      },
    },
  )
}

// ---- Delete (ConfirmDialog) -------------------------------------------------
const deleteNote = useDeleteNoteMutation(orgId, () => props.id)
const deleteNoteTarget = ref<{ id: string } | null>(null)
const deleteNoteDialogOpen = ref(false)

function openDeleteNote(note: NotePayload) {
  deleteNoteTarget.value = { id: note.id }
  deleteNote.reset()
  deleteNoteDialogOpen.value = true
}

function closeDeleteNoteDialog() {
  if (deleteNote.isPending.value) return
  deleteNoteDialogOpen.value = false
}

function confirmDeleteNote() {
  const target = deleteNoteTarget.value
  if (!target || deleteNote.isPending.value) return
  deleteNote.mutate(
    { personId: props.id, noteId: target.id },
    {
      onSuccess: () => {
        deleteNoteDialogOpen.value = false
        if (editingNote.value?.id === target.id) editingNote.value = null
      },
    },
  )
}

// ---- Tasks (SLICE_016.md §8, §12.8) ----------------------------------------
// A Tasks card above History: an Add form, then one row per open task in
// server order (`open_for_person`: due_at ASC NULLS LAST, created_at, id —
// never re-sorted here). Complete/Edit/Delete render only where
// `task.can_manage`. Mutations are pessimistic (the note precedent above).
const TASK_KIND_OPTIONS: { value: TaskKind; label: string }[] = [
  { value: 'follow_up', label: 'Follow up' },
  { value: 'call', label: 'Call' },
  { value: 'email', label: 'Email' },
  { value: 'text', label: 'Text' },
  { value: 'other', label: 'Other' },
]
const TASK_KIND_LABEL: Record<TaskKind, string> = {
  follow_up: 'Follow up',
  call: 'Call',
  email: 'Email',
  text: 'Text',
  other: 'Other',
}

// Members for the assignee picker: filtered client-side to `status ===
// 'active'` (the `PreviewTodayFeedDialog.vue` precedent) — a deactivated
// member is never a choosable assignee (rule 3: tasks require an ACTIVE
// assignee, unlike `AssignPerson`). The full (unfiltered) list backs the
// "(inactive)" suffix on an existing row's already-deactivated assignee.
const activeMemberOptions = computed(() =>
  (membersData.value?.members ?? [])
    .filter((member) => member.status === 'active')
    .map((member) => ({ id: member.user_id, display_name: member.display_name })),
)
const memberStatusById = computed(() => {
  const map = new Map<string, 'active' | 'inactive'>()
  for (const member of membersData.value?.members ?? []) map.set(member.user_id, member.status)
  return map
})
function assigneeSuffix(assignee: ActorRef | null): string {
  if (!assignee) return ''
  return memberStatusById.value.get(assignee.id) === 'inactive' ? ' (inactive)' : ''
}

// Round-2 review, item 3: the Edit form's assignee picker is
// `activeMemberOptions` plus, when the task being edited holds an
// assignee who is NOT active (deactivated since the task was assigned,
// or — reaching here only via an admin's edit — an imported task's
// unmatched-then-since-assigned member), that one member appended with
// an "(inactive)" suffix, so the field is never blank and an untouched
// Save re-sends the stored id rather than silently switching assignee.
const editTaskAssigneeOptions = computed(() => {
  const options = [...activeMemberOptions.value]
  const currentId = editingTask.value?.assigneeUserId
  if (currentId && !options.some((option) => option.id === currentId)) {
    const member = (membersData.value?.members ?? []).find((m) => m.user_id === currentId)
    if (member) {
      options.push({ id: member.user_id, display_name: `${member.display_name} (inactive)` })
    }
  }
  return options
})

// Rule 2: a date-only pick is converted to LOCAL end of day (23:59:59 in
// the browser's own zone) — client-side, no Organization timezone exists.
// An optional time input picks an exact local instant on that date instead.
function dateOnlyToLocalEndOfDay(dateOnly: string): string {
  const [year, month, day] = dateOnly.split('-').map(Number)
  return new Date(year, month - 1, day, 23, 59, 59, 0).toISOString()
}
function dateAndOptionalTimeToLocalInstant(dateOnly: string, timeOnly: string): string {
  if (timeOnly === '') return dateOnlyToLocalEndOfDay(dateOnly)
  const [year, month, day] = dateOnly.split('-').map(Number)
  const [hour, minute] = timeOnly.split(':').map(Number)
  return new Date(year, month - 1, day, hour, minute, 0, 0).toISOString()
}
function isoToLocalDateInput(iso: string): string {
  const d = new Date(iso)
  const y = String(d.getFullYear()).padStart(4, '0')
  const m = String(d.getMonth() + 1).padStart(2, '0')
  const day = String(d.getDate()).padStart(2, '0')
  return `${y}-${m}-${day}`
}
function isoToLocalTimeInput(iso: string): string {
  const d = new Date(iso)
  const h = String(d.getHours()).padStart(2, '0')
  const m = String(d.getMinutes()).padStart(2, '0')
  return `${h}:${m}`
}

/** The due badge (§8: "Overdue", "Due today", or the date; monochrome per
 * D-045 — never a colored tint). `null` when the task has no `due_at` (it
 * lives on the Person page only, rule 2). Overdue takes precedence over
 * "same calendar day" (a task due earlier today is overdue, not "due
 * today"). */
function dueBadge(dueAt: string | null): { label: string; overdue: boolean } | null {
  if (dueAt === null) return null
  const due = new Date(dueAt)
  const now = new Date()
  if (due.getTime() < now.getTime()) return { label: 'Overdue', overdue: true }
  const sameDay =
    due.getFullYear() === now.getFullYear() && due.getMonth() === now.getMonth() && due.getDate() === now.getDate()
  if (sameDay) return { label: 'Due today', overdue: false }
  return { label: formatDateOnly(dueAt), overdue: false }
}

// ---- Add task form ----------------------------------------------------
const createTask = useCreateTaskMutation(orgId, () => props.id)
const taskAddTitle = ref('')
const taskAddKind = ref<TaskKind>('follow_up')
const taskAddDate = ref('')
const taskAddTime = ref('')
const taskAddAssignee = ref('')
const taskAddError = ref<string | null>(null)

// Defaults the assignee picker to the viewer once `me` resolves — never
// overwritten again, so a member who deliberately clears it stays cleared.
watch(
  () => me.value?.user.id,
  (id) => {
    if (id && taskAddAssignee.value === '') taskAddAssignee.value = id
  },
  { immediate: true },
)

const taskAddDisabled = computed(() => taskAddTitle.value.trim() === '' || createTask.isPending.value)

function resetTaskAddForm() {
  taskAddTitle.value = ''
  taskAddKind.value = 'follow_up'
  taskAddDate.value = ''
  taskAddTime.value = ''
  // Assignee intentionally kept (defaults to the viewer for the next add).
}

function submitTask() {
  if (taskAddDisabled.value) return
  taskAddError.value = null
  const dueAt = taskAddDate.value === '' ? null : dateAndOptionalTimeToLocalInstant(taskAddDate.value, taskAddTime.value)
  createTask.mutate(
    {
      personId: props.id,
      title: taskAddTitle.value,
      kind: taskAddKind.value,
      dueAt,
      assigneeUserId: taskAddAssignee.value === '' ? null : taskAddAssignee.value,
    },
    {
      onSuccess: () => { resetTaskAddForm() },
      onError: (err) => {
        if (err instanceof ApiError && err.code === 'invalid_assignee') {
          void queryClient.invalidateQueries({ queryKey: queryKeys.members(orgId.value) })
        }
        taskAddError.value = describeTaskError(err, 'Could not add this task.')
      },
    },
  )
}

function onTaskAddTitleKeydown(event: KeyboardEvent) {
  if ((event.ctrlKey || event.metaKey) && event.key === 'Enter') {
    event.preventDefault()
    submitTask()
  }
}

// ---- Inline edit (local state keyed by task id, the note precedent) ------
interface EditingTask {
  id: string
  title: string
  kind: TaskKind
  dateStr: string
  timeStr: string
  assigneeUserId: string
  // True once the viewer has touched the date or time input — otherwise
  // Save re-sends `originalDueAt` VERBATIM (§1 rule 2 "an untouched date
  // re-sends the stored instant"; §12.8 "an untouched Save yields
  // changed: false"), since the inputs' minute granularity would otherwise
  // silently drop a stored second/sub-second offset and manufacture a
  // spurious change.
  dateTouched: boolean
  originalDueAt: string | null
  error: string | null
}
const editingTask = ref<EditingTask | null>(null)
const updateTask = useUpdateTaskMutation(orgId, () => props.id)
const editTaskButtonRefs: Record<string, HTMLButtonElement | null> = {}
function setEditTaskButtonRef(id: string, el: unknown) {
  editTaskButtonRefs[id] = el instanceof HTMLButtonElement ? el : null
}

// Round-2 review, item 4: a Save 404 (the task was deleted elsewhere
// between the last read and the write) is a minimal resolution — the
// editor sits inside the `v-for`, so the settle refetch that follows
// unmounts it outright rather than leaving it live the way the note
// editor's "deleted elsewhere" case does. Once a refetch shows the task
// being edited is neither open nor completed (genuinely gone, not just
// completed by someone else — that would still show it under `history`),
// clear the editor and surface one dismissible banner instead of trying
// to keep a draft for a row that no longer exists.
const taskEditGoneMessage = ref<string | null>(null)
// Round-2 review, items 4 and 9: a refetch (any reason — this mutation's
// own settle after a 403/404, realtime, focus) can show the task being
// edited in one of three states relative to when editing started: still
// open (possibly with `can_manage` now false — a 403 race), completed by
// someone else while editing (also possibly `can_manage: false` now), or
// genuinely gone (deleted elsewhere). Only the last shows the dismissible
// banner; the first two just close the editor quietly, since the row
// itself is still visible, only read-only now.
watch(
  () => detail.value,
  () => {
    const editing = editingTask.value
    if (!editing) return
    const stillOpen = tasks.value.find((t) => t.id === editing.id)
    if (stillOpen) {
      if (!stillOpen.can_manage) editingTask.value = null
      return
    }
    const completedEntry = history.value.find(
      (entry) => entry.kind === 'task_completed' && entry.id === editing.id,
    )
    if (completedEntry) {
      if (completedEntry.kind === 'task_completed' && !completedEntry.detail.can_manage) {
        editingTask.value = null
      }
      return
    }
    editingTask.value = null
    taskEditGoneMessage.value = 'This task was deleted by someone else.'
  },
)

function startEditTask(task: Task) {
  if (editingTask.value?.id === task.id) return
  editingTask.value = {
    id: task.id,
    title: task.title,
    kind: task.kind,
    dateStr: task.due_at ? isoToLocalDateInput(task.due_at) : '',
    timeStr: task.due_at ? isoToLocalTimeInput(task.due_at) : '',
    // Round-2 review, item 3: an imported task can have no assignee at
    // all (rule 1); default the field to the viewer rather than leaving
    // it blank, so Save can never send `assignee_user_id: ""`. Reaching
    // this form at all already requires `can_manage` (an admin, since a
    // NULL-assignee task's creator is also NULL), so the viewer is
    // always a legitimate active member to fall back to.
    assigneeUserId: task.assignee?.id ?? me.value?.user.id ?? '',
    dateTouched: false,
    originalDueAt: task.due_at,
    error: null,
  }
}

function cancelEditTask() {
  if (!editingTask.value || updateTask.isPending.value) return
  const id = editingTask.value.id
  editingTask.value = null
  void nextTick(() => editTaskButtonRefs[id]?.focus())
}

function onEditTaskKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape') {
    event.preventDefault()
    cancelEditTask()
  }
}

const editingTaskTitleModel = computed<string>({
  get: () => editingTask.value?.title ?? '',
  set: (value) => { if (editingTask.value) editingTask.value.title = value },
})
const editingTaskKindModel = computed<TaskKind>({
  get: () => editingTask.value?.kind ?? 'follow_up',
  set: (value) => { if (editingTask.value) editingTask.value.kind = value },
})
const editingTaskDateModel = computed<string>({
  get: () => editingTask.value?.dateStr ?? '',
  set: (value) => {
    if (!editingTask.value) return
    editingTask.value.dateStr = value
    editingTask.value.dateTouched = true
  },
})
const editingTaskTimeModel = computed<string>({
  get: () => editingTask.value?.timeStr ?? '',
  set: (value) => {
    if (!editingTask.value) return
    editingTask.value.timeStr = value
    editingTask.value.dateTouched = true
  },
})
const editingTaskAssigneeModel = computed<string>({
  get: () => editingTask.value?.assigneeUserId ?? '',
  set: (value) => { if (editingTask.value) editingTask.value.assigneeUserId = value },
})

const editTaskSaveDisabled = computed(() => {
  const editing = editingTask.value
  if (!editing) return true
  return editing.title.trim() === '' || updateTask.isPending.value
})

function saveEditTask() {
  const editing = editingTask.value
  if (!editing || editTaskSaveDisabled.value) return
  editing.error = null
  const dueAt = editing.dateTouched
    ? (editing.dateStr === '' ? null : dateAndOptionalTimeToLocalInstant(editing.dateStr, editing.timeStr))
    : editing.originalDueAt
  updateTask.mutate(
    {
      personId: props.id,
      taskId: editing.id,
      title: editing.title,
      kind: editing.kind,
      dueAt,
      assigneeUserId: editing.assigneeUserId,
    },
    {
      onSuccess: () => {
        if (editingTask.value?.id === editing.id) editingTask.value = null
      },
      onError: (err) => {
        const current = editingTask.value
        if (current?.id !== editing.id) return
        if (err instanceof ApiError && err.code === 'invalid_assignee') {
          void queryClient.invalidateQueries({ queryKey: queryKeys.members(orgId.value) })
        }
        current.error = describeTaskError(err, 'Could not save this task.')
      },
    },
  )
}

// ---- Complete / Reopen --------------------------------------------------
const completeTask = useCompleteTaskMutation(orgId, () => props.id)
const reopenTask = useReopenTaskMutation(orgId, () => props.id)
const taskActionError = ref<{ id: string; message: string } | null>(null)

// Round-2 review, item 2: the handler itself guards on `isPending`, not
// only the button's `:disabled` — a real double-click can fire both
// native click events before Vue's next render paints the disabled
// attribute, so the template guard alone is not sufficient.
function onCompleteTask(task: Task) {
  if (completeTask.isPending.value) return
  taskActionError.value = null
  completeTask.mutate(
    { personId: props.id, taskId: task.id },
    { onError: (err) => { taskActionError.value = { id: task.id, message: describeTaskError(err, 'Could not complete this task.') } } },
  )
}
function isCompletingTask(taskId: string): boolean {
  return completeTask.isPending.value && completeTask.variables.value?.taskId === taskId
}

function onReopenTask(taskId: string) {
  if (reopenTask.isPending.value) return
  taskActionError.value = null
  reopenTask.mutate(
    { personId: props.id, taskId },
    { onError: (err) => { taskActionError.value = { id: taskId, message: describeTaskError(err, 'Could not reopen this task.') } } },
  )
}
function isReopeningTask(taskId: string): boolean {
  return reopenTask.isPending.value && reopenTask.variables.value?.taskId === taskId
}

// ---- Delete (ConfirmDialog) -----------------------------------------------
const deleteTask = useDeleteTaskMutation(orgId, () => props.id)
const deleteTaskTarget = ref<{ id: string } | null>(null)
const deleteTaskDialogOpen = ref(false)

function openDeleteTask(taskId: string) {
  deleteTaskTarget.value = { id: taskId }
  deleteTask.reset()
  deleteTaskDialogOpen.value = true
}
function closeDeleteTaskDialog() {
  if (deleteTask.isPending.value) return
  deleteTaskDialogOpen.value = false
}
function confirmDeleteTask() {
  const target = deleteTaskTarget.value
  if (!target || deleteTask.isPending.value) return
  deleteTask.mutate(
    { personId: props.id, taskId: target.id },
    {
      onSuccess: () => {
        deleteTaskDialogOpen.value = false
        if (editingTask.value?.id === target.id) editingTask.value = null
      },
    },
  )
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
    addTagOpen.value = false
    noteDraft.value = ''
    noteAddError.value = null
    editingNote.value = null
    deleteNoteDialogOpen.value = false
    taskAddTitle.value = ''
    taskAddError.value = null
    editingTask.value = null
    taskActionError.value = null
    taskEditGoneMessage.value = null
    deleteTaskDialogOpen.value = false
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
    case 'correspondence': {
      // SLICE_009 §8: no address/subject/message-id (D-042.1/2) — direction,
      // agent, and whether the row is a retroactively-forwarded placement
      // are the only renderable facts.
      const { direction, agent, backdated } = entry.detail
      const label = direction === 'outbound' ? 'Outbound email' : 'Inbound email'
      return backdated ? `${label} — ${agent.display_name} (forwarded)` : `${label} — ${agent.display_name}`
    }
    case 'note': {
      // §5: "Note by <author>", "· edited" when edited. `actor` is null
      // only for an imported note whose FUB author matched no member.
      const base = entry.actor ? `Note by ${entry.actor.display_name}` : 'Note'
      return entry.detail.edited ? `${base} · edited` : base
    }
    case 'task_completed':
      // §8: "Completed task: <title>" — the secondary "<kind> · was due
      // <date>" line lives in `taskCompletedDescription` below, the note
      // body-line precedent.
      return `Completed task: ${entry.detail.title}`
    default:
      // §5: any `kind` this bundle's `HistoryEntry` union does not know
      // about (a future additive kind an old tab hasn't reloaded for)
      // renders as a generic row instead of throwing or showing nothing.
      return 'Activity'
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
/** Set only for `kind === 'task_completed'` (§8): the fields the row's
 * secondary description line and ghost Reopen button need. */
interface TaskCompletedPayload {
  id: string
  title: string
  /** "<kind> · was due <date>", or the kind alone with no due date. */
  description: string
  canManage: boolean
}

interface HistoryRow {
  key: string
  icon: Component
  summary: string
  actor: ActorRef | null
  occurredAt: string
  /** The Set/Change-outcome target when the row's call is mine and has an
   * effective attempt; `outcome` null while the call is incomplete. */
  change: OutcomeTarget | null
  /** Set only for `kind === 'note'` (§5): `id`, `body`, `edited`,
   * `canManage` — the fields the row's inline body/Edit/Delete UI needs. */
  note: NotePayload | null
  task: TaskCompletedPayload | null
}

type AttemptEntry = HistoryEntry & { kind: 'contact_attempted'; detail: ContactAttemptedDetail }

function plainRow(entry: HistoryEntry): HistoryRow {
  return {
    key: entry.id,
    icon: historyIcon(entry.kind),
    summary: historySummary(entry),
    actor: entry.actor,
    occurredAt: entry.occurred_at,
    change: null,
    note:
      entry.kind === 'note'
        ? { id: entry.id, body: entry.detail.body, edited: entry.detail.edited, canManage: entry.detail.can_manage }
        : null,
    // §8: "<kind> · was due <date>" under the "Completed task: <title>"
    // summary line; a task with no due date (rare — it never reaches
    // Today, but can still be completed) shows the kind alone.
    task:
      entry.kind === 'task_completed'
        ? {
            id: entry.id,
            title: entry.detail.title,
            description: entry.detail.due_at
              ? `${TASK_KIND_LABEL[entry.detail.kind]} · was due ${formatDateOnly(entry.detail.due_at)}`
              : TASK_KIND_LABEL[entry.detail.kind],
            canManage: entry.detail.can_manage,
          }
        : null,
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

    rows.push({
      key: entry.id,
      icon: HISTORY_ICON.call_completed,
      summary,
      actor,
      occurredAt: entry.occurred_at,
      change,
      note: null,
      task: null,
    })
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

          <FormField
            label="Tags"
            bare
          >
            <div class="flex flex-wrap items-center gap-2">
              <span
                v-for="tag in personTags"
                :key="tag.id"
                data-testid="person-tag-chip"
                class="inline-flex items-stretch rounded-lg border border-border bg-surface-0 text-small text-text"
              >
                <span class="inline-flex min-h-10 items-center px-3">{{ tag.name }}</span>
                <button
                  type="button"
                  class="inline-flex min-h-10 w-10 shrink-0 items-center justify-center rounded-r-lg hover:bg-surface-1 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-focus disabled:opacity-50"
                  :aria-label="`Remove tag ${tag.name}`"
                  :disabled="isRemovingTag(tag.id)"
                  data-testid="remove-person-tag"
                  @click="removeTag(tag.id)"
                >
                  <X
                    class="h-3.5 w-3.5"
                    stroke-width="1.5"
                    aria-hidden="true"
                  />
                </button>
              </span>

              <div
                ref="addTagRoot"
                class="relative"
              >
                <button
                  ref="addTagButton"
                  type="button"
                  :class="buttonClasses('ghost')"
                  :aria-expanded="addTagOpen"
                  aria-haspopup="dialog"
                  aria-controls="add-tag-popover"
                  data-testid="add-tag-button"
                  @click="toggleAddTag"
                >
                  <Plus
                    class="h-4 w-4"
                    stroke-width="1.5"
                    aria-hidden="true"
                  />
                  Add tag
                </button>
                <div
                  v-if="addTagOpen"
                  id="add-tag-popover"
                  role="dialog"
                  aria-label="Add tag"
                  class="glass-panel absolute left-0 top-full z-50 mt-2 w-64 p-2"
                  data-testid="add-tag-popover"
                >
                  <input
                    ref="addTagInput"
                    v-model="addTagQuery"
                    type="text"
                    aria-label="Search tags"
                    placeholder="Search or create a tag"
                    :disabled="addTagPending"
                    :class="INPUT_CLASSES"
                    data-testid="add-tag-search"
                    @keydown="onAddTagInputKeydown"
                  >
                  <ul
                    role="listbox"
                    aria-label="Tag options"
                    class="mt-2 max-h-48 space-y-0.5 overflow-auto"
                  >
                    <li
                      v-for="(option, index) in addTagOptions"
                      :key="option.kind === 'existing' ? option.tag.id : `create:${option.name}`"
                      role="option"
                      :aria-selected="index === addTagActiveIndex"
                      :data-testid="option.kind === 'create' ? 'add-tag-create-row' : 'add-tag-option'"
                      class="flex min-h-10 cursor-pointer items-center rounded-md px-2 text-body"
                      :class="index === addTagActiveIndex ? 'bg-surface-2 text-text' : 'text-text-muted'"
                      @mouseenter="addTagActiveIndex = index"
                      @click="chooseAddTagOption(option)"
                    >
                      {{ option.kind === 'existing' ? option.tag.name : `Create '${option.name}'` }}
                    </li>
                    <li
                      v-if="addTagOptions.length === 0"
                      class="px-2 py-2 text-small text-text-muted"
                    >
                      No matching tags.
                    </li>
                  </ul>
                  <p
                    v-if="addTagError"
                    role="alert"
                    class="mt-2 text-small text-danger"
                    data-testid="add-tag-error"
                  >
                    {{ addTagError }}
                  </p>
                </div>
              </div>
            </div>
            <p
              v-if="removeTagError"
              role="alert"
              class="mt-1.5 text-small text-danger"
              data-testid="remove-tag-error"
            >
              {{ removeTagError }}
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

      <div
        v-if="taskEditGoneMessage"
        role="alert"
        class="mb-4 flex items-center justify-between gap-3 rounded-xl border border-border bg-surface-0 p-3 text-body text-danger"
        data-testid="task-edit-gone-banner"
      >
        <span>{{ taskEditGoneMessage }}</span>
        <button
          type="button"
          :class="buttonClasses('ghost')"
          data-testid="task-edit-gone-dismiss"
          @click="taskEditGoneMessage = null"
        >
          Dismiss
        </button>
      </div>

      <Card>
        <h2 class="mb-4 text-section font-semibold text-text">
          Tasks
        </h2>

        <div
          class="mb-4 rounded-xl border border-border p-3"
          data-testid="task-add-form"
        >
          <div class="grid gap-3 sm:grid-cols-2">
            <div class="sm:col-span-2">
              <label
                for="task-add-title"
                :class="LABEL_CLASSES"
              >Title</label>
              <input
                id="task-add-title"
                v-model="taskAddTitle"
                type="text"
                :class="INPUT_CLASSES"
                placeholder="What needs to happen?"
                :disabled="createTask.isPending.value"
                data-testid="task-add-title"
                @keydown="onTaskAddTitleKeydown"
              >
            </div>
            <FormField
              label="Kind"
              bare
            >
              <Select
                v-model="taskAddKind"
                :options="TASK_KIND_OPTIONS"
                option-label="label"
                option-value="value"
                aria-label="Kind"
                :disabled="createTask.isPending.value"
                :pt="selectPt()"
                data-testid="task-add-kind"
              />
            </FormField>
            <FormField
              label="Assignee"
              bare
            >
              <Select
                v-model="taskAddAssignee"
                :options="activeMemberOptions"
                option-label="display_name"
                option-value="id"
                aria-label="Assignee"
                :loading="membersPending"
                :disabled="createTask.isPending.value"
                :pt="selectPt()"
                data-testid="task-add-assignee"
              />
            </FormField>
            <div>
              <label
                for="task-add-date"
                :class="LABEL_CLASSES"
              >Due date</label>
              <input
                id="task-add-date"
                v-model="taskAddDate"
                type="date"
                :class="INPUT_CLASSES"
                :disabled="createTask.isPending.value"
                data-testid="task-add-date"
              >
            </div>
            <div>
              <label
                for="task-add-time"
                :class="LABEL_CLASSES"
              >Due time (optional)</label>
              <input
                id="task-add-time"
                v-model="taskAddTime"
                type="time"
                :class="INPUT_CLASSES"
                :disabled="createTask.isPending.value || taskAddDate === ''"
                data-testid="task-add-time"
              >
            </div>
          </div>
          <div class="mt-3 flex justify-end">
            <button
              type="button"
              :class="buttonClasses('primary')"
              :disabled="taskAddDisabled"
              data-testid="task-add-submit"
              @click="submitTask"
            >
              {{ createTask.isPending.value ? 'Adding…' : 'Add task' }}
            </button>
          </div>
          <p
            v-if="taskAddError"
            role="alert"
            class="mt-2 text-small text-danger"
            data-testid="task-add-error"
          >
            {{ taskAddError }}
          </p>
        </div>

        <ul
          v-if="tasks.length > 0"
          class="divide-y divide-border"
          data-testid="task-list"
        >
          <li
            v-for="task in tasks"
            :key="task.id"
            class="flex min-h-14 items-start gap-3 py-2 first:pt-0 last:pb-0"
            data-testid="task-row"
          >
            <div class="min-w-0 flex-1 self-center">
              <template v-if="editingTask?.id === task.id">
                <div class="grid gap-2 sm:grid-cols-2">
                  <label
                    :for="`task-edit-title-${task.id}`"
                    class="sr-only"
                  >Edit task title</label>
                  <input
                    :id="`task-edit-title-${task.id}`"
                    v-model="editingTaskTitleModel"
                    type="text"
                    :class="[INPUT_CLASSES, 'sm:col-span-2']"
                    :disabled="updateTask.isPending.value"
                    data-testid="task-edit-title"
                    @keydown="onEditTaskKeydown"
                  >
                  <Select
                    v-model="editingTaskKindModel"
                    :options="TASK_KIND_OPTIONS"
                    option-label="label"
                    option-value="value"
                    aria-label="Kind"
                    :disabled="updateTask.isPending.value"
                    :pt="selectPt()"
                    data-testid="task-edit-kind"
                  />
                  <Select
                    v-model="editingTaskAssigneeModel"
                    :options="editTaskAssigneeOptions"
                    option-label="display_name"
                    option-value="id"
                    aria-label="Assignee"
                    :disabled="updateTask.isPending.value"
                    :pt="selectPt()"
                    data-testid="task-edit-assignee"
                  />
                  <input
                    v-model="editingTaskDateModel"
                    type="date"
                    aria-label="Due date"
                    :class="INPUT_CLASSES"
                    :disabled="updateTask.isPending.value"
                    data-testid="task-edit-date"
                  >
                  <input
                    v-model="editingTaskTimeModel"
                    type="time"
                    aria-label="Due time (optional)"
                    :class="INPUT_CLASSES"
                    :disabled="updateTask.isPending.value || editingTaskDateModel === ''"
                    data-testid="task-edit-time"
                  >
                </div>
                <p
                  v-if="editingTask?.error"
                  role="alert"
                  class="mt-2 text-small text-danger"
                  data-testid="task-edit-error"
                >
                  {{ editingTask?.error }}
                </p>
                <div class="mt-2 flex items-center gap-2">
                  <button
                    type="button"
                    :class="buttonClasses('primary')"
                    :disabled="editTaskSaveDisabled"
                    data-testid="task-edit-save"
                    @click="saveEditTask"
                  >
                    {{ updateTask.isPending.value ? 'Saving…' : 'Save' }}
                  </button>
                  <button
                    type="button"
                    :class="buttonClasses('secondary')"
                    :disabled="updateTask.isPending.value"
                    data-testid="task-edit-cancel"
                    @click="cancelEditTask"
                  >
                    Cancel
                  </button>
                </div>
              </template>
              <template v-else>
                <div class="flex flex-wrap items-center gap-2">
                  <span data-testid="task-kind-badge">
                    <Badge tint="neutral">
                      {{ TASK_KIND_LABEL[task.kind] }}
                    </Badge>
                  </span>
                  <span
                    class="text-body text-text"
                    data-testid="task-title"
                  >{{ task.title }}</span>
                  <span
                    v-if="dueBadge(task.due_at)"
                    data-testid="task-due-badge"
                  >
                    <Badge tint="neutral">
                      {{ dueBadge(task.due_at)!.label }}
                    </Badge>
                  </span>
                </div>
                <p
                  class="mt-1 text-small text-text-muted"
                  data-testid="task-assignee"
                >
                  {{ task.assignee ? `${task.assignee.display_name}${assigneeSuffix(task.assignee)}` : 'Unassigned' }}
                </p>
                <p
                  v-if="taskActionError?.id === task.id"
                  role="alert"
                  class="mt-1 text-small text-danger"
                  data-testid="task-action-error"
                >
                  {{ taskActionError.message }}
                </p>
              </template>
            </div>
            <div
              v-if="task.can_manage && editingTask?.id !== task.id"
              class="flex shrink-0 items-center gap-1"
            >
              <button
                type="button"
                class="inline-flex h-10 w-10 items-center justify-center rounded-lg text-text-muted transition-colors duration-150 ease-out hover:bg-surface-2 hover:text-text focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-focus disabled:opacity-50"
                aria-label="Complete task"
                :disabled="isCompletingTask(task.id)"
                data-testid="complete-task"
                @click="onCompleteTask(task)"
              >
                <Check
                  class="h-4 w-4"
                  stroke-width="1.5"
                  aria-hidden="true"
                />
              </button>
              <button
                :ref="(el) => setEditTaskButtonRef(task.id, el)"
                type="button"
                class="inline-flex h-10 w-10 items-center justify-center rounded-lg text-text-muted transition-colors duration-150 ease-out hover:bg-surface-2 hover:text-text focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-focus"
                aria-label="Edit task"
                data-testid="edit-task"
                @click="startEditTask(task)"
              >
                <Pencil
                  class="h-4 w-4"
                  stroke-width="1.5"
                  aria-hidden="true"
                />
              </button>
              <button
                type="button"
                class="inline-flex h-10 w-10 items-center justify-center rounded-lg text-text-muted transition-colors duration-150 ease-out hover:bg-surface-2 hover:text-text focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-focus"
                aria-label="Delete task"
                data-testid="delete-task"
                @click="openDeleteTask(task.id)"
              >
                <Trash2
                  class="h-4 w-4"
                  stroke-width="1.5"
                  aria-hidden="true"
                />
              </button>
            </div>
          </li>
        </ul>
        <p
          v-else
          class="text-body text-text-muted"
          data-testid="task-list-empty"
        >
          No open tasks.
        </p>
      </Card>

      <Card>
        <h2 class="mb-4 text-section font-semibold text-text">
          History
        </h2>

        <div
          class="mb-4 rounded-xl border border-border p-3"
          data-testid="note-composer"
        >
          <label
            for="note-composer-textarea"
            :class="LABEL_CLASSES"
          >Add a note</label>
          <textarea
            id="note-composer-textarea"
            v-model="noteDraft"
            :class="TEXTAREA_CLASSES"
            placeholder="Add a note…"
            :disabled="addNote.isPending.value"
            data-testid="note-composer-textarea"
            @keydown="onNoteComposerKeydown"
          />
          <div class="mt-2 flex items-center justify-between gap-3">
            <p
              v-if="noteDraftCodePoints > NOTE_COUNTER_THRESHOLD"
              class="text-small"
              :class="noteDraftOverLimit ? 'text-danger' : 'text-text-muted'"
              data-testid="note-composer-counter"
            >
              {{ noteDraftCodePoints }}/{{ NOTE_MAX_CHARS }}
            </p>
            <span v-else />
            <button
              type="button"
              :class="buttonClasses('primary')"
              :disabled="noteAddDisabled"
              data-testid="note-composer-submit"
              @click="submitNote"
            >
              {{ addNote.isPending.value ? 'Adding…' : 'Add note' }}
            </button>
          </div>
          <p
            v-if="noteAddError"
            role="alert"
            class="mt-2 text-small text-danger"
            data-testid="note-composer-error"
          >
            {{ noteAddError }}
          </p>
        </div>

        <ul
          v-if="history.length > 0 || (editingNote && editingNoteGone)"
          class="divide-y divide-border"
        >
          <li
            v-if="editingNote && editingNoteGone"
            class="flex items-start gap-3 py-2 first:pt-0"
            data-testid="note-edit-gone"
          >
            <div class="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-surface-2">
              <StickyNote
                class="h-4 w-4 text-text-muted"
                stroke-width="1.5"
              />
            </div>
            <div class="min-w-0 flex-1">
              <p
                class="text-body text-danger"
                data-testid="note-edit-gone-message"
              >
                This note was deleted by someone else.
              </p>
              <textarea
                v-model="editingNoteDraftModel"
                :class="[TEXTAREA_CLASSES, 'mt-2']"
                aria-label="Note draft"
                data-testid="note-edit-gone-draft"
              />
              <div class="mt-2">
                <button
                  type="button"
                  :class="buttonClasses('secondary')"
                  data-testid="note-edit-gone-dismiss"
                  @click="editingNote = null"
                >
                  Dismiss
                </button>
              </div>
            </div>
          </li>
          <li
            v-for="row in historyRows"
            :key="row.key"
            class="flex min-h-14 items-start gap-3 py-2 first:pt-0 last:pb-0"
          >
            <div class="mt-1 flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-surface-2">
              <component
                :is="row.icon"
                class="h-4 w-4 text-text-muted"
                stroke-width="1.5"
              />
            </div>
            <div class="min-w-0 flex-1 self-center">
              <template v-if="row.note && editingNote?.id === row.note.id">
                <label
                  :for="`note-edit-${row.note.id}`"
                  class="sr-only"
                >Edit note</label>
                <textarea
                  :id="`note-edit-${row.note.id}`"
                  v-model="editingNoteDraftModel"
                  :class="TEXTAREA_CLASSES"
                  :disabled="editNote.isPending.value"
                  data-testid="note-edit-textarea"
                  @keydown="onEditNoteKeydown"
                />
                <p
                  v-if="editNoteCodePoints > NOTE_COUNTER_THRESHOLD"
                  class="mt-1 text-small"
                  :class="editNoteCodePoints > NOTE_MAX_CHARS ? 'text-danger' : 'text-text-muted'"
                  data-testid="note-edit-counter"
                >
                  {{ editNoteCodePoints }}/{{ NOTE_MAX_CHARS }}
                </p>
                <p
                  v-if="editingNote?.error"
                  role="alert"
                  class="mt-1 text-small text-danger"
                  data-testid="note-edit-error"
                >
                  {{ editingNote?.error }}
                </p>
                <div class="mt-2 flex items-center gap-2">
                  <button
                    type="button"
                    :class="buttonClasses('primary')"
                    :disabled="editNoteSaveDisabled"
                    data-testid="note-edit-save"
                    @click="saveEditNote"
                  >
                    {{ editNote.isPending.value ? 'Saving…' : 'Save' }}
                  </button>
                  <button
                    type="button"
                    :class="buttonClasses('secondary')"
                    :disabled="editNote.isPending.value"
                    data-testid="note-edit-cancel"
                    @click="cancelEditNote"
                  >
                    Cancel
                  </button>
                </div>
              </template>
              <template v-else>
                <p
                  class="text-body text-text"
                  data-testid="history-summary"
                >
                  {{ row.summary }}
                </p>
                <p
                  v-if="row.note"
                  class="mt-1 whitespace-pre-wrap text-body text-text"
                  data-testid="note-body"
                >
                  {{ row.note.body }}
                </p>
                <p
                  v-if="row.task"
                  class="mt-1 text-body text-text-muted"
                  data-testid="task-completed-detail"
                >
                  {{ row.task.description }}
                </p>
                <p
                  v-if="row.task && taskActionError?.id === row.task.id"
                  role="alert"
                  class="mt-1 text-small text-danger"
                  data-testid="task-action-error"
                >
                  {{ taskActionError.message }}
                </p>
                <p class="text-small text-text-muted">
                  {{ row.actor?.display_name ?? 'System' }} ·
                  <span :title="formatAbsoluteTime(row.occurredAt)">{{ formatRelativeTime(row.occurredAt) }}</span>
                </p>
              </template>
            </div>
            <div
              v-if="row.note && row.note.canManage && editingNote?.id !== row.note.id"
              class="flex shrink-0 items-center gap-1"
            >
              <button
                :ref="(el) => setEditButtonRef(row.note!.id, el)"
                type="button"
                class="inline-flex h-10 w-10 items-center justify-center rounded-lg text-text-muted transition-colors duration-150 ease-out hover:bg-surface-2 hover:text-text focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-focus"
                aria-label="Edit note"
                data-testid="edit-note"
                @click="startEditNote(row.note!)"
              >
                <Pencil
                  class="h-4 w-4"
                  stroke-width="1.5"
                  aria-hidden="true"
                />
              </button>
              <button
                type="button"
                class="inline-flex h-10 w-10 items-center justify-center rounded-lg text-text-muted transition-colors duration-150 ease-out hover:bg-surface-2 hover:text-text focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-focus"
                aria-label="Delete note"
                data-testid="delete-note"
                @click="openDeleteNote(row.note!)"
              >
                <Trash2
                  class="h-4 w-4"
                  stroke-width="1.5"
                  aria-hidden="true"
                />
              </button>
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
            <button
              v-if="row.task && row.task.canManage"
              type="button"
              :class="buttonClasses('ghost')"
              :disabled="isReopeningTask(row.task.id)"
              data-testid="reopen-task"
              @click="onReopenTask(row.task.id)"
            >
              <Undo2
                class="h-4 w-4"
                stroke-width="1.5"
                aria-hidden="true"
              />
              Reopen
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

      <ConfirmDialog
        :visible="deleteNoteDialogOpen"
        title="Delete note"
        message="Delete this note? This cannot be undone."
        confirm-label="Delete"
        confirm-variant="danger"
        :is-pending="deleteNote.isPending.value"
        :error="deleteNote.error.value"
        error-fallback="Could not delete this note."
        @update:visible="(value: boolean) => { if (!value) closeDeleteNoteDialog() }"
        @confirm="confirmDeleteNote"
      />

      <ConfirmDialog
        :visible="deleteTaskDialogOpen"
        title="Delete task"
        message="Delete this task? This cannot be undone."
        confirm-label="Delete"
        confirm-variant="danger"
        :is-pending="deleteTask.isPending.value"
        :error="deleteTask.error.value"
        error-fallback="Could not delete this task."
        @update:visible="(value: boolean) => { if (!value) closeDeleteTaskDialog() }"
        @confirm="confirmDeleteTask"
      />

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
