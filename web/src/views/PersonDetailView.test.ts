// SLICE_006 §13 item 4: Call disabled without a phone; number picker only
// with ≥ 2 phones; `call_completed` history rendering; one primary per
// view; the Call → panel → Hang up flow against a fake room. SLICE_006c §13
// item 3 / §5a (D-033): the post-call prompt's forced choice against the
// mocked route (no default, no Skip, Save gated on a pick and the server's
// status — seeded `answered`, refetched `ended`), the error copy per code,
// one-row-per-call history rendering (an agent choice names the outcome;
// the automatic root reads "outcome needed"), Set/Change outcome only on
// the caller's own call rows, the Today → Person `?outcome=` path, and the
// manual dialog's widened vocabulary. Service-free:
// `apiFetch` is mocked, the LiveKit room is a fake injected via the
// `createRoom` prop, and PrimeVue runs unstyled as in main.ts.
import { flushPromises, mount } from '@vue/test-utils'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import PrimeVue from 'primevue/config'
import Select from 'primevue/select'
import { createMemoryHistory, createRouter } from 'vue-router'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ApiError, apiFetch } from '../api/client'
import type {
  ActorRef,
  AddNoteResponse,
  CallCompletedDetail,
  CallCompletedOutcome,
  CallView,
  CompleteTaskResponse,
  ContactAttemptedDetail,
  ContactMethod,
  CorrectOutcomeResponse,
  CreateTaskResponse,
  CustomField,
  DeleteNoteResponse,
  DeleteTaskResponse,
  EditNoteResponse,
  HistoryEntry,
  MeResponse,
  Member,
  PersonCustomFieldValue,
  PersonCustomFieldValueMutationResponse,
  PersonDetailResponse,
  ReopenTaskResponse,
  RoutingStrategy,
  SetCustomFieldValueRequest,
  Tag,
  TagRef,
  Task,
  TaskKind,
  UpdateTaskResponse,
} from '../api/types'
import { personMutationKey, queryKeys } from '../api/queries'
import type { CallRoom, CallRoomEvents, CallRoomFactory } from '../telephony/useCall'
import { defineComponent, nextTick } from 'vue'
import PersonDetailView from './PersonDetailView.vue'
import CallHostPanel from '../components/CallHostPanel.vue'
import { provideCallHost } from '../telephony/callHost'

vi.mock('../api/client', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api/client')>()
  return { ...actual, apiFetch: vi.fn() }
})

const apiFetchMock = vi.mocked(apiFetch)

const ORG_ID = '11111111-1111-1111-1111-111111111111'
const PERSON_ID = '33333333-3333-3333-3333-333333333333'
const CALL_ID = '55555555-5555-5555-5555-555555555555'
const PREVIOUS_CALL_ID = '66666666-6666-6666-6666-666666666666'
const PHONE_A: ContactMethod = { id: 'cm-phone-a', kind: 'phone', value: '+1 555 0100' }
const PHONE_B: ContactMethod = { id: 'cm-phone-b', kind: 'phone', value: '+1 555 0101' }
const EMAIL: ContactMethod = { id: 'cm-email', kind: 'email', value: 'grace@example.com' }

function me(): MeResponse {
  return {
    user: { id: 'u-alice', email: 'alice@acme.test', display_name: 'Alice' },
    organization: { workspace_mode: 'operational', workspace_revision: '1', id: ORG_ID, name: 'Acme Realty', role: 'member' },
    platform_admin: false,
  }
}

function detail(
  contactMethods: ContactMethod[],
  history: HistoryEntry[] = [],
  tags: TagRef[] = [],
  tasks: Task[] = [],
  customFields: PersonCustomFieldValue[] = [],
): PersonDetailResponse {
  return {
    person: {
      id: PERSON_ID,
      first_name: 'Grace',
      last_name: 'Hopper',
      display_name: 'Grace Hopper',
      stage: { id: 'stage-lead', name: 'Lead' },
      assigned_user: null,
      primary_email: null,
      primary_phone: null,
      inquiry_count: 0,
      last_inquiry_at: null,
      created_at: '2026-08-22T09:00:00.000Z',
    },
    contact_methods: contactMethods,
    inquiries: [],
    history,
    tags,
    tasks,
    custom_fields: customFields,
  }
}

function noteEntry(overrides: {
  id?: string
  body?: string
  edited?: boolean
  canManage?: boolean
  actor?: ActorRef | null
  occurredAt?: string
} = {}): HistoryEntry {
  const {
    id = 'note-1',
    body = 'A note',
    edited = false,
    canManage = true,
    actor = { id: 'u-alice', display_name: 'Alice' },
    occurredAt = '2026-08-22T09:30:00.000Z',
  } = overrides
  return {
    kind: 'note',
    id,
    occurred_at: occurredAt,
    recorded_at: occurredAt,
    actor,
    origin: 'web_session',
    correlation_id: 'corr-note',
    detail: { body, updated_at: occurredAt, edited, can_manage: canManage },
  }
}

function callView(overrides: Partial<CallView> = {}): CallView {
  return {
    id: CALL_ID,
    person_id: PERSON_ID,
    contact_method_id: PHONE_A.id,
    caller: { id: 'u-alice', display_name: 'Alice' },
    status: 'placing',
    failure_reason: null,
    end_reason: null,
    placed_at: '2026-08-22T10:00:00.000Z',
    ringing_at: null,
    answered_at: null,
    ended_at: null,
    talk_seconds: null,
    ...overrides,
  }
}

function correction(changed: boolean): CorrectOutcomeResponse {
  return {
    attempt: {
      id: 'att-2',
      channel: 'call',
      outcome: 'left_message',
      occurred_at: '2026-08-22T10:00:00.000Z',
      recorded_at: '2026-08-22T10:05:00.000Z',
      corrects_id: 'att-1',
    },
    changed,
  }
}

class FakeRoom implements CallRoom {
  handlers: { [E in keyof CallRoomEvents]: CallRoomEvents[E][] } = {
    participantConnected: [],
    participantDisconnected: [],
    participantAttributesChanged: [],
    trackSubscribed: [],
    trackUnsubscribed: [],
    disconnected: [],
  }
  on<E extends keyof CallRoomEvents>(event: E, handler: CallRoomEvents[E]): void {
    this.handlers[event].push(handler)
  }
  emit<E extends keyof CallRoomEvents>(event: E, ...args: Parameters<CallRoomEvents[E]>): void {
    for (const handler of this.handlers[event]) {
      ;(handler as (...a: Parameters<CallRoomEvents[E]>) => void)(...args)
    }
  }
  async load(): Promise<void> {}
  async acquireMicrophone(): Promise<void> {}
  async connect(): Promise<void> {}
  async setMicrophoneMuted(): Promise<void> {}
  async disconnect(): Promise<void> {
    this.emit('disconnected')
  }
}

interface StubOptions {
  /** `GET /api/me` response. Defaults to `me()` (an active member). Only
   *  the custom-fields admin-empty-state test (SLICE_019.md §9.11) needs a
   *  different role. */
  meOverride?: MeResponse
  settledHangup?: CallView
  /** Per-attempt start responses (thrown if an Error); the last repeats. */
  starts?: Array<unknown>
  /** `GET /api/calls/{id}` responses in order; the last repeats. Defaults to `placing`. */
  gets?: CallView[]
  /** `POST /api/calls/{id}/outcome` response, or an Error to throw. */
  outcome?: CorrectOutcomeResponse | Error
  /** `GET /api/tags` — the Organization's tag index (SLICE_011e §9.9). */
  orgTags?: Tag[]
  /** `POST /api/tags` response, or an Error to throw. Defaults to a fresh, unused tag. */
  createTag?: (name: string) => { tag: Tag; created: boolean } | Error
  /** `PUT`/`DELETE /api/people/{id}/tags/{tag_id}` response, or an Error. Defaults to a
   *  target-state-idempotent toggle against a locally tracked applied set. */
  personTag?: (tagId: string, applying: boolean) => { tags: TagRef[]; changed: boolean } | Error
  /** `POST /api/people/{id}/notes` response, or an Error to throw. Defaults to a fresh
   *  note by Alice, `can_manage: true`, appended to the tracked history. */
  noteAdd?: (body: string) => AddNoteResponse | Error
  /** `PUT /api/people/{id}/notes/{note_id}` response, or an Error. Defaults to updating
   *  the tracked history entry in place (`changed: true`, `edited: true`). */
  noteEdit?: (noteId: string, body: string) => EditNoteResponse | Error
  /** `DELETE /api/people/{id}/notes/{note_id}` response, or an Error. Defaults to
   *  removing the entry from the tracked history (tombstones are invisible, §1 rule 3). */
  noteDelete?: (noteId: string) => DeleteNoteResponse | Error
  /** `GET /organization/members` — the assignee picker's source (docs/specs/
   *  SLICE_016.md §8). Defaults to Alice (active, the viewer) only. */
  members?: Member[]
  /** `POST /api/people/{id}/tasks` response, or an Error. Defaults to a fresh
   *  open task, `can_manage: true`, appended to the tracked open task list. */
  taskAdd?: (body: { title: string; kind: TaskKind; due_at: string | null; assignee_user_id: string | null }) =>
    CreateTaskResponse | Error
  /** `PUT /api/people/{id}/tasks/{task_id}` response, or an Error. Defaults to
   *  updating the tracked open task in place (`changed` by field comparison). */
  taskUpdate?: (
    taskId: string,
    body: { title: string; kind: TaskKind; due_at: string | null; assignee_user_id: string },
  ) => UpdateTaskResponse | Error
  /** `POST .../tasks/{task_id}/complete` response, or an Error. Defaults to
   *  moving the tracked task from open to a `task_completed` history entry. */
  taskComplete?: (taskId: string) => CompleteTaskResponse | Error
  /** `POST .../tasks/{task_id}/reopen` response, or an Error. Defaults to
   *  moving the tracked `task_completed` entry back to the open list. */
  taskReopen?: (taskId: string) => ReopenTaskResponse | Error
  /** `DELETE .../tasks/{task_id}` response, or an Error. Defaults to removing
   *  the task from the tracked open list (tombstones are invisible). */
  taskDelete?: (taskId: string) => DeleteTaskResponse | Error
  /** `GET /api/custom-fields` — the Organization's field index (SLICE_019.md
   *  §9.11). Defaults to no live fields. */
  customFieldDefinitions?: CustomField[]
  /** `PUT /api/people/{id}/custom-fields/{field_id}` response, or an Error.
   *  Defaults to writing the tracked value in place from the field's own
   *  definition (label/type/option_label resolved from
   *  `customFieldDefinitions`). */
  customFieldSet?: (
    fieldId: string,
    body: SetCustomFieldValueRequest,
  ) => PersonCustomFieldValueMutationResponse | Error
  /** `DELETE /api/people/{id}/custom-fields/{field_id}` response, or an
   *  Error. Defaults to removing the tracked value (`changed` by presence). */
  customFieldClear?: (fieldId: string) => PersonCustomFieldValueMutationResponse | Error
  /** `PUT /api/people/{id}/custom-fields/{field_id}`, allowing a deferred
   *  (manually-resolved) response — the concurrent-rows test (SLICE_019.md
   *  §9.11) needs two saves in flight at once, each settling independently. */
  customFieldSetAsync?: (
    fieldId: string,
    body: SetCustomFieldValueRequest,
  ) => Promise<PersonCustomFieldValueMutationResponse | Error>
}

function taskFixture(overrides: Partial<Task> = {}): Task {
  return {
    id: 'task-1',
    person_id: PERSON_ID,
    title: 'A task',
    kind: 'follow_up',
    due_at: null,
    assignee: { id: 'u-alice', display_name: 'Alice' },
    created_by: { id: 'u-alice', display_name: 'Alice' },
    completed_at: null,
    completed_by: null,
    created_at: '2026-08-22T09:00:00.000Z',
    updated_at: '2026-08-22T09:00:00.000Z',
    can_manage: true,
    ...overrides,
  }
}

function taskCompletedEntry(overrides: {
  id?: string
  title?: string
  kind?: TaskKind
  dueAt?: string | null
  assignee?: ActorRef | null
  createdBy?: ActorRef | null
  canManage?: boolean
  actor?: ActorRef | null
  occurredAt?: string
} = {}): HistoryEntry {
  const {
    id = 'task-1',
    title = 'A task',
    kind = 'follow_up',
    dueAt = null,
    assignee = { id: 'u-alice', display_name: 'Alice' },
    createdBy = { id: 'u-alice', display_name: 'Alice' },
    canManage = true,
    actor = { id: 'u-alice', display_name: 'Alice' },
    occurredAt = '2026-08-22T09:30:00.000Z',
  } = overrides
  return {
    kind: 'task_completed',
    id,
    occurred_at: occurredAt,
    recorded_at: occurredAt,
    actor,
    origin: 'web_session',
    correlation_id: 'corr-task',
    detail: { title, kind, due_at: dueAt, assignee, created_by: createdBy, can_manage: canManage },
  }
}

function stubApi(personDetail: PersonDetailResponse, options: StubOptions = {}) {
  const settledHangup = options.settledHangup ?? callView({ status: 'failed', failure_reason: 'cancelled', ringing_at: 'x' })
  const starts = [...(options.starts ?? [])]
  const gets = [...(options.gets ?? [])]
  const appliedTags = new Map(personDetail.tags.map((tag) => [tag.id, tag]))
  const orgTags = new Map((options.orgTags ?? []).map((tag) => [tag.id, tag]))
  // Mutable so a note add/edit/delete is reflected on the NEXT GET — the
  // detail refetch every mutation settles into (§5, §9.10).
  let currentHistory = [...personDetail.history]
  let noteSeq = 0
  // Mutable the same way for task add/update/complete/reopen/delete (§8).
  let currentTasks = [...personDetail.tasks]
  let taskSeq = 0
  // Mutable the same way for a custom-field value set/clear (§9.11).
  let currentCustomFields = [...personDetail.custom_fields]
  const members: Member[] = options.members ?? [
    {
      user_id: 'u-alice',
      display_name: 'Alice',
      email: 'alice@acme.test',
      role: 'member',
      status: 'active',
      joined_at: '2026-01-01T00:00:00.000Z',
      assigned_people_count: 0,
    },
  ]
  apiFetchMock.mockImplementation(async (path: string, init?: RequestInit) => {
    const method = init?.method ?? 'GET'
    if (path === '/me') return options.meOverride ?? me()
    if (path.endsWith('/import-provenance')) throw new ApiError(404, 'not_found')
    const personTagMatch = /^\/people\/([^/]+)\/tags\/([^/]+)$/.exec(path)
    if (personTagMatch && (method === 'PUT' || method === 'DELETE')) {
      const tagId = decodeURIComponent(personTagMatch[2])
      const applying = method === 'PUT'
      const result = options.personTag?.(tagId, applying)
      if (result instanceof Error) {
        // A 404 here means the tag genuinely no longer exists (not merely
        // that this one write lost a race) — a subsequent GET must not
        // keep showing it, matching real backend behavior.
        if (result instanceof ApiError && result.status === 404) appliedTags.delete(tagId)
        throw result
      }
      if (result) return result
      const already = appliedTags.has(tagId)
      const changed = applying ? !already : already
      if (applying) {
        const tag = orgTags.get(tagId)
        if (tag) appliedTags.set(tagId, { id: tag.id, name: tag.name })
      } else {
        appliedTags.delete(tagId)
      }
      return {
        tags: [...appliedTags.values()].sort((a, b) => a.name.localeCompare(b.name)),
        changed,
      }
    }
    const noteCollectionMatch = /^\/people\/([^/]+)\/notes$/.exec(path)
    if (noteCollectionMatch && method === 'POST') {
      const body = JSON.parse(String(init?.body ?? '{}')) as { body: string }
      const result = options.noteAdd?.(body.body)
      if (result instanceof Error) throw result
      if (result) {
        currentHistory = [...currentHistory, noteEntry({ id: result.note.id, body: result.note.body, edited: result.note.edited, canManage: result.note.can_manage, actor: result.note.author })]
        return result
      }
      noteSeq += 1
      const id = `note-added-${noteSeq}`
      const note: AddNoteResponse['note'] = {
        id,
        person_id: decodeURIComponent(noteCollectionMatch[1]),
        body: body.body,
        author: { id: 'u-alice', display_name: 'Alice' },
        created_at: '2026-08-22T11:00:00.000Z',
        updated_at: '2026-08-22T11:00:00.000Z',
        edited: false,
        can_manage: true,
      }
      currentHistory = [...currentHistory, noteEntry({ id, body: note.body, occurredAt: note.created_at })]
      return { note }
    }
    const noteItemMatch = /^\/people\/([^/]+)\/notes\/([^/]+)$/.exec(path)
    if (noteItemMatch && (method === 'PUT' || method === 'DELETE')) {
      const noteId = decodeURIComponent(noteItemMatch[2])
      if (method === 'PUT') {
        const body = JSON.parse(String(init?.body ?? '{}')) as { body: string }
        const result = options.noteEdit?.(noteId, body.body)
        if (result instanceof Error) {
          // A 404 here means the note genuinely no longer exists (deleted
          // elsewhere) — a subsequent refetch must not keep showing it,
          // matching real backend behavior (the tags precedent above).
          if (result instanceof ApiError && result.status === 404) {
            currentHistory = currentHistory.filter((entry) => !(entry.kind === 'note' && entry.id === noteId))
          }
          throw result
        }
        if (result) {
          currentHistory = currentHistory.map((entry) =>
            entry.kind === 'note' && entry.id === noteId
              ? noteEntry({ id: noteId, body: result.note.body, edited: result.note.edited, canManage: result.note.can_manage, actor: result.note.author, occurredAt: entry.occurred_at })
              : entry,
          )
          return result
        }
        let updated: HistoryEntry | undefined
        currentHistory = currentHistory.map((entry) => {
          if (entry.kind !== 'note' || entry.id !== noteId) return entry
          updated = noteEntry({ id: noteId, body: body.body, edited: true, canManage: entry.detail.can_manage, actor: entry.actor, occurredAt: entry.occurred_at })
          return updated
        })
        if (!updated || updated.kind !== 'note') throw new ApiError(404, 'not_found')
        return { note: { id: noteId, person_id: PERSON_ID, body: updated.detail.body, author: updated.actor, created_at: updated.occurred_at, updated_at: updated.detail.updated_at, edited: updated.detail.edited, can_manage: updated.detail.can_manage }, changed: true }
      }
      const result = options.noteDelete?.(noteId)
      if (result instanceof Error) throw result
      const existed = currentHistory.some((entry) => entry.kind === 'note' && entry.id === noteId)
      currentHistory = currentHistory.filter((entry) => !(entry.kind === 'note' && entry.id === noteId))
      if (result) return result
      if (!existed) throw new ApiError(404, 'not_found')
      return { deleted: true }
    }
    const taskCollectionMatch = /^\/people\/([^/]+)\/tasks$/.exec(path)
    if (taskCollectionMatch && method === 'POST') {
      const body = JSON.parse(String(init?.body ?? '{}')) as {
        title: string
        kind?: TaskKind
        due_at?: string | null
        assignee_user_id?: string | null
      }
      const kind = body.kind ?? 'follow_up'
      const dueAt = body.due_at ?? null
      const assigneeUserId = body.assignee_user_id ?? null
      const result = options.taskAdd?.({ title: body.title, kind, due_at: dueAt, assignee_user_id: assigneeUserId })
      if (result instanceof Error) throw result
      if (result) {
        currentTasks = [...currentTasks, result.task]
        return result
      }
      taskSeq += 1
      const assigneeMember = assigneeUserId ? members.find((m) => m.user_id === assigneeUserId) : undefined
      const task = taskFixture({
        id: `task-added-${taskSeq}`,
        title: body.title,
        kind,
        due_at: dueAt,
        assignee: assigneeUserId ? { id: assigneeUserId, display_name: assigneeMember?.display_name ?? 'Alice' } : null,
        created_by: { id: 'u-alice', display_name: 'Alice' },
        created_at: '2026-08-22T11:00:00.000Z',
        updated_at: '2026-08-22T11:00:00.000Z',
        can_manage: true,
      })
      currentTasks = [...currentTasks, task]
      return { task }
    }
    const taskItemMatch = /^\/people\/([^/]+)\/tasks\/([^/]+)$/.exec(path)
    if (taskItemMatch && (method === 'PUT' || method === 'DELETE')) {
      const taskId = decodeURIComponent(taskItemMatch[2])
      if (method === 'PUT') {
        const body = JSON.parse(String(init?.body ?? '{}')) as {
          title: string
          kind: TaskKind
          due_at: string | null
          assignee_user_id: string
        }
        const result = options.taskUpdate?.(taskId, body)
        if (result instanceof Error) {
          // A 404 here means the task genuinely no longer exists (deleted
          // elsewhere) — a subsequent refetch must not keep showing it,
          // matching real backend behavior (the note precedent above).
          if (result instanceof ApiError && result.status === 404) {
            currentTasks = currentTasks.filter((t) => t.id !== taskId)
          }
          throw result
        }
        if (result) {
          currentTasks = currentTasks.map((t) => (t.id === taskId ? result.task : t))
          return result
        }
        const existing = currentTasks.find((t) => t.id === taskId)
        if (!existing) throw new ApiError(404, 'not_found')
        const assigneeMember = members.find((m) => m.user_id === body.assignee_user_id)
        const newAssignee = assigneeMember
          ? { id: assigneeMember.user_id, display_name: assigneeMember.display_name }
          : existing.assignee
        const changed =
          existing.title !== body.title ||
          existing.kind !== body.kind ||
          existing.due_at !== body.due_at ||
          (existing.assignee?.id ?? '') !== body.assignee_user_id
        const updated: Task = changed
          ? { ...existing, title: body.title, kind: body.kind, due_at: body.due_at, assignee: newAssignee, updated_at: '2026-08-22T12:00:00.000Z' }
          : existing
        currentTasks = currentTasks.map((t) => (t.id === taskId ? updated : t))
        return { task: updated, changed }
      }
      const result = options.taskDelete?.(taskId)
      if (result instanceof Error) throw result
      const existed = currentTasks.some((t) => t.id === taskId)
      currentTasks = currentTasks.filter((t) => t.id !== taskId)
      if (result) return result
      if (!existed) throw new ApiError(404, 'not_found')
      return { deleted: true }
    }
    const taskCompleteMatch = /^\/people\/([^/]+)\/tasks\/([^/]+)\/complete$/.exec(path)
    if (taskCompleteMatch && method === 'POST') {
      const taskId = decodeURIComponent(taskCompleteMatch[2])
      const result = options.taskComplete?.(taskId)
      if (result instanceof Error) {
        // A 404 here means the task genuinely no longer exists (deleted
        // elsewhere) — a subsequent refetch must not keep showing it.
        if (result instanceof ApiError && result.status === 404) {
          currentTasks = currentTasks.filter((t) => t.id !== taskId)
        }
        throw result
      }
      if (result) {
        if (result.changed) currentTasks = currentTasks.filter((t) => t.id !== taskId)
        return result
      }
      const existing = currentTasks.find((t) => t.id === taskId)
      if (!existing) throw new ApiError(404, 'not_found')
      const completedAt = '2026-08-22T13:00:00.000Z'
      const completedBy = { id: 'u-alice', display_name: 'Alice' }
      currentTasks = currentTasks.filter((t) => t.id !== taskId)
      currentHistory = [
        ...currentHistory,
        taskCompletedEntry({
          id: taskId,
          title: existing.title,
          kind: existing.kind,
          dueAt: existing.due_at,
          assignee: existing.assignee,
          createdBy: existing.created_by,
          canManage: existing.can_manage,
          actor: completedBy,
          occurredAt: completedAt,
        }),
      ]
      return { task: { ...existing, completed_at: completedAt, completed_by: completedBy }, changed: true }
    }
    const taskReopenMatch = /^\/people\/([^/]+)\/tasks\/([^/]+)\/reopen$/.exec(path)
    if (taskReopenMatch && method === 'POST') {
      const taskId = decodeURIComponent(taskReopenMatch[2])
      const result = options.taskReopen?.(taskId)
      if (result instanceof Error) throw result
      if (result) {
        if (result.changed) {
          currentHistory = currentHistory.filter((entry) => !(entry.kind === 'task_completed' && entry.id === taskId))
          currentTasks = [...currentTasks, result.task]
        }
        return result
      }
      const entry = currentHistory.find((e) => e.kind === 'task_completed' && e.id === taskId)
      if (!entry || entry.kind !== 'task_completed') throw new ApiError(404, 'not_found')
      const reopened = taskFixture({
        id: taskId,
        title: entry.detail.title,
        kind: entry.detail.kind,
        due_at: entry.detail.due_at,
        assignee: entry.detail.assignee,
        created_by: entry.detail.created_by,
        can_manage: entry.detail.can_manage,
        completed_at: null,
        completed_by: null,
      })
      currentHistory = currentHistory.filter((e) => !(e.kind === 'task_completed' && e.id === taskId))
      currentTasks = [...currentTasks, reopened]
      return { task: reopened, changed: true }
    }
    if (path.startsWith('/people/') && !path.endsWith('/calls') && method === 'GET') {
      const id = path.slice('/people/'.length)
      const currentTags = [...appliedTags.values()].sort((a, b) => a.name.localeCompare(b.name))
      return id === PERSON_ID
        ? { ...personDetail, history: currentHistory, tags: currentTags, tasks: currentTasks, custom_fields: currentCustomFields }
        : {
            ...personDetail,
            person: { ...personDetail.person, id, display_name: 'Someone Else' },
            history: currentHistory,
            tags: currentTags,
            tasks: currentTasks,
            custom_fields: currentCustomFields,
          }
    }
    if (path === '/custom-fields' && method === 'GET') {
      return { fields: options.customFieldDefinitions ?? [] }
    }
    const customFieldValueMatch = /^\/people\/([^/]+)\/custom-fields\/([^/]+)$/.exec(path)
    if (customFieldValueMatch && method === 'PUT') {
      const fieldId = decodeURIComponent(customFieldValueMatch[2])
      const body = JSON.parse(String(init?.body ?? '{}')) as SetCustomFieldValueRequest
      const result = options.customFieldSetAsync
        ? await options.customFieldSetAsync(fieldId, body)
        : options.customFieldSet?.(fieldId, body)
      if (result instanceof Error) throw result
      if (result) {
        currentCustomFields = result.custom_fields
        return result
      }
      const definition = (options.customFieldDefinitions ?? []).find((f) => f.id === fieldId)
      const value = body.value
      const optionLabel = 'option_id' in value
        ? definition?.options.find((o) => o.id === value.option_id)?.label ?? null
        : null
      const updated: PersonCustomFieldValue = {
        field_id: fieldId,
        label: definition?.label ?? 'Field',
        field_type: definition?.field_type ?? 'text',
        value: body.value,
        option_label: optionLabel,
        updated_at: '2026-09-10T12:00:00.000Z',
      }
      currentCustomFields = [...currentCustomFields.filter((v) => v.field_id !== fieldId), updated]
      return { custom_fields: currentCustomFields, changed: true } satisfies PersonCustomFieldValueMutationResponse
    }
    if (customFieldValueMatch && method === 'DELETE') {
      const fieldId = decodeURIComponent(customFieldValueMatch[2])
      const result = options.customFieldClear?.(fieldId)
      if (result instanceof Error) throw result
      if (result) {
        currentCustomFields = result.custom_fields
        return result
      }
      const changed = currentCustomFields.some((v) => v.field_id === fieldId)
      currentCustomFields = currentCustomFields.filter((v) => v.field_id !== fieldId)
      return { custom_fields: currentCustomFields, changed } satisfies PersonCustomFieldValueMutationResponse
    }
    if (path === '/stages') return { stages: [{ id: 'stage-lead', name: 'Lead', position: 1 }] }
    if (path === '/organization/members') return { members }
    if (path === '/tags' && method === 'GET') return { tags: [...orgTags.values()] }
    if (path === '/tags' && method === 'POST') {
      const body = JSON.parse(String(init?.body ?? '{}')) as { name: string }
      const result = options.createTag?.(body.name)
      if (result instanceof Error) throw result
      if (result) return result
      const tag: Tag = { id: `tag-${body.name.toLowerCase()}`, name: body.name, person_count: 0, can_manage: true }
      orgTags.set(tag.id, tag)
      return { tag, created: true }
    }
    if (path === `/people/${PERSON_ID}/calls`) {
      const next = starts.length > 1 ? starts.shift() : starts[0]
      if (next instanceof Error) throw next
      return next ?? { call: callView(), join: { url: 'wss://livekit.test', token: 't', room: `call:${CALL_ID}` } }
    }
    if (path === `/calls/${PREVIOUS_CALL_ID}/hangup`) {
      return { call: callView({ id: PREVIOUS_CALL_ID, status: 'ended', end_reason: 'agent_hangup' }) }
    }
    if (path === `/calls/${CALL_ID}/dial`) return { call: callView() }
    if (path === `/calls/${CALL_ID}/hangup`) return { call: settledHangup }
    if (path === `/calls/${CALL_ID}/outcome`) {
      if (options.outcome instanceof Error) throw options.outcome
      return options.outcome ?? correction(true)
    }
    if (path === `/calls/${CALL_ID}`) {
      const next = gets.length > 1 ? gets.shift() : gets[0]
      return { call: next ?? callView() }
    }
    throw new Error(`unexpected ${method} ${path}`)
  })
}

