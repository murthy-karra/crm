// Organization review-mode fencing is independent of cookie/session identity.
// Keep no source data or command body here; only trusted /me state and an epoch.
import { computed, ref } from 'vue'
import type { MeResponse } from './api/types'

const epoch = ref(0)
const pending = ref(false)
const failure = ref<string | null>(null)
const verified = ref<MeResponse | null>(null)
let handlers: { discard: () => void; verify: () => Promise<MeResponse>; install: (me: MeResponse) => void } | undefined
let inFlight: Promise<void> | undefined
const requests = new Set<AbortController>()

export class WorkspaceVerificationPendingError extends Error {
  constructor() { super('Workspace access must be verified'); this.name = 'WorkspaceVerificationPendingError' }
}
export const useWorkspaceEpoch = () => epoch
export const useWorkspacePending = () => pending
export const useWorkspaceError = () => failure
export const useWorkspaceOperational = () => computed(() => workspaceOperational(verified.value))
export function workspaceOperational(me: MeResponse | null | undefined): boolean {
  return !pending.value && !failure.value && me?.organization?.workspace_mode === 'operational'
}
function identity(me: MeResponse | null): string {
  return me ? [me.user.id, me.organization?.id, me.organization?.role, me.organization?.workspace_mode, me.organization?.workspace_revision, me.platform_admin].join(':') : ''
}
function valid(me: MeResponse): boolean {
  const org = me.organization
  return org === null || (!!org && (org.workspace_mode === 'operational' || org.workspace_mode === 'migration_review') && /^[1-9][0-9]*$/.test(org.workspace_revision))
}
function fence() {
  epoch.value++
  for (const request of requests) request.abort()
  requests.clear()
  handlers?.discard()
}
export function configureWorkspaceLifecycle(value: NonNullable<typeof handlers>) { handlers = value }
export function observeWorkspace(me: MeResponse) {
  if (!valid(me)) {
    pending.value = true; failure.value = 'Could not verify workspace access. Try again.'; fence()
    throw new WorkspaceVerificationPendingError()
  }
  if (verified.value && identity(verified.value) !== identity(me)) fence()
  verified.value = me
}
export function resetWorkspace() {
  // Session coordination separately discards every cache and component.
  epoch.value++
  for (const request of requests) request.abort()
  requests.clear()
  verified.value = null; pending.value = false; failure.value = null; inFlight = undefined
}
export function currentWorkspaceEpoch() { return epoch.value }
export function assertWorkspaceEpoch(expected: number) {
  if (expected !== epoch.value || pending.value) throw new WorkspaceVerificationPendingError()
}
export function trackWorkspaceRequest(controller: AbortController) {
  requests.add(controller)
  return () => { requests.delete(controller) }
}
export function workspaceRequestAllowed(path: string, method = 'GET'): boolean {
  const route = path.split('?')[0]!
  if (verified.value && /^\/calls\/[^/]+\/hangup$/.test(route) && method === 'POST') return true
  if (pending.value || !verified.value) throw new WorkspaceVerificationPendingError()
  const me = verified.value
  if (route.startsWith('/platform') && me.platform_admin) return true
  if (me.organization?.workspace_mode === 'operational') return true
  if (me.organization?.role !== 'admin') return false
  if (route.startsWith('/migrations/')) return true
  if (/^\/organization\/(members|invitations)(\/|$)/.test(route)) return true
  return method === 'GET' && !/^\/(today|operator|realtime)(\/|$)/.test(route)
}
export function refreshWorkspace(): Promise<void> {
  if (inFlight) return inFlight
  if (!handlers) return Promise.reject(new WorkspaceVerificationPendingError())
  pending.value = true; failure.value = null
  fence()
  const expected = epoch.value
  const currentHandlers = handlers
  const promise = currentHandlers.verify().then(me => {
    if (expected !== epoch.value) return
    if (!valid(me)) throw new WorkspaceVerificationPendingError()
    verified.value = me
    currentHandlers.install(me)
    pending.value = false
  }).catch((error: unknown) => {
    if (expected !== epoch.value) return
    failure.value = 'Could not verify workspace access. Try again.'
    throw error
  }).finally(() => { if (inFlight === promise) inFlight = undefined })
  inFlight = promise
  return promise
}
