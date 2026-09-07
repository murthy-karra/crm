// A browser session cookie is shared by every same-origin tab, while Vue
// state and QueryClient caches are not. This module distributes only an
// opaque lifecycle marker so a cookie replacement invalidates private state
// everywhere before any tab learns the replacement identity from `/me`.
// It deliberately never stores an actor, Organization, URL, token, or
// customer content.
import { ref } from 'vue'

export const SESSION_LIFECYCLE_STORAGE_KEY = 'crm.session-lifecycle.v1'
const SESSION_LIFECYCLE_PENDING_PREFIX = `${SESSION_LIFECYCLE_STORAGE_KEY}.pending.`
const SESSION_LIFECYCLE_PROBE_PREFIX = `${SESSION_LIFECYCLE_STORAGE_KEY}.probe.`

type SessionPhase = 'changing' | 'settled'

interface SessionMarker {
  epoch: string
  sequence: number
  phase: SessionPhase
}

interface LifecycleHandlers {
  discardPrivateState: () => void
  verify: (generation: number) => void
  installVerifiedIdentity: (identity: unknown, generation: number) => void
}

export interface SessionVerificationResult {
  generation: number
  identity?: unknown
  error?: unknown
}

const authSessionLifetime = ref(0)
const verificationPending = ref(false)
const sessionCoordinationUnavailable = ref(false)
const routeAuthorizationReplayPending = ref(false)
// F1/F3: two mutually exclusive reasons `verificationPending` can be true,
// exposed separately so a view can render non-alarming copy for the
// ordinary `/me` round trip (in flight) and reserve a recovery control for
// a genuinely stuck cross-tab attempt (blocked). Recomputed at every point
// that can change either input — see `refreshSessionPendingRefs` below.
const sessionBlockedByOutstandingAttempt = ref(false)
const sessionVerificationInFlight = ref(false)
let generation = 0
// Do not treat a marker read during module evaluation as consumed. A tab can
// boot while another one is in `changing`; configuration below applies that
// marker with real handlers before any private view is allowed to render.
let marker: SessionMarker | undefined
let handlers: LifecycleHandlers | undefined
let verificationWaiters: Array<() => void> = []
let verificationResult: SessionVerificationResult | undefined
let verificationStartedGeneration: number | undefined
let configured = false
let channel: BroadcastChannel | undefined
// A transition that failed before its auth request was dispatched must never
// leave a durable pending record behind. Keep its opaque epoch locally until
// storage recovers so we can remove only that record; no other tab's pending
// attempt is ever treated as ours.
const abandonedBeforeDispatch = new Set<string>()
let settlementRequiredAfterAbandonedCleanup = false
// A BroadcastChannel can deliver a marker promptly, but it cannot be read
// synchronously before a private submission. Keep this separate from a
// readable persistent marker: a tab with channel-only coordination performs
// a local recovery when it regains focus in case an event was missed.
let durableMarkerStorage = false
const sessionLifecycleProbeKey = `${SESSION_LIFECYCLE_PROBE_PREFIX}${opaqueEpoch()}`

function markStorageUnavailable() {
  const wasAvailable = durableMarkerStorage
  durableMarkerStorage = false
  sessionCoordinationUnavailable.value = true
  if (wasAvailable) {
    generation += 1
    authSessionLifetime.value += 1
    verificationPending.value = true
    verificationResult = undefined
    verificationStartedGeneration = undefined
    handlers?.discardPrivateState()
  }
  refreshSessionPendingRefs()
}

/** A readable marker alone is insufficient: quota/read-only modes can read
 * an old value but cannot publish the boundary that keeps other tabs safe.
 * Probe a separate opaque key, never the lifecycle marker itself. */
function readMarkerWithDurableWriteProbe(): SessionMarker | undefined {
  if (typeof window === 'undefined') return undefined
  try {
    const markerValue = window.localStorage.getItem(SESSION_LIFECYCLE_STORAGE_KEY)
    const probe = opaqueEpoch()
    window.localStorage.setItem(sessionLifecycleProbeKey, probe)
    if (window.localStorage.getItem(sessionLifecycleProbeKey) !== probe) throw new Error('storage probe mismatch')
    window.localStorage.removeItem(sessionLifecycleProbeKey)
    const recovering = sessionCoordinationUnavailable.value
    durableMarkerStorage = true
    sessionCoordinationUnavailable.value = false
    if (recovering && configured && !settlementRequiredAfterAbandonedCleanup) beginVerificationWhenSafe()
    refreshSessionPendingRefs()
    return parseMarker(markerValue)
  } catch {
    markStorageUnavailable()
    return undefined
  }
}