async function mountView(query = '', seed?: PersonDetailResponse) {
  const rooms: FakeRoom[] = []
  const createRoom: CallRoomFactory = () => {
    const room = new FakeRoom()
    rooms.push(room)
    return room
  }
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/people', component: { template: '<div />' } },
      { path: '/people/:id', component: { template: '<div />' } },
    ],
  })
  await router.push(`/people/${PERSON_ID}${query}`)
  await router.isReady()
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  })
  // A stale cached detail (a prior visit): rendered at once, refetched on mount.
  if (seed) queryClient.setQueryData(queryKeys.person(ORG_ID, PERSON_ID), seed)
  // SLICE_006b §6: the call host lives above the view (AppShell in prod);
  // the harness mirrors that — the view plus the one docked panel.
  // eslint-disable-next-line vue/one-component-per-file -- test harness
  const Harness = defineComponent({
    components: { PersonDetailView, CallHostPanel },
    props: { id: { type: String, required: true } },
    setup() {
      provideCallHost({ orgId: () => ORG_ID, createRoom })
    },
    template: '<PersonDetailView :id="id" /><CallHostPanel />',
  })
  const wrapper = mount(Harness, {
    props: { id: PERSON_ID },
    global: { plugins: [router, [VueQueryPlugin, { queryClient }], [PrimeVue, { unstyled: true }]] },
    attachTo: document.body,
  })
  await flushPromises()
  return { wrapper, rooms, queryClient, router }
}

function requests(): string[] {
  return apiFetchMock.mock.calls.map(([path, init]) => `${init?.method ?? 'GET'} ${path}`)
}

beforeEach(() => {
  apiFetchMock.mockReset()
})

afterEach(() => {
  document.body.innerHTML = ''
})

describe('PersonDetailView — Call button', () => {
  it('is disabled with "No phone number" when the Person has no phone', async () => {
    stubApi(detail([EMAIL]))
    const { wrapper } = await mountView()
    const button = wrapper.get('[data-testid="call-button"]')
    expect(button.attributes('disabled')).toBeDefined()
    expect(button.attributes('title')).toBe('No phone number')
    expect(wrapper.get('[data-testid="call-no-phone"]').text()).toBe('No phone number')
    await button.trigger('click')
    expect(requests().some((r) => r.endsWith('/calls'))).toBe(false)
    expect(wrapper.find('[data-testid="call-panel"]').exists()).toBe(false)
  })

  it('is the one primary while Log contact is secondary', async () => {
    stubApi(detail([PHONE_A]))
    const { wrapper } = await mountView()
    expect(wrapper.get('[data-testid="call-button"]').classes()).toContain('bg-accent')
    expect(wrapper.get('[data-testid="log-contact"]').classes()).not.toContain('bg-accent')
    // The note composer's always-visible "Add note" button and the task
    // Add form's always-visible "Add task" button (docs/specs/SLICE_015.md
    // §5, docs/specs/SLICE_016.md §8) are also primary-styled and
    // independent of the call/outcome state this assertion is actually
    // about, so both are excluded from the count rather than folded into
    // the call header's own one-primary invariant.
    expect(
      wrapper.findAll('.bg-accent').filter((el) => !['note-composer-submit', 'task-add-submit'].includes(el.attributes('data-testid') ?? '')),
    ).toHaveLength(1)
  })

  it('with one phone, starts the call directly — no picker, only contact_method_id on the wire', async () => {
    stubApi(detail([PHONE_A, EMAIL]))
    const { wrapper, rooms } = await mountView()
    await wrapper.get('[data-testid="call-button"]').trigger('click')
    await flushPromises()
    expect(wrapper.find('[data-testid="call-number-picker"]').exists()).toBe(false)
    const start = apiFetchMock.mock.calls.find(([path]) => path === `/people/${PERSON_ID}/calls`)
    expect(start).toBeDefined()
    expect(JSON.parse(String(start?.[1]?.body))).toEqual({ contact_method_id: PHONE_A.id })
    expect(JSON.stringify(apiFetchMock.mock.calls)).not.toContain(PHONE_A.value)
    expect(rooms).toHaveLength(1)
  })

  it('with two phones, shows the number picker and calls the chosen one', async () => {
    stubApi(detail([PHONE_A, PHONE_B]))
    const { wrapper } = await mountView()
    const button = wrapper.get('[data-testid="call-button"]')
    expect(button.attributes('aria-haspopup')).toBe('menu')
    await button.trigger('click')
    expect(requests().some((r) => r.endsWith('/calls'))).toBe(false)
    const picker = wrapper.get('[data-testid="call-number-picker"]')
    const items = picker.findAll('[role="menuitem"]')
    expect(items.map((i) => i.text())).toEqual([PHONE_A.value, PHONE_B.value])
    await items[1].trigger('click')
    await flushPromises()
    expect(wrapper.find('[data-testid="call-number-picker"]').exists()).toBe(false)
    const start = apiFetchMock.mock.calls.find(([path]) => path === `/people/${PERSON_ID}/calls`)
    expect(JSON.parse(String(start?.[1]?.body))).toEqual({ contact_method_id: PHONE_B.id })
  })

  it('walks Connecting… → Ringing… → Hang up → the no-answer line, with Hang up the only primary meanwhile', async () => {
    stubApi(detail([PHONE_A]))
    const { wrapper, rooms } = await mountView()
    await wrapper.get('[data-testid="call-button"]').trigger('click')
    await flushPromises()

    const panel = wrapper.get('[data-testid="call-panel"]')
    expect(panel.text()).toContain('Grace Hopper')
    expect(wrapper.get('[data-testid="call-status"]').text()).toBe('Connecting…')
    expect(wrapper.get('[data-testid="call-button"]').attributes('disabled')).toBeDefined()
    // The note composer's always-visible "Add note" button and the task
    // Add form's always-visible "Add task" button (docs/specs/SLICE_015.md
    // §5, docs/specs/SLICE_016.md §8) are also primary-styled and
    // independent of the call/outcome state this assertion is actually
    // about, so both are excluded from the count rather than folded into
    // the call header's own one-primary invariant.
    expect(
      wrapper.findAll('.bg-accent').filter((el) => !['note-composer-submit', 'task-add-submit'].includes(el.attributes('data-testid') ?? '')),
    ).toHaveLength(1)
    expect(wrapper.get('[data-testid="call-hangup"]').classes()).toContain('bg-accent')

    rooms[0].emit('participantConnected', { identity: `sip:${CALL_ID}`, attributes: {} })
    await flushPromises()
    expect(wrapper.get('[data-testid="call-status"]').text()).toBe('Ringing…')

    await wrapper.get('[data-testid="call-hangup"]').trigger('click')
    await flushPromises()
    expect(requests().filter((r) => r === `POST /calls/${CALL_ID}/hangup`)).toHaveLength(1)
    expect(wrapper.get('[data-testid="call-status"]').text()).toBe('No answer')
    // SLICE_006c: the prompt replaces the logged line; Save outcome is the
    // one primary, so the Call button (enabled again) is secondary.
    expect(wrapper.get('[data-testid="call-outcome-prompt"]').text()).toBe('How did it go?')
    expect(wrapper.get('[data-testid="call-button"]').attributes('disabled')).toBeUndefined()
    // The note composer's always-visible "Add note" button and the task
    // Add form's always-visible "Add task" button (docs/specs/SLICE_015.md
    // §5, docs/specs/SLICE_016.md §8) are also primary-styled and
    // independent of the call/outcome state this assertion is actually
    // about, so both are excluded from the count rather than folded into
    // the call header's own one-primary invariant.
    expect(
      wrapper.findAll('.bg-accent').filter((el) => !['note-composer-submit', 'task-add-submit'].includes(el.attributes('data-testid') ?? '')),
    ).toHaveLength(1)
    expect(wrapper.get('[data-testid="call-outcome-save"]').classes()).toContain('bg-accent')

    // D-033: forced choice — nothing selected, no Skip, Save disabled until a pick.
    expect(wrapper.find('[data-testid="call-outcome-skip"]').exists()).toBe(false)
    expect(wrapper.find('[data-testid="call-dismiss"]').exists()).toBe(false)
    expect(wrapper.find('[data-testid="outcome-picker"] [aria-checked="true"]').exists()).toBe(false)
    expect(wrapper.get('[data-testid="call-outcome-save"]').attributes('disabled')).toBeDefined()
    await wrapper.get('[data-outcome="no_answer"]').trigger('click')
    expect(wrapper.get('[data-testid="call-outcome-save"]').attributes('disabled')).toBeUndefined()
    await wrapper.get('[data-testid="call-outcome-save"]').trigger('click')
    await flushPromises()
    expect(requests().filter((r) => r === `POST /calls/${CALL_ID}/outcome`)).toHaveLength(1)
    expect(wrapper.get('[data-testid="call-outcome-saved"]').text()).toBe('Outcome saved — no answer')
    await wrapper.get('[data-testid="call-dismiss"]').trigger('click')
    expect(wrapper.find('[data-testid="call-panel"]').exists()).toBe(false)
    // The Call button is the primary again.
    expect(wrapper.get('[data-testid="call-button"]').classes()).toContain('bg-accent')
  })
})

describe('PersonDetailView — number picker dismissal', () => {
  it('closes on Escape and on an outside click, stays open on an inside click', async () => {
    stubApi(detail([PHONE_A, PHONE_B]))
    const { wrapper } = await mountView()
    const button = wrapper.get('[data-testid="call-button"]')
    await button.trigger('click')
    expect(wrapper.find('[data-testid="call-number-picker"]').exists()).toBe(true)
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
    await flushPromises()
    expect(wrapper.find('[data-testid="call-number-picker"]').exists()).toBe(false)

    await button.trigger('click')
    expect(wrapper.find('[data-testid="call-number-picker"]').exists()).toBe(true)
    wrapper.get('[data-testid="call-number-picker"]').element.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await flushPromises()
    expect(wrapper.find('[data-testid="call-number-picker"]').exists()).toBe(true)
    document.body.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await flushPromises()
    expect(wrapper.find('[data-testid="call-number-picker"]').exists()).toBe(false)
    expect(requests().some((r) => r.endsWith('/calls'))).toBe(false)
  })
})

describe('PersonDetailView — call in progress (409) and mid-call navigation', () => {
  it('409 → Hang up previous call → Call again → 201', async () => {
    stubApi(detail([PHONE_A]), {
      starts: [new ApiError(409, 'call_in_progress', { call_id: PREVIOUS_CALL_ID }), undefined],
    })
    const { wrapper } = await mountView()
    await wrapper.get('[data-testid="call-button"]').trigger('click')
    await flushPromises()
    expect(wrapper.get('[data-testid="call-error"]').text()).toBe('You already have a call in progress.')
    await wrapper.get('[data-testid="call-hangup-previous"]').trigger('click')
    await flushPromises()
    expect(requests()).toContain(`POST /calls/${PREVIOUS_CALL_ID}/hangup`)
    expect(wrapper.find('[data-testid="call-panel"]').exists()).toBe(false)
    await wrapper.get('[data-testid="call-button"]').trigger('click')
    await flushPromises()
    expect(wrapper.get('[data-testid="call-status"]').text()).toBe('Connecting…')
    expect(requests().filter((r) => r === `POST /people/${PERSON_ID}/calls`)).toHaveLength(2)
    expect(requests()).toContain(`POST /calls/${CALL_ID}/dial`)
  })

  it('keeps the original callee name in the panel when the route param changes mid-call', async () => {
    stubApi(detail([PHONE_A]))
    const { wrapper } = await mountView()
    await wrapper.get('[data-testid="call-button"]').trigger('click')
    await flushPromises()
    expect(wrapper.get('[data-testid="call-panel"]').text()).toContain('Grace Hopper')
    await wrapper.setProps({ id: '77777777-7777-7777-7777-777777777777' })
    await flushPromises()
    expect(wrapper.get('h1').text()).toBe('Someone Else')
    expect(wrapper.get('[data-testid="call-panel"]').text()).toContain('Grace Hopper')
    expect(wrapper.get('[data-testid="call-panel"]').text()).not.toContain('Someone Else')
    expect(wrapper.get('[data-testid="call-status"]').text()).toBe('Connecting…')
  })
})

describe('PersonDetailView — the call survives navigation (SLICE_006b §6)', () => {
  async function mountThroughRouter() {
    const rooms: FakeRoom[] = []
    const createRoom: CallRoomFactory = () => {
      const room = new FakeRoom()
      rooms.push(room)
      return room
    }
    const router = createRouter({
      history: createMemoryHistory(),
      routes: [
        { path: '/people', component: { template: '<div data-testid="people" />' } },
        {
          path: '/people/:id',
          component: PersonDetailView,
          props: (route) => ({ id: route.params.id }),
        },
      ],
    })
    await router.push(`/people/${PERSON_ID}`)
    await router.isReady()
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
    // AppShell's shape: host + RouterView + the docked panel outside it.
    // eslint-disable-next-line vue/one-component-per-file -- test harness
    const Shell = defineComponent({
      components: { CallHostPanel },
      setup() {
        provideCallHost({ orgId: () => ORG_ID, createRoom })
      },
      template: '<RouterView /><CallHostPanel />',
    })
    const wrapper = mount(Shell, {
      global: { plugins: [router, [VueQueryPlugin, { queryClient }], [PrimeVue, { unstyled: true }]] },
      attachTo: document.body,
    })
    await flushPromises()
    return { wrapper, router, rooms }
  }

  it('keeps the call and panel alive across navigation — no prompt, no hangup', async () => {
    stubApi(detail([PHONE_A]))
    const { wrapper, router } = await mountThroughRouter()
    await wrapper.get('[data-testid="call-button"]').trigger('click')
    await flushPromises()
    const confirm = vi.fn(() => false)
    vi.stubGlobal('confirm', confirm)
    await router.push('/people')
    await flushPromises()
    // The old route-leave guard is gone: navigation succeeds, nothing asks,
    // the call is not hung up, and the docked panel is still there.
    expect(confirm).not.toHaveBeenCalled()
    expect(router.currentRoute.value.path).toBe('/people')
    expect(requests().filter((r) => r === `POST /calls/${CALL_ID}/hangup`)).toHaveLength(0)
    expect(wrapper.find('[data-testid="call-panel"]').exists()).toBe(true)
    vi.unstubAllGlobals()
  })

  it('unmounting the whole shell (tab close) still hangs up once', async () => {
    stubApi(detail([PHONE_A]))
    const { wrapper } = await mountThroughRouter()
    await wrapper.get('[data-testid="call-button"]').trigger('click')
    await flushPromises()
    wrapper.unmount()
    await flushPromises()
    expect(requests().filter((r) => r === `POST /calls/${CALL_ID}/hangup`)).toHaveLength(1)
  })
})

describe('PersonDetailView — call_completed history', () => {
  function entry(outcome: CallCompletedOutcome, talkSeconds: number | null): HistoryEntry {
    return {
      kind: 'call_completed',
      id: `h-${outcome}`,
      occurred_at: '2026-08-22T10:01:12.000Z',
      recorded_at: '2026-08-22T10:01:12.000Z',
      actor: { id: 'u-alice', display_name: 'Alice' },
      origin: 'web_session',
      correlation_id: 'corr',
      detail: { call_id: CALL_ID, outcome, talk_seconds: talkSeconds, answered_at: null },
    }
  }

  it('renders "Call — reached, 1 min 12 s" and "Call — no answer"', async () => {
    stubApi(detail([PHONE_A], [entry('reached', 72), entry('no_answer', null)]))
    const { wrapper } = await mountView()
    const text = wrapper.text()
    expect(text).toContain('Call — reached, 1 min 12 s')
    expect(text).toContain('Call — no answer')
    expect(text).toContain('Alice')
  })
})

