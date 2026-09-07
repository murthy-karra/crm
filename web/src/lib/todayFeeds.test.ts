import { describe, expect, it } from 'vitest'
import {
  TODAY_FEED_ANCHOR_KIND,
  TODAY_FEED_ORDER,
  adminFeedStatus,
  canonicalDraftClauses,
  fallbackFeedMessage,
  memberFeedMarker,
  requiresAssigneeMe,
  usesFreshWindow,
} from './todayFeeds'

describe('TODAY_FEED_ORDER', () => {
  it('is the fixed §1 order', () => {
    expect(TODAY_FEED_ORDER).toEqual(['unanswered_inquiry', 'client_replied', 'call_outcome_needed'])
  })
})

describe('requiresAssigneeMe / usesFreshWindow', () => {
  it('is true for the two person-state feeds and false for the call feed', () => {
    expect(requiresAssigneeMe('unanswered_inquiry')).toBe(true)
    expect(requiresAssigneeMe('client_replied')).toBe(true)
    expect(requiresAssigneeMe('call_outcome_needed')).toBe(false)
    expect(usesFreshWindow('unanswered_inquiry')).toBe(true)
    expect(usesFreshWindow('client_replied')).toBe(true)
    expect(usesFreshWindow('call_outcome_needed')).toBe(false)
  })
})

describe('canonicalDraftClauses', () => {
  it('includes assigned_to: [me] and the anchor clause for a person-state feed', () => {
    expect(canonicalDraftClauses('unanswered_inquiry')).toEqual([
      { kind: 'assigned_to', assignees: ['me'] },
      { kind: 'awaiting_response', value: true },
    ])
  })

  it('omits assigned_to for the call feed', () => {
    expect(canonicalDraftClauses('call_outcome_needed')).toEqual([
      { kind: TODAY_FEED_ANCHOR_KIND.call_outcome_needed, value: true },
    ])
  })
})

describe('memberFeedMarker', () => {
  it('prioritizes off over default/changed', () => {
    expect(memberFeedMarker({ enabled: false, is_default: true })).toBe('off')
    expect(memberFeedMarker({ enabled: false, is_default: false })).toBe('off')
  })
  it('distinguishes default from changed when enabled', () => {
    expect(memberFeedMarker({ enabled: true, is_default: true })).toBe('default')
    expect(memberFeedMarker({ enabled: true, is_default: false })).toBe('changed')
  })
})

describe('adminFeedStatus', () => {
  const base = { enabled: true, is_default: true, filter_error: null, updated_by: null, updated_at: '2026-09-01T00:00:00Z' }

  it('is off when disabled, even with a customization or invalid rule', () => {
    expect(adminFeedStatus({ ...base, enabled: false })).toEqual({ kind: 'off' })
    expect(adminFeedStatus({ ...base, enabled: false, filter_error: 'invalid_stage', is_default: false })).toEqual({ kind: 'off' })
  })

  it('reports the invalid-fallback status when enabled with a filter_error', () => {
    expect(adminFeedStatus({ ...base, filter_error: 'invalid_stage' })).toEqual({ kind: 'invalid_fallback' })
  })

  it('reports default when enabled, valid and unedited', () => {
    expect(adminFeedStatus(base)).toEqual({ kind: 'default' })
  })

  it('reports the editor and timestamp when customized', () => {
    expect(adminFeedStatus({ ...base, is_default: false, updated_by: { display_name: 'Alice' } })).toEqual({
      kind: 'customized', updatedBy: 'Alice', updatedAt: base.updated_at,
    })
  })

  it('falls back to "Unknown" for a customization with no recorded editor', () => {
    expect(adminFeedStatus({ ...base, is_default: false })).toEqual({
      kind: 'customized', updatedBy: 'Unknown', updatedAt: base.updated_at,
    })
  })
})

describe('fallbackFeedMessage', () => {
  it('names the feed in the fixed sentence shape', () => {
    expect(fallbackFeedMessage({ feed_key: 'client_replied', error: 'invalid_definition', fallback: true })).toBe(
      'The client replied rule is invalid; the default rule is being used.',
    )
  })
})