function writeOpaque(key: string, value: string): boolean {
  if (typeof window === 'undefined') {
    markStorageUnavailable()
    return false
  }
  try {
    window.localStorage.setItem(key, value)
    if (window.localStorage.getItem(key) !== value) throw new Error('storage write was not durable')
    const recovering = sessionCoordinationUnavailable.value
    durableMarkerStorage = true
    sessionCoordinationUnavailable.value = false
    if (recovering && configured) beginVerificationWhenSafe()
    refreshSessionPendingRefs()
    return true
  } catch {
    markStorageUnavailable()
    return false
  }
}

function pendingKey(epoch: string) {
  return `${SESSION_LIFECYCLE_PENDING_PREFIX}${epoch}`
}

function writePending(epoch: string): boolean {
  return writeOpaque(pendingKey(epoch), epoch)
}

function cleanupAbandonedBeforeDispatch(): boolean {
  if (typeof window === 'undefined') return false
  let cleaned = false
  for (const epoch of [...abandonedBeforeDispatch]) {
    try {
      window.localStorage.removeItem(pendingKey(epoch))
      if (window.localStorage.getItem(pendingKey(epoch)) !== null) {
        throw new Error('pre-dispatch pending marker remained')
      }
      abandonedBeforeDispatch.delete(epoch)
      cleaned = true
    } catch {
      markStorageUnavailable()
      return false
    }
  }
  if (cleaned) refreshSessionPendingRefs()
  return cleaned
}

function settleAbandonedBeforeDispatchCleanup() {
  if (!durableMarkerStorage) {
    settlementRequiredAfterAbandonedCleanup = true
    return
  }
  const next: SessionMarker = {
    epoch: opaqueEpoch(),
    sequence: Math.max(Date.now(), (marker?.sequence ?? 0) + 1),
    phase: 'settled',
  }
  // A failed pre-dispatch BEGIN can have reached another tab through
  // BroadcastChannel even though its durable marker write failed. Publish a
  // distinct settled boundary after removing only our own pending record so
  // every coordinator clears the changing state and verifies again.
  receive(next, false)
  if (!persist(next)) {
    settlementRequiredAfterAbandonedCleanup = true
    return
  }
  settlementRequiredAfterAbandonedCleanup = false
  if (!hasPendingTransitions()) beginVerificationWhenSafe()
}

function clearPending(epoch: string): boolean {
  if (typeof window === 'undefined') {
    markStorageUnavailable()
    return false
  }
  try {
    window.localStorage.removeItem(pendingKey(epoch))
    if (window.localStorage.getItem(pendingKey(epoch)) !== null) throw new Error('pending marker remained')
    const recovering = sessionCoordinationUnavailable.value
    durableMarkerStorage = true
    sessionCoordinationUnavailable.value = false
    if (recovering && configured) beginVerificationWhenSafe()
    refreshSessionPendingRefs()
    return true
  } catch {
    markStorageUnavailable()
    return false
  }
}

function hasPendingTransitions(): boolean {
  if (!durableMarkerStorage || typeof window === 'undefined') return true
  try {
    for (let index = 0; index < window.localStorage.length; index += 1) {
      if (window.localStorage.key(index)?.startsWith(SESSION_LIFECYCLE_PENDING_PREFIX)) return true
    }
    return false
  } catch {
    markStorageUnavailable()
    return true
  }
}

/** (a) "blocked by an outstanding attempt": a `changing` marker is current,
 * or any durable pending record exists (own or foreign). (b) "verification
 * in flight for the current generation": not blocked, and this
 * generation's own `/me` verification has been dispatched and has not yet
 * resolved. `verificationPending.value` is deliberately part of (b), not
 * just `verificationStartedGeneration === generation`: that comparison
 * alone would stay true forever after `completeSessionVerification`
 * succeeds, since completion does not reset `verificationStartedGeneration`. */
function refreshSessionPendingRefs() {
  const blocked = marker?.phase === 'changing' || hasPendingTransitions()
  sessionBlockedByOutstandingAttempt.value = blocked
  sessionVerificationInFlight.value =
    !blocked && verificationPending.value && verificationStartedGeneration === generation
}

function parseMarker(value: string | null): SessionMarker | undefined {
  if (!value) return undefined
  try {
    const candidate: unknown = JSON.parse(value)
    if (
      typeof candidate === 'object' && candidate !== null &&
      typeof (candidate as SessionMarker).epoch === 'string' &&
      typeof (candidate as SessionMarker).sequence === 'number' &&
      Number.isSafeInteger((candidate as SessionMarker).sequence) &&
      ((candidate as SessionMarker).phase === 'changing' || (candidate as SessionMarker).phase === 'settled')
    ) {
      return candidate as SessionMarker
    }
  } catch {
    // An unrelated malformed value is never an identity. Ignore it and wait
    // for the next well-formed session lifecycle marker.
  }
  return undefined
}

