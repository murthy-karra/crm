import { describe, expect, it, vi } from 'vitest'
import { queryKeys } from '../api/queries'
import { invalidationsFor, reconnectInvalidations, type PersonChange } from './events'

const ORG_ID = '11111111-1111-1111-1111-111111111111'
const OTHER_ORG_ID = '22222222-2222-2222-2222-222222222222'
const PERSON_ID = '33333333-3333-3333-3333-333333333333'
const RAW_PAYLOAD_ID = '44444444-4444-4444-4444-444444444444'

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
  it('maps assignment_changed to person, people, and today', () => {
    expect(invalidationsFor(personChanged('assignment_changed'), ORG_ID)).toEqual([
      queryKeys.person(ORG_ID, PERSON_ID),
      queryKeys.people(ORG_ID),
      queryKeys.today(ORG_ID),
    ])
  })

  it('maps stage_changed to person, people, and today', () => {
    expect(invalidationsFor(personChanged('stage_changed'), ORG_ID)).toEqual([
      queryKeys.person(ORG_ID, PERSON_ID),
      queryKeys.people(ORG_ID),
      queryKeys.today(ORG_ID),
    ])
  })

  it('maps contact_attempted to person, people, and today', () => {
    expect(invalidationsFor(personChanged('contact_attempted'), ORG_ID)).toEqual([
      queryKeys.person(ORG_ID, PERSON_ID),
      queryKeys.people(ORG_ID),
      queryKeys.today(ORG_ID),
    ])
  })

  it('maps inquiry_received to person, people, today, AND unresolved (§6 special case)', () => {
    expect(invalidationsFor(personChanged('inquiry_received'), ORG_ID)).toEqual([
      queryKeys.person(ORG_ID, PERSON_ID),
      queryKeys.people(ORG_ID),
      queryKeys.today(ORG_ID),
      queryKeys.unresolved(ORG_ID),
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
})

describe('reconnectInvalidations', () => {
  it('invalidates everything under the Organization', () => {
    expect(reconnectInvalidations(ORG_ID)).toEqual([queryKeys.org(ORG_ID)])
  })
})
