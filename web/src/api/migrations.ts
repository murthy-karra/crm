// Slice 010a's bounded FUB assessment contract. Credentials deliberately do
// not appear in a TanStack mutation type or query key: callers submit them
// directly and clear their local password field before awaiting the request.
import { apiFetch } from './client'

export type AssessmentState = 'queued' | 'running' | 'waiting_retry' | 'paused' | 'completed' | 'cancelled'
export type Coverage = 'partial' | 'unavailable' | 'not_checked' | 'complete_for_query'

export interface FubConnection {
  id: string
  source_account_id: number
  source_display_name: string | null
  source_access_scope: 'owner' | 'admin' | 'restricted' | 'unknown' | string
  status: string
  revision: number
  created_at: string
  updated_at: string
}

export interface FubAssessmentCheck {
  check_key: string
  state: string
  reported_total: string | null
  retrieved_count: string
  destination_readiness: 'model_available' | 'review_required' | 'destination_missing' | string
  reason_codes: string[]
  next_action: string
  coverage: Coverage | string
  error_code: string | null
  attempts: number
  observed_at: string | null
}

export interface FubAssessment {
  destination_organization_id: string
  source_account_id: number
  source_display_name: string | null
  source_access_scope: 'owner' | 'admin' | 'restricted' | 'unknown' | string
  id: string
  connection_id: string
  connection_revision: number
  profile_version: string
  state: AssessmentState
  pause_reason: string | null
  created_at: string
  started_at: string | null
  completed_at: string | null
  checks: FubAssessmentCheck[]
}

export interface FubMigrationSummary {
  connection: FubConnection | null
  active_assessment: FubAssessment | null
  latest_assessment: FubAssessment | null
  latest_report: FubAssessment | null
}

interface ConnectionEnvelope { connection: FubConnection; request_id: string }
interface AssessmentEnvelope { assessment: FubAssessment; request_id?: string }

export function fetchFubMigrationSummary(signal?: AbortSignal) {
  return apiFetch<FubMigrationSummary>('/migrations/fub/', { signal })
}

export function createFubConnection(requestId: string, apiKey: string) {
  return apiFetch<ConnectionEnvelope>('/migrations/fub/connections', {
    method: 'POST',
    body: JSON.stringify({ request_id: requestId, api_key: apiKey }),
  })
}

export function replaceFubCredential(connectionId: string, revision: number, requestId: string, apiKey: string) {
  return apiFetch<ConnectionEnvelope>(`/migrations/fub/connections/${encodeURIComponent(connectionId)}/credential`, {
    method: 'PUT',
    body: JSON.stringify({ request_id: requestId, expected_revision: revision, api_key: apiKey }),
  })
}

export function disconnectFubConnection(connectionId: string) {
  return apiFetch<void>(`/migrations/fub/connections/${encodeURIComponent(connectionId)}`, { method: 'DELETE' })
}

export function startFubAssessment(connectionId: string, revision: number, requestId: string) {
  return apiFetch<AssessmentEnvelope>('/migrations/fub/assessments', {
    method: 'POST',
    body: JSON.stringify({ request_id: requestId, connection_id: connectionId, expected_revision: revision }),
  })
}

export function retryFubAssessment(assessmentId: string) {
  return apiFetch<AssessmentEnvelope>(`/migrations/fub/assessments/${encodeURIComponent(assessmentId)}/retry`, { method: 'POST' })
}

export function cancelFubAssessment(assessmentId: string) {
  return apiFetch<AssessmentEnvelope>(`/migrations/fub/assessments/${encodeURIComponent(assessmentId)}/cancel`, { method: 'POST' })
}