// ---- SLICE_009: correspondence history --------------------------------

describe('PersonDetailView — correspondence history (SLICE_009 §8)', () => {
  function correspondenceEntry(
    id: string,
    direction: 'inbound' | 'outbound',
    agentName: string,
    backdated: boolean,
  ): HistoryEntry {
    return {
      kind: 'correspondence',
      id,
      occurred_at: '2026-08-27T09:00:00.000Z',
      recorded_at: '2026-08-27T09:00:05.000Z',
      // Actor::System — no human actor caused an unattended capture
      // (spec §4); the agent lives in detail.agent instead.
      actor: null,
      origin: 'webhook',
      correlation_id: 'corr',
      detail: {
        direction,
        agent: { id: 'u-alice', display_name: agentName },
        captured_at: '2026-08-27T09:00:05.000Z',
        via: 'cc',
        backdated,
      },
    }
  }

  it('renders "Outbound email — Alice" and "Inbound email — Bob"', async () => {
    stubApi(
      detail(
        [EMAIL],
        [
          correspondenceEntry('h-corr-out', 'outbound', 'Alice', false),
          correspondenceEntry('h-corr-in', 'inbound', 'Bob', false),
        ],
      ),
    )
    const { wrapper } = await mountView()
    const text = wrapper.text()
    expect(text).toContain('Outbound email — Alice')
    expect(text).toContain('Inbound email — Bob')
  })

  it('a backdated (retroactively forwarded) row is labelled "(forwarded)"', async () => {
    stubApi(detail([EMAIL], [correspondenceEntry('h-corr-bd', 'inbound', 'Alice', true)]))
    const { wrapper } = await mountView()
    expect(wrapper.text()).toContain('Inbound email — Alice (forwarded)')
  })

  it('never renders an address, subject, or message-id (D-042.1/2 shape pin)', async () => {
    // Phone-only contact methods: the Person's OWN contact card
    // legitimately renders an email address when one exists (EMAIL
    // fixture) — using PHONE_A here isolates the assertion to content the
    // correspondence history row itself could have leaked.
    stubApi(detail([PHONE_A], [correspondenceEntry('h-corr-shape', 'outbound', 'Alice', false)]))
    const { wrapper } = await mountView()
    const html = wrapper.html()
    expect(html).not.toContain('@example.com')
    expect(html).not.toContain('message_id')
    expect(html).not.toContain('subject')
  })
})

// ---- SLICE_007c: system-actor routing labels --------------------------------

describe('PersonDetailView — system-actor routing history (SLICE_007c §6)', () => {
  function routingEntry(
    strategy: RoutingStrategy,
    assignee: { id: string; display_name: string } | null,
  ): HistoryEntry {
    return {
      kind: 'routing_decision',
      id: `h-routing-${strategy}`,
      occurred_at: '2026-08-24T10:00:00.000Z',
      recorded_at: '2026-08-24T10:00:00.000Z',
      // A system-actor fact carries no user actor (docs/specs/
      // SLICE_007c.md §4) — the template's existing `?? 'System'` fallback
      // renders it, unchanged by this slice.
      actor: null,
      origin: 'cli',
      correlation_id: 'corr',
      detail: { inquiry_id: 'inq-1', strategy, assignee },
    }
  }

  it('organization_default with an assignee: "Routed to Bob (the organization default)", actor System', async () => {
    stubApi(
      detail([EMAIL], [routingEntry('organization_default', { id: 'u-bob', display_name: 'Bob' })]),
    )
    const { wrapper } = await mountView()
    const text = wrapper.text()
    expect(text).toContain('Routed to Bob (the organization default)')
    expect(text).toContain('System')
  })

  it('unassigned with no assignee: "Routing decided (no default assignee) — left unassigned"', async () => {
    stubApi(detail([EMAIL], [routingEntry('unassigned', null)]))
    const { wrapper } = await mountView()
    expect(wrapper.text()).toContain('Routing decided (no default assignee) — left unassigned')
  })
})

// ---- SLICE_006c ------------------------------------------------------------

const ATTEMPT_ID = '88888888-8888-8888-8888-888888888888'

function attemptEntry(
  id: string,
  detail: Partial<ContactAttemptedDetail>,
  actor: { id: string; display_name: string } | null = { id: 'u-alice', display_name: 'Alice' },
): HistoryEntry {
  return {
    kind: 'contact_attempted',
    id,
    occurred_at: '2026-08-22T10:00:00.000Z',
    recorded_at: '2026-08-22T10:00:00.000Z',
    actor,
    origin: 'web_session',
    correlation_id: 'corr',
    detail: { channel: 'call', outcome: 'reached', call_id: CALL_ID, corrects_id: null, superseded: false, ...detail },
  }
}

/** Call → SIP leg answered → remote hangup. The hangup response leaves the
 * server at `answered` (the hangup request races the webhook), so the
 * panel's Save is gated until a `GET` returns `ended`. */
async function answeredCallEnded(options: StubOptions = {}) {
  stubApi(detail([PHONE_A]), {
    settledHangup: callView({ status: 'answered', answered_at: 'x' }),
    gets: [callView({ status: 'ended', end_reason: 'remote_hangup', answered_at: 'x', talk_seconds: 5 })],
    ...options,
  })
  const mounted = await mountView()
  await mounted.wrapper.get('[data-testid="call-button"]').trigger('click')
  await flushPromises()
  mounted.rooms[0].emit('participantConnected', { identity: `sip:${CALL_ID}`, attributes: { 'sip.callStatus': 'active' } })
  await flushPromises()
  mounted.rooms[0].emit('participantDisconnected', { identity: `sip:${CALL_ID}`, attributes: {} })
  await flushPromises()
  return mounted
}

describe('PersonDetailView — How did it go? (SLICE_006c §10)', () => {
  it('Save waits for the server: disabled at answered, enabled after the call.changed refetch shows ended', async () => {
    const { wrapper, queryClient } = await answeredCallEnded()
    expect(wrapper.find('[data-testid="call-outcome-prompt"]').exists()).toBe(true)
    await wrapper.get('[data-outcome="reached"]').trigger('click')
    expect(wrapper.get('[data-testid="call-outcome-save"]').attributes('disabled')).toBeDefined()
    // D-023: `call.changed` is invalidation-only — the refetch carries the status.
    await queryClient.invalidateQueries({ queryKey: queryKeys.call(ORG_ID, CALL_ID) })
    await flushPromises()
    expect(wrapper.get('[data-testid="call-outcome-save"]').attributes('disabled')).toBeUndefined()
  })

  it('Save posts exactly {"outcome"} to /calls/{id}/outcome, shows the saved line, Done closes', async () => {
    const { wrapper, queryClient } = await answeredCallEnded()
    await queryClient.invalidateQueries({ queryKey: queryKeys.call(ORG_ID, CALL_ID) })
    await flushPromises()
    const invalidate = vi.spyOn(queryClient, 'invalidateQueries')
    await wrapper.get('[data-outcome="left_message"]').trigger('click')
    await wrapper.get('[data-testid="call-outcome-save"]').trigger('click')
    await flushPromises()
    const post = apiFetchMock.mock.calls.find(([path]) => path === `/calls/${CALL_ID}/outcome`)
    expect(post?.[1]?.method).toBe('POST')
    expect(JSON.parse(String(post?.[1]?.body))).toEqual({ outcome: 'left_message' })
    expect(wrapper.get('[data-testid="call-outcome-saved"]').text()).toBe('Outcome saved — voicemail')
    const keys = invalidate.mock.calls.map(([f]) => JSON.stringify((typeof f === 'function' ? f() : f)?.queryKey))
    expect(keys).toContain(JSON.stringify(queryKeys.person(ORG_ID, PERSON_ID)))
    expect(keys).toContain(JSON.stringify(queryKeys.today(ORG_ID)))
    await wrapper.get('[data-testid="call-dismiss"]').trigger('click')
    expect(wrapper.find('[data-testid="call-panel"]').exists()).toBe(false)
  })

  it('saving the outcome already recorded (changed: false) still shows the saved line → Done (never a silent dismissal)', async () => {
    const { wrapper, queryClient } = await answeredCallEnded({ outcome: correction(false) })
    await queryClient.invalidateQueries({ queryKey: queryKeys.call(ORG_ID, CALL_ID) })
    await flushPromises()
    await wrapper.get('[data-outcome="left_message"]').trigger('click')
    await wrapper.get('[data-testid="call-outcome-save"]').trigger('click')
    await flushPromises()
    expect(requests().filter((r) => r === `POST /calls/${CALL_ID}/outcome`)).toHaveLength(1)
    expect(wrapper.find('[data-testid="call-panel"]').exists()).toBe(true)
    expect(wrapper.get('[data-testid="call-outcome-saved"]').text()).toBe('Outcome saved — voicemail')
    await wrapper.get('[data-testid="call-dismiss"]').trigger('click')
    expect(wrapper.find('[data-testid="call-panel"]').exists()).toBe(false)
  })

  it('two synchronous Save clicks post once', async () => {
    const { wrapper, queryClient } = await answeredCallEnded()
    await queryClient.invalidateQueries({ queryKey: queryKeys.call(ORG_ID, CALL_ID) })
    await flushPromises()
    await wrapper.get('[data-outcome="busy"]').trigger('click')
    const save = wrapper.get('[data-testid="call-outcome-save"]').element as HTMLButtonElement
    save.click()
    save.click()
    await flushPromises()
    expect(requests().filter((r) => r === `POST /calls/${CALL_ID}/outcome`)).toHaveLength(1)
    expect(wrapper.get('[data-testid="call-outcome-saved"]').text()).toBe('Outcome saved — busy')
  })

  it.each([
    [new ApiError(409, 'invalid_call_state'), "The call hasn't finished yet."],
    [new ApiError(422, 'no_contact_attempt'), "There's no contact attempt to set an outcome on."],
    [new ApiError(409, 'correction_conflict'), 'This outcome was just changed — refreshed.'],
    [new ApiError(500, 'internal_error'), 'Could not save the outcome.'],
  ])('renders the §10 copy for %s and keeps the prompt open', async (failure, message) => {
    const { wrapper, queryClient } = await answeredCallEnded({ outcome: failure })
    await queryClient.invalidateQueries({ queryKey: queryKeys.call(ORG_ID, CALL_ID) })
    await flushPromises()
    const personGets = () => requests().filter((r) => r === `GET /people/${PERSON_ID}`).length
    const before = personGets()
    await wrapper.get('[data-outcome="busy"]').trigger('click')
    await wrapper.get('[data-testid="call-outcome-save"]').trigger('click')
    await flushPromises()
    expect(wrapper.get('[data-testid="call-outcome-error"]').text()).toBe(message)
    expect(wrapper.find('[data-testid="outcome-picker"]').exists()).toBe(true)
    expect(wrapper.get('[data-testid="call-outcome-save"]').attributes('disabled')).toBeUndefined()
    // correction_conflict refetches the Person (history) — the others do not.
    expect(personGets() > before).toBe(failure.code === 'correction_conflict')
  })

  it('forced choice: nothing pre-selected, no Skip or Done, Save disabled until a pick even once the server is terminal', async () => {
    const { wrapper, queryClient } = await answeredCallEnded()
    await queryClient.invalidateQueries({ queryKey: queryKeys.call(ORG_ID, CALL_ID) })
    await flushPromises()
    expect(wrapper.find('[data-testid="outcome-picker"] [aria-checked="true"]').exists()).toBe(false)
    expect(wrapper.find('[data-testid="call-outcome-skip"]').exists()).toBe(false)
    expect(wrapper.find('[data-testid="call-dismiss"]').exists()).toBe(false)
    expect(wrapper.get('[data-testid="call-panel"]').text()).not.toContain('Skip')
    expect(wrapper.get('[data-testid="call-outcome-save"]').attributes('disabled')).toBeDefined()
    expect(wrapper.find('[data-testid="call-outcome-finishing"]').exists()).toBe(false)
    await wrapper.get('[data-outcome="wrong_number"]').trigger('click')
    expect(wrapper.get('[data-testid="call-outcome-save"]').attributes('disabled')).toBeUndefined()
    expect(requests().some((r) => r.endsWith('/outcome'))).toBe(false)
    expect(wrapper.find('[data-testid="call-panel"]').exists()).toBe(true)
  })

  it('shows "Finishing up…" under a disabled Save and a follow-up GET enables it without a realtime event', async () => {
    const { wrapper } = await answeredCallEnded()
    await wrapper.get('[data-outcome="reached"]').trigger('click')
    expect(wrapper.get('[data-testid="call-outcome-finishing"]').text()).toBe('Finishing up…')
    const getCount = () => requests().filter((r) => r === `GET /calls/${CALL_ID}`).length
    expect(getCount()).toBe(0)
    await new Promise((resolve) => setTimeout(resolve, 1100))
    await flushPromises()
    expect(getCount()).toBeGreaterThanOrEqual(1)
    expect(wrapper.get('[data-testid="call-outcome-save"]').attributes('disabled')).toBeUndefined()
    expect(wrapper.find('[data-testid="call-outcome-finishing"]').exists()).toBe(false)
  })

  it('a call that never reached the callee gets Done, not the prompt', async () => {
    stubApi(detail([PHONE_A]), { settledHangup: callView({ status: 'failed', failure_reason: 'cancelled', ringing_at: null }) })
    const { wrapper } = await mountView()
    await wrapper.get('[data-testid="call-button"]').trigger('click')
    await flushPromises()
    await wrapper.get('[data-testid="call-hangup"]').trigger('click')
    await flushPromises()
    expect(wrapper.find('[data-testid="call-outcome-prompt"]').exists()).toBe(false)
    expect(wrapper.find('[data-testid="call-dismiss"]').exists()).toBe(true)
  })
})

function completedEntry(
  id: string,
  detail: Partial<CallCompletedDetail> = {},
  actor: { id: string; display_name: string } | null = { id: 'u-alice', display_name: 'Alice' },
): HistoryEntry {
  return {
    kind: 'call_completed',
    id,
    occurred_at: '2026-08-22T10:00:05.000Z',
    recorded_at: '2026-08-22T10:00:05.000Z',
    actor,
    origin: 'system',
    correlation_id: 'corr',
    detail: { call_id: CALL_ID, outcome: 'reached', talk_seconds: 7, answered_at: 'x', ...detail },
  }
}

/** The open outcome dialog's heading (PrimeVue teleports it to body). */
function dialogTitle(): string | undefined {
  return document.querySelector('[role="dialog"] h2')?.textContent?.trim()
}

/** The History rows as [summary, sub-line] pairs, in render order. */
function historyRows(wrapper: Awaited<ReturnType<typeof mountView>>['wrapper']): Array<[string, string]> {
  return wrapper
    .findAll('li')
    .filter((li) => li.find('p.text-body').exists())
    .map((li) => [li.get('p.text-body').text(), li.get('p.text-small').text().replace(/\s+/g, ' ')])
}

describe('PersonDetailView — one row per call (SLICE_006c §5a, D-033)', () => {
  it('an answered call with an agent choice is one row naming the choice and the duration — no note', async () => {
    // §2 order: automatic attempt → call_completed → the agent's choice;
    // the choice is not adjacent to its root.
    stubApi(
      detail(
        [PHONE_A],
        [
          attemptEntry(ATTEMPT_ID, { outcome: 'reached', superseded: true }),
          completedEntry('h-completed'),
          attemptEntry('att-2', { outcome: 'left_message', corrects_id: ATTEMPT_ID }),
        ],
      ),
    )
    const { wrapper } = await mountView()
    const rows = historyRows(wrapper)
    expect(rows).toEqual([['Call — voicemail, 7 s', expect.stringMatching(/^Alice · [^·]+$/)]])
    expect(wrapper.find('[data-testid="correction-note"]').exists()).toBe(false)
    expect(wrapper.text()).not.toContain('superseded')
    expect(wrapper.text()).not.toContain('corrected')
    expect(wrapper.text()).not.toContain('Contact attempted')
    expect(wrapper.text()).not.toContain('outcome needed')
    expect(wrapper.get('[data-testid="change-outcome"]').text()).toBe('Change outcome')
  })

  it('an answered call without a choice reads "Call — 7 s · outcome needed" and never the system\'s word', async () => {
    stubApi(detail([PHONE_A], [attemptEntry(ATTEMPT_ID, { outcome: 'reached' }), completedEntry('h-completed', { talk_seconds: 72 })]))
    const { wrapper } = await mountView()
    expect(historyRows(wrapper)).toEqual([['Call — 1 min 12 s · outcome needed', expect.stringMatching(/^Alice · [^·]+$/)]])
    expect(wrapper.text()).not.toContain('talked to them')
    expect(wrapper.text()).not.toContain('reached')
    expect(wrapper.get('[data-testid="change-outcome"]').text()).toBe('Set outcome')
  })

  it('a not-answered call without a choice reads "Call · outcome needed"', async () => {
    stubApi(
      detail(
        [PHONE_A],
        [
          attemptEntry(ATTEMPT_ID, { outcome: 'no_answer' }),
          completedEntry('h-completed', { outcome: 'ring_timeout', talk_seconds: null, answered_at: null }),
        ],
      ),
    )
    const { wrapper } = await mountView()
    expect(historyRows(wrapper).map(([summary]) => summary)).toEqual(['Call · outcome needed'])
    expect(wrapper.text()).not.toContain('no answer')
    expect(wrapper.get('[data-testid="change-outcome"]').text()).toBe('Set outcome')
  })

  it('a chain: the head of the chain is the outcome', async () => {
    stubApi(
      detail(
        [PHONE_A],
        [
          attemptEntry(ATTEMPT_ID, { outcome: 'reached', superseded: true }),
          completedEntry('h-completed'),
          attemptEntry('att-2', { outcome: 'left_message', corrects_id: ATTEMPT_ID, superseded: true }),
          attemptEntry('att-3', { outcome: 'wrong_number', corrects_id: 'att-2' }),
        ],
      ),
    )
    const { wrapper } = await mountView()
    const rows = historyRows(wrapper)
    expect(rows).toHaveLength(1)
    expect(rows[0][0]).toBe('Call — wrong number, 7 s')
    expect(rows[0][1]).not.toContain('talked to them')
    expect(wrapper.findAll('[data-testid="change-outcome"]')).toHaveLength(1)
  })

  it('two calls on one Person each get their own row with their own state', async () => {
    stubApi(
      detail(
        [PHONE_A],
        [
          attemptEntry('a1', { outcome: 'no_answer', call_id: 'call-a', superseded: true }),
          completedEntry('c-a', { call_id: 'call-a', outcome: 'busy', talk_seconds: null, answered_at: null }),
          attemptEntry('a1-fix', { outcome: 'busy', call_id: 'call-a', corrects_id: 'a1' }),
          attemptEntry('b1', { outcome: 'reached', call_id: 'call-b' }),
          completedEntry('c-b', { call_id: 'call-b', talk_seconds: 40 }),
        ],
      ),
    )
    const { wrapper } = await mountView()
    expect(historyRows(wrapper).map(([summary]) => summary)).toEqual(['Call — busy', 'Call — 40 s · outcome needed'])
    expect(wrapper.findAll('[data-testid="change-outcome"]').map((b) => b.text())).toEqual(['Change outcome', 'Set outcome'])
  })

  it('a failed call with no attempt keeps the plain call_completed line', async () => {
    stubApi(
      detail([PHONE_A], [completedEntry('h-failed', { outcome: 'provider_error', talk_seconds: null, answered_at: null })]),
    )
    const { wrapper } = await mountView()
    expect(historyRows(wrapper)).toEqual([['Call — failed', expect.stringMatching(/^Alice · [^·]+$/)]])
    expect(wrapper.find('[data-testid="change-outcome"]').exists()).toBe(false)
  })

  it('manual attempts render as before, alongside a call row', async () => {
    stubApi(
      detail(
        [PHONE_A],
        [
          attemptEntry('manual', { call_id: null, channel: 'text', outcome: 'sent' }),
          attemptEntry(ATTEMPT_ID, { outcome: 'no_answer', superseded: true }),
          completedEntry('h-completed', { outcome: 'no_answer', talk_seconds: null, answered_at: null }),
          attemptEntry('att-2', { outcome: 'no_answer', corrects_id: ATTEMPT_ID }),
        ],
      ),
    )
    const { wrapper } = await mountView()
    expect(historyRows(wrapper).map(([summary]) => summary)).toEqual(['Contact attempted — text, sent', 'Call — no answer'])
  })

  it('a call-derived attempt without a call_completed row falls back to its own row', async () => {
    stubApi(detail([PHONE_A], [attemptEntry(ATTEMPT_ID, { outcome: 'no_answer' })]))
    const { wrapper } = await mountView()
    expect(historyRows(wrapper).map(([summary]) => summary)).toEqual(['Contact attempted — call, no answer'])
  })
})

