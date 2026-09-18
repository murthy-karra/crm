import { apiFetch } from './client'

export interface OutcomeTotals { applied: string; already_current: string; held: string; excluded: string; unprocessed: string }
export interface CoverageDescriptor { family: string; path: string; source_evidence: string; destination_surface: string; blocker_codes: string[] }
export interface Blocker { code: string; evidence_id: string | null }
export interface LatestBundle { bundle_id: string; plan_id: string; state: string; outcome_totals: OutcomeTotals; evidence_ids: string[] }
export interface ReconciliationCohort { cohort_id: string; cohort_origin: 'original'|'admission'|'recovery'; result_totals: OutcomeTotals; latest_bundle: LatestBundle | null; evidence_ids: string[]; blockers: Blocker[] }
export interface ReconciliationFamily { coverage: CoverageDescriptor; unit: string; cohorts: ReconciliationCohort[]; blockers: Blocker[] }
export interface ReconciliationSummary { original_import_id: string; workspace_revision: string; generated_at: string; review_hold: true; families: ReconciliationFamily[]; blockers: Blocker[] }
export const fetchMigrationReconciliation = (importId: string, signal?: AbortSignal) =>
  apiFetch<ReconciliationSummary>(`/migrations/fub/imports/${encodeURIComponent(importId)}/reconciliation`, { signal, cache: 'no-store' })
