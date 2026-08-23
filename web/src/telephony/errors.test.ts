// SLICE_006c §10 error copy per code, verbatim; everything else falls
// through to the generic pattern.
import { describe, expect, it } from 'vitest'
import { ApiError } from '../api/client'
import { describeOutcomeError } from './errors'

describe('describeOutcomeError (SLICE_006c §10)', () => {
  it.each([
    [409, 'invalid_call_state', "The call hasn't finished yet."],
    [422, 'no_contact_attempt', "There's no contact attempt to set an outcome on."],
    [409, 'correction_conflict', 'This outcome was just changed — refreshed.'],
    [403, 'forbidden', 'Only the caller can change this outcome.'],
    [404, 'not_found', 'This call no longer exists.'],
    [503, 'unavailable', 'The server is temporarily unavailable. Try again shortly.'],
    [400, 'malformed_request', 'That request was not valid.'],
    [500, 'internal_error', 'Could not save the outcome.'],
  ])('%i %s', (status, code, message) => {
    expect(describeOutcomeError(new ApiError(status, code))).toBe(message)
  })

  it('falls back for a non-ApiError', () => {
    expect(describeOutcomeError(new Error('boom'))).toBe('Could not save the outcome.')
  })
})