describe('PersonDetailView — Set / Change outcome (SLICE_006c §1 step 7, §5a)', () => {
  it('sits on the call row, only when the caller is me and an effective attempt exists', async () => {
    stubApi(
      detail(
        [PHONE_A],
        [
          attemptEntry('mine', { call_id: 'call-mine' }),
          completedEntry('c-mine', { call_id: 'call-mine' }),
          attemptEntry('manual', { call_id: null }),
          completedEntry('c-no-attempt', { call_id: 'call-no-attempt', outcome: 'expired', talk_seconds: null, answered_at: null }),
          attemptEntry('carols', { call_id: 'call-carol' }, { id: 'u-carol', display_name: 'Carol' }),
          completedEntry('c-carol', { call_id: 'call-carol' }, { id: 'u-carol', display_name: 'Carol' }),
        ],
      ),
    )
    const { wrapper } = await mountView()
    const rows = historyRows(wrapper)
    expect(rows.map(([summary]) => summary)).toEqual([
      'Call — 7 s · outcome needed',
      'Contact attempted — call, reached',
      'Call — failed',
      'Call — 7 s · outcome needed',
    ])
    const items = wrapper.findAll('li').filter((li) => li.find('p.text-body').exists())
    expect(items.map((li) => li.findAll('[data-testid="change-outcome"]').length)).toEqual([1, 0, 0, 0])
  })

  it('Change outcome opens the picker on the current choice and posts to /calls/{call_id}/outcome', async () => {
    stubApi(
      detail(
        [PHONE_A],
        [
          attemptEntry(ATTEMPT_ID, { outcome: 'reached', superseded: true }),
          completedEntry('h-completed'),
          attemptEntry('att-2', { outcome: 'no_answer', corrects_id: ATTEMPT_ID }),
        ],
      ),
    )
    const { wrapper } = await mountView()
    const action = wrapper.get('[data-testid="change-outcome"]')
    expect(action.classes()).toContain('bg-transparent')
    expect(action.text()).toBe('Change outcome')
    await action.trigger('click')
    await flushPromises()
    expect(dialogTitle()).toBe('Change outcome')
    const picker = document.querySelector('[data-testid="outcome-picker"]')
    expect(picker).not.toBeNull()
    expect(picker?.querySelector('[aria-checked="true"]')?.getAttribute('data-outcome')).toBe('no_answer')
    ;(picker?.querySelector('[data-outcome="busy"]') as HTMLButtonElement).click()
    await flushPromises()
    ;(document.querySelector('[data-testid="change-outcome-save"]') as HTMLButtonElement).click()
    await flushPromises()
    const post = apiFetchMock.mock.calls.find(([path]) => path === `/calls/${CALL_ID}/outcome`)
    expect(JSON.parse(String(post?.[1]?.body))).toEqual({ outcome: 'busy' })
    expect(document.querySelector('[data-testid="outcome-picker"]')).toBeNull()
  })

  it('Set outcome on an incomplete call opens with nothing selected, Save disabled until a pick, Cancel kept', async () => {
    stubApi(detail([PHONE_A], [attemptEntry(ATTEMPT_ID, { outcome: 'reached' }), completedEntry('h-completed')]))
    const { wrapper } = await mountView()
    const action = wrapper.get('[data-testid="change-outcome"]')
    expect(action.text()).toBe('Set outcome')
    await action.trigger('click')
    await flushPromises()
    expect(dialogTitle()).toBe('Set outcome')
    const picker = document.querySelector('[data-testid="outcome-picker"]')
    expect(picker?.querySelector('[aria-checked="true"]')).toBeNull()
    const save = document.querySelector('[data-testid="change-outcome-save"]') as HTMLButtonElement
    expect(save.disabled).toBe(true)
    expect(document.querySelector('[data-testid="change-outcome-cancel"]')).not.toBeNull()
    ;(picker?.querySelector('[data-outcome="left_message"]') as HTMLButtonElement).click()
    await flushPromises()
    expect(save.disabled).toBe(false)
    save.click()
    await flushPromises()
    const post = apiFetchMock.mock.calls.find(([path]) => path === `/calls/${CALL_ID}/outcome`)
    expect(JSON.parse(String(post?.[1]?.body))).toEqual({ outcome: 'left_message' })
    expect(document.querySelector('[data-testid="outcome-picker"]')).toBeNull()
  })

  it('is disabled while the post-call prompt is open (one primary)', async () => {
    stubApi(detail([PHONE_A], [attemptEntry(ATTEMPT_ID, {}), completedEntry('h-completed')]), {
      settledHangup: callView({ status: 'ended', end_reason: 'remote_hangup', answered_at: 'x' }),
    })
    const { wrapper, rooms } = await mountView()
    expect(wrapper.get('[data-testid="change-outcome"]').attributes('disabled')).toBeUndefined()
    await wrapper.get('[data-testid="call-button"]').trigger('click')
    await flushPromises()
    rooms[0].emit('participantConnected', { identity: `sip:${CALL_ID}`, attributes: { 'sip.callStatus': 'active' } })
    await flushPromises()
    rooms[0].emit('participantDisconnected', { identity: `sip:${CALL_ID}`, attributes: {} })
    await flushPromises()
    expect(wrapper.find('[data-testid="call-outcome-prompt"]').exists()).toBe(true)
    const action = wrapper.get('[data-testid="change-outcome"]')
    expect(action.attributes('disabled')).toBeDefined()
    action.element.removeAttribute('disabled')
    await action.trigger('click')
    await flushPromises()
    expect(document.querySelector('[data-testid="change-outcome-save"]')).toBeNull()
    // The note composer's always-visible "Add note" button and the task
    // Add form's always-visible "Add task" button (docs/specs/SLICE_015.md
    // §5, docs/specs/SLICE_016.md §8) are also primary-styled and
    // independent of the call/outcome state this assertion is actually
    // about, so both are excluded from the count rather than folded into
    // the call header's own one-primary invariant.
    expect(
      wrapper.findAll('.bg-accent').filter((el) => !['note-composer-submit', 'task-add-submit'].includes(el.attributes('data-testid') ?? '')),
    ).toHaveLength(1)
    // Only a saved outcome releases the prompt (no Skip).
    await wrapper.get('[data-outcome="reached"]').trigger('click')
    await wrapper.get('[data-testid="call-outcome-save"]').trigger('click')
    await flushPromises()
    await wrapper.get('[data-testid="call-dismiss"]').trigger('click')
    await flushPromises()
    expect(wrapper.get('[data-testid="change-outcome"]').attributes('disabled')).toBeUndefined()
  })

  it('re-seeds the picker when reopened on a different call row (chosen → incomplete)', async () => {
    stubApi(
      detail(
        [PHONE_A],
        [
          attemptEntry('a1', { outcome: 'reached', call_id: 'call-a', superseded: true }),
          completedEntry('c-a', { call_id: 'call-a' }),
          attemptEntry('a1-fix', { outcome: 'no_answer', call_id: 'call-a', corrects_id: 'a1' }),
          attemptEntry('b1', { outcome: 'reached', call_id: 'call-b' }),
          completedEntry('c-b', { call_id: 'call-b' }),
        ],
      ),
    )
    const { wrapper } = await mountView()
    const actions = wrapper.findAll('[data-testid="change-outcome"]')
    expect(actions.map((a) => a.text())).toEqual(['Change outcome', 'Set outcome'])
    const checkedOutcome = () =>
      document.querySelector('[data-testid="outcome-picker"] [aria-checked="true"]')?.getAttribute('data-outcome')
    await actions[0].trigger('click')
    await flushPromises()
    expect(checkedOutcome()).toBe('no_answer')
    ;(document.querySelector('[data-testid="outcome-picker"] [data-outcome="busy"]') as HTMLButtonElement).click()
    await flushPromises()
    expect(checkedOutcome()).toBe('busy')
    ;(document.querySelector('[data-testid="change-outcome-cancel"]') as HTMLButtonElement).click()
    await flushPromises()
    await actions[1].trigger('click')
    await flushPromises()
    expect(checkedOutcome()).toBeUndefined()
  })

  it('shows the §10 copy inside the dialog on failure', async () => {
    stubApi(detail([PHONE_A], [attemptEntry(ATTEMPT_ID, {}), completedEntry('h-completed')]), {
      outcome: new ApiError(422, 'no_contact_attempt'),
    })
    const { wrapper } = await mountView()
    await wrapper.get('[data-testid="change-outcome"]').trigger('click')
    await flushPromises()
    ;(document.querySelector('[data-testid="outcome-picker"] [data-outcome="busy"]') as HTMLButtonElement).click()
    await flushPromises()
    ;(document.querySelector('[data-testid="change-outcome-save"]') as HTMLButtonElement).click()
    await flushPromises()
    expect(document.querySelector('[data-testid="change-outcome-error"]')?.textContent?.trim()).toBe(
      "There's no contact attempt to set an outcome on.",
    )
  })
})

describe('PersonDetailView — Today → Person ?outcome=<call_id> (SLICE_006c §5a)', () => {
  const incomplete = () => detail([PHONE_A], [attemptEntry(ATTEMPT_ID, { outcome: 'reached' }), completedEntry('h-completed')])

  it('opens the Set-outcome dialog for that call with nothing selected; Save closes it and clears the param', async () => {
    stubApi(incomplete())
    const { router } = await mountView(`?outcome=${CALL_ID}`)
    expect(dialogTitle()).toBe('Set outcome')
    const picker = document.querySelector('[data-testid="outcome-picker"]')
    expect(picker).not.toBeNull()
    expect(picker?.querySelector('[aria-checked="true"]')).toBeNull()
    expect(router.currentRoute.value.query.outcome).toBe(CALL_ID)
    ;(picker?.querySelector('[data-outcome="no_answer"]') as HTMLButtonElement).click()
    await flushPromises()
    ;(document.querySelector('[data-testid="change-outcome-save"]') as HTMLButtonElement).click()
    await flushPromises()
    const post = apiFetchMock.mock.calls.find(([path]) => path === `/calls/${CALL_ID}/outcome`)
    expect(JSON.parse(String(post?.[1]?.body))).toEqual({ outcome: 'no_answer' })
    expect(document.querySelector('[data-testid="outcome-picker"]')).toBeNull()
    expect(router.currentRoute.value.query.outcome).toBeUndefined()
    expect(router.currentRoute.value.path).toBe(`/people/${PERSON_ID}`)
  })

  it('Cancel closes the dialog and clears the param without a request', async () => {
    stubApi(incomplete())
    const { router } = await mountView(`?outcome=${CALL_ID}`)
    expect(document.querySelector('[data-testid="outcome-picker"]')).not.toBeNull()
    ;(document.querySelector('[data-testid="change-outcome-cancel"]') as HTMLButtonElement).click()
    await flushPromises()
    expect(document.querySelector('[data-testid="outcome-picker"]')).toBeNull()
    expect(router.currentRoute.value.query.outcome).toBeUndefined()
    expect(requests().some((r) => r.endsWith('/outcome'))).toBe(false)
  })

  it('an unknown call id (or one I cannot act on) just clears the param', async () => {
    stubApi(incomplete())
    const { router } = await mountView('?outcome=not-a-call')
    expect(document.querySelector('[data-testid="outcome-picker"]')).toBeNull()
    expect(router.currentRoute.value.query.outcome).toBeUndefined()
  })

  it('a call with an existing choice opens as "Change outcome" with that choice pre-selected', async () => {
    stubApi(
      detail(
        [PHONE_A],
        [
          attemptEntry(ATTEMPT_ID, { outcome: 'reached', superseded: true }),
          completedEntry('h-completed'),
          attemptEntry('att-fix', { outcome: 'no_answer', corrects_id: ATTEMPT_ID }),
        ],
      ),
    )
    const { router } = await mountView(`?outcome=${CALL_ID}`)
    expect(dialogTitle()).toBe('Change outcome')
    expect(
      document.querySelector('[data-testid="outcome-picker"] [aria-checked="true"]')?.getAttribute('data-outcome'),
    ).toBe('no_answer')
    expect(router.currentRoute.value.query.outcome).toBe(CALL_ID)
  })

  it('does not clear the param while a refetch is in flight; the settled fetch decides', async () => {
    // Stale cache without the call row (a prior visit, before the call); the
    // refetch on mount carries the incomplete call. The param must survive
    // the stale render and open the dialog once the fetch settles.
    stubApi(incomplete())
    const { router } = await mountView(`?outcome=${CALL_ID}`, detail([PHONE_A]))
    expect(dialogTitle()).toBe('Set outcome')
    expect(router.currentRoute.value.query.outcome).toBe(CALL_ID)
  })
})

describe('PersonDetailView — Log contact dialog vocabulary (SLICE_006c §1)', () => {
  it('offers Voicemail / left message, Busy and Wrong number', async () => {
    stubApi(detail([PHONE_A]))
    const { wrapper } = await mountView()
    await wrapper.get('[data-testid="log-contact"]').trigger('click')
    await flushPromises()
    const outcome = document.querySelector('[aria-label="Outcome"]') as HTMLElement
    outcome.click()
    await flushPromises()
    const labels = Array.from(document.querySelectorAll('[role="option"]')).map((o) => o.textContent?.trim())
    expect(labels).toEqual(['Reached', 'No answer', 'Voicemail / left message', 'Sent', 'Busy', 'Wrong number'])
    // Explicit unmount: this test leaves a PrimeVue Select overlay open
    // (never closed or dismissed), and the surrounding suite's `afterEach`
    // only clears `document.body` rather than tearing down the component
    // tree. Left alone, that overlay's own pending update can resolve
    // during a LATER, unrelated test and throw trying to patch DOM this
    // test's own body-clear already removed.
    wrapper.unmount()
    await flushPromises()
  })
})

// SLICE_011e §5, §9.9: chips beside Stage/Assignee, the Add tag popover
// (existing-tag apply and inline create, in order), remove target/name,
// inline 409s, and Escape/focus-return.
describe('PersonDetailView — Tags', () => {
  const SPHERE: Tag = { id: 'tag-sphere', name: 'Sphere', person_count: 3, can_manage: false }
  const INVESTOR: Tag = { id: 'tag-investor', name: 'Investor', person_count: 0, can_manage: true }

  // A tag mutation's success path invalidates and refetches BOTH the tags
  // and person queries; without an explicit unmount, that refetch can
  // resolve after a later test's own `afterEach` has already cleared
  // `document.body`, throwing on the detached DOM. Unmounting first (this
  // block's own `afterEach` runs before the outer one, which only clears
  // `document.body.innerHTML`) lets every pending query settle against a
  // torn-down component instead of a wiped body.
  let activeWrapper: Awaited<ReturnType<typeof mountView>>['wrapper'] | null = null
  afterEach(async () => {
    activeWrapper?.unmount()
    activeWrapper = null
    await flushPromises()
  })

  it('renders chips from the detail response with a 40px remove target and accessible name', async () => {
    stubApi(detail([PHONE_A], [], [{ id: SPHERE.id, name: SPHERE.name }]))
    const { wrapper } = await mountView()
    activeWrapper = wrapper
    const chips = wrapper.findAll('[data-testid="person-tag-chip"]')
    expect(chips).toHaveLength(1)
    expect(chips[0].text()).toContain('Sphere')
    const remove = wrapper.get('[aria-label="Remove tag Sphere"]')
    expect(remove.classes()).toContain('min-h-10')
    expect(remove.classes()).toContain('w-10')
  })

  it('adds an existing tag via the popover (PUT only, no create call)', async () => {
    stubApi(detail([PHONE_A]), { orgTags: [INVESTOR] })
    const { wrapper } = await mountView()
    activeWrapper = wrapper
    await wrapper.get('[data-testid="add-tag-button"]').trigger('click')
    await flushPromises()
    const option = wrapper.get('[data-testid="add-tag-option"]')
    expect(option.text()).toBe('Investor')
    await option.trigger('click')
    await flushPromises()
    expect(wrapper.find('[data-testid="add-tag-popover"]').exists()).toBe(false)
    expect(wrapper.get('[data-testid="person-tag-chip"]').text()).toContain('Investor')
    const tagRequests = apiFetchMock.mock.calls.filter(([path]) => path.startsWith('/tags') || path.includes('/tags/'))
    expect(tagRequests.some(([, init]) => (init?.method ?? 'GET') === 'POST')).toBe(false)
  })

  it('creates a new tag inline as two requests, in order (POST then PUT)', async () => {
    stubApi(detail([PHONE_A]), { orgTags: [] })
    const { wrapper } = await mountView()
    activeWrapper = wrapper
    await wrapper.get('[data-testid="add-tag-button"]').trigger('click')
    await wrapper.get('[data-testid="add-tag-search"]').setValue('Past Client')
    await flushPromises()
    const createRow = wrapper.get('[data-testid="add-tag-create-row"]')
    expect(createRow.text()).toBe("Create 'Past Client'")
    await createRow.trigger('click')
    await flushPromises()
    const order = apiFetchMock.mock.calls
      .map(([path, init]) => `${init?.method ?? 'GET'} ${path}`)
      .filter((r) => r === 'POST /tags' || r.startsWith(`PUT /people/${PERSON_ID}/tags/`))
    expect(order).toEqual(['POST /tags', `PUT /people/${PERSON_ID}/tags/${encodeURIComponent('tag-past client')}`])
    expect(wrapper.get('[data-testid="person-tag-chip"]').text()).toContain('Past Client')
  })

  it('shows the 20-tags and 200-tags inline 409 messages', async () => {
    stubApi(detail([PHONE_A]), {
      orgTags: [INVESTOR],
      personTag: () => new ApiError(409, 'person_tag_limit_reached'),
    })
    const { wrapper } = await mountView()
    activeWrapper = wrapper
    await wrapper.get('[data-testid="add-tag-button"]').trigger('click')
    await wrapper.get('[data-testid="add-tag-option"]').trigger('click')
    await flushPromises()
    expect(wrapper.get('[data-testid="add-tag-error"]').text()).toBe('This person already has 20 tags.')

    stubApi(detail([PHONE_A]), {
      orgTags: [],
      createTag: () => new ApiError(409, 'tag_limit_reached'),
    })
    await wrapper.get('[data-testid="add-tag-search"]').setValue('Overflow')
    await flushPromises()
    await wrapper.get('[data-testid="add-tag-create-row"]').trigger('click')
    await flushPromises()
    expect(wrapper.get('[data-testid="add-tag-error"]').text()).toBe('This Organization already has 200 tags.')
  })

  it('removes a tag and re-fetches the tags query AND the Person on a 404; the chip is gone and an inline message explains it', async () => {
    stubApi(detail([PHONE_A], [], [{ id: SPHERE.id, name: SPHERE.name }]), {
      orgTags: [SPHERE],
      personTag: () => new ApiError(404, 'not_found'),
    })
    const { wrapper } = await mountView()
    activeWrapper = wrapper
    const tagsBefore = apiFetchMock.mock.calls.filter(([path]) => path === '/tags').length
    const personBefore = apiFetchMock.mock.calls.filter(([path]) => path === `/people/${PERSON_ID}`).length
    await wrapper.get('[aria-label="Remove tag Sphere"]').trigger('click')
    await flushPromises()
    const tagsAfter = apiFetchMock.mock.calls.filter(([path]) => path === '/tags').length
    const personAfter = apiFetchMock.mock.calls.filter(([path]) => path === `/people/${PERSON_ID}`).length
    expect(tagsAfter).toBeGreaterThan(tagsBefore)
    expect(personAfter).toBeGreaterThan(personBefore)
    expect(wrapper.find('[data-testid="person-tag-chip"]').exists()).toBe(false)
    expect(wrapper.get('[data-testid="remove-tag-error"]').text()).toBe('That tag no longer exists; refreshed.')
  })

  it('Escape closes the popover and returns focus to the Add tag button', async () => {
    stubApi(detail([PHONE_A]), { orgTags: [INVESTOR] })
    const { wrapper } = await mountView()
    activeWrapper = wrapper
    const button = wrapper.get('[data-testid="add-tag-button"]')
    await button.trigger('click')
    expect(wrapper.find('[data-testid="add-tag-popover"]').exists()).toBe(true)
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
    await flushPromises()
    expect(wrapper.find('[data-testid="add-tag-popover"]').exists()).toBe(false)
    expect(document.activeElement).toBe(button.element)
  })

  it('closes on an outside click without stealing focus', async () => {
    stubApi(detail([PHONE_A]), { orgTags: [INVESTOR] })
    const { wrapper } = await mountView()
    activeWrapper = wrapper
    await wrapper.get('[data-testid="add-tag-button"]').trigger('click')
    document.body.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await flushPromises()
    expect(wrapper.find('[data-testid="add-tag-popover"]').exists()).toBe(false)
  })
})

/** A manually-resolved custom-field PUT response — lets a test hold one
 * row's save open while a sibling row's save resolves, to exercise the
 * per-row (not shared-mutation-observer) pending/error tracking. */
function deferredCustomFieldResult() {
  let resolve!: (value: PersonCustomFieldValueMutationResponse | Error) => void
  const promise = new Promise<PersonCustomFieldValueMutationResponse | Error>((yes) => { resolve = yes })
  return { promise, resolve }
}

