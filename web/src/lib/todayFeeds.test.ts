import { describe, expect, it } from 'vitest'
import {
  TODAY_FEED_ANCHOR_KIND,
  TODAY_FEED_LABEL,
  TODAY_FEED_ORDER,
  adminFeedStatus,
  canonicalDraftClauses,
  fallbackFeedMessage,
  memberFeedMarker,
  requiresAssigneeMe,
  todayFeedIssueMessage,
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

  // Round-2 review fix 6: goes through the same guarded lookup
  // `todayFeedIssueMessage` uses, rather than indexing `TODAY_FEED_LABEL`
  // directly — a parsed HTTP response carries no runtime guarantee that
  // `feed_key` is a key this bundle recognizes.
  it('never interpolates "undefined" for an unrecognized feed key', () => {
    const message = fallbackFeedMessage({
      feed_key: 'some_future_feed' as unknown as 'client_replied',
      error: 'invalid_definition',
      fallback: true,
    })
    expect(message).not.toContain('undefined')
    expect(message).toBe('A Today rule is invalid; the default rule is being used.')
  })
})

// Slice 016b (docs/specs/SLICE_016.md §5, §8): the `task_due` issue token
// and the generic fallback for an unrecognized key.
describe('TODAY_FEED_LABEL / todayFeedIssueMessage', () => {
  it('carries the Due tasks label for task_due', () => {
    expect(TODAY_FEED_LABEL.task_due).toBe('Due tasks')
  })

  it('renders "<label> could not load." for a known key, including task_due', () => {
    expect(todayFeedIssueMessage('call_outcome_needed')).toBe('Call outcome needed could not load.')
    expect(todayFeedIssueMessage('task_due')).toBe('Due tasks could not load.')
  })

  it('renders the generic sentence for an unknown key, never "undefined"', () => {
    expect(todayFeedIssueMessage('some_future_key')).toBe('A Today rule could not load.')
    expect(todayFeedIssueMessage('some_future_key')).not.toContain('undefined')
  })
})
