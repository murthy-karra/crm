import { apiFetch } from './client'
import { useImportAccess } from './imports'
export type AdmissionPlan={id:string;revision:string;digest:string;expires_at:string|null;counts:{eligible:string;held:string;total:string}}
export type PeopleAdmission={id:string;parent_import_id:string;report_id:string;state:string;lifecycle_revision:string;plan?:AdmissionPlan;actions?:{confirm?:boolean;cancel?:boolean;retry?:boolean}}
const base='/migrations/fub/people-admissions'
const post=<T>(url:string,body:unknown)=>apiFetch<T>(url,{method:'POST',body:JSON.stringify(body)})
export function usePeopleAdmissionAccess(){return useImportAccess('people-admissions')}
export function fetchPeopleAdmissions(parent_import_id:string,cursor?:string,signal?:AbortSignal){const q=new URLSearchParams({parent_import_id});if(cursor)q.set('cursor',cursor);return apiFetch<{items:PeopleAdmission[];next_cursor:string|null}>(`${base}?${q}`,{signal})}
export function fetchPeopleAdmission(id:string,signal?:AbortSignal){return apiFetch<PeopleAdmission>(`${base}/${id}`,{signal})}
export function preparePeopleAdmission(report_id:string,request_id:string){return post<{admission_id:string;state:string}>(base,{report_id,request_id})}
export function confirmPeopleAdmission(id:string,body:Record<string,unknown>){return post<{admission_id:string;state:string}>(`${base}/${id}/confirm`,body)}
export function cancelPeopleAdmission(id:string,request_id:string,expected_lifecycle_revision:string){return post<{admission_id:string;state:string}>(`${base}/${id}/cancel`,{request_id,expected_lifecycle_revision:Number(expected_lifecycle_revision)})}
export function retryPeopleAdmission(id:string,request_id:string,expected_lifecycle_revision:string){return post<{admission_id:string;state:string}>(`${base}/${id}/retry`,{request_id,expected_lifecycle_revision:Number(expected_lifecycle_revision)})}
export function fetchAdmissionProvenance(person:string,signal?:AbortSignal){return apiFetch<Record<string,unknown>>(`/api/people/${person}/admission-provenance`,{signal})}
export function fetchAdmissionProvenanceContacts(person:string,signal?:AbortSignal){return apiFetch<{items:unknown[]}>(`/api/people/${person}/admission-provenance/contacts`,{signal})}