describe('PersonDetailView — Custom fields (SLICE_019.md §9.11)', () => {
  const BUDGET_FIELD: CustomField = {
    id: 'field-budget',
    label: 'Budget',
    field_type: 'number',
    position: 1,
    archived_at: null,
    person_count: 1,
    options: [],
  }
  const TEMPERATURE_FIELD: CustomField = {
    id: 'field-temperature',
    label: 'Lead temperature',
    field_type: 'choice',
    position: 2,
    archived_at: null,
    person_count: 0,
    options: [
      { id: 'opt-cold', label: 'Cold', position: 1, archived_at: null },
      { id: 'opt-warm', label: 'Warm', position: 2, archived_at: null },
    ],
  }

  // Same unmount-before-clear reasoning as the Tags block above: a
  // settling mutation's refetch must not resolve against a body a later
  // test has already wiped.
  let activeWrapper: Awaited<ReturnType<typeof mountView>>['wrapper'] | null = null
  afterEach(async () => {
    activeWrapper?.unmount()
    activeWrapper = null
    await flushPromises()
  })

  // `settlePersonMutation` (api/queries.ts) defers its invalidate/refetch
  // decision past a macrotask (the Notes block's own precedent below) — a
  // genuine `setTimeout(fn, 0)` tick plus a further flush lets that
  // deferred decision, and the Person refetch it triggers, actually land.
  async function settleTick() {
    await new Promise((resolve) => setTimeout(resolve, 0))
    await flushPromises()
  }

  it('renders every live field in position order with the right editor per type, pre-filled from the set value', async () => {
    stubApi(detail([PHONE_A], [], [], [], [
      { field_id: BUDGET_FIELD.id, label: 'Budget', field_type: 'number', value: { number: '400000' }, option_label: null, updated_at: '2026-08-22T09:00:00.000Z' },
    ]), { customFieldDefinitions: [BUDGET_FIELD, TEMPERATURE_FIELD] })
    const { wrapper } = await mountView()
    activeWrapper = wrapper
    const rows = wrapper.findAll('[data-testid="custom-field-row"]')
    expect(rows).toHaveLength(2)
    expect(rows[0].text()).toContain('Budget')
    expect(rows[1].text()).toContain('Lead temperature')
    expect((wrapper.get('[data-testid="custom-field-number-input"]').element as HTMLInputElement).value).toBe('400000')
    expect(wrapper.find('[data-testid="custom-field-choice-select"]').exists()).toBe(true)
  })

  it('shows "No custom fields yet." with an admin link only for an admin', async () => {
    stubApi(detail([PHONE_A]), { customFieldDefinitions: [] })
    const { wrapper } = await mountView()
    activeWrapper = wrapper
    expect(wrapper.get('[data-testid="custom-fields-empty"]').text()).toContain('No custom fields yet.')
    expect(wrapper.find('a[href="/manage/fields"]').exists()).toBe(false)
  })

  it('shows both admin links to Manage → Fields for an org admin', async () => {
    stubApi(detail([PHONE_A]), {
      customFieldDefinitions: [],
      meOverride: {
        user: { id: 'u-alice', email: 'alice@acme.test', display_name: 'Alice' },
        organization: { workspace_mode: 'operational', workspace_revision: '1', id: ORG_ID, name: 'Acme Realty', role: 'admin' },
        platform_admin: false,
      },
    })
    const { wrapper } = await mountView()
    activeWrapper = wrapper
    expect(wrapper.get('[data-testid="custom-fields-empty"]').text()).toContain('No custom fields yet.')
    expect(wrapper.findAll('a[href="/manage/fields"]')).toHaveLength(2)
  })

  it('text: blur saves, Enter saves, Escape reverts without saving', async () => {
    const referrerField: CustomField = { id: 'field-referrer', label: 'Referrer', field_type: 'text', position: 1, archived_at: null, person_count: 0, options: [] }
    stubApi(detail([PHONE_A]), { customFieldDefinitions: [referrerField] })
    const { wrapper } = await mountView()
    activeWrapper = wrapper
    const input = wrapper.get('[data-testid="custom-field-text-input"]')
    await input.setValue('Zillow')
    await input.trigger('blur')
    await settleTick()
    let call = apiFetchMock.mock.calls.find(([path, init]) => path === `/people/${PERSON_ID}/custom-fields/${referrerField.id}` && init?.method === 'PUT')
    expect(JSON.parse(String(call?.[1]?.body))).toEqual({ value: { text: 'Zillow' } })

    apiFetchMock.mockClear()
    await input.setValue('Referral')
    await input.trigger('keydown', { key: 'Enter' })
    await settleTick()
    call = apiFetchMock.mock.calls.find(([path, init]) => path === `/people/${PERSON_ID}/custom-fields/${referrerField.id}` && init?.method === 'PUT')
    expect(JSON.parse(String(call?.[1]?.body))).toEqual({ value: { text: 'Referral' } })

    apiFetchMock.mockClear()
    await input.setValue('Not saved')
    await input.trigger('keydown', { key: 'Escape' })
    await flushPromises()
    expect(apiFetchMock.mock.calls.some(([, init]) => init?.method === 'PUT')).toBe(false)
    expect((wrapper.get('[data-testid="custom-field-text-input"]').element as HTMLInputElement).value).toBe('Referral')

    // W1 regression: reverting clears `dirty`, so the `@blur` this Escape
    // itself triggers (via the handler's `.blur()`) must not re-send.
    apiFetchMock.mockClear()
    await input.trigger('blur')
    await flushPromises()
    expect(apiFetchMock.mock.calls.some(([, init]) => init?.method === 'PUT' || init?.method === 'DELETE')).toBe(false)
  })

  it('blur on an untouched (pre-filled, unedited) field sends no request', async () => {
    stubApi(
      detail([PHONE_A], [], [], [], [
        { field_id: 'field-referrer', label: 'Referrer', field_type: 'text', value: { text: 'Zillow' }, option_label: null, updated_at: '2026-08-22T09:00:00.000Z' },
      ]),
      { customFieldDefinitions: [{ id: 'field-referrer', label: 'Referrer', field_type: 'text', position: 1, archived_at: null, person_count: 1, options: [] }] },
    )
    const { wrapper } = await mountView()
    activeWrapper = wrapper
    apiFetchMock.mockClear()
    const input = wrapper.get('[data-testid="custom-field-text-input"]')
    await input.trigger('focus')
    await input.trigger('blur')
    await flushPromises()
    expect(apiFetchMock.mock.calls.some(([, init]) => init?.method === 'PUT' || init?.method === 'DELETE')).toBe(false)
  })

  it('after a successful save, an immediate further blur with no new edit sends nothing', async () => {
    const referrerField: CustomField = { id: 'field-referrer', label: 'Referrer', field_type: 'text', position: 1, archived_at: null, person_count: 0, options: [] }
    stubApi(detail([PHONE_A]), { customFieldDefinitions: [referrerField] })
    const { wrapper } = await mountView()
    activeWrapper = wrapper
    const input = wrapper.get('[data-testid="custom-field-text-input"]')
    await input.setValue('Zillow')
    await input.trigger('blur')
    await settleTick()
    expect(apiFetchMock.mock.calls.some(([path, init]) => path === `/people/${PERSON_ID}/custom-fields/${referrerField.id}` && init?.method === 'PUT')).toBe(true)

    apiFetchMock.mockClear()
    await input.trigger('blur')
    await flushPromises()
    expect(apiFetchMock.mock.calls.some(([, init]) => init?.method === 'PUT' || init?.method === 'DELETE')).toBe(false)
  })

  it('a dirty (in-progress) draft is not clobbered by a server value that changes underneath it', async () => {
    stubApi(
      detail([PHONE_A], [], [], [], [
        { field_id: 'field-referrer', label: 'Referrer', field_type: 'text', value: { text: 'Zillow' }, option_label: null, updated_at: '2026-08-22T09:00:00.000Z' },
      ]),
      { customFieldDefinitions: [{ id: 'field-referrer', label: 'Referrer', field_type: 'text', position: 1, archived_at: null, person_count: 1, options: [] }] },
    )
    const { wrapper, queryClient } = await mountView()
    activeWrapper = wrapper
    const input = wrapper.get('[data-testid="custom-field-text-input"]')
    await input.setValue('Typing a new value')
    queryClient.setQueryData(queryKeys.person(ORG_ID, PERSON_ID), (old: PersonDetailResponse | undefined) =>
      old && {
        ...old,
        custom_fields: old.custom_fields.map((v) =>
          v.field_id === 'field-referrer' ? { ...v, value: { text: 'Changed elsewhere' } } : v,
        ),
      },
    )
    await nextTick()
    expect((wrapper.get('[data-testid="custom-field-text-input"]').element as HTMLInputElement).value).toBe('Typing a new value')
  })

  it('once a field is not dirty, a server value that changes underneath it updates the displayed draft', async () => {
    stubApi(
      detail([PHONE_A], [], [], [], [
        { field_id: 'field-referrer', label: 'Referrer', field_type: 'text', value: { text: 'Zillow' }, option_label: null, updated_at: '2026-08-22T09:00:00.000Z' },
      ]),
      { customFieldDefinitions: [{ id: 'field-referrer', label: 'Referrer', field_type: 'text', position: 1, archived_at: null, person_count: 1, options: [] }] },
    )
    const { wrapper, queryClient } = await mountView()
    activeWrapper = wrapper
    expect((wrapper.get('[data-testid="custom-field-text-input"]').element as HTMLInputElement).value).toBe('Zillow')
    queryClient.setQueryData(queryKeys.person(ORG_ID, PERSON_ID), (old: PersonDetailResponse | undefined) =>
      old && {
        ...old,
        custom_fields: old.custom_fields.map((v) =>
          v.field_id === 'field-referrer' ? { ...v, value: { text: 'Changed elsewhere' } } : v,
        ),
      },
    )
    await nextTick()
    expect((wrapper.get('[data-testid="custom-field-text-input"]').element as HTMLInputElement).value).toBe('Changed elsewhere')
  })

  it('two different rows saving concurrently do not interfere with each other\'s pending or error state', async () => {
    const referrerField: CustomField = { id: 'field-referrer', label: 'Referrer', field_type: 'text', position: 1, archived_at: null, person_count: 0, options: [] }
    const referrerDeferred = deferredCustomFieldResult()
    const budgetDeferred = deferredCustomFieldResult()
    stubApi(detail([PHONE_A]), {
      customFieldDefinitions: [referrerField, BUDGET_FIELD],
      customFieldSetAsync: (fieldId) => (fieldId === referrerField.id ? referrerDeferred.promise : budgetDeferred.promise),
    })
    const { wrapper } = await mountView()
    activeWrapper = wrapper
    const textInput = wrapper.get('[data-testid="custom-field-text-input"]')
    const numberInput = wrapper.get('[data-testid="custom-field-number-input"]')
    await textInput.setValue('Zillow')
    await textInput.trigger('blur')
    await numberInput.setValue('500')
    await numberInput.trigger('blur')
    await flushPromises()
    expect((textInput.element as HTMLInputElement).disabled).toBe(true)
    expect((numberInput.element as HTMLInputElement).disabled).toBe(true)

    // Budget's save fails; Referrer's is still in flight — the failure must
    // not leak onto the sibling row.
    budgetDeferred.resolve(new ApiError(404, 'not_found'))
    await settleTick()
    expect((numberInput.element as HTMLInputElement).disabled).toBe(false)
    expect(wrapper.findAll('[data-testid="custom-field-error"]')).toHaveLength(1)
    expect((textInput.element as HTMLInputElement).disabled).toBe(true)

    referrerDeferred.resolve({
      custom_fields: [
        { field_id: referrerField.id, label: 'Referrer', field_type: 'text', value: { text: 'Zillow' }, option_label: null, updated_at: '2026-09-10T12:00:00.000Z' },
      ],
      changed: true,
    })
    await settleTick()
    expect((textInput.element as HTMLInputElement).disabled).toBe(false)
  })

  it('date: blur with no input sends nothing; a valid date saves with an exact payload', async () => {
    const dateField: CustomField = { id: 'field-closing', label: 'Closing date', field_type: 'date', position: 1, archived_at: null, person_count: 0, options: [] }
    stubApi(detail([PHONE_A]), { customFieldDefinitions: [dateField] })
    const { wrapper } = await mountView()
    activeWrapper = wrapper
    const input = wrapper.get('[data-testid="custom-field-date-input"]')
    await input.trigger('focus')
    await input.trigger('blur')
    await flushPromises()
    expect(apiFetchMock.mock.calls.some(([, init]) => init?.method === 'PUT' || init?.method === 'DELETE')).toBe(false)

    await input.setValue('2026-10-15')
    await input.trigger('blur')
    await flushPromises()
    const call = apiFetchMock.mock.calls.find(([path, init]) => path === `/people/${PERSON_ID}/custom-fields/${dateField.id}` && init?.method === 'PUT')
    expect(JSON.parse(String(call?.[1]?.body))).toEqual({ value: { date: '2026-10-15' } })
  })

  it('a partially-typed (invalid) date shows an inline error and sends nothing', async () => {
    const dateField: CustomField = { id: 'field-closing', label: 'Closing date', field_type: 'date', position: 1, archived_at: null, person_count: 0, options: [] }
    stubApi(detail([PHONE_A]), { customFieldDefinitions: [dateField] })
    const { wrapper } = await mountView()
    activeWrapper = wrapper
    const input = wrapper.get('[data-testid="custom-field-date-input"]')
    await input.setValue('2026-09')
    // jsdom's native date input never reports `badInput`; a real browser
    // does for a partially-typed value (an empty, otherwise-valid `.value`).
    Object.defineProperty(input.element, 'validity', { value: { badInput: true }, configurable: true })
    await input.trigger('blur')
    await flushPromises()
    expect(wrapper.get('[data-testid="custom-field-error"]').text()).toBe('Enter a complete date.')
    expect(apiFetchMock.mock.calls.some(([, init]) => init?.method === 'PUT' || init?.method === 'DELETE')).toBe(false)
  })

  it('an empty draft clears the value instead of sending an empty string', async () => {
    stubApi(
      detail([PHONE_A], [], [], [], [
        { field_id: 'field-referrer', label: 'Referrer', field_type: 'text', value: { text: 'Zillow' }, option_label: null, updated_at: '2026-08-22T09:00:00.000Z' },
      ]),
      { customFieldDefinitions: [{ id: 'field-referrer', label: 'Referrer', field_type: 'text', position: 1, archived_at: null, person_count: 1, options: [] }] },
    )
    const { wrapper } = await mountView()
    activeWrapper = wrapper
    const input = wrapper.get('[data-testid="custom-field-text-input"]')
    await input.setValue('   ')
    await input.trigger('blur')
    await flushPromises()
    expect(apiFetchMock.mock.calls.some(([path, init]) => path === `/people/${PERSON_ID}/custom-fields/field-referrer` && init?.method === 'DELETE')).toBe(true)
  })

  it('number: an invalid pattern shows an inline error with no request; a valid one saves', async () => {
    stubApi(detail([PHONE_A]), { customFieldDefinitions: [BUDGET_FIELD] })
    const { wrapper } = await mountView()
    activeWrapper = wrapper
    const input = wrapper.get('[data-testid="custom-field-number-input"]')
    await input.setValue('1e5')
    await input.trigger('blur')
    await flushPromises()
    expect(wrapper.get('[data-testid="custom-field-error"]').text()).toBe('Enter a number with up to 4 decimal places.')
    expect(apiFetchMock.mock.calls.some(([, init]) => init?.method === 'PUT')).toBe(false)

    await input.setValue('12.50')
    await input.trigger('blur')
    await flushPromises()
    const call = apiFetchMock.mock.calls.find(([path, init]) => path === `/people/${PERSON_ID}/custom-fields/${BUDGET_FIELD.id}` && init?.method === 'PUT')
    expect(JSON.parse(String(call?.[1]?.body))).toEqual({ value: { number: '12.50' } })
  })

  it('choice: selecting a live option saves it; selecting Clear removes it', async () => {
    stubApi(detail([PHONE_A]), { customFieldDefinitions: [TEMPERATURE_FIELD] })
    const { wrapper } = await mountView()
    activeWrapper = wrapper
    const select = wrapper.get('[data-testid="custom-field-choice-select"]').findComponent(Select)
    await select.vm.$emit('update:model-value', 'opt-warm')
    await flushPromises()
    let call = apiFetchMock.mock.calls.find(([path, init]) => path === `/people/${PERSON_ID}/custom-fields/${TEMPERATURE_FIELD.id}` && init?.method === 'PUT')
    expect(JSON.parse(String(call?.[1]?.body))).toEqual({ value: { option_id: 'opt-warm' } })

    apiFetchMock.mockClear()
    await select.vm.$emit('update:model-value', null)
    await flushPromises()
    call = apiFetchMock.mock.calls.find(([path, init]) => path === `/people/${PERSON_ID}/custom-fields/${TEMPERATURE_FIELD.id}` && init?.method === 'DELETE')
    expect(call).toBeTruthy()
  })

  it('a held-but-archived option renders as "keep current"; re-selecting it is a no-op (no request)', async () => {
    const archivedHeldField: CustomField = {
      ...TEMPERATURE_FIELD,
      options: [{ id: 'opt-cold', label: 'Cold', position: 1, archived_at: '2026-09-01T00:00:00.000Z' }, TEMPERATURE_FIELD.options[1]!],
    }
    stubApi(
      detail([PHONE_A], [], [], [], [
        { field_id: TEMPERATURE_FIELD.id, label: 'Lead temperature', field_type: 'choice', value: { option_id: 'opt-cold' }, option_label: 'Cold', updated_at: '2026-08-22T09:00:00.000Z' },
      ]),
      { customFieldDefinitions: [archivedHeldField] },
    )
    const { wrapper } = await mountView()
    activeWrapper = wrapper
    const select = wrapper.get('[data-testid="custom-field-choice-select"]').findComponent(Select)
    expect((select.props('options') as Array<{ label: string }>).some((o) => o.label.includes('Cold') && o.label.includes('archived'))).toBe(true)
    apiFetchMock.mockClear()
    await select.vm.$emit('update:model-value', 'opt-cold')
    await flushPromises()
    expect(apiFetchMock.mock.calls.some(([, init]) => init?.method === 'PUT' || init?.method === 'DELETE')).toBe(false)
  })

  it('a 404 on save refetches both the definitions and the Person', async () => {
    stubApi(detail([PHONE_A]), {
      customFieldDefinitions: [BUDGET_FIELD],
      customFieldSet: () => new ApiError(404, 'not_found'),
    })
    const { wrapper } = await mountView()
    activeWrapper = wrapper
    const definitionsBefore = apiFetchMock.mock.calls.filter(([path]) => path === '/custom-fields').length
    const personBefore = apiFetchMock.mock.calls.filter(([path]) => path === `/people/${PERSON_ID}`).length
    const input = wrapper.get('[data-testid="custom-field-number-input"]')
    await input.setValue('100')
    await input.trigger('blur')
    await flushPromises()
    const definitionsAfter = apiFetchMock.mock.calls.filter(([path]) => path === '/custom-fields').length
    const personAfter = apiFetchMock.mock.calls.filter(([path]) => path === `/people/${PERSON_ID}`).length
    expect(definitionsAfter).toBeGreaterThan(definitionsBefore)
    expect(personAfter).toBeGreaterThan(personBefore)
    expect(wrapper.get('[data-testid="custom-field-error"]').text()).toBe('Could not save this value.')
  })

  it('a 409 field_archived on save refetches both the definitions and the Person', async () => {
    stubApi(detail([PHONE_A]), {
      customFieldDefinitions: [BUDGET_FIELD],
      customFieldSet: () => new ApiError(409, 'field_archived'),
    })
    const { wrapper } = await mountView()
    activeWrapper = wrapper
    const definitionsBefore = apiFetchMock.mock.calls.filter(([path]) => path === '/custom-fields').length
    const personBefore = apiFetchMock.mock.calls.filter(([path]) => path === `/people/${PERSON_ID}`).length
    const input = wrapper.get('[data-testid="custom-field-number-input"]')
    await input.setValue('100')
    await input.trigger('blur')
    await flushPromises()
    const definitionsAfter = apiFetchMock.mock.calls.filter(([path]) => path === '/custom-fields').length
    const personAfter = apiFetchMock.mock.calls.filter(([path]) => path === `/people/${PERSON_ID}`).length
    expect(definitionsAfter).toBeGreaterThan(definitionsBefore)
    expect(personAfter).toBeGreaterThan(personBefore)
    expect(wrapper.get('[data-testid="custom-field-error"]').text()).toBe('Could not save this value.')
  })

  it('a 422 unknown_option on a choice save refetches both the definitions and the Person', async () => {
    stubApi(detail([PHONE_A]), {
      customFieldDefinitions: [TEMPERATURE_FIELD],
      customFieldSet: () => new ApiError(422, 'unknown_option'),
    })
    const { wrapper } = await mountView()
    activeWrapper = wrapper
    const definitionsBefore = apiFetchMock.mock.calls.filter(([path]) => path === '/custom-fields').length
    const personBefore = apiFetchMock.mock.calls.filter(([path]) => path === `/people/${PERSON_ID}`).length
    const select = wrapper.get('[data-testid="custom-field-choice-select"]').findComponent(Select)
    await select.vm.$emit('update:model-value', 'opt-warm')
    await flushPromises()
    const definitionsAfter = apiFetchMock.mock.calls.filter(([path]) => path === '/custom-fields').length
    const personAfter = apiFetchMock.mock.calls.filter(([path]) => path === `/people/${PERSON_ID}`).length
    expect(definitionsAfter).toBeGreaterThan(definitionsBefore)
    expect(personAfter).toBeGreaterThan(personBefore)
    expect(wrapper.get('[data-testid="custom-field-error"]').text()).toBe('Could not save this value.')
  })
})