function opaqueEpoch(): string {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') return crypto.randomUUID()
  return `${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`
}

function isLater(candidate: SessionMarker, current: SessionMarker | undefined): boolean {
  if (!current) return true
  if (candidate.epoch === current.epoch) return candidate.phase === 'settled' && current.phase === 'changing'
  return candidate.sequence > current.sequence ||
    (candidate.sequence === current.sequence && candidate.epoch > current.epoch)
}

function persist(next: SessionMarker): boolean {
  const serialized = JSON.stringify(next)
  const persisted = writeOpaque(SESSION_LIFECYCLE_STORAGE_KEY, serialized)
  if (channel) {
    try {
      channel.postMessage(serialized)
    } catch {
      // If both browser coordination primitives are unavailable, focus/
      // visibility below forces a local identity reset before interaction.
    }
  }
  return persisted
}

function beginVerification() {
  if (!handlers || verificationStartedGeneration === generation) return
  const activeGeneration = generation
  verificationPending.value = true
  verificationResult = undefined
  verificationStartedGeneration = activeGeneration
  handlers.verify(activeGeneration)
  refreshSessionPendingRefs()
}

function beginVerificationWhenSafe() {
  if (!durableMarkerStorage || hasPendingTransitions()) return
  beginVerification()
}

function settleVerificationWaiters() {
  const waiters = verificationWaiters
  verificationWaiters = []
  waiters.forEach((resolve) => resolve())
}

function receive(next: SessionMarker, startVerification = true) {
  if (!isLater(next, marker)) return
  const newEpoch = marker?.epoch !== next.epoch
  marker = next
  if (newEpoch) {
    generation += 1
    authSessionLifetime.value += 1
    verificationPending.value = true
    verificationResult = undefined
    verificationStartedGeneration = undefined
    handlers?.discardPrivateState()
  }
  if (next.phase === 'settled' && startVerification) {
    beginVerificationWhenSafe()
  }
  refreshSessionPendingRefs()
}

/** Observe a marker synchronously before starting a private request. */
export function synchronizeSessionLifecycle() {
  const persisted = readMarkerWithDurableWriteProbe()
  if (persisted) receive(persisted)
  if (cleanupAbandonedBeforeDispatch() || settlementRequiredAfterAbandonedCleanup) {
    settleAbandonedBeforeDispatchCleanup()
  }
  refreshSessionPendingRefs()
}

/** Throws before transport dispatch while another tab replaces the cookie. */
export class SessionVerificationPendingError extends Error {
  constructor() {
    super('session verification is pending')
    this.name = 'SessionVerificationPendingError'
  }
}

/** Durable opaque marker storage is required to prevent a delayed tab from
 * replaying cached private state after another tab replaces the shared
 * cookie. BroadcastChannel alone cannot be read synchronously at submit
 * time, so fail closed while storage is unavailable. */
export class SessionCoordinationUnavailableError extends Error {
  constructor() {
    super('secure cross-tab session coordination is unavailable')
    this.name = 'SessionCoordinationUnavailableError'
  }
}

export function requireVerifiedSession() {
  synchronizeSessionLifecycle()
  if (!durableMarkerStorage) {
    throw new SessionCoordinationUnavailableError()
  }
  if (verificationPending.value || hasPendingTransitions()) throw new SessionVerificationPendingError()
}

/** A synchronous no-transport check for local state that would otherwise be
 * captured before `apiFetch` can observe a missed cross-tab marker. */
export function isSessionVerified(): boolean {
  synchronizeSessionLifecycle()
  return durableMarkerStorage && !verificationPending.value && !hasPendingTransitions()
}

export function currentSessionGeneration(): number {
  synchronizeSessionLifecycle()
  return generation
}

export function isCurrentSessionGeneration(candidate: number): boolean {
  synchronizeSessionLifecycle()
  return durableMarkerStorage && !hasPendingTransitions() && candidate === generation
}

/** Router guards wait for a known auth boundary to settle rather than
 * fetching `/me` with a cookie another tab is in the middle of replacing. */
export async function waitForSessionVerification(): Promise<SessionVerificationResult | undefined> {
  synchronizeSessionLifecycle()
  if (!verificationPending.value) return undefined
  await new Promise<void>((resolve) => verificationWaiters.push(resolve))
  return verificationResult
}

