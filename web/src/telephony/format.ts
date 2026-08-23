// Pure helpers behind CallPanel.vue and the `call_completed` history row —
// kept out of the components so the copy is unit-testable per code.
import type { CallCompletedOutcome, CallView } from '../api/types'
import type { CallPhase } from './useCall'

/** "00:12", "1:02:03" — the panel's elapsed timer (§1 "Connected 00:12"). */
export function formatElapsed(seconds: number): string {
  const s = Math.max(0, Math.floor(seconds))
  const hours = Math.floor(s / 3600)
  const minutes = Math.floor((s % 3600) / 60)
  const rest = s % 60
  const mm = String(minutes).padStart(2, '0')
  const ss = String(rest).padStart(2, '0')
  return hours > 0 ? `${hours}:${mm}:${ss}` : `${mm}:${ss}`
}

/** "1 min 12 s", "45 s", "1 h 2 min 3 s" — history's talk time (§1 step 4). */
export function formatTalkSeconds(seconds: number): string {
  const s = Math.max(0, Math.floor(seconds))
  const hours = Math.floor(s / 3600)
  const minutes = Math.floor((s % 3600) / 60)
  const rest = s % 60
  const parts: string[] = []
  if (hours > 0) parts.push(`${hours} h`)
  if (minutes > 0) parts.push(`${minutes} min`)
  if (rest > 0 || parts.length === 0) parts.push(`${rest} s`)
  return parts.join(' ')
}

/** The automatic contact attempt D-031 wrote for a settled call, derived
 * from the server's `CallView`: answered → `reached`; busy / declined /
 * ring-out (and a cancel after ringing started) → `no_answer`; nothing
 * reached the callee → null. Mirrors backend `transitions.rs`'s attempt
 * mapping exactly — do not widen. */
export function attemptOutcome(call: CallView | undefined): 'reached' | 'no_answer' | null {
  if (!call) return null
  if (call.status === 'answered' || call.status === 'ended') return 'reached'
  if (call.status !== 'failed') return null
  switch (call.failure_reason) {
    case 'no_answer':
    case 'busy':
    case 'declined':
    case 'ring_timeout':
      return 'no_answer'
    case 'cancelled':
      return call.ringing_at !== null ? 'no_answer' : null
    default:
      return null
  }
}

/** SLICE_006c §10: whether the panel's post-call block is the "How did it
 * go?" prompt — a finished call (terminal phase, no call error) that wrote
 * an automatic attempt, whose outcome has not been saved yet. Computed once
 * by the view (it also gates the header's primary and the History action)
 * and passed to CallPanel. */
export function showsOutcomePrompt(phase: CallPhase, hasError: boolean, call: CallView | undefined, saved: boolean): boolean {
  return (phase === 'ended' || phase === 'failed') && !hasError && !saved && attemptOutcome(call) !== null
}

/** The panel's status line per phase (§1 step 2, step 5). A terminal phase
 * without an error reads from the server's view when it has settled. */
export function statusLine(phase: CallPhase, elapsedSeconds: number, call: CallView | undefined): string {
  switch (phase) {
    case 'idle':
      return ''
    case 'requesting_mic':
    case 'joining':
    case 'placing':
      return 'Connecting…'
    case 'ringing':
      return 'Ringing…'
    case 'connected':
      return `Connected ${formatElapsed(elapsedSeconds)}`
    case 'ended':
      return `Call ended · ${formatElapsed(elapsedSeconds)}`
    case 'failed':
      return attemptOutcome(call) === 'no_answer' ? 'No answer' : 'Call not connected'
  }
}

/** History: "Call — reached, 1 min 12 s" / "Call — no answer" (§1 steps 4–5). */
export function callCompletedSummary(outcome: CallCompletedOutcome, talkSeconds: number | null): string {
  switch (outcome) {
    case 'reached':
      return talkSeconds === null ? 'Call — reached' : `Call — reached, ${formatTalkSeconds(talkSeconds)}`
    case 'no_answer':
    case 'busy':
    case 'declined':
    case 'ring_timeout':
      return 'Call — no answer'
    case 'cancelled':
      return 'Call — cancelled'
    case 'agent_not_joined':
    case 'provider_error':
    case 'expired':
      return 'Call — failed'
  }
}