describe('PersonDetailView — Notes (SLICE_015 §9.10)', () => {
  let activeWrapper: Awaited<ReturnType<typeof mountView>>['wrapper'] | null = null
  afterEach(async () => {
    activeWrapper?.unmount()
    activeWrapper = null
    await flushPromises()
  })

  // `settlePersonMutation` (api/queries.ts) defers its invalidate/refetch
  // decision past a macrotask so a sibling mutation settling in the same
  // tick is not mistaken for itself — real, not fake, timers here, so a
  // genuine `setTimeout(fn, 0)` tick plus a further flush lets that
  // deferred decision, and the refetch it may trigger, actually land.
  async function settleTick() {
    await new Promise((resolve) => setTimeout(resolve, 0))
    await flushPromises()
  }

  it('the composer is disabled while empty or whitespace-only; a plain Enter does nothing; Ctrl+Enter and Cmd (meta)+Enter both submit', async () => {
    stubApi(detail([PHONE_A]))
    const { wrapper } = await mountView()
    activeWrapper = wrapper
    const submit = wrapper.get('[data-testid="note-composer-submit"]')
    expect(submit.attributes('disabled')).toBeDefined()
    const textarea = wrapper.get('[data-testid="note-composer-textarea"]')
    await textarea.setValue('   ')
    expect(wrapper.get('[data-testid="note-composer-submit"]').attributes('disabled')).toBeDefined()
    await textarea.setValue('A real note')
    expect(wrapper.get('[data-testid="note-composer-submit"]').attributes('disabled')).toBeUndefined()

    function posts() {
      return apiFetchMock.mock.calls.filter(
        ([path, init]) => path === `/people/${PERSON_ID}/notes` && (init?.method ?? 'GET') === 'POST',
      )
    }

    // A plain Enter (no modifier) must never submit — the textarea takes
    // a literal newline instead.
    await textarea.trigger('keydown', { key: 'Enter' })
    await flushPromises()
    expect(posts()).toHaveLength(0)

    await textarea.trigger('keydown', { key: 'Enter', ctrlKey: true })
    await flushPromises()
    expect(posts()).toHaveLength(1)

    // Cmd (meta) + Enter submits too (macOS), a second, independent note.
    await textarea.setValue('A second real note')
    await textarea.trigger('keydown', { key: 'Enter', metaKey: true })
    await flushPromises()
    expect(posts()).toHaveLength(2)
  })

  it('a live counter appears past 9,000 code points and disables past 10,000 — counting code points, not UTF-16 units', async () => {
    stubApi(detail([PHONE_A]))
    const { wrapper } = await mountView()
    activeWrapper = wrapper
    const textarea = wrapper.get('[data-testid="note-composer-textarea"]')

    await textarea.setValue('a'.repeat(9000))
    expect(wrapper.find('[data-testid="note-composer-counter"]').exists()).toBe(false)

    await textarea.setValue('a'.repeat(9001))
    expect(wrapper.get('[data-testid="note-composer-counter"]').text()).toBe('9001/10000')
    expect(wrapper.get('[data-testid="note-composer-submit"]').attributes('disabled')).toBeUndefined()

    // 10,000 four-byte astral code points (each two UTF-16 units): the
    // counter must read 10,000, not 20,000, and Add must stay enabled.
    await textarea.setValue('\u{1F600}'.repeat(10000))
    expect(wrapper.get('[data-testid="note-composer-counter"]').text()).toBe('10000/10000')
    expect(wrapper.get('[data-testid="note-composer-submit"]').attributes('disabled')).toBeUndefined()

    await textarea.setValue('\u{1F600}'.repeat(10001))
    expect(wrapper.get('[data-testid="note-composer-submit"]').attributes('disabled')).toBeDefined()
  })

  it('a successful add clears the composer and the new note appears after the refetch', async () => {
    stubApi(detail([PHONE_A]))
    const { wrapper } = await mountView()
    activeWrapper = wrapper
    const textarea = wrapper.get('[data-testid="note-composer-textarea"]')
    await textarea.setValue('Called and left a voicemail')
    await wrapper.get('[data-testid="note-composer-submit"]').trigger('click')
    await flushPromises()
    await settleTick()
    expect((wrapper.get('[data-testid="note-composer-textarea"]').element as HTMLTextAreaElement).value).toBe('')
    expect(wrapper.get('[data-testid="note-body"]').text()).toBe('Called and left a voicemail')
  })

  // The `useRealtime.test.ts` keying/settle precedent (~333–371): fake
  // timers ONLY inside this one test (not the describe block, whose other
  // tests rely on real timers via `settleTick`), enabled after the
  // initial mount settles so `mountView()`'s own `flushPromises()` is
  // never itself at the mercy of the fake clock.
  it('keys the add mutation with personMutationKey and settles by invalidating only the person key, not the People list', async () => {
    stubApi(detail([PHONE_A]))
    const { wrapper, queryClient } = await mountView()
    activeWrapper = wrapper

    vi.useFakeTimers()
    try {
      let releaseAdd: () => void = () => {}
      const addGate = new Promise<void>((resolve) => { releaseAdd = resolve })
      const defaultImpl = apiFetchMock.getMockImplementation()!
      apiFetchMock.mockImplementation(async (path: string, init?: RequestInit) => {
        if (path === `/people/${PERSON_ID}/notes` && (init?.method ?? 'GET') === 'POST') {
          await addGate
        }
        return defaultImpl(path, init)
      })

      await wrapper.get('[data-testid="note-composer-textarea"]').setValue('Called and left a voicemail')
      await wrapper.get('[data-testid="note-composer-submit"]').trigger('click')
      await nextTick()

      expect(queryClient.isMutating({ mutationKey: personMutationKey(ORG_ID, PERSON_ID) })).toBe(1)

      const invalidateSpy = vi.spyOn(queryClient, 'invalidateQueries')
      releaseAdd()
      await nextTick()
      await nextTick()
      await vi.advanceTimersByTimeAsync(0)
      await nextTick()

      expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: queryKeys.person(ORG_ID, PERSON_ID) })
      expect(invalidateSpy).not.toHaveBeenCalledWith({ queryKey: queryKeys.people(ORG_ID) })
    } finally {
      vi.useRealTimers()
    }
  })

  it('while an add is pending: the button reads "Adding…" and is disabled, the textarea is disabled, and a click plus Ctrl+Enter both no-op (exactly one POST) until it settles', async () => {
    stubApi(detail([PHONE_A]))
    const { wrapper } = await mountView()
    activeWrapper = wrapper

    let releaseAdd: () => void = () => {}
    const addGate = new Promise<void>((resolve) => { releaseAdd = resolve })
    const defaultImpl = apiFetchMock.getMockImplementation()!
    apiFetchMock.mockImplementation(async (path: string, init?: RequestInit) => {
      if (path === `/people/${PERSON_ID}/notes` && (init?.method ?? 'GET') === 'POST') {
        await addGate
      }
      return defaultImpl(path, init)
    })

    const textarea = wrapper.get('[data-testid="note-composer-textarea"]')
    await textarea.setValue('Called and left a voicemail')
    await wrapper.get('[data-testid="note-composer-submit"]').trigger('click')
    await flushPromises()

    const submit = wrapper.get('[data-testid="note-composer-submit"]')
    expect(submit.text()).toBe('Adding…')
    expect(submit.attributes('disabled')).toBeDefined()
    expect(wrapper.get('[data-testid="note-composer-textarea"]').attributes('disabled')).toBeDefined()

    function posts() {
      return apiFetchMock.mock.calls.filter(
        ([path, init]) => path === `/people/${PERSON_ID}/notes` && (init?.method ?? 'GET') === 'POST',
      )
    }
    expect(posts()).toHaveLength(1)

    // A click and a Ctrl+Enter while still pending both no-op.
    await submit.trigger('click')
    await textarea.trigger('keydown', { key: 'Enter', ctrlKey: true })
    await flushPromises()
    expect(posts()).toHaveLength(1)

    releaseAdd()
    await flushPromises()
    await settleTick()
    expect((wrapper.get('[data-testid="note-composer-textarea"]').element as HTMLTextAreaElement).value).toBe('')
    expect(wrapper.findAll('[data-testid="note-body"]')).toHaveLength(1)
  })

  // LATER batch (2026-09-10) item 5 (015 LATER, reviewer round 2): the
  // test above proves the DOM `disabled` attribute blocks a second
  // trigger, not `submitNote`'s own `noteAddDisabled` guard — vue-test-utils/
  // jsdom never dispatch a `click` to an already-disabled button at all
  // (the button's activation behaviour), so that test would pass exactly
  // the same way even with the guard deleted. This test dispatches two
  // Ctrl+Enter keydowns back-to-back with NO awaited tick between them —
  // before Vue has re-rendered `disabled` on the textarea — so only the
  // reactive guard (TanStack Query flips `isPending` synchronously inside
  // `mutate()`, before any DOM update) can be what stops the second post.
  it('a second submit while the add-note mutation is pending posts nothing (the guard itself, not the disabled attribute)', async () => {
    stubApi(detail([PHONE_A]))
    const { wrapper } = await mountView()
    activeWrapper = wrapper

    let releaseAdd: () => void = () => {}
    const addGate = new Promise<void>((resolve) => { releaseAdd = resolve })
    const defaultImpl = apiFetchMock.getMockImplementation()!
    apiFetchMock.mockImplementation(async (path: string, init?: RequestInit) => {
      if (path === `/people/${PERSON_ID}/notes` && (init?.method ?? 'GET') === 'POST') {
        await addGate
      }
      return defaultImpl(path, init)
    })

    const textarea = wrapper.get('[data-testid="note-composer-textarea"]')
    await textarea.setValue('Called and left a voicemail')

    function posts() {
      return apiFetchMock.mock.calls.filter(
        ([path, init]) => path === `/people/${PERSON_ID}/notes` && (init?.method ?? 'GET') === 'POST',
      )
    }

    const ctrlEnter = { key: 'Enter', ctrlKey: true, bubbles: true } as const
    textarea.element.dispatchEvent(new KeyboardEvent('keydown', ctrlEnter))
    textarea.element.dispatchEvent(new KeyboardEvent('keydown', ctrlEnter))
    await flushPromises()
    expect(posts()).toHaveLength(1)

    releaseAdd()
    await flushPromises()
    await settleTick()
  })

  it('a failed add (400) keeps the draft and shows the note-specific malformed_request copy', async () => {
    stubApi(detail([PHONE_A]), { noteAdd: () => new ApiError(400, 'malformed_request') })
    const { wrapper } = await mountView()
    activeWrapper = wrapper
    const textarea = wrapper.get('[data-testid="note-composer-textarea"]')
    await textarea.setValue('Too long or whatever')
    await wrapper.get('[data-testid="note-composer-submit"]').trigger('click')
    await flushPromises()
    expect(wrapper.get('[data-testid="note-composer-error"]').text()).toBe('Notes are 1–10,000 characters of plain text')
    expect((wrapper.get('[data-testid="note-composer-textarea"]').element as HTMLTextAreaElement).value).toBe('Too long or whatever')
    expect(wrapper.get('[data-testid="note-composer-submit"]').attributes('disabled')).toBeUndefined()
  })

  it('a 404 on add (the Person vanished) uses the page\'s existing not-found handling once the settle refetch lands', async () => {
    stubApi(detail([PHONE_A]), { noteAdd: () => new ApiError(404, 'not_found') })
    const { wrapper } = await mountView()
    activeWrapper = wrapper
    const textarea = wrapper.get('[data-testid="note-composer-textarea"]')
    await textarea.setValue('Should trigger a vanished-person 404')

    // The Person has, in fact, vanished: the detail GET the settle's
    // refetch issues now 404s too. Swap the mock BEFORE the click: the
    // settle refetch is scheduled on a macrotask from `onError`, and
    // `flushPromises` is itself a macrotask, so a swap after the click
    // races the refetch (round-2 confirmation: 1 failure in 3 runs).
    const defaultImpl = apiFetchMock.getMockImplementation()!
    apiFetchMock.mockImplementation(async (path: string, init?: RequestInit) => {
      if (path === `/people/${PERSON_ID}` && (init?.method ?? 'GET') === 'GET') {
        throw new ApiError(404, 'not_found')
      }
      return defaultImpl(path, init)
    })
    await wrapper.get('[data-testid="note-composer-submit"]').trigger('click')
    await flushPromises()
    await settleTick()
    expect(wrapper.text()).toContain('Person not found.')
  })

  it('renders a note row pre-wrap, "Note by <author>", and the edited marker', async () => {
    stubApi(detail([PHONE_A], [
      noteEntry({ id: 'note-1', body: 'Line one\nLine two', edited: false }),
      noteEntry({ id: 'note-2', body: 'Fixed the typo', edited: true, occurredAt: '2026-08-22T09:31:00.000Z' }),
    ]))
    const { wrapper } = await mountView()
    activeWrapper = wrapper
    const bodies = wrapper.findAll('[data-testid="note-body"]')
    expect(bodies[0].text()).toBe('Line one\nLine two')
    expect(bodies[0].classes()).toContain('whitespace-pre-wrap')
    // The "· edited" marker is matched on the SUMMARY paragraph
    // specifically (not the whole row's text, which also contains the
    // body and the actor/time meta line — a body containing the literal
    // word "edited" must never be mistaken for the marker).
    const summaries = wrapper.findAll('[data-testid="history-summary"]').map((p) => p.text())
    expect(summaries).toContain('Note by Alice')
    expect(summaries).toContain('Note by Alice · edited')
  })

  it('a body containing markup renders literally (text interpolation, never v-html) — no img element is created', async () => {
    const hostile = '<img src=x onerror="x">'
    stubApi(detail([PHONE_A], [noteEntry({ id: 'note-1', body: hostile })]))
    const { wrapper } = await mountView()
    activeWrapper = wrapper
    expect(wrapper.get('[data-testid="note-body"]').text()).toBe(hostile)
    expect(wrapper.find('img').exists()).toBe(false)
  })

  it('an imported note with no matched author (actor: null) renders the summary "Note", never "Note by undefined"', async () => {
    stubApi(detail([PHONE_A], [noteEntry({ id: 'note-1', body: 'Imported from FUB', actor: null })]))
    const { wrapper } = await mountView()
    activeWrapper = wrapper
    const summaries = wrapper.findAll('[data-testid="history-summary"]').map((p) => p.text())
    expect(summaries).toContain('Note')
    expect(summaries.some((text) => text.includes('undefined'))).toBe(false)
    // The meta line uses the same null-actor fallback every other history
    // kind already uses ("System"), not a note-specific string.
    expect(wrapper.text()).toContain('System')
  })

  it('Edit/Delete render only where detail.can_manage is true, with 40px targets and accessible names', async () => {
    stubApi(detail([PHONE_A], [
      noteEntry({ id: 'note-mine', body: 'Mine', canManage: true }),
      noteEntry({ id: 'note-theirs', body: 'Not mine', canManage: false, actor: { id: 'u-bob', display_name: 'Bob' } }),
    ]))
    const { wrapper } = await mountView()
    activeWrapper = wrapper

    // A page-wide length-1 count would also pass with INVERTED logic (the
    // buttons attached to the wrong row) — locate each note's own `li` by
    // its body and check inside it specifically.
    const rows = wrapper.findAll('li')
    const mineRow = rows.find((row) => row.find('[data-testid="note-body"]').exists() && row.text().includes('Mine'))
    const theirsRow = rows.find((row) => row.find('[data-testid="note-body"]').exists() && row.text().includes('Not mine'))
    expect(mineRow).toBeTruthy()
    expect(theirsRow).toBeTruthy()

    const edit = mineRow!.get('[data-testid="edit-note"]')
    const del = mineRow!.get('[data-testid="delete-note"]')
    expect(edit.attributes('aria-label')).toBe('Edit note')
    expect(edit.classes()).toContain('h-10')
    expect(edit.classes()).toContain('w-10')
    expect(del.attributes('aria-label')).toBe('Delete note')
    expect(del.classes()).toContain('h-10')
    expect(del.classes()).toContain('w-10')

    expect(theirsRow!.find('[data-testid="edit-note"]').exists()).toBe(false)
    expect(theirsRow!.find('[data-testid="delete-note"]').exists()).toBe(false)

    // And, page-wide, exactly one of each — belonging to the row just
    // checked above.
    expect(wrapper.findAll('[data-testid="edit-note"]')).toHaveLength(1)
    expect(wrapper.findAll('[data-testid="delete-note"]')).toHaveLength(1)
  })

  it('inline edit: Save persists, Cancel discards, Escape cancels and returns focus to Edit', async () => {
    stubApi(detail([PHONE_A], [noteEntry({ id: 'note-1', body: 'Original body' })]))
    const { wrapper } = await mountView()
    activeWrapper = wrapper
    const editButton = wrapper.get('[data-testid="edit-note"]')
    await editButton.trigger('click')
    const textarea = wrapper.get('[data-testid="note-edit-textarea"]')
    expect((textarea.element as HTMLTextAreaElement).value).toBe('Original body')

    // Escape cancels and returns focus to the Edit button.
    await textarea.trigger('keydown', { key: 'Escape' })
    await flushPromises()
    expect(wrapper.find('[data-testid="note-edit-textarea"]').exists()).toBe(false)
    expect(document.activeElement).toBe(wrapper.get('[data-testid="edit-note"]').element)

    // Re-open, edit, Cancel discards.
    await wrapper.get('[data-testid="edit-note"]').trigger('click')
    await wrapper.get('[data-testid="note-edit-textarea"]').setValue('Discarded draft')
    await wrapper.get('[data-testid="note-edit-cancel"]').trigger('click')
    await flushPromises()
    expect(wrapper.get('[data-testid="note-body"]').text()).toBe('Original body')

    // Re-open, edit, Save persists (after the settle refetch).
    await wrapper.get('[data-testid="edit-note"]').trigger('click')
    await wrapper.get('[data-testid="note-edit-textarea"]').setValue('Edited body')
    await wrapper.get('[data-testid="note-edit-save"]').trigger('click')
    await flushPromises()
    await settleTick()
    expect(wrapper.get('[data-testid="note-body"]').text()).toBe('Edited body')
  })

  it('Escape does nothing while a save is pending — the same guard the Cancel button itself uses', async () => {
    stubApi(detail([PHONE_A], [noteEntry({ id: 'note-1', body: 'Original body' })]))
    const { wrapper } = await mountView()
    activeWrapper = wrapper

    let releaseEdit: () => void = () => {}
    const editGate = new Promise<void>((resolve) => { releaseEdit = resolve })
    const defaultImpl = apiFetchMock.getMockImplementation()!
    apiFetchMock.mockImplementation(async (path: string, init?: RequestInit) => {
      if (path === `/people/${PERSON_ID}/notes/note-1` && (init?.method ?? 'GET') === 'PUT') {
        await editGate
      }
      return defaultImpl(path, init)
    })

    await wrapper.get('[data-testid="edit-note"]').trigger('click')
    const textarea = wrapper.get('[data-testid="note-edit-textarea"]')
    await textarea.setValue('Editing…')
    await wrapper.get('[data-testid="note-edit-save"]').trigger('click')
    await flushPromises()
    expect(wrapper.get('[data-testid="note-edit-textarea"]').attributes('disabled')).toBeDefined()

    await wrapper.get('[data-testid="note-edit-textarea"]').trigger('keydown', { key: 'Escape' })
    await flushPromises()
    expect(wrapper.find('[data-testid="note-edit-textarea"]').exists()).toBe(true)

    releaseEdit()
    await flushPromises()
    await settleTick()
  })

  it('an inline edit survives a refetch that removes the note, keeping the draft and explaining it was deleted elsewhere', async () => {
    stubApi(detail([PHONE_A], [noteEntry({ id: 'note-1', body: 'Original body' })]), {
      // Simulate another tab deleting the note the instant this tab's own
      // PUT lands: the server sees it already gone (404), matching real
      // backend behavior (tombstones are invisible, §1 rule 3).
      noteEdit: () => new ApiError(404, 'not_found'),
    })
    const { wrapper } = await mountView()
    activeWrapper = wrapper
    await wrapper.get('[data-testid="edit-note"]').trigger('click')
    await wrapper.get('[data-testid="note-edit-textarea"]').setValue('A draft that will be orphaned')
    await wrapper.get('[data-testid="note-edit-save"]').trigger('click')
    await flushPromises()
    await settleTick()
    expect(wrapper.get('[data-testid="note-edit-gone-message"]').text()).toBe('This note was deleted by someone else.')
    expect((wrapper.get('[data-testid="note-edit-gone-draft"]').element as HTMLTextAreaElement).value).toBe(
      'A draft that will be orphaned',
    )
    await wrapper.get('[data-testid="note-edit-gone-dismiss"]').trigger('click')
    await flushPromises()
    expect(wrapper.find('[data-testid="note-edit-gone"]').exists()).toBe(false)
  })

  // LATER batch (2026-09-10) item 4.
  it('Escape on the "deleted elsewhere" draft textarea does exactly what Dismiss does', async () => {
    stubApi(detail([PHONE_A], [noteEntry({ id: 'note-1', body: 'Original body' })]), {
      noteEdit: () => new ApiError(404, 'not_found'),
    })
    const { wrapper } = await mountView()
    activeWrapper = wrapper
    await wrapper.get('[data-testid="edit-note"]').trigger('click')
    await wrapper.get('[data-testid="note-edit-textarea"]').setValue('A draft that will be orphaned')
    await wrapper.get('[data-testid="note-edit-save"]').trigger('click')
    await flushPromises()
    await settleTick()
    expect(wrapper.find('[data-testid="note-edit-gone"]').exists()).toBe(true)

    await wrapper.get('[data-testid="note-edit-gone-draft"]').trigger('keydown', { key: 'Escape' })
    await flushPromises()
    expect(wrapper.find('[data-testid="note-edit-gone"]').exists()).toBe(false)
  })

  it('a realtime-driven refetch (no mutation in flight) that removes the note being edited shows the same "deleted elsewhere" state', async () => {
    stubApi(detail([PHONE_A], [noteEntry({ id: 'note-1', body: 'Original body' })]))
    const { wrapper, queryClient } = await mountView()
    activeWrapper = wrapper

    await wrapper.get('[data-testid="edit-note"]').trigger('click')
    await wrapper.get('[data-testid="note-edit-textarea"]').setValue('A draft nobody sent yet')

    // Another tab deleted the note; this tab's OWN note_changed-driven
    // refetch (not a mutation of its own) picks that up.
    const defaultImpl = apiFetchMock.getMockImplementation()!
    apiFetchMock.mockImplementation(async (path: string, init?: RequestInit) => {
      if (path === `/people/${PERSON_ID}` && (init?.method ?? 'GET') === 'GET') {
        return detail([PHONE_A])
      }
      return defaultImpl(path, init)
    })
    const putsBefore = apiFetchMock.mock.calls.filter(([, init]) => (init?.method ?? 'GET') === 'PUT').length
    await queryClient.invalidateQueries({ queryKey: queryKeys.person(ORG_ID, PERSON_ID) })
    await flushPromises()

    expect(wrapper.get('[data-testid="note-edit-gone-message"]').text()).toBe('This note was deleted by someone else.')
    expect((wrapper.get('[data-testid="note-edit-gone-draft"]').element as HTMLTextAreaElement).value).toBe(
      'A draft nobody sent yet',
    )
    const putsAfter = apiFetchMock.mock.calls.filter(([, init]) => (init?.method ?? 'GET') === 'PUT').length
    expect(putsAfter).toBe(putsBefore)
  })

  it('delete opens a ConfirmDialog with the exact copy, disables Confirm while pending, and removes the row on success', async () => {
    stubApi(detail([PHONE_A], [noteEntry({ id: 'note-1', body: 'Delete me' })]))
    const { wrapper } = await mountView()
    activeWrapper = wrapper
    await wrapper.get('[data-testid="delete-note"]').trigger('click')
    await flushPromises()
    expect(document.body.textContent).toContain('Delete this note? This cannot be undone.')

    // Hold the DELETE request open so the pending state is observable,
    // then release it and let the normal stub take over again.
    let releaseDelete: () => void = () => {}
    const deleteGate = new Promise<void>((resolve) => { releaseDelete = resolve })
    const defaultImpl = apiFetchMock.getMockImplementation()!
    apiFetchMock.mockImplementation(async (path: string, init?: RequestInit) => {
      if (path === `/people/${PERSON_ID}/notes/note-1` && (init?.method ?? 'GET') === 'DELETE') {
        await deleteGate
      }
      return defaultImpl(path, init)
    })

    const confirmButton = [...document.body.querySelectorAll('button')].find((b) => b.textContent === 'Delete')
    expect(confirmButton).toBeTruthy()
    confirmButton?.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await flushPromises()
    const pendingConfirm = [...document.body.querySelectorAll('button')].find((b) => b.textContent === 'Working…')
    expect(pendingConfirm).toBeTruthy()
    expect(pendingConfirm?.hasAttribute('disabled')).toBe(true)

    releaseDelete()
    await flushPromises()
    await settleTick()
    expect(wrapper.find('[data-testid="note-body"]').exists()).toBe(false)
  })

  // LATER batch (2026-09-10) item 4: after a delete, focus moves to the
  // composer textarea (never falls to the body) — attached: true (the
  // default `attachTo: document.body`) is required for `document.activeElement`
  // to reflect a real focus rather than jsdom's no-op on a detached tree.
  it('after a delete, focus moves to the note composer textarea', async () => {
    stubApi(detail([PHONE_A], [noteEntry({ id: 'note-1', body: 'Delete me' })]))
    const { wrapper } = await mountView()
    activeWrapper = wrapper
    await wrapper.get('[data-testid="delete-note"]').trigger('click')
    await flushPromises()
    const confirmButton = [...document.body.querySelectorAll('button')].find((b) => b.textContent === 'Delete')
    confirmButton?.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await flushPromises()
    await settleTick()
    expect(wrapper.find('[data-testid="note-body"]').exists()).toBe(false)
    expect(document.activeElement).toBe(wrapper.get('[data-testid="note-composer-textarea"]').element)
  })

  it('a Save 403 (role/authorship changed under the viewer) shows the specific copy and removes Edit/Delete once the refetch shows can_manage: false', async () => {
    stubApi(detail([PHONE_A], [noteEntry({ id: 'note-1', body: 'Mine', canManage: true })]), {
      noteEdit: () => new ApiError(403, 'forbidden'),
    })
    const { wrapper } = await mountView()
    activeWrapper = wrapper
    await wrapper.get('[data-testid="edit-note"]').trigger('click')
    await wrapper.get('[data-testid="note-edit-textarea"]').setValue('Trying to save')

    // The role/authorship change that CAUSED the 403 is also what the
    // settle refetch's own GET now reflects.
    const defaultImpl = apiFetchMock.getMockImplementation()!
    apiFetchMock.mockImplementation(async (path: string, init?: RequestInit) => {
      if (path === `/people/${PERSON_ID}` && (init?.method ?? 'GET') === 'GET') {
        return detail([PHONE_A], [noteEntry({ id: 'note-1', body: 'Mine', canManage: false })])
      }
      return defaultImpl(path, init)
    })

    await wrapper.get('[data-testid="note-edit-save"]').trigger('click')
    await flushPromises()
    expect(wrapper.get('[data-testid="note-edit-error"]').text()).toBe('You can no longer edit this note.')

    await settleTick()
    await wrapper.get('[data-testid="note-edit-cancel"]').trigger('click')
    await flushPromises()
    expect(wrapper.find('[data-testid="edit-note"]').exists()).toBe(false)
    expect(wrapper.find('[data-testid="delete-note"]').exists()).toBe(false)
  })

  it('a Delete 404 (deleted elsewhere) explains "Not found." in the dialog, removes the row after the refetch, and Cancel closes it', async () => {
    stubApi(detail([PHONE_A], [noteEntry({ id: 'note-1', body: 'Gone already' })]), {
      noteDelete: () => new ApiError(404, 'not_found'),
    })
    const { wrapper } = await mountView()
    activeWrapper = wrapper
    await wrapper.get('[data-testid="delete-note"]').trigger('click')
    await flushPromises()

    const defaultImpl = apiFetchMock.getMockImplementation()!
    apiFetchMock.mockImplementation(async (path: string, init?: RequestInit) => {
      if (path === `/people/${PERSON_ID}` && (init?.method ?? 'GET') === 'GET') {
        return detail([PHONE_A])
      }
      return defaultImpl(path, init)
    })

    const confirmButton = [...document.body.querySelectorAll('button')].find((b) => b.textContent === 'Delete')
    confirmButton?.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await flushPromises()
    expect(document.body.textContent).toContain('Not found.')

    await settleTick()
    expect(wrapper.find('[data-testid="note-body"]').exists()).toBe(false)

    // PrimeVue renders the button's text with surrounding whitespace from
    // the template's own line breaks/indentation — `.trim()` first,
    // unlike the exact-match "Delete"/"Working…" labels elsewhere, which
    // have no such whitespace.
    const cancelButton = [...document.body.querySelectorAll('button')].find((b) => b.textContent?.trim() === 'Cancel')
    cancelButton?.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await flushPromises()
    expect(document.body.textContent).not.toContain('Delete this note? This cannot be undone.')
  })

  it('a Delete 403 shows the permission copy in the dialog and removes Edit/Delete once the refetch shows can_manage: false', async () => {
    stubApi(detail([PHONE_A], [noteEntry({ id: 'note-1', body: 'Not yours anymore', canManage: true })]), {
      noteDelete: () => new ApiError(403, 'forbidden'),
    })
    const { wrapper } = await mountView()
    activeWrapper = wrapper
    await wrapper.get('[data-testid="delete-note"]').trigger('click')
    await flushPromises()

    const defaultImpl = apiFetchMock.getMockImplementation()!
    apiFetchMock.mockImplementation(async (path: string, init?: RequestInit) => {
      if (path === `/people/${PERSON_ID}` && (init?.method ?? 'GET') === 'GET') {
        return detail([PHONE_A], [noteEntry({ id: 'note-1', body: 'Not yours anymore', canManage: false })])
      }
      return defaultImpl(path, init)
    })

    const confirmButton = [...document.body.querySelectorAll('button')].find((b) => b.textContent === 'Delete')
    confirmButton?.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await flushPromises()
    expect(document.body.textContent).toContain('You do not have permission to do that.')

    await settleTick()
    expect(wrapper.find('[data-testid="edit-note"]').exists()).toBe(false)
    expect(wrapper.find('[data-testid="delete-note"]').exists()).toBe(false)
  })

  it('an unrecognised history kind renders as a generic Activity row instead of throwing', async () => {
    const unknownEntry = {
      kind: 'future_kind',
      id: 'future-1',
      occurred_at: '2026-08-22T09:40:00.000Z',
      recorded_at: '2026-08-22T09:40:00.000Z',
      actor: { id: 'u-alice', display_name: 'Alice' },
      origin: 'web_session',
      correlation_id: 'corr-future',
      detail: {},
    } as unknown as HistoryEntry
    stubApi(detail([PHONE_A], [unknownEntry]))
    const { wrapper } = await mountView()
    activeWrapper = wrapper
    await flushPromises()
    const summaries = wrapper.findAll('[data-testid="history-summary"]').filter((p) => p.text() === 'Activity')
    expect(summaries).toHaveLength(1)
    expect(wrapper.find('[data-testid="note-body"]').exists()).toBe(false)
  })
})

