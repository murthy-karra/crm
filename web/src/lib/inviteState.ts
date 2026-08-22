import { ApiError } from '../api/client'

/**
 * SLICE_004 §1 step 4, §10: the public `/invite/:token` page's state
 * machine — loading, invalid (404), expired (410), used (409), or valid
 * (preview succeeded). Extracted as a pure function, rather than left as an
 * inline computed in InviteView.vue, so it can be unit tested directly: this
 * project has no @vue/test-utils (see router.test.ts's comment on the same
 * constraint), so a mounted-component test isn't available — every other
 * Vitest suite here (realtime/*) tests composable/pure logic for the same
 * reason.
 */
export type InviteState = 'loading' | 'invalid' | 'expired' | 'used' | 'valid'

export function deriveInviteState(isPending: boolean, error: unknown): InviteState {
  if (isPending) return 'loading'
  if (error) {
    if (error instanceof ApiError) {
      if (error.status === 410) return 'expired'
      if (error.status === 409) return 'used'
    }
    // 404 not_found (unknown, malformed, or revoked token — §5/§9
    // deliberately does not distinguish these) and anything else land here.
    return 'invalid'
  }
  return 'valid'
}
