import { apiFetch } from './client'
import { useImportAccess } from './imports'
export type Family = 'events' | 'calls' | 'text_messages'
export type Disposition = 'eligible' | 'equal_repeat' | 'excluded' | 'held' | 'imported' | 'already_present'
export interface Plan { id:string; state:'building'|'ready'|'expired'; expires_at:string|null; counts:Record<string, string> }
export interface Root { id:string; state:'preparing'|'ready'|'queued'|'running'|'paused'|'completed'|'cancelled'; phase:string; revision:string; admission_id:string; history_capture_id:string; workspace_revision:string; source_binding:Record<string, unknown>; coverage:{ streams:Array<{ family:Family; state:string; reported_total:string|null }> }; latest_plan:Plan; results:Record<string,string>; current_attempt_id:string|null; pause_reason:string|null; run_byte_limit:string; retained_bytes:string; reserved_bytes:string; actions:{confirm:boolean;resume:boolean;cancel:boolean;remainder:boolean} }
export interface Envelope { import: Root }
export interface Page<T> { manifests?:T[]; results?:T[]; issues?:T[]; next_cursor:string|null }
const root='/migrations/fub/admitted-history-imports'; const path=(id:string)=>`${root}/${encodeURIComponent(id)}`
const q=(v:Record<string,string|number|undefined>)=>{const p=new URLSearchParams();for(const [k,x] of Object.entries(v))if(x!==undefined&&x!=='')p.set(k,String(x));return `?${p}`}
const post=(url:string,body:unknown,signal?:AbortSignal)=>apiFetch<Envelope>(url,{method:'POST',body:JSON.stringify(body),signal})
export const fetchAdmittedHistoryImports=(admission?:string,cursor?:string,signal?:AbortSignal)=>apiFetch<{imports:Root[];next_cursor:string|null}>(root+q({admission_id:admission,cursor,limit:25}),{signal})
export const fetchAdmittedHistoryImport=(id:string,signal?:AbortSignal)=>apiFetch<Root>(path(id),{signal})
export const prepareAdmittedHistory=(body:{request_id:string;admission_id:string;history_capture_id:string},signal?:AbortSignal)=>post(root,body,signal)
export const confirmAdmittedHistory=(id:string,body:{request_id:string;plan_id:string;expected_revision:string;acknowledge_held:boolean;acknowledge_coverage:boolean})=>post(`${path(id)}/confirm`,body)
export const historyAction=(id:string,action:'resume'|'cancel',body:{request_id:string;expected_revision:string})=>post(`${path(id)}/${action}`,body)
export const increaseAdmittedHistoryBudget=(id:string,body:{request_id:string;expected_revision:string;run_byte_limit:string})=>post(`${path(id)}/budget`,body)
export const createAdmittedHistoryRemainder=(id:string,body:{request_id:string;attempt_id:string;expected_revision:string})=>post(`${path(id)}/remainder`,body)
export const fetchAdmittedHistoryPage=(id:string,plan:string,kind:'manifests'|'results'|'issues',filters:{family?:Family;disposition?:Disposition}={},cursor?:string,signal?:AbortSignal)=>apiFetch<Page<Record<string,unknown>>>(`${path(id)}/plans/${encodeURIComponent(plan)}/${kind}${q({...filters,cursor,limit:25})}`,{signal})
export const fetchAdmittedHistoryRemainder=(id:string,signal?:AbortSignal)=>apiFetch<{remainder:Record<string,string>|null}>(`${path(id)}/remainder`,{signal})
export const useAdmittedHistoryAccess=()=>useImportAccess('admitted-history-imports')
export const historyActive=(r?:Root)=>!!r&&['preparing','queued','running'].includes(r.state)