describe('PersonDetailView — Tasks (SLICE_016.md §12.8)', () => {
  let activeWrapper: Awaited<ReturnType<typeof mountView>>['wrapper'] | null = null
  afterEach(async () => {
    activeWrapper?.unmount()
    activeWrapper = null
    await flushPromises()
  })

  // §8's date-only conversion is pinned to two exact instants under a
  // fixed America/New_York clock (spring-forward and fall-back boundaries)
  // — the coordinator-mandated TZ for every date case in this block.
  // `process` is a real Node global at Vitest runtime (not browser-typed
  // by this app's tsconfig, hence the cast) — Node/V8 re-resolve `TZ` on
  // every new `Date`/`Intl.DateTimeFormat`, not just at process start.
  const nodeProcess = (globalThis as unknown as { process: { env: Record<string, string | undefined> } }).process
  let originalTz: string | undefined
  beforeEach(() => { originalTz = nodeProcess.env.TZ; nodeProcess.env.TZ = 'America/New_York' })
  afterEach(() => { nodeProcess.env.TZ = originalTz })

  async function settleTick() {
    await new Promise((resolve) => setTimeout(resolve, 0))
    await flushPromises()
  }

  it('the empty state, an open row with its kind/title/assignee, and Complete/Edit/Delete present only where can_manage', async () => {
    stubApi(detail([PHONE_A]))
    const { wrapper } = await mountView()
    activeWrapper = wrapper
    expect(wrapper.get('[data-testid="task-list-empty"]').text()).toBe('No open tasks.')

    activeWrapper.unmount()
    stubApi(
      detail([PHONE_A], [], [], [
        taskFixture({ id: 'task-mine', title: 'Follow up with Grace', can_manage: true }),
        taskFixture({ id: 'task-not-mine', title: 'Someone else handles this', can_manage: false, assignee: { id: 'u-bob', display_name: 'Bob' } }),
      ]),
    )
    const { wrapper: wrapper2 } = await mountView()
    activeWrapper = wrapper2
    const rows = wrapper2.findAll('[data-testid="task-row"]')
    expect(rows).toHaveLength(2)
    // can_manage asserted PER ROW, never by count (the house rule).
    expect(rows[0]!.find('[data-testid="complete-task"]').exists()).toBe(true)
    expect(rows[0]!.find('[data-testid="edit-task"]').exists()).toBe(true)
    expect(rows[0]!.find('[data-testid="delete-task"]').exists()).toBe(true)
    expect(rows[1]!.find('[data-testid="complete-task"]').exists()).toBe(false)
    expect(rows[1]!.find('[data-testid="edit-task"]').exists()).toBe(false)
    expect(rows[1]!.find('[data-testid="delete-task"]').exists()).toBe(false)
    expect(rows[0]!.get('[data-testid="task-title"]').text()).toBe('Follow up with Grace')
    expect(rows[0]!.get('[data-testid="task-kind-badge"]').text()).toBe('Follow up')
    expect(rows[0]!.get('[data-testid="task-assignee"]').text()).toBe('Alice')
  })

  it('the due badge reads Overdue, Due today, or the date, and an inactive assignee gets the "(inactive)" suffix', async () => {
    vi.useFakeTimers()
    try {
      vi.setSystemTime(new Date('2026-09-10T18:00:00.000Z'))
      stubApi(
        detail([PHONE_A], [], [], [
          taskFixture({ id: 'task-overdue', title: 'Overdue one', due_at: '2026-09-10T10:00:00.000Z' }),
          taskFixture({ id: 'task-today', title: 'Due today one', due_at: '2026-09-10T23:00:00.000Z' }),
          taskFixture({ id: 'task-future', title: 'Future one', due_at: '2026-09-20T12:00:00.000Z' }),
          taskFixture({
            id: 'task-inactive-assignee',
            title: 'Held by a deactivated member',
            assignee: { id: 'u-inactive', display_name: 'Dan' },
          }),
        ]),
        { members: [
          { user_id: 'u-alice', display_name: 'Alice', email: 'alice@acme.test', role: 'member', status: 'active', joined_at: '2026-01-01T00:00:00.000Z', assigned_people_count: 0 },
          { user_id: 'u-inactive', display_name: 'Dan', email: 'dan@acme.test', role: 'member', status: 'inactive', joined_at: '2026-01-01T00:00:00.000Z', assigned_people_count: 0 },
        ] },
      )
      const { wrapper } = await mountView()
      activeWrapper = wrapper
      const rows = wrapper.findAll('[data-testid="task-row"]')
      expect(rows[0]!.get('[data-testid="task-due-badge"]').text()).toBe('Overdue')
      expect(rows[1]!.get('[data-testid="task-due-badge"]').text()).toBe('Due today')
      expect(rows[2]!.find('[data-testid="task-due-badge"]').exists()).toBe(true)
      expect(rows[2]!.get('[data-testid="task-due-badge"]').text()).not.toBe('Overdue')
      expect(rows[2]!.get('[data-testid="task-due-badge"]').text()).not.toBe('Due today')
      expect(rows[3]!.get('[data-testid="task-assignee"]').text()).toBe('Dan (inactive)')
    } finally {
      vi.useRealTimers()
    }
  })

  it('the Add form: disabled while empty/whitespace/pending, kind defaults to Follow up, the assignee picker is limited to active members and defaults to the viewer, and a plain Enter does not submit while Ctrl/Cmd+Enter does', async () => {
    stubApi(detail([PHONE_A]), {
      members: [
        { user_id: 'u-alice', display_name: 'Alice', email: 'alice@acme.test', role: 'member', status: 'active', joined_at: '2026-01-01T00:00:00.000Z', assigned_people_count: 0 },
        { user_id: 'u-bob', display_name: 'Bob', email: 'bob@acme.test', role: 'member', status: 'active', joined_at: '2026-01-01T00:00:00.000Z', assigned_people_count: 0 },
        { user_id: 'u-carol', display_name: 'Carol', email: 'carol@acme.test', role: 'member', status: 'inactive', joined_at: '2026-01-01T00:00:00.000Z', assigned_people_count: 0 },
      ],
    })
    const { wrapper } = await mountView()
    activeWrapper = wrapper
    const submit = wrapper.get('[data-testid="task-add-submit"]')
    expect(submit.attributes('disabled')).toBeDefined()
    const title = wrapper.get('[data-testid="task-add-title"]')
    await title.setValue('   ')
    expect(wrapper.get('[data-testid="task-add-submit"]').attributes('disabled')).toBeDefined()
    await title.setValue('Call the client')
    expect(wrapper.get('[data-testid="task-add-submit"]').attributes('disabled')).toBeUndefined()

    // Kind default and assignee default (the viewer).
    expect(wrapper.get('[data-testid="task-add-kind"]').findComponent(Select).props('modelValue')).toBe('follow_up')
    expect(wrapper.get('[data-testid="task-add-assignee"]').findComponent(Select).props('modelValue')).toBe('u-alice')
    // The assignee picker offers only active members (Carol is inactive).
    expect(wrapper.get('[data-testid="task-add-assignee"]').findComponent(Select).props('options')).toEqual([
      { id: 'u-alice', display_name: 'Alice' },
      { id: 'u-bob', display_name: 'Bob' },
    ])

    function posts() {
      return apiFetchMock.mock.calls.filter(
        ([path, init]) => path === `/people/${PERSON_ID}/tasks` && (init?.method ?? 'GET') === 'POST',
      )
    }
    await title.trigger('keydown', { key: 'Enter' })
    await flushPromises()
    expect(posts()).toHaveLength(0)
    await title.trigger('keydown', { key: 'Enter', ctrlKey: true })
    await flushPromises()
    expect(posts()).toHaveLength(1)
    const firstBody = JSON.parse(String(posts()[0]![1]?.body)) as { assignee_user_id: string | null }
    expect(firstBody.assignee_user_id).toBe('u-alice')

    await title.setValue('Second task')
    await title.trigger('keydown', { key: 'Enter', metaKey: true })
    await flushPromises()
    expect(posts()).toHaveLength(2)
  })

  it('date-only converts to local end of day under America/New_York, spanning both a fall-back and a spring-forward boundary', async () => {
    stubApi(detail([PHONE_A]))
    const { wrapper } = await mountView()
    activeWrapper = wrapper
    await wrapper.get('[data-testid="task-add-title"]').setValue('Fall-back task')
    await wrapper.get('[data-testid="task-add-date"]').setValue('2026-11-01')
    await wrapper.get('[data-testid="task-add-submit"]').trigger('click')
    await flushPromises()
    const firstPost = apiFetchMock.mock.calls.find(
      ([path, init]) => path === `/people/${PERSON_ID}/tasks` && (init?.method ?? 'GET') === 'POST',
    )!
    expect(JSON.parse(String(firstPost[1]?.body)).due_at).toBe('2026-11-02T04:59:59.000Z')

    await wrapper.get('[data-testid="task-add-title"]').setValue('Spring-forward task')
    await wrapper.get('[data-testid="task-add-date"]').setValue('2026-03-08')
    await wrapper.get('[data-testid="task-add-submit"]').trigger('click')
    await flushPromises()
    const secondPost = apiFetchMock.mock.calls.filter(
      ([path, init]) => path === `/people/${PERSON_ID}/tasks` && (init?.method ?? 'GET') === 'POST',
    )[1]!
    expect(JSON.parse(String(secondPost[1]?.body)).due_at).toBe('2026-03-09T03:59:59.000Z')
  })

  it('a date with an explicit time converts using that local time, not end of day', async () => {
    stubApi(detail([PHONE_A]))
    const { wrapper } = await mountView()
    activeWrapper = wrapper
    await wrapper.get('[data-testid="task-add-title"]').setValue('Timed task')
    await wrapper.get('[data-testid="task-add-date"]').setValue('2026-11-01')
    await wrapper.get('[data-testid="task-add-time"]').setValue('09:30')
    await wrapper.get('[data-testid="task-add-submit"]').trigger('click')
    await flushPromises()
    const post = apiFetchMock.mock.calls.find(
      ([path, init]) => path === `/people/${PERSON_ID}/tasks` && (init?.method ?? 'GET') === 'POST',
    )!
    expect(JSON.parse(String(post[1]?.body)).due_at).toBe('2026-11-01T14:30:00.000Z')
  })

  it('opening Edit on a task pre-fills the local date and time from its stored instant', async () => {
    const task = taskFixture({ id: 'task-1', title: 'Prefilled', due_at: '2026-09-10T14:23:07.123Z' })
    stubApi(detail([PHONE_A], [], [], [task]))
    const { wrapper } = await mountView()
    activeWrapper = wrapper
    await wrapper.get('[data-testid="edit-task"]').trigger('click')
    expect((wrapper.get('[data-testid="task-edit-date"]').element as HTMLInputElement).value).toBe(
      '2026-09-10',
    )
    expect((wrapper.get('[data-testid="task-edit-time"]').element as HTMLInputElement).value).toBe(
      '10:23',
    )
  })

  it('clearing the date on Edit sends due_at: null', async () => {
    const task = taskFixture({ id: 'task-1', title: 'Clear my date', due_at: '2026-09-10T14:23:07.123Z' })
    stubApi(detail([PHONE_A], [], [], [task]))
    const { wrapper } = await mountView()
    activeWrapper = wrapper
    await wrapper.get('[data-testid="edit-task"]').trigger('click')
    await wrapper.get('[data-testid="task-edit-date"]').setValue('')
    await wrapper.get('[data-testid="task-edit-save"]').trigger('click')
    await flushPromises()
    const put = apiFetchMock.mock.calls.find(
      ([path, init]) => path === `/people/${PERSON_ID}/tasks/task-1` && (init?.method ?? 'GET') === 'PUT',
    )!
    expect(JSON.parse(String(put[1]?.body)).due_at).toBeNull()
  })

  it('the due badge: boundary between "Due today" and the date, and between "Due today" and "Overdue", under a fixed clock', async () => {
    // Two SEPARATE page loads at two different clock times (rather than
    // one page live-ticking across a time advance, which `dueBadge()`'s
    // plain, non-reactive `new Date()` read does not support and is not
    // a stated requirement): each proves one boundary.
    vi.useFakeTimers()
    try {
      // 2026-09-11T03:30:00Z = 2026-09-10T23:30 local (America/New_York,
      // EDT, UTC-4) — "today" is Sept 10 for the badge's local-day compare.
      vi.setSystemTime(new Date('2026-09-11T03:30:00.000Z'))
      stubApi(
        detail([PHONE_A], [], [], [
          // Local midnight boundary: 03:59:59Z = 23:59:59 local Sept 10
          // (still "today"); 04:00:00Z = 00:00:00 local Sept 11 (a new
          // local day — no longer "today").
          taskFixture({ id: 'task-today', title: 'Still today', due_at: '2026-09-11T03:59:59.000Z' }),
          taskFixture({ id: 'task-tomorrow', title: 'A new local day', due_at: '2026-09-11T04:00:00.000Z' }),
        ]),
      )
      const { wrapper } = await mountView()
      activeWrapper = wrapper
      const rows = wrapper.findAll('[data-testid="task-row"]')
      expect(rows[0]!.get('[data-testid="task-due-badge"]').text()).toBe('Due today')
      expect(rows[1]!.get('[data-testid="task-due-badge"]').text()).not.toBe('Due today')
      expect(rows[1]!.get('[data-testid="task-due-badge"]').text()).not.toBe('Overdue')
      wrapper.unmount()
      activeWrapper = null

      // A fresh load at exactly 04:00:00Z: the first task (due 03:59:59Z)
      // is now in the past — Overdue, not "Due today".
      vi.setSystemTime(new Date('2026-09-11T04:00:00.000Z'))
      stubApi(
        detail([PHONE_A], [], [], [
          taskFixture({ id: 'task-today', title: 'Still today', due_at: '2026-09-11T03:59:59.000Z' }),
          taskFixture({ id: 'task-tomorrow', title: 'A new local day', due_at: '2026-09-11T04:00:00.000Z' }),
        ]),
      )
      const { wrapper: wrapperAfter } = await mountView()
      activeWrapper = wrapperAfter
      const rowsAfter = wrapperAfter.findAll('[data-testid="task-row"]')
      expect(rowsAfter[0]!.get('[data-testid="task-due-badge"]').text()).toBe('Overdue')
    } finally {
      vi.useRealTimers()
    }
  })

  it('editing without touching the date re-sends the stored instant, and an otherwise-untouched Save yields changed: false', async () => {
    const task = taskFixture({ id: 'task-1', title: 'Original title', due_at: '2026-09-10T14:23:07.123Z' })
    stubApi(detail([PHONE_A], [], [], [task]))
    const { wrapper } = await mountView()
    activeWrapper = wrapper
    await wrapper.get('[data-testid="edit-task"]').trigger('click')
    // Retitle only — the date/time inputs are never touched.
    await wrapper.get('[data-testid="task-edit-title"]').setValue('Retitled, date untouched')
    await wrapper.get('[data-testid="task-edit-save"]').trigger('click')
    await flushPromises()
    const put = apiFetchMock.mock.calls.find(
      ([path, init]) => path === `/people/${PERSON_ID}/tasks/task-1` && (init?.method ?? 'GET') === 'PUT',
    )!
    expect(JSON.parse(String(put[1]?.body)).due_at).toBe(task.due_at)

    // Positive control: an entirely untouched Save (no field changed at
    // all) must read `changed: false` from the mock's own field
    // comparison, proving the re-sent instant is byte-identical.
    const callsBeforeSecondSave = apiFetchMock.mock.calls.length
    await wrapper.get('[data-testid="edit-task"]').trigger('click')
    await wrapper.get('[data-testid="task-edit-save"]').trigger('click')
    await flushPromises()
    let putCallIndex = -1
    for (let i = callsBeforeSecondSave; i < apiFetchMock.mock.calls.length; i += 1) {
      const [path, init] = apiFetchMock.mock.calls[i]!
      if (path === `/people/${PERSON_ID}/tasks/task-1` && (init?.method ?? 'GET') === 'PUT') putCallIndex = i
    }
    expect(putCallIndex).toBeGreaterThanOrEqual(0)
    const putBody = JSON.parse(String(apiFetchMock.mock.calls[putCallIndex]![1]?.body))
    expect(putBody.title).toBe('Retitled, date untouched')
    expect(putBody.due_at).toBe(task.due_at)
    expect(putBody.assignee_user_id).toBe('u-alice')
    expect(putBody.kind).toBe(task.kind)
    const putResult = (await apiFetchMock.mock.results[putCallIndex]!.value) as { changed: boolean }
    expect(putResult.changed).toBe(false)
  })

  it('a failed add keeps the draft, and an invalid_assignee 422 shows the exact copy and refetches members', async () => {
    stubApi(detail([PHONE_A]), { taskAdd: () => new ApiError(422, 'invalid_assignee') })
    const { wrapper } = await mountView()
    activeWrapper = wrapper
    function membersGetCount() {
      return apiFetchMock.mock.calls.filter(([path]) => path === '/organization/members').length
    }
    // Captured AFTER mount (which already did its own initial members GET)
    // so a grown count actually proves a REFETCH happened, not merely the
    // page's own load.
    const membersGetCountBefore = membersGetCount()
    await wrapper.get('[data-testid="task-add-title"]').setValue('Should keep this draft')
    await wrapper.get('[data-testid="task-add-submit"]').trigger('click')
    await flushPromises()
    await settleTick()
    expect((wrapper.get('[data-testid="task-add-title"]').element as HTMLInputElement).value).toBe(
      'Should keep this draft',
    )
    expect(wrapper.get('[data-testid="task-add-error"]').text()).toBe('That member is not active')
    expect(membersGetCount()).toBeGreaterThan(membersGetCountBefore)
  })

  it('while an add is pending: the button reads "Adding…" and is disabled, the title input is disabled, and a click plus Ctrl+Enter both no-op (exactly one POST) until it settles', async () => {
    stubApi(detail([PHONE_A]))
    const { wrapper } = await mountView()
    activeWrapper = wrapper
    let releaseAdd: () => void = () => {}
    const gate = new Promise<void>((resolve) => { releaseAdd = resolve })
    const defaultImpl = apiFetchMock.getMockImplementation()!
    apiFetchMock.mockImplementation(async (path: string, init?: RequestInit) => {
      if (path === `/people/${PERSON_ID}/tasks` && (init?.method ?? 'GET') === 'POST') await gate
      return defaultImpl(path, init)
    })
    await wrapper.get('[data-testid="task-add-title"]').setValue('Pending task')
    await wrapper.get('[data-testid="task-add-submit"]').trigger('click')
    await nextTick()
    expect(wrapper.get('[data-testid="task-add-submit"]').text()).toBe('Adding…')
    expect(wrapper.get('[data-testid="task-add-submit"]').attributes('disabled')).toBeDefined()
    expect(wrapper.get('[data-testid="task-add-title"]').attributes('disabled')).toBeDefined()

    await wrapper.get('[data-testid="task-add-submit"]').trigger('click')
    await wrapper.get('[data-testid="task-add-title"]').trigger('keydown', { key: 'Enter', ctrlKey: true })
    await flushPromises()
    const posts = () => apiFetchMock.mock.calls.filter(
      ([path, init]) => path === `/people/${PERSON_ID}/tasks` && (init?.method ?? 'GET') === 'POST',
    )
    expect(posts()).toHaveLength(1)
    releaseAdd()
    await flushPromises()
  })

  it('Edit: Save/Cancel/Escape, focus returns to Edit on cancel, and Cancel/Escape are inert while a save is pending', async () => {
    stubApi(detail([PHONE_A], [], [], [taskFixture({ id: 'task-1', title: 'Editable task' })]))
    const { wrapper } = await mountView()
    activeWrapper = wrapper
    await wrapper.get('[data-testid="edit-task"]').trigger('click')
    expect(wrapper.find('[data-testid="task-edit-title"]').exists()).toBe(true)

    // Escape cancels and returns focus to Edit (re-queried: the Edit
    // button is inside a `v-if` sibling that unmounts and remounts a new
    // DOM node each time editing starts/stops, the note precedent).
    await wrapper.get('[data-testid="task-edit-title"]').trigger('keydown', { key: 'Escape' })
    expect(wrapper.find('[data-testid="task-edit-title"]').exists()).toBe(false)
    expect(document.activeElement).toBe(wrapper.get('[data-testid="edit-task"]').element)

    // Cancel/Escape inert while a save is pending.
    await wrapper.get('[data-testid="edit-task"]').trigger('click')
    let releaseSave: () => void = () => {}
    const gate = new Promise<void>((resolve) => { releaseSave = resolve })
    const defaultImpl = apiFetchMock.getMockImplementation()!
    apiFetchMock.mockImplementation(async (path: string, init?: RequestInit) => {
      if (path === `/people/${PERSON_ID}/tasks/task-1` && (init?.method ?? 'GET') === 'PUT') await gate
      return defaultImpl(path, init)
    })
    await wrapper.get('[data-testid="task-edit-title"]').setValue('Changed while pending')
    await wrapper.get('[data-testid="task-edit-save"]').trigger('click')
    await nextTick()
    expect(wrapper.get('[data-testid="task-edit-cancel"]').attributes('disabled')).toBeDefined()
    await wrapper.get('[data-testid="task-edit-title"]').trigger('keydown', { key: 'Escape' })
    expect(wrapper.find('[data-testid="task-edit-title"]').exists()).toBe(true)
    releaseSave()
    await flushPromises()
  })

  it('Edit-422 (the assignee was deactivated after the members list loaded): the exact copy, the editor stays open with the draft, and members are refetched', async () => {
    stubApi(detail([PHONE_A], [], [], [taskFixture({ id: 'task-1', title: 'Editable task' })]), {
      taskUpdate: () => new ApiError(422, 'invalid_assignee'),
    })
    const { wrapper } = await mountView()
    activeWrapper = wrapper
    function membersGetCount() {
      return apiFetchMock.mock.calls.filter(([path]) => path === '/organization/members').length
    }
    const membersGetCountBefore = membersGetCount()

    await wrapper.get('[data-testid="edit-task"]').trigger('click')
    await wrapper.get('[data-testid="task-edit-title"]').setValue('Still editing')
    await wrapper.get('[data-testid="task-edit-save"]').trigger('click')
    await flushPromises()
    await settleTick()

    expect(wrapper.get('[data-testid="task-edit-error"]').text()).toBe('That member is not active')
    // The editor stays open with the draft (not cleared, not closed).
    expect((wrapper.get('[data-testid="task-edit-title"]').element as HTMLInputElement).value).toBe(
      'Still editing',
    )
    expect(membersGetCount()).toBeGreaterThan(membersGetCountBefore)
  })

  it('Save-403 (the viewer\'s rule-1 verdict changed server-side): the exact copy, then the settle refetch (can_manage now false) closes the editor and removes every control', async () => {
    stubApi(detail([PHONE_A], [], [], [taskFixture({ id: 'task-1', title: 'Editable task', can_manage: true })]), {
      taskUpdate: () => new ApiError(403, 'forbidden'),
    })
    const { wrapper } = await mountView()
    activeWrapper = wrapper

    // The settle's refetch is GATED (not merely swapped) so the inline
    // error can be observed deterministically before that refetch (and
    // the can_manage-false watch it triggers) has a chance to land —
    // `flushPromises()` alone does not reliably order a same-macrotask
    // `setTimeout(fn, 0)` after a prior one (the settlePersonMutation
    // precedent this suite's own `settleTick` helper exists for).
    let releaseGet: () => void = () => {}
    const getGate = new Promise<void>((resolve) => { releaseGet = resolve })
    const defaultImpl = apiFetchMock.getMockImplementation()!
    apiFetchMock.mockImplementation(async (path: string, init?: RequestInit) => {
      if (path === `/people/${PERSON_ID}` && (init?.method ?? 'GET') === 'GET') {
        await getGate
        return detail([PHONE_A], [], [], [
          taskFixture({ id: 'task-1', title: 'Editable task', can_manage: false }),
        ])
      }
      return defaultImpl(path, init)
    })

    await wrapper.get('[data-testid="edit-task"]').trigger('click')
    await wrapper.get('[data-testid="task-edit-title"]').setValue('Should not save')
    await wrapper.get('[data-testid="task-edit-save"]').trigger('click')
    await flushPromises()
    expect(wrapper.get('[data-testid="task-edit-error"]').text()).toBe('You can no longer manage this task.')

    releaseGet()
    await settleTick()
    expect(wrapper.find('[data-testid="task-edit-title"]').exists()).toBe(false)
    expect(wrapper.find('[data-testid="complete-task"]').exists()).toBe(false)
    expect(wrapper.find('[data-testid="edit-task"]').exists()).toBe(false)
    expect(wrapper.find('[data-testid="delete-task"]').exists()).toBe(false)
  })

  it('Save-404 (deleted elsewhere): the editor closes and a dismissible "deleted by someone else" banner appears once the settle refetch omits the task', async () => {
    stubApi(detail([PHONE_A], [], [], [taskFixture({ id: 'task-1', title: 'Editable task' })]), {
      taskUpdate: () => new ApiError(404, 'not_found'),
    })
    const { wrapper } = await mountView()
    activeWrapper = wrapper

    const defaultImpl = apiFetchMock.getMockImplementation()!
    apiFetchMock.mockImplementation(async (path: string, init?: RequestInit) => {
      if (path === `/people/${PERSON_ID}` && (init?.method ?? 'GET') === 'GET') {
        return detail([PHONE_A], [], [], [])
      }
      return defaultImpl(path, init)
    })

    await wrapper.get('[data-testid="edit-task"]').trigger('click')
    await wrapper.get('[data-testid="task-edit-save"]').trigger('click')
    await flushPromises()
    await settleTick()

    expect(wrapper.find('[data-testid="task-edit-title"]').exists()).toBe(false)
    expect(wrapper.get('[data-testid="task-edit-gone-banner"]').text()).toContain(
      'This task was deleted by someone else.',
    )
    expect(wrapper.text()).not.toContain('undefined')

    await wrapper.get('[data-testid="task-edit-gone-dismiss"]').trigger('click')
    expect(wrapper.find('[data-testid="task-edit-gone-banner"]').exists()).toBe(false)
  })

  it('Delete: ConfirmDialog with the exact copy, confirm disabled while pending, and the row is gone after the refetch', async () => {
    stubApi(detail([PHONE_A], [], [], [taskFixture({ id: 'task-1', title: 'Delete me' })]))
    const { wrapper } = await mountView()
    activeWrapper = wrapper
    await wrapper.get('[data-testid="delete-task"]').trigger('click')
    await flushPromises()
    // PrimeVue's Dialog teleports its content to <body>, outside the
    // wrapper's own subtree (the note-delete precedent).
    expect(document.body.textContent).toContain('Delete this task? This cannot be undone.')

    let releaseDelete: () => void = () => {}
    const gate = new Promise<void>((resolve) => { releaseDelete = resolve })
    const defaultImpl = apiFetchMock.getMockImplementation()!
    apiFetchMock.mockImplementation(async (path: string, init?: RequestInit) => {
      if (path === `/people/${PERSON_ID}/tasks/task-1` && (init?.method ?? 'GET') === 'DELETE') await gate
      return defaultImpl(path, init)
    })
    const confirmButton = [...document.body.querySelectorAll('button')].find((b) => b.textContent === 'Delete')
    expect(confirmButton).toBeTruthy()
    confirmButton?.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await flushPromises()
    const pendingConfirm = [...document.body.querySelectorAll('button')].find((b) => b.textContent === 'Working…')
    expect(pendingConfirm).toBeTruthy()
    expect(pendingConfirm?.hasAttribute('disabled')).toBe(true)
    releaseDelete()
    await flushPromises()
    await settleTick()
    expect(wrapper.find('[data-testid="task-row"]').exists()).toBe(false)
  })

  it('Complete moves the task to History as "Completed task: <title>" with a Reopen control where can_manage, and Reopen brings it back to the open list', async () => {
    stubApi(detail([PHONE_A], [], [], [taskFixture({ id: 'task-1', title: 'Call the client', kind: 'call', due_at: '2026-09-10T18:00:00.000Z' })]))
    const { wrapper } = await mountView()
    activeWrapper = wrapper
    await wrapper.get('[data-testid="complete-task"]').trigger('click')
    await flushPromises()
    await settleTick()
    expect(wrapper.find('[data-testid="task-row"]').exists()).toBe(false)
    const summaries = wrapper.findAll('[data-testid="history-summary"]')
    expect(summaries.some((s) => s.text() === 'Completed task: Call the client')).toBe(true)
    expect(wrapper.get('[data-testid="task-completed-detail"]').text()).toContain('Call')
    expect(wrapper.get('[data-testid="task-completed-detail"]').text()).toContain('was due')

    await wrapper.get('[data-testid="reopen-task"]').trigger('click')
    await flushPromises()
    await settleTick()
    expect(wrapper.get('[data-testid="task-row"]').get('[data-testid="task-title"]').text()).toBe('Call the client')
    expect(wrapper.findAll('[data-testid="history-summary"]').some((s) => s.text() === 'Completed task: Call the client')).toBe(false)
  })

  it('a 403 on Complete (the viewer\'s rule-1 verdict changed server-side since the last read) explains inline, then the settle refetch (can_manage now false) removes every control', async () => {
    // can_manage: true at read time (the Complete button renders), but the
    // server's own re-decision under the row lock says otherwise by the
    // time the click lands — the realistic race this copy exists for.
    stubApi(detail([PHONE_A], [], [], [taskFixture({ id: 'task-1', title: 'Race target', can_manage: true })]), {
      taskComplete: () => new ApiError(403, 'forbidden'),
    })
    const { wrapper } = await mountView()
    activeWrapper = wrapper

    // Round-2 review, item 9: swap the GET so the settle refetch this
    // 403 triggers shows the row's own re-decision (can_manage: false) —
    // the note precedent (db_notes.rs's "Delete 403 ... removes Edit/
    // Delete once the refetch shows can_manage: false").
    const defaultImpl = apiFetchMock.getMockImplementation()!
    apiFetchMock.mockImplementation(async (path: string, init?: RequestInit) => {
      if (path === `/people/${PERSON_ID}` && (init?.method ?? 'GET') === 'GET') {
        return detail([PHONE_A], [], [], [
          taskFixture({ id: 'task-1', title: 'Race target', can_manage: false }),
        ])
      }
      return defaultImpl(path, init)
    })

    await wrapper.get('[data-testid="complete-task"]').trigger('click')
    await flushPromises()
    expect(wrapper.get('[data-testid="task-action-error"]').text()).toBe('You can no longer manage this task.')
    await settleTick()
    expect(wrapper.find('[data-testid="complete-task"]').exists()).toBe(false)
    expect(wrapper.find('[data-testid="edit-task"]').exists()).toBe(false)
    expect(wrapper.find('[data-testid="delete-task"]').exists()).toBe(false)
  })

  it('a 404 on Complete (deleted elsewhere) refetches and the row is gone', async () => {
    stubApi(detail([PHONE_A], [], [], [taskFixture({ id: 'task-1', title: 'Gone target', can_manage: true })]), {
      taskComplete: () => new ApiError(404, 'not_found'),
    })
    const { wrapper } = await mountView()
    activeWrapper = wrapper
    await wrapper.get('[data-testid="complete-task"]').trigger('click')
    await flushPromises()
    await settleTick()
    expect(wrapper.find('[data-testid="task-row"]').exists()).toBe(false)
  })

  it('a literal-markup title renders as text with no element, and an imported task_completed row (actor: null) renders without "undefined"', async () => {
    stubApi(
      detail([PHONE_A], [
        taskCompletedEntry({
          id: 'task-imported',
          title: 'Imported task',
          actor: null,
          assignee: null,
          createdBy: null,
          canManage: false,
        }),
      ], [], [
        taskFixture({ id: 'task-xss', title: '<img src=x onerror=alert(1)>' }),
      ]),
    )
    const { wrapper } = await mountView()
    activeWrapper = wrapper
    expect(wrapper.get('[data-testid="task-title"]').text()).toBe('<img src=x onerror=alert(1)>')
    expect(wrapper.find('[data-testid="task-title"] img').exists()).toBe(false)
    const importedSummary = wrapper.findAll('[data-testid="history-summary"]').find((s) => s.text().includes('Imported task'))!
    const importedRow = importedSummary.element.closest('li')!
    expect(importedRow.textContent).not.toContain('undefined')
    expect(importedRow.textContent).toContain('System')
    // `can_manage: false` on the only task_completed row: no Reopen anywhere.
    expect(wrapper.find('[data-testid="reopen-task"]').exists()).toBe(false)
  })

  it('mixed History: two task_completed rows with different can_manage each carry (or lack) their OWN Reopen control, asserted per row', async () => {
    stubApi(
      detail([PHONE_A], [
        taskCompletedEntry({ id: 'task-manageable', title: 'Manageable one', canManage: true }),
        taskCompletedEntry({ id: 'task-not-manageable', title: 'Not manageable one', canManage: false }),
      ]),
    )
    const { wrapper } = await mountView()
    activeWrapper = wrapper
    const manageableSummary = wrapper
      .findAll('[data-testid="history-summary"]')
      .find((s) => s.text().includes('Manageable one'))!
    const notManageableSummary = wrapper
      .findAll('[data-testid="history-summary"]')
      .find((s) => s.text().includes('Not manageable one'))!
    const manageableRow = manageableSummary.element.closest('li')!
    const notManageableRow = notManageableSummary.element.closest('li')!
    expect(manageableRow.querySelector('[data-testid="reopen-task"]')).toBeTruthy()
    expect(notManageableRow.querySelector('[data-testid="reopen-task"]')).toBeFalsy()
  })

  it('an open task with a null assignee (and null created_by) renders "Unassigned" with no "undefined"', async () => {
    stubApi(
      detail([PHONE_A], [], [], [
        taskFixture({ id: 'task-unassigned', title: 'Nobody has this yet', assignee: null, created_by: null }),
      ]),
    )
    const { wrapper } = await mountView()
    activeWrapper = wrapper
    expect(wrapper.get('[data-testid="task-assignee"]').text()).toBe('Unassigned')
    expect(wrapper.text()).not.toContain('undefined')
  })

  it('mutations are keyed with personMutationKey and settle by invalidating the person and today keys, never the People list', async () => {
    stubApi(detail([PHONE_A]))
    const { wrapper, queryClient } = await mountView()
    activeWrapper = wrapper
    vi.useFakeTimers()
    try {
      let releaseAdd: () => void = () => {}
      const gate = new Promise<void>((resolve) => { releaseAdd = resolve })
      const defaultImpl = apiFetchMock.getMockImplementation()!
      apiFetchMock.mockImplementation(async (path: string, init?: RequestInit) => {
        if (path === `/people/${PERSON_ID}/tasks` && (init?.method ?? 'GET') === 'POST') await gate
        return defaultImpl(path, init)
      })
      await wrapper.get('[data-testid="task-add-title"]').setValue('Keyed task')
      await wrapper.get('[data-testid="task-add-submit"]').trigger('click')
      await nextTick()
      expect(queryClient.isMutating({ mutationKey: personMutationKey(ORG_ID, PERSON_ID) })).toBe(1)
      const invalidateSpy = vi.spyOn(queryClient, 'invalidateQueries')
      releaseAdd()
      await nextTick()
      await nextTick()
      await vi.advanceTimersByTimeAsync(0)
      await nextTick()
      expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: queryKeys.person(ORG_ID, PERSON_ID) })
      expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: queryKeys.today(ORG_ID) })
      expect(invalidateSpy).not.toHaveBeenCalledWith({ queryKey: queryKeys.people(ORG_ID) })
    } finally {
      vi.useRealTimers()
    }
  })

  it('an unknown history kind (an older bundle seeing a future kind) still renders the generic Activity row alongside a task_completed row', async () => {
    stubApi(
      detail([PHONE_A], [
        { ...taskCompletedEntry({ id: 'task-1', title: 'A completed task' }) },
        { kind: 'made_up_future_kind', id: 'x', occurred_at: '2026-08-22T09:00:00.000Z', recorded_at: '2026-08-22T09:00:00.000Z', actor: null, origin: 'web_session', correlation_id: 'c', detail: {} } as unknown as HistoryEntry,
      ]),
    )
    const { wrapper } = await mountView()
    activeWrapper = wrapper
    const summaries = wrapper.findAll('[data-testid="history-summary"]')
    expect(summaries.some((s) => s.text() === 'Activity')).toBe(true)
    expect(summaries.some((s) => s.text() === 'Completed task: A completed task')).toBe(true)
  })
})


