import { describe, expect, it, vi } from 'vitest'
vi.mock('./client', () => ({ apiFetch: vi.fn() }))
import { apiFetch } from './client'
import { createAdmittedHistoryRemainder, prepareAdmittedHistory } from './admittedHistoryImports'
describe('admitted history API', () => { it('sends frozen prepare and remainder bodies', () => { prepareAdmittedHistory({ request_id: 'r', admission_id: 'a', history_capture_id: 'c' }); createAdmittedHistoryRemainder('root', { request_id: 'r', attempt_id: 'attempt', expected_revision: '9' }); expect(apiFetch).toHaveBeenNthCalledWith(1, '/migrations/fub/admitted-history-imports', expect.objectContaining({ method: 'POST' })); expect(apiFetch).toHaveBeenNthCalledWith(2, '/migrations/fub/admitted-history-imports/root/remainder', expect.objectContaining({ method: 'POST' })) }) })
