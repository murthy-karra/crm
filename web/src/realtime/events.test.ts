import { QueryClient } from '@tanstack/vue-query'
import { describe, expect, it, vi } from 'vitest'
import { queryKeys } from '../api/queries'
import { invalidationsFor, reconnectInvalidations, type PersonChange } from './events'

const ORG_ID = '11111111-1111-1111-1111-111111111111'
const OTHER_ORG_ID = '22222222-2222-2222-2222-222222222222'
const PERSON_ID = '33333333-3333-3333-3333-333333333333'
const RAW_PAYLOAD_ID = '44444444-4444-4444-4444-444444444444'
const CALL_ID = '55555555-5555-5555-5555-555555555555'

function personChanged(change: PersonChange, organizationId = ORG_ID) {
  return {
    v: 1,
    type: 'person.changed',
    organization_id: organizationId,
    occurred_at: '2026-08-21T18:02:11.512Z',
    correlation_id: 'corr-1',
    data: { person_id: PERSON_ID, change },
  }
}

describe('invalidationsFor', () => {
  it('maps assignment_changed to person, people, today, and saved-list counts', () => {
    expect(invalidationsFor(personChanged('assignment_changed'), ORG_ID)).toEqual([
      queryKeys.person(ORG_ID, PERSON_ID),
      queryKeys.people(ORG_ID),
      queryKeys.today(ORG_ID),
      queryKeys.savedListCounts(ORG_ID),
    ])
  })

  it('maps stage_changed to person, people, today, and saved-list counts', () => {
    expect(invalidationsFor(personChanged('stage_changed'), ORG_ID)).toEqual([
      queryKeys.person(ORG_ID, PERSON_ID),
      queryKeys.people(ORG_ID),
      queryKeys.today(ORG_ID),
      queryKeys.savedListCounts(ORG_ID),
    ])
  })

  it('maps contact_attempted to person, people, today, and saved-list counts', () => {
    expect(invalidationsFor(personChanged('contact_attempted'), ORG_ID)).toEqual([
      queryKeys.person(ORG_ID, PERSON_ID),
      queryKeys.people(ORG_ID),
      queryKeys.today(ORG_ID),
      queryKeys.savedListCounts(ORG_ID),
    ])
  })

  it('maps correspondence_captured to person, people, today, and saved-list counts', () => {
    expect(invalidationsFor(personChanged('correspondence_captured'), ORG_ID)).toEqual([
      queryKeys.person(ORG_ID, PERSON_ID),
      queryKeys.people(ORG_ID),
      queryKeys.today(ORG_ID),
      queryKeys.savedListCounts(ORG_ID),
    ])
  })

  // SLICE_011e §5, §7: `tags_changed` is additive on the existing
  // `person.changed` event — no new case needed, since this handler already
  // invalidates person/people/today/saved-list counts for every change value.
  it('maps tags_changed to person, people, today, and saved-list counts', () => {
    expect(invalidationsFor(personChanged('tags_changed'), ORG_ID)).toEqual([
      queryKeys.person(ORG_ID, PERSON_ID),
      queryKeys.people(ORG_ID),
      queryKeys.today(ORG_ID),
      queryKeys.savedListCounts(ORG_ID),
    ])
  })

  // SLICE_015 §5, rule 5/6: unlike every other `person.changed` variant
  // above, `note_changed` invalidates ONLY the Person detail — a note
  // changes no People row, Today queue, or list count, so the wide
  // default would make every connected tab refetch three query families
  // per note.
  it('maps note_changed to the Person detail only', () => {
    expect(invalidationsFor(personChanged('note_changed'), ORG_ID)).toEqual([
      queryKeys.person(ORG_ID, PERSON_ID),
    ])
  })

  // SLICE_016.md §6, rule 5/6: `task_changed` invalidates the Person
  // detail, Today (a due task changes the viewer's Today) AND (016b) the
  // Tasks-panel prefix — but, unlike every full `person.changed` variant
  // above, never People or list counts. Full-array `toEqual` pins ALL
  // three claims at once: an accidental extra key (People, saved-list
  // counts) or a missing one (Today, the tasks prefix) would both fail
  // this exact assertion, where a `.some(...)`/subset check would only
  // catch the second. `queryKeys.tasks(ORG_ID)` (no actorId) is the whole
  // prefix — every actor's panel cache under this Organization.
  it('maps task_changed to person, today and the tasks prefix only, never people or list counts', () => {
    expect(invalidationsFor(personChanged('task_changed'), ORG_ID)).toEqual([
      queryKeys.person(ORG_ID, PERSON_ID),
      queryKeys.today(ORG_ID),
      queryKeys.tasks(ORG_ID),
    ])
  })

  // The note precedent's own claim, made explicit here so the two
  // additive kinds are pinned side by side: `note_changed` does NOT
  // include `today` (a note changes no Today reason).
  it('note_changed does not include today (unlike task_changed)', () => {
    const keys = invalidationsFor(personChanged('note_changed'), ORG_ID)
    expect(keys).not.toContainEqual(queryKeys.today(ORG_ID))
  })

  it('maps inquiry_received to person, people, today, saved-list counts, unresolved, and inquiry sources', () => {
    expect(invalidationsFor(personChanged('inquiry_received'), ORG_ID)).toEqual([
      queryKeys.person(ORG_ID, PERSON_ID),
      queryKeys.people(ORG_ID),
      queryKeys.today(ORG_ID),
      queryKeys.savedListCounts(ORG_ID),
      queryKeys.unresolved(ORG_ID),
      queryKeys.inquirySources(ORG_ID),
    ])
  })

  it('maps intake.unresolved_changed to just unresolved', () => {
    const event = {
      v: 1,
      type: 'intake.unresolved_changed',
      organization_id: ORG_ID,
      occurred_at: '2026-08-21T18:02:11.512Z',
      correlation_id: 'corr-2',
      data: { raw_payload_id: RAW_PAYLOAD_ID },
    }
    expect(invalidationsFor(event, ORG_ID)).toEqual([queryKeys.unresolved(ORG_ID)])
  })

  // SLICE_006 §6 / §13 item 4: `invalidationsFor('call.changed')`.
  it('maps call.changed to exactly the call key and the person key', () => {
    const event = {
      v: 1,
      type: 'call.changed',
      organization_id: ORG_ID,
      occurred_at: '2026-08-22T10:00:00.000Z',
      correlation_id: 'corr-4',
      data: { call_id: CALL_ID, person_id: PERSON_ID },
    }
    expect(invalidationsFor(event, ORG_ID)).toEqual([
      queryKeys.call(ORG_ID, CALL_ID),
      queryKeys.person(ORG_ID, PERSON_ID),
    ])
  })

  it('ignores a call.changed event missing either id', () => {
    const base = {
      v: 1,
      type: 'call.changed',
      organization_id: ORG_ID,
      occurred_at: '2026-08-22T10:00:00.000Z',
      correlation_id: 'corr-5',
    }
    expect(invalidationsFor({ ...base, data: { call_id: CALL_ID } }, ORG_ID)).toEqual([])
    expect(invalidationsFor({ ...base, data: { person_id: PERSON_ID } }, ORG_ID)).toEqual([])
  })

  it('drops a foreign-Organization call.changed event', () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {})
    const event = {
      v: 1,
      type: 'call.changed',
      organization_id: OTHER_ORG_ID,
      occurred_at: '2026-08-22T10:00:00.000Z',
      correlation_id: 'corr-6',
      data: { call_id: CALL_ID, person_id: PERSON_ID },
    }
    expect(invalidationsFor(event, ORG_ID)).toEqual([])
    expect(warn).toHaveBeenCalledTimes(1)
    warn.mockRestore()
  })

  it('ignores an unknown event type without warning', () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {})
    const event = {
      v: 1,
      type: 'call.ended',
      organization_id: ORG_ID,
      occurred_at: '2026-08-21T18:02:11.512Z',
      correlation_id: 'corr-3',
      data: {},
    }
    expect(invalidationsFor(event, ORG_ID)).toEqual([])
    expect(warn).not.toHaveBeenCalled()
    warn.mockRestore()
  })

  it('drops a foreign-Organization event and warns (§6, §7 defense in depth)', () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {})
    expect(invalidationsFor(personChanged('inquiry_received', OTHER_ORG_ID), ORG_ID)).toEqual([])
    expect(warn).toHaveBeenCalledTimes(1)
    expect(warn.mock.calls[0]?.[0]).toContain(OTHER_ORG_ID)
    warn.mockRestore()
  })

  it('ignores a malformed event body rather than throwing', () => {
    expect(invalidationsFor(null, ORG_ID)).toEqual([])
    expect(invalidationsFor(undefined, ORG_ID)).toEqual([])
    expect(invalidationsFor('not an object', ORG_ID)).toEqual([])
    expect(invalidationsFor({ type: 'person.changed', organization_id: ORG_ID }, ORG_ID)).toEqual([])
  })

  // SLICE_011b_SORT.md §9: the sort element only ever extends the factory —
  // `queryKeys.people(ORG_ID)` is still a valid PREFIX of a sorted key, so
  // every existing `person.changed` invalidation keeps covering sorted
  // People caches with zero changes to this mapping.
  it('invalidates a cached sorted People query through the unqualified people prefix', () => {
    const queryClient = new QueryClient()
    const sortedKey = queryKeys.people(ORG_ID, undefined, 'name.asc')
    queryClient.setQueryData(sortedKey, { people: [], truncated: false })
    expect(queryClient.getQueryState(sortedKey)?.isInvalidated).toBe(false)

    for (const key of invalidationsFor(personChanged('stage_changed'), ORG_ID)) {
      void queryClient.invalidateQueries({ queryKey: key })
    }

    expect(queryClient.getQueryState(sortedKey)?.isInvalidated).toBe(true)
  })
})

describe('reconnectInvalidations', () => {
  it('invalidates everything under the Organization', () => {
    expect(reconnectInvalidations(ORG_ID)).toEqual([queryKeys.org(ORG_ID)])
  })
})
