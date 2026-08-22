import { describe, expect, it } from 'vitest'
import { ApiError } from '../api/client'
import { deriveInviteState } from './inviteState'

// SLICE_004 §1 step 4, §10 / task brief step 2: "Vitest state-machine test
// over mocked responses" for the /invite/:token page. `ApiError` instances
// stand in for the mocked HTTP responses `POST /api/invitations/preview`
// (§5) can return.
describe('deriveInviteState', () => {
  it('is "loading" while the preview query is pending, regardless of any stale error', () => {
    expect(deriveInviteState(true, undefined)).toBe('loading')
    expect(deriveInviteState(true, new ApiError(410, 'invitation_expired'))).toBe('loading')
  })

  it('is "valid" once the preview resolves with no error', () => {
    expect(deriveInviteState(false, undefined)).toBe('valid')
  })

  it('is "expired" on 410 invitation_expired', () => {
    expect(deriveInviteState(false, new ApiError(410, 'invitation_expired'))).toBe('expired')
  })

  it('is "used" on 409 invitation_used', () => {
    expect(deriveInviteState(false, new ApiError(409, 'invitation_used'))).toBe('used')
  })

  it('is "invalid" on 404 not_found (unknown, malformed, or revoked token)', () => {
    expect(deriveInviteState(false, new ApiError(404, 'not_found'))).toBe('invalid')
  })

  it('is "invalid" on 400 malformed_request (bad JSON)', () => {
    expect(deriveInviteState(false, new ApiError(400, 'malformed_request'))).toBe('invalid')
  })

  it('is "invalid" on a network failure (status 0) rather than surfacing a raw error', () => {
    expect(deriveInviteState(false, new ApiError(0, 'network_error'))).toBe('invalid')
  })

  it('is "invalid" on a non-ApiError error', () => {
    expect(deriveInviteState(false, new Error('boom'))).toBe('invalid')
  })
})
