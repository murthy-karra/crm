// Wire shapes and the invalidation mapping for the realtime events frozen in
// docs/specs/SLICE_003.md §6 (D-023). Events are ids-only invalidation
// hints — never state, never PII — so every handler here does is decide
// which TanStack Query keys to invalidate; the actual refetch goes through
// the normal authenticated API (D-011).
import type { QueryKey } from '@tanstack/vue-query'
import { queryKeys } from '../api/queries'

// SLICE_009 §6's declared additive variant: no new event type — the
// `person.changed` handler below already invalidates person/people/today
// for every change value, so this widening needs no new case.
export type PersonChange =
  | 'inquiry_received'
  | 'assignment_changed'
  | 'stage_changed'
  | 'contact_attempted'
  | 'correspondence_captured'

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
      const keys: QueryKey[] = [queryKeys.person(orgId, personId), queryKeys.people(orgId), queryKeys.today(orgId)]
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