export function useAuthSessionLifetime() {
  return authSessionLifetime
}

export function useSessionVerificationPending() {
  return verificationPending
}

export function useSessionCoordinationUnavailable() {
  return sessionCoordinationUnavailable
}

/** F1: true only while a `changing` marker is current or a durable pending
 * record (own or foreign) exists — a genuinely stuck cross-tab attempt that
 * a timer must never "trust" has finished. A view should offer
 * `resetSessionCoordination()` only in this state. */
export function useSessionBlockedByOutstandingAttempt() {
  return sessionBlockedByOutstandingAttempt
}

/** F3: true only for the ordinary, non-alarming case — this generation's
 * own `/me` verification has been dispatched and is awaiting its response,
 * with no other tab's attempt outstanding. A view should render neutral
 * "loading" copy here, never F1's recovery copy or control. */
export function useSessionVerificationInFlight() {
  return sessionVerificationInFlight
}

/** The router holds private rendering while it replays a paused route through
 * role authorization after a verified cross-tab session boundary. */
export function useRouteAuthorizationReplayPending() {
  return routeAuthorizationReplayPending
}

export function setRouteAuthorizationReplayPending(pending: boolean) {
  routeAuthorizationReplayPending.value = pending
}

/** Publish the pre-cookie-replacement boundary before login/logout/accept. */
export function beginSessionTransition(): string {
  synchronizeSessionLifecycle()
  const next: SessionMarker = {
    epoch: opaqueEpoch(),
    sequence: Math.max(Date.now(), (marker?.sequence ?? 0) + 1),
    phase: 'changing',
  }
  // This distinct opaque record is durable before the auth fetch can change
  // the shared cookie. Other tabs inspect it synchronously even when their
  // storage/BroadcastChannel event has not yet been delivered.
  if (!writePending(next.epoch)) {
    // A write can fail after the browser accepted the set but before our
    // durable read-back. No auth request will run, so remember and retire
    // only this own opaque key as soon as storage permits it.
    abandonedBeforeDispatch.add(next.epoch)
    settlementRequiredAfterAbandonedCleanup = true
    cleanupAbandonedBeforeDispatch()
    throw new SessionCoordinationUnavailableError()
  }
  receive(next, false)
  if (!persist(next)) {
    // The request has not started, so this pending record is safe to retire.
    // If storage is still unavailable, retain its opaque key locally and
    // remove it on the first verified recovery rather than stranding every
    // private route behind an attempt that never reached the server.
    abandonedBeforeDispatch.add(next.epoch)
    if (cleanupAbandonedBeforeDispatch()) settleAbandonedBeforeDispatchCleanup()
    throw new SessionCoordinationUnavailableError()
  }
  return next.epoch
}

/**
 * Logout cannot install a different identity. It must remain available even
 * when storage has become unavailable, after immediately fencing this tab's
 * private state. When durable coordination works it uses the same pending
 * protocol as login so other tabs pause before the shared cookie disappears.
 */
export function beginLogoutSessionTransition(): string {
  synchronizeSessionLifecycle()
  const next: SessionMarker = {
    epoch: opaqueEpoch(),
    sequence: Math.max(Date.now(), (marker?.sequence ?? 0) + 1),
    phase: 'changing',
  }
  void writePending(next.epoch)
  receive(next, false)
  void persist(next)
  // Do not block DELETE /session on coordination loss. The local lifetime
  // has already advanced and private caches are cleared; a later completion
  // can retire a pending record only when one was durably written.
  // Always return the own epoch so `onSuccess`/`onError` can attempt to
  // retire a record that was accepted by storage but failed a read-back.
  // DELETE /session still proceeds when neither primitive is available.
  return next.epoch
}

/** Complete a local auth attempt by verifying the current shared cookie. */
export function settleSessionTransition(epoch: string, _identity?: unknown) {
  void _identity
  // Every actual auth response may have changed the shared cookie. Retire
  // only its own pending record and publish a fresh completion epoch; never
  // reuse the BEGIN epoch or an abandoned timer's settled marker.
  synchronizeSessionLifecycle()
  const cleared = clearPending(epoch)
  if (!cleared) {
    // This response has already settled, so a record that could not be
    // removed is an own completed attempt, not an unknown orphan. Retire it
    // through the same durable recovery path instead of stranding private
    // work behind a permanent pending marker.
    abandonedBeforeDispatch.add(epoch)
    settlementRequiredAfterAbandonedCleanup = true
  }
  const next: SessionMarker = {
    epoch: opaqueEpoch(),
    sequence: Math.max(Date.now(), (marker?.sequence ?? 0) + 1),
    phase: 'settled',
  }
  receive(next, false)
  persist(next)
  if (cleared && durableMarkerStorage && !hasPendingTransitions()) beginVerificationWhenSafe()
}

