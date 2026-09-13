import { apiFetch } from './client'
import { useImportAccess } from './imports'

export const admissionDispositions = ['eligible', 'already_imported', 'already_admitted', 'excluded_original', 'held_baseline_gap', 'held_evidence_gap', 'held_mapping_gap', 'held_identity', 'held_target', 'settled', 'cancelled'] as const
export type AdmissionDisposition = typeof admissionDispositions[number]
export interface AdmissionCounts { total:string; eligible:string; already_imported:string; already_admitted:string; excluded_original:string; held:string; intended_contacts:string }
export interface AdmissionPlan { id:string; revision:string; digest:string; expires_at:string|null; counts:AdmissionCounts }
export interface PeopleAdmission {
  id:string; parent_import_id:string; report_id:string; state:string; lifecycle_revision:string; created_at:string; updated_at:string; completed_at:string|null; pause_reason:string|null
  newer_snapshot_id:string; newer_sequence:string; retained_bytes:string; reserved_bytes:string; progress:{settled_items:string}; plan?:AdmissionPlan
  actions:{confirm:boolean;repreview:boolean;retry:boolean;cancel:boolean}
}
export interface AdmissionItem { id:string; source_id:string|null; prospective_person_id:string; disposition:AdmissionDisposition; settled_at:string|null; plan_id:string; plan_revision:string; projection:Record<string, unknown> }
export interface AdmissionContact { id:string; kind:'email'|'phone'; import_order:string; primary:boolean; value:{value:string;normalized_value?:string} }
export interface AdmissionResult { id:string; item_id:string; person_id:string|null; source_id:string; disposition:AdmissionDisposition; committed_at:string }
export interface AdmissionPage<T> { plan_id:string; plan_revision:string; next_cursor:string|null; items?:T[]; contacts?:T[] }
export interface AdmissionFieldFragment { plan_id:string; plan_revision:string; side:string; field:string; offset:string; total_bytes:string; fragment:string; next_cursor:string|null }
export interface ProvenanceFieldSummary { prefix:string; total_bytes:string; truncated:boolean }
export interface AdmissionProvenance { person_id:string; admission_id:string; item_id:string; result_id:string; parent_import_id:string; parent_plan_id:string; original_snapshot_id:string; newer_snapshot_id:string; coverage:string; provenance:Record<string, unknown> }
export interface ProvenancePage { plan_id:string; plan_revision:string; items:AdmissionContact[]; next_cursor:string|null }
export interface ProvenanceFieldFragment { field:string; offset:string; total_bytes:string; fragment:string; next_cursor:string|null }
export interface AdmissionReceipt { admission_id:string; state:string }
export interface AdmissionConfirm { request_id:string; plan_id:string; plan_revision:string; plan_digest:string; eligible_count:string; acknowledged_coverage:boolean; acknowledged_mappings:boolean; acknowledged_distinct_contacts:boolean; acknowledged_review_hold:boolean }

