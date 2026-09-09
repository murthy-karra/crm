// Shared helpers for the three Today system feeds (docs/specs/SLICE_011d.md
// §1, §3, §6) — used by TodayFeedsView.vue (the admin editing surface) and
// TodayView.vue's Rules section, so the fixed order, display names and
// per-feed editing rules cannot drift between the two screens.
import type { FilterClause, SystemFeedIssue, TodayFeedKey } from '../api/types'

/** §1: "one card per feed in fixed order" — every feed listing on both
 * screens iterates this array, never `Object.keys` or server array order
 * (which the wire contract already fixes, but this is the one place both
 * views read it from). */
export const TODAY_FEED_ORDER: TodayFeedKey[] = ['unanswered_inquiry', 'client_replied', 'call_outcome_needed']

// Slice 016b (docs/specs/SLICE_016.md §5, §8): `'task_due'` joins this
// label map — a distinct, WIDER record type than `TodayFeedKey` itself
// (the admin PUT paths' closed vocabulary, not widened) so the same map
// serves both `system_feed_issues` renderings without a cast at each
// call site.
export const TODAY_FEED_LABEL: Record<TodayFeedKey | 'task_due', string> = {
  unanswered_inquiry: 'Unanswered inquiry',
  client_replied: 'Client replied',
  call_outcome_needed: 'Call outcome needed',
  task_due: 'Due tasks',
}

/** §2's canonical defaults table: each feed's derived axis, `value: true`
 * (§1 rule 4's "anchor clause"). Typed to the three derived boolean kinds
 * (narrower than `FilterClauseKind`, which every consumer here still accepts
 * — e.g. FilterBar's `lockedAnchorKind` prop) so `canonicalDraftClauses`
 * below builds a properly discriminated `FilterClause` literal. */
type AnchorClauseKind = 'awaiting_response' | 'client_replied_unanswered' | 'awaiting_call_outcome'
export const TODAY_FEED_ANCHOR_KIND: Record<TodayFeedKey, AnchorClauseKind> = {
  unanswered_inquiry: 'awaiting_response',
  client_replied: 'client_replied_unanswered',
  call_outcome_needed: 'awaiting_call_outcome',
}

/** §1 rule 3: only the two person-state feeds must keep `assigned_to: [me]`;
 * the call feed is viewer-relative by its axis and carries no such clause. */
export function requiresAssigneeMe(feedKey: TodayFeedKey): boolean {
  return feedKey !== 'call_outcome_needed'
}

/** §3: freshness is a feed parameter, not a clause, and only the two
 * person-state feeds carry one — the call feed's is always `null`/"none". */
export function usesFreshWindow(feedKey: TodayFeedKey): boolean {
  return feedKey !== 'call_outcome_needed'
}

/** A starting editor draft for a feed that has never been customized, or a
 * reload after Revert: the anchor clause plus (for the person-state feeds)
 * `assigned_to: [me]` — exactly `canonical_default(feed_key)` (§3). Callers
 * normally prefer the server's own `Feed.filter ?? Feed.default.filter`
 * (never re-derive canonical criteria on the client), but this exists for
 * the one case that needs the *shape* rather than server data: seeding
 * `lockedAnchorKind`'s value before any Feed has loaded. */
export function canonicalDraftClauses(feedKey: TodayFeedKey): FilterClause[] {
  const clauses: FilterClause[] = []
  if (requiresAssigneeMe(feedKey)) clauses.push({ kind: 'assigned_to', assignees: ['me'] })
  clauses.push({ kind: TODAY_FEED_ANCHOR_KIND[feedKey], value: true })
  return clauses
}

/** The member-facing Rules section marker (§6: "Default", "Changed by your
 * admin" or "Off"). */
export function memberFeedMarker(feed: { enabled: boolean; is_default: boolean }): 'default' | 'changed' | 'off' {
  if (!feed.enabled) return 'off'
  return feed.is_default ? 'default' : 'changed'
}

export const MEMBER_FEED_MARKER_LABEL: Record<ReturnType<typeof memberFeedMarker>, string> = {
  default: 'Default',
  changed: 'Changed by your admin',
  off: 'Off',
}

/** The admin card's status line (§6: "Default / Customized by *name* on
 * *date* / off / using default because the saved rule is invalid"). Off
 * takes precedence over an invalid stored rule — a disabled feed's stored
 * definition does not currently evaluate at all, so surfacing the
 * enabled/disabled state first is the more actionable message. */
export type AdminFeedStatus =
  | { kind: 'off' }
  | { kind: 'invalid_fallback' }
  | { kind: 'default' }
  | { kind: 'customized'; updatedBy: string; updatedAt: string }

export function adminFeedStatus(feed: {
  enabled: boolean
  is_default: boolean
  filter_error: string | null
  updated_by: { display_name: string } | null
  updated_at: string
}): AdminFeedStatus {
  if (!feed.enabled) return { kind: 'off' }
  if (feed.filter_error) return { kind: 'invalid_fallback' }
  if (feed.is_default) return { kind: 'default' }
  return { kind: 'customized', updatedBy: feed.updated_by?.display_name ?? 'Unknown', updatedAt: feed.updated_at }
}

/** §6: "a `fallback:true` entry reads 'The *feed* rule is invalid; the
 * default rule is being used.'" Lowercased feed name reads naturally inside
 * the sentence, matching the existing today-source issue copy style. */
export function fallbackFeedMessage(issue: SystemFeedIssue): string {
  return `The ${TODAY_FEED_LABEL[issue.feed_key].toLowerCase()} rule is invalid; the default rule is being used.`
}

/** Slice 016b (docs/specs/SLICE_016.md §8): the non-fallback "<label> could
 * not load." sentence for a `system_feed_issues` entry, generalized to
 * render "A Today rule could not load." for a key this bundle's
 * `TODAY_FEED_LABEL` does not recognize — a future additive token, the
 * same forward-compatibility discipline as `HistoryEntry`'s generic
 * "Activity" fallback — instead of interpolating `undefined`. `feedKey` is
 * `string`, wider than `SystemFeedIssue['feed_key']`'s own declared union,
 * specifically so this defensive path type-checks for a value outside
 * that union (parsed JSON has no compile-time guarantee of matching it). */
export function todayFeedIssueMessage(feedKey: string): string {
  const label = (TODAY_FEED_LABEL as Record<string, string | undefined>)[feedKey]
  return label ? `${label} could not load.` : 'A Today rule could not load.'
}
