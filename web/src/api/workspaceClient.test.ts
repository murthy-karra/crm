import { afterEach, describe, expect, it, vi } from 'vitest'
import type { MeResponse } from './types'

function identity(): MeResponse { return { user: { id: 'actor', display_name: 'Actor', email: 'actor@example.test' }, organization: { id: 'org', name: 'Org', role: 'admin', workspace_mode: 'operational', workspace_revision: '1' }, platform_admin: false } }
function response(status: number, body: unknown) { return new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } }) }
function deferred<T>() { let resolve!: (value: T) => void; const promise = new Promise<T>(done => { resolve = done }); return { promise, resolve } }
afterEach(() => { vi.unstubAllGlobals(); vi.resetModules() })
async function setup(verify: () => Promise<MeResponse>) {
  const workspace = await import('../workspaceLifecycle'); workspace.configureWorkspaceLifecycle({ discard: vi.fn(), verify, install: vi.fn() }); workspace.observeWorkspace(identity())
  return { workspace, ...await import('./client') }
}

describe('workspace transport authority', () => {
  it('rejects a delayed private response after a workspace fence even if transport ignores abort', async () => {
    const read = deferred<Response>(); const authority = deferred<MeResponse>()
    const { workspace, apiFetch } = await setup(() => authority.promise)
    const transport = vi.fn(() => read.promise); vi.stubGlobal('fetch', transport)
    const pending = apiFetch('/people'); const discarded = expect(pending).rejects.toBeInstanceOf(workspace.WorkspaceVerificationPendingError)
    const refresh = workspace.refreshWorkspace()
    expect((transport.mock.calls[0] as unknown as [string, RequestInit])[1].signal!.aborted).toBe(true)
    read.resolve(response(200, { private: 'old data' })); await discarded
    authority.resolve({ ...identity(), organization: { ...identity().organization!, workspace_mode: 'migration_review', workspace_revision: '2' } }); await refresh
  })
  it('a server review rejection synchronously starts authoritative verification without trusting receipt fields', async () => {
    const authority = deferred<MeResponse>(); const verify = vi.fn(() => authority.promise)
    const { workspace, apiFetch, ApiError } = await setup(verify)
    vi.stubGlobal('fetch', vi.fn(async () => response(409, { error: 'workspace_in_migration_review', workspace_mode: 'operational' })))
    await expect(apiFetch('/people/p/notes', { method: 'POST', body: '{}' })).rejects.toBeInstanceOf(ApiError)
    expect(workspace.useWorkspacePending().value).toBe(true); expect(verify).toHaveBeenCalledTimes(1)
    const refresh = workspace.refreshWorkspace(); authority.resolve({ ...identity(), organization: { ...identity().organization!, workspace_mode: 'migration_review', workspace_revision: '2' } }); await refresh
    const calls = vi.mocked(fetch).mock.calls.length
    await expect(apiFetch('/people/p/notes', { method: 'POST', body: '{}' })).rejects.toMatchObject({ status: 409, code: 'workspace_in_migration_review' })
    expect(vi.mocked(fetch)).toHaveBeenCalledTimes(calls)
  })
  it('refreshes current authorization after a private 403 and blocks further dispatch while it is unresolved', async () => {
    const authority = deferred<MeResponse>(); const { workspace, apiFetch } = await setup(() => authority.promise)
    vi.stubGlobal('fetch', vi.fn(async () => response(403, { error: 'forbidden' })))
    await expect(apiFetch('/migrations/fub/imports/i')).rejects.toMatchObject({ status: 403 })
    await expect(apiFetch('/people')).rejects.toBeInstanceOf(workspace.WorkspaceVerificationPendingError)
    expect(fetch).toHaveBeenCalledTimes(1)
    const refresh = workspace.refreshWorkspace(); authority.resolve({ ...identity(), organization: { ...identity().organization!, role: 'member', workspace_mode: 'migration_review', workspace_revision: '2' } }); await refresh
    expect(workspace.workspaceRequestAllowed('/people')).toBe(false)
  })
})