const root = '/migrations/fub/people-admissions'
const path = (id:string) => `${root}/${encodeURIComponent(id)}`
const itemPath = (id:string,item:string) => `${path(id)}/items/${encodeURIComponent(item)}`
function query(values:Record<string,string|undefined>) { const p=new URLSearchParams(); for (const [k,v] of Object.entries(values)) if (v) p.set(k,v); return `?${p}` }
function post<T>(url:string, body:unknown, signal?:AbortSignal) { return apiFetch<T>(url,{method:'POST',body:JSON.stringify(body),signal}) }
export const fetchPeopleAdmissions=(parent:string,cursor?:string,signal?:AbortSignal)=>apiFetch<{items:PeopleAdmission[];next_cursor:string|null}>(`${root}${query({parent_import_id:parent,cursor,limit:'20'})}`,{signal})
export const fetchPeopleAdmission=(id:string,signal?:AbortSignal)=>apiFetch<PeopleAdmission>(path(id),{signal})
export const preparePeopleAdmission=(report_id:string,request_id:string,signal?:AbortSignal)=>post<AdmissionReceipt>(root,{report_id,request_id},signal)
export const repreviewPeopleAdmission=(id:string,request_id:string,expected_plan_revision:string,signal?:AbortSignal)=>post<AdmissionReceipt>(`${path(id)}/plans`,{request_id,expected_plan_revision},signal)
export const confirmPeopleAdmission=(id:string,body:AdmissionConfirm,signal?:AbortSignal)=>post<AdmissionReceipt>(`${path(id)}/confirm`,body,signal)
export const retryPeopleAdmission=(id:string,request_id:string,expected_lifecycle_revision:string,signal?:AbortSignal)=>post<AdmissionReceipt>(`${path(id)}/retry`,{request_id,expected_lifecycle_revision},signal)
export const cancelPeopleAdmission=(id:string,request_id:string,expected_lifecycle_revision:string,signal?:AbortSignal)=>post<AdmissionReceipt>(`${path(id)}/cancel`,{request_id,expected_lifecycle_revision},signal)
export const fetchAdmissionItems=(id:string,plan:string,disposition?:AdmissionDisposition,cursor?:string,signal?:AbortSignal)=>apiFetch<AdmissionPage<AdmissionItem>>(`${path(id)}/items${query({plan_id:plan,disposition,cursor,limit:'50'})}`,{signal})
export const fetchAdmissionItem=(id:string,item:string,signal?:AbortSignal)=>apiFetch<AdmissionItem>(itemPath(id,item),{signal})
export const fetchAdmissionContacts=(id:string,item:string,cursor?:string,signal?:AbortSignal)=>apiFetch<AdmissionPage<AdmissionContact>>(`${itemPath(id,item)}/contacts${query({cursor,limit:'50'})}`,{signal})
export const fetchAdmissionField=(id:string,item:string,field:string,cursor?:string,signal?:AbortSignal)=>apiFetch<AdmissionFieldFragment>(`${itemPath(id,item)}/fields/${encodeURIComponent(field)}${query({cursor,limit:'16384'})}`,{signal})
export const fetchAdmissionResults=(id:string,cursor?:string,signal?:AbortSignal)=>apiFetch<{results:AdmissionResult[];next_cursor:string|null}>(`${path(id)}/results${query({cursor,limit:'50'})}`,{signal})
export const fetchAdmissionProvenance=(person:string,signal?:AbortSignal)=>apiFetch<AdmissionProvenance>(`/people/${encodeURIComponent(person)}/admission-provenance`,{signal})
export const fetchAdmissionProvenanceContacts=(person:string,cursor?:string,signal?:AbortSignal)=>apiFetch<ProvenancePage>(`/people/${encodeURIComponent(person)}/admission-provenance/contacts${query({cursor,limit:'50'})}`,{signal})
export const fetchAdmissionProvenanceField=(person:string,field:string,cursor?:string,signal?:AbortSignal)=>apiFetch<ProvenanceFieldFragment>(`/people/${encodeURIComponent(person)}/admission-provenance/fields/${encodeURIComponent(field)}${query({cursor,limit:'16384'})}`,{signal})
export const usePeopleAdmissionAccess=()=>useImportAccess('people-admissions')
export const admissionActive=(v?:PeopleAdmission)=>!!v && ['preparing','queued','running'].includes(v.state)
export const admissionLabel=(value:string)=>({preparing:'Preparing qualified People',ready:'Ready for admission review',queued:'Queued',running:'Admitting People',paused:'Paused',completed:'Completed',cancelled:'Cancelled',eligible:'Eligible new Person',already_imported:'Already in original import',already_admitted:'Already admitted',excluded_original:'Excluded: present at original boundary',held_baseline_gap:'Held: original boundary gap',held_evidence_gap:'Held: retained evidence gap',held_mapping_gap:'Held: inherited mapping gap',held_identity:'Held: source identity',held_target:'Held: target unavailable',settled:'Admitted'} as Record<string,string>)[value] ?? 'Unrecognized admission detail'
export const admissionAccessError=(e:unknown)=>e instanceof Error && 'status' in e && ([401,403].includes((e as {status:number}).status))
