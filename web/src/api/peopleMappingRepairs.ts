import { apiFetch } from './client'
export type RepairOwner = 'original' | 'admitted'
export interface RepairSummary { choices_digest: string; candidate_count: string; approval_only_count: string; unassigned_count: string }
export interface RepairAck { choices_digest: string; candidate_count: number; approval_only_count: number; unassigned_count: number }
export interface RepairChoice { key_id: string; disposition: 'existing' | 'member' | 'unassigned' | 'unresolved'; target_id: string | null }
export interface RepairMapping {
  id: string; kind: 'stage' | 'assignee'; source: { kind: 'stage_label' | 'missing_stage' | 'assignee'; key?: string }; affected_count: string
  choice: { disposition: RepairChoice['disposition']; target_id: string | null; target: { name?: string; email?: string } | null } | null
}
export const repairPath = (owner: RepairOwner, id: string) => `/migrations/fub/${owner === 'original' ? 'people-refreshes' : 'admitted-people-refreshes'}/${encodeURIComponent(id)}`
export const fetchRepairMappings = (owner: RepairOwner, id: string, cursor?: string, signal?: AbortSignal) => apiFetch<{ items: RepairMapping[]; next_cursor: string | null; draft_revision: string }>(`${repairPath(owner, id)}/repair-mappings?${new URLSearchParams({ limit: '20', ...(cursor ? { cursor } : {}) })}`, { signal })
export const postRepair = (path: string, body: unknown) => apiFetch<{ refresh_id: string }>(path, { method: 'POST', body: JSON.stringify(body) })
