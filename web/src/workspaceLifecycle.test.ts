import { afterEach, describe, expect, it, vi } from 'vitest'
import type { MeResponse } from './api/types'

function me(mode: 'operational' | 'migration_review' = 'operational', role: 'admin' | 'member' = 'admin'): MeResponse {
  return { user: { id: 'actor', email: 'actor@example.test', display_name: 'Actor' }, organization: { id: 'org', name: 'Organization', role, workspace_mode: mode, workspace_revision: mode === 'operational' ? '1' : '2' }, platform_admin: false }
}
function deferred<T>() { let resolve!: (value: T) => void; let reject!: (value: unknown) => void; const promise = new Promise<T>((ok, fail) => { resolve = ok; reject = fail }); return { promise, resolve, reject } }
afterEach(() => { vi.resetModules(); vi.unstubAllGlobals() })
async function configured(verify: () => Promise<MeResponse>) {
  const lifecycle = await import('./workspaceLifecycle')
  const discard = vi.fn(); const install = vi.fn()
  lifecycle.configureWorkspaceLifecycle({ discard, verify, install })
  lifecycle.observeWorkspace(me())
  return { ...lifecycle, discard, install }
}

describe('authoritative workspace access lifecycle', () => {
  it('never dispatches private work before authoritative workspace state is installed', async () => {
    const lifecycle = await import('./workspaceLifecycle')
    expect(() => lifecycle.workspaceRequestAllowed('/people')).toThrow(lifecycle.WorkspaceVerificationPendingError)
  })
  it('fences synchronously, aborts old reads and coalesces refresh without replacing the session lifetime', async () => {
    const response = deferred<MeResponse>(); const verify = vi.fn(() => response.promise)
    const lifecycle = await configured(verify)
    const session = await import('./sessionLifecycle'); const lifetime = session.useAuthSessionLifetime().value
    const request = new AbortController(); lifecycle.trackWorkspaceRequest(request)
    const before = lifecycle.currentWorkspaceEpoch()
    const first = lifecycle.refreshWorkspace(); const second = lifecycle.refreshWorkspace()
    expect(first).toBe(second); expect(verify).toHaveBeenCalledTimes(1)
    expect(request.signal.aborted).toBe(true); expect(lifecycle.discard).toHaveBeenCalledTimes(1)
    expect(lifecycle.useWorkspacePending().value).toBe(true)
    expect(() => lifecycle.workspaceRequestAllowed('/people')).toThrow(lifecycle.WorkspaceVerificationPendingError)
    expect(() => lifecycle.assertWorkspaceEpoch(before)).toThrow()
    response.resolve(me('migration_review')); await first
    expect(lifecycle.install).toHaveBeenCalledExactlyOnceWith(me('migration_review'))
    expect(lifecycle.useWorkspacePending().value).toBe(false)
    expect(session.useAuthSessionLifetime().value).toBe(lifetime)
    expect(lifecycle.workspaceRequestAllowed('/people')).toBe(true)
    expect(lifecycle.workspaceRequestAllowed('/people/p/stage', 'POST')).toBe(false)
    expect(lifecycle.workspaceRequestAllowed('/realtime/token', 'POST')).toBe(false)
    expect(lifecycle.workspaceRequestAllowed('/today')).toBe(false)
    expect(lifecycle.workspaceRequestAllowed('/operator/turn', 'POST')).toBe(false)
    expect(lifecycle.workspaceRequestAllowed('/migrations/fub/imports/i/confirm', 'POST')).toBe(true)
  })

  it('keeps failed verification closed until explicit retry succeeds', async () => {
    const verify = vi.fn<() => Promise<MeResponse>>().mockRejectedValueOnce(new Error('offline')).mockResolvedValueOnce(me('migration_review'))
    const lifecycle = await configured(verify)
    await expect(lifecycle.refreshWorkspace()).rejects.toThrow('offline')
    expect(lifecycle.useWorkspacePending().value).toBe(true); expect(lifecycle.useWorkspaceError().value).toBeTruthy()
    expect(lifecycle.install).not.toHaveBeenCalled()
    await lifecycle.refreshWorkspace()
    expect(lifecycle.useWorkspaceError().value).toBeNull(); expect(lifecycle.useWorkspacePending().value).toBe(false)
  })

  it('ignores a verification completed after the session has changed', async () => {
    const response = deferred<MeResponse>(); const lifecycle = await configured(() => response.promise)
    const pending = lifecycle.refreshWorkspace(); lifecycle.resetWorkspace()
    response.resolve(me('migration_review')); await pending
    expect(lifecycle.install).not.toHaveBeenCalled()
    expect(() => lifecycle.workspaceRequestAllowed('/people')).toThrow()
  })

  it('fails closed on a missing or unknown authoritative mode or revision', async () => {
    for (const bad of [{ workspace_mode: 'unknown' }, { workspace_revision: '0' }, { workspace_revision: undefined }]) {
      const identity = me(); Object.assign(identity.organization!, bad)
      const lifecycle = await configured(async () => identity)
      await expect(lifecycle.refreshWorkspace()).rejects.toThrow(lifecycle.WorkspaceVerificationPendingError)
      expect(lifecycle.useWorkspacePending().value).toBe(true)
      lifecycle.resetWorkspace()
    }
  })

  it('member review blocks private reads while owned terminal hangup remains dispatchable during verification', async () => {
    const response = deferred<MeResponse>(); const lifecycle = await configured(() => response.promise)
    const pending = lifecycle.refreshWorkspace()
    expect(lifecycle.workspaceRequestAllowed('/calls/c/hangup', 'POST')).toBe(true)
    response.resolve(me('migration_review', 'member')); await pending
    expect(lifecycle.workspaceRequestAllowed('/people')).toBe(false)
    expect(lifecycle.workspaceRequestAllowed('/migrations/fub/imports')).toBe(false)
    expect(lifecycle.workspaceRequestAllowed('/calls/c/hangup', 'POST')).toBe(true)
  })

  it('changed role, actor or workspace revision synchronously discards the prior private epoch', async () => {
    const lifecycle = await configured(async () => me())
    for (const identity of [me('migration_review'), me('migration_review', 'member'), { ...me(), user: { ...me().user, id: 'other' } }]) {
      const epoch = lifecycle.currentWorkspaceEpoch(); lifecycle.observeWorkspace(identity)
      expect(lifecycle.currentWorkspaceEpoch()).toBeGreaterThan(epoch)
    }
    expect(lifecycle.discard).toHaveBeenCalledTimes(3)
  })
})