describe('Person review-only workspace', () => {
  it('renders contacts, assignment and import history without offering ordinary writes or outbound actions', async () => {
    const identity = me(); identity.organization = { ...identity.organization!, role: 'admin', workspace_mode: 'migration_review', workspace_revision: '2' }
    const imported: HistoryEntry = { id: 'import-fact', kind: 'person_imported', occurred_at: '2026-09-11T12:00:00Z', recorded_at: '2026-09-11T12:00:00Z', actor: null, origin: 'migration', correlation_id: 'import', detail: { import_id: 'import', plan_id: 'plan', source_record_id: 'record', capture_id: 'capture', on_behalf_of_user_id: 'actor' } }
    stubApi(detail([PHONE_A, EMAIL], [imported], [{ id: 'tag', name: 'Imported' }]), { meOverride: identity })
    const { wrapper } = await mountView()
    expect(wrapper.text()).toContain('Person imported from Follow Up Boss')
    expect(wrapper.text()).toContain(EMAIL.value)
    expect(wrapper.text()).toContain('No inquiries')
    for (const id of ['log-contact', 'call-button', 'add-tag-button', 'remove-person-tag', 'task-add-form', 'note-composer']) expect(wrapper.find(`[data-testid="${id}"]`).exists(), id).toBe(false)
    for (const select of wrapper.findAllComponents(Select)) expect(select.props('disabled')).toBe(true)
    expect(requests().filter(request => !request.startsWith('GET '))).toEqual([])
    expect(requests()).toContain(`GET /people/${PERSON_ID}/import-provenance`)
    wrapper.unmount()
  })
})
