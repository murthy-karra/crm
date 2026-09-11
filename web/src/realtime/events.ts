// Wire shapes and the invalidation mapping for the realtime events frozen in
// docs/specs/SLICE_003.md §6 (D-023). Events are ids-only invalidation
// hints — never state, never PII — so every handler here does is decide
// which TanStack Query keys to invalidate; the actual refetch goes through
// the normal authenticated API (D-011).
import type { QueryKey } from '@tanstack/vue-query'
import { queryKeys } from '../api/queries'

// SLICE_009 §6's declared additive variant: no new event type — the
// `person.changed` handler below already invalidates person/people/today
// for every change value, so this widening needs no new case. SLICE_011e §5
// adds `tags_changed` the same way (published only on a changing
// add/remove; rename/delete publish nothing).
// SLICE_015 §5 adds `note_changed`: published on a committed add, a
// changing edit, and a delete — never on `changed: false`. Unlike the
// other variants, it deliberately does NOT fall into the wide default
// below (rule 5: a note changes no People row, Today queue, or list
// count) — see the dedicated branch in the `person.changed` case.
// SLICE_016.md §6 adds `task_changed` the same way: published on create, a
// changing update, complete, reopen, snooze, and delete — never on
// `changed: false`. A due task changes the viewer's Today, so (unlike
// `note_changed`) it invalidates `queryKeys.today` too, but still never
// People or list counts — see the dedicated branch below.
// SLICE_019.md §5 adds `custom_field_changed`: published after a set/clear
// that changed a row — never on `changed: false`, never on a definition/
// option write (rename/archive/restore publish nothing, the tag-rename
// precedent). A value touches nothing but the Person detail (§5), so it
// falls into the same narrow `note_changed`-style branch, not the wide
// default.
export type PersonChange =
  | 'inquiry_received'
  | 'assignment_changed'
  | 'stage_changed'
  | 'contact_attempted'
  | 'correspondence_captured'
  | 'tags_changed'
  | 'note_changed'
  | 'task_changed'
  | 'custom_field_changed'

interface RealtimeEnvelopeBase {
  v: 1
  organization_id: string
  occurred_at: string
  correlation_id: string
}

export interface PersonChangedEvent extends RealtimeEnvelopeBase {
  type: 'person.changed'
  data: { person_id: string; change: PersonChange }
}

export interface IntakeUnresolvedChangedEvent extends RealtimeEnvelopeBase {
  type: 'intake.unresolved_changed'
  data: { raw_payload_id: string }
}

/** SLICE_006 §6: the additive third event type — ids only, published after
 * every committed call transition. Invalidation only (D-023): in-call state
 * comes from LiveKit, never from this event. */
export interface CallChangedEvent extends RealtimeEnvelopeBase {
  type: 'call.changed'
  data: { call_id: string; person_id: string }
}

/** The known event shapes (§6). `invalidationsFor` accepts `unknown`, not
 * this union, because the wire payload must also tolerate an unrecognized
 * `type` (future additive event) or a malformed body without throwing —
 * §6: "Unknown type → ignored." */
export type RealtimeEvent = PersonChangedEvent | IntakeUnresolvedChangedEvent | CallChangedEvent

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null
}

/**
 * Maps one realtime event to the TanStack Query keys it invalidates
 * (SLICE_003 §6 "Client invalidation mapping", exact). Every key comes from
 * the `queryKeys` factory (api/queries.ts) — never hand-written here — so a
 * key shape can only drift in one place.
 *
 * Defense in depth (§7): an event whose `organization_id` does not match the
 * viewer's own is dropped with `console.warn` rather than trusted, even
 * though the server-side channel subscription (D-023 §1) should make this
 * impossible.
 */
export function invalidationsFor(event: unknown, orgId: string): QueryKey[] {
  if (!isRecord(event) || typeof event.type !== 'string' || typeof event.organization_id !== 'string') {
    return []
  }

  if (event.organization_id !== orgId) {
    console.warn(
      `realtime: dropped ${event.type} event for organization ${event.organization_id} (connected as ${orgId})`,
    )
    return []
  }

  const data = isRecord(event.data) ? event.data : {}

  switch (event.type) {
    case 'person.changed': {
      const personId = typeof data.person_id === 'string' ? data.person_id : ''
      if (personId === '') return []
      // SLICE_015 §5 rule 5/6: a note changes no People row, Today queue,
      // or list count, so the wide default below would make every
      // connected tab refetch three query families per note. Restrict to
      // the Person detail only. Older bundles that don't recognize
      // `note_changed` fall through to the wide default, which is safe
      // (over-invalidation, never under-invalidation).
      // SLICE_019.md §5, §9: a custom-field value change is the same
      // narrow case as `note_changed` — it touches no People row, Today
      // queue, or list count (a custom field never appears in the filter
      // vocabulary in 019a).
      if (data.change === 'note_changed' || data.change === 'custom_field_changed') {
        return [queryKeys.person(orgId, personId)]
      }
      // SLICE_016.md §6: a due task changes the viewer's Today, so this
      // gets `queryKeys.today` in addition to the Person detail — but
      // still never People or list counts (rule 6: tasks don't appear on
      // People rows). 016b: also the tasks prefix (the Today page's Tasks
      // panel), whole-prefix since the changed task's assignee is not
      // necessarily this viewer (a reassignment moves it between two
      // members' panel caches at once).
      if (data.change === 'task_changed') {
        return [queryKeys.person(orgId, personId), queryKeys.today(orgId), queryKeys.tasks(orgId)]
      }
      const keys: QueryKey[] = [
        queryKeys.person(orgId, personId),
        queryKeys.people(orgId),
        queryKeys.today(orgId),
        // Saved-list counts are current query results. No saved-list id or
        // criteria travels on the Organization realtime channel.
        queryKeys.savedListCounts(orgId),
      ]
      // §6: a re-POST that resolves a `pending` row removes it from the
      // unresolved queue but publishes only `person.changed` — so the
      // `inquiry_received` change also invalidates the unresolved list.
      // SLICE_011a M11 (adversarial-review follow-up): the same change can
      // introduce a brand-new inquiry source, so it also invalidates the
      // FilterBar's Source picker — `queryKeys.inquirySources` lives
      // outside the `queryKeys.people` prefix and would otherwise go
      // stale until an unrelated refetch.
      if (data.change === 'inquiry_received') {
        keys.push(queryKeys.unresolved(orgId))
        keys.push(queryKeys.inquirySources(orgId))
      }
      return keys
    }
    case 'intake.unresolved_changed':
      return [queryKeys.unresolved(orgId)]
    case 'call.changed': {
      // SLICE_006 §6, exact: ['org', orgId, 'call', callId] and
      // ['org', orgId, 'person', personId]. Today/People are covered by the
      // separate `person.changed{contact_attempted}` the attempt publishes.
      const callId = typeof data.call_id === 'string' ? data.call_id : ''
      const personId = typeof data.person_id === 'string' ? data.person_id : ''
      if (callId === '' || personId === '') return []
      return [queryKeys.call(orgId, callId), queryKeys.person(orgId, personId)]
    }
    default:
      return []
  }
}

/** Recovery invalidation (§6, §9, D-011): every `connected` after a prior
 * disconnect invalidates everything under the Organization, since events
 * missed while disconnected are never replayed (no Centrifugo history). */
export function reconnectInvalidations(orgId: string): QueryKey[] {
  return [queryKeys.org(orgId)]
}