/**
 * F1: user-driven recovery from an auth attempt that will never settle —
 * its tab closed, reloaded, or crashed mid-login/logout, leaving a durable
 * pending record that would otherwise block `hasPendingTransitions()`
 * (and therefore verification) in every tab forever, with no timer ever
 * "trusting" that the attempt finished. Only a person clicking a control
 * reaches this function.
 *
 * Removes EVERY pending record — this tab's own and any other tab's — then
 * publishes a fresh settled boundary (a new opaque epoch, so generation
 * advances and private state is discarded everywhere it is observed) and
 * starts verification. Throws `SessionCoordinationUnavailableError` if
 * storage is unavailable; callers should fall back to the existing
 * storage-unavailable recovery copy in that case.
 */
export function resetSessionCoordination(): void {
  synchronizeSessionLifecycle()
  if (!durableMarkerStorage || typeof window === 'undefined') {
    throw new SessionCoordinationUnavailableError()
  }
  try {
    const pendingKeys: string[] = []
    for (let index = 0; index < window.localStorage.length; index += 1) {
      const key = window.localStorage.key(index)
      if (key?.startsWith(SESSION_LIFECYCLE_PENDING_PREFIX)) pendingKeys.push(key)
    }
    // Collect before removing: mutating storage while iterating `key(index)`
    // risks skipping entries as the browser re-indexes remaining keys.
    for (const key of pendingKeys) {
      window.localStorage.removeItem(key)
      if (window.localStorage.getItem(key) !== null) throw new Error('pending marker survived reset')
    }
  } catch {
    markStorageUnavailable()
    throw new SessionCoordinationUnavailableError()
  }
  // Every storage record a locally retained abandoned epoch could refer to
  // was just removed above, whether it was this tab's own or foreign.
  abandonedBeforeDispatch.clear()
  settlementRequiredAfterAbandonedCleanup = false
  const next: SessionMarker = {
    epoch: opaqueEpoch(),
    sequence: Math.max(Date.now(), (marker?.sequence ?? 0) + 1),
    phase: 'settled',
  }
  receive(next, false)
  if (!persist(next)) throw new SessionCoordinationUnavailableError()
  beginVerificationWhenSafe()
}

/** Called only from api/queries after its QueryClient and `/me` decoder exist. */
export function configureSessionLifecycle(next: LifecycleHandlers) {
  handlers = next
  if (!configured) {
    configured = true
    const initial = readMarkerWithDurableWriteProbe()
    if (initial) receive(initial)
    return
  }
  synchronizeSessionLifecycle()
  // A marker could have arrived during module setup before handlers existed.
  // Apply its safety effects now; a changing marker stays blocked until its
  // settled marker or bounded recovery starts verification.
  if (verificationPending.value) {
    handlers.discardPrivateState()
    if (marker?.phase === 'settled') beginVerificationWhenSafe()
  }
}

export function completeSessionVerification(candidate: number, identity?: unknown, error?: unknown) {
  if (candidate !== generation || !durableMarkerStorage || hasPendingTransitions()) return false
  verificationPending.value = false
  verificationResult = { generation: candidate, identity, error }
  if (identity !== undefined) handlers?.installVerifiedIdentity(identity, candidate)
  settleVerificationWaiters()
  refreshSessionPendingRefs()
  return true
}

if (typeof window !== 'undefined') {
  try {
    // Use the window-scoped constructor so embedders and tests can disable
    // the browser primitive consistently with localStorage.
    if (typeof window.BroadcastChannel === 'undefined') throw new Error('BroadcastChannel unavailable')
    channel = new window.BroadcastChannel(SESSION_LIFECYCLE_STORAGE_KEY)
    channel.addEventListener('message', (event) => {
      const next = typeof event.data === 'string' ? parseMarker(event.data) : undefined
      if (next) receive(next)
    })
  } catch {
    channel = undefined
  }
  window.addEventListener('storage', (event) => {
    if (event.key === SESSION_LIFECYCLE_STORAGE_KEY) {
      const next = parseMarker(event.newValue)
      if (next) receive(next)
    }
  })
  // A frozen tab can miss storage delivery. Re-read the persisted opaque
  // marker before ordinary focus-driven private refetches begin.
  for (const event of ['focus', 'visibilitychange', 'pageshow']) {
    window.addEventListener(event, () => {
      synchronizeSessionLifecycle()
      if (marker?.phase === 'settled') beginVerificationWhenSafe()
    })
  }
}
