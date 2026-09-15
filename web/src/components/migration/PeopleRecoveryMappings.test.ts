import { flushPromises, mount } from '@vue/test-utils'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { apiFetch, ApiError } from '../../api/client'
import { queryKeys } from '../../api/queries'
import { resetWorkspace } from '../../workspaceLifecycle'
import type { PeopleAdmission } from '../../api/peopleAdmissions'
import PeopleRecoveryMappings from './PeopleRecoveryMappings.vue'
vi.mock('../../api/client',async original=>({...await original<typeof import('../../api/client')>(),apiFetch:vi.fn()}))
const api=vi.mocked(apiFetch);const clean:Array<()=>void>=[]
const me={user:{id:'actor',email:'a@test',display_name:'Admin'},organization:{id:'org',name:'Org',role:'admin',workspace_mode:'migration_review',workspace_revision:'2'},platform_admin:false}
async function setup(write?:(body:string)=>Promise<unknown>){
 api.mockImplementation(async(url,options)=>{
  if(url==='/me')return me as never
  if(options?.method==='POST')return await(write?.(String(options.body))??Promise.resolve({draft_revision:'2'})) as never
  if(url.includes('/recovery-mappings/key/field'))return {fragment:'full private source key',next_cursor:null} as never
  if(url.includes('/recovery-mappings'))return {draft_revision:'9007199254740993',candidates_complete:true,candidate_count:'5',items:[{id:'key',source:{kind:'stage_label',key:'private source'},source_key_bytes:'2000',source_key_truncated:true,kind:'stage',choice_id:null,disposition:null,target_id:null,dependent_count:'5'}],next_cursor:'signed +/'} as never
  if(url.includes('/stages'))return {stages:[{id:'stage',name:'Lead'}]} as never
  if(url.includes('/members'))return {members:[]} as never
  throw Error(url)
 })
 const client=new QueryClient({defaultOptions:{queries:{retry:false}}})
 const wrapper=mount(PeopleRecoveryMappings,{props:{current:{id:'run',mode:'mapping_recovery',state:'paused',lifecycle_revision:'2',confirmed_admission_plan_id:null} as PeopleAdmission,disabled:false},global:{plugins:[[VueQueryPlugin,{queryClient:client}]]}})
 clean.push(()=>{wrapper.unmount();client.clear()});await flushPromises();return{wrapper,client}
}
beforeEach(()=>{resetWorkspace();api.mockReset();vi.stubGlobal('crypto',{randomUUID:()=> 'fixed-request'})})
afterEach(()=>{clean.splice(0).forEach(f=>f());vi.unstubAllGlobals();resetWorkspace()})
it('keeps the exact request and decimal revision on uncertain save, and does not seal while dirty',async()=>{
 let attempts=0;const {wrapper}=await setup(async()=>{if(++attempts===1)throw new ApiError(0,'network_error');return{}})
 await wrapper.get('select').setValue('existing:stage')
 const button=(label:string)=>wrapper.findAll('button').find(b=>b.text()===label)!
 expect(button('Prepare full recovery preview').attributes('disabled')).toBeDefined()
 await button('Save mapping').trigger('click');await flushPromises()
 expect(wrapper.text()).toContain('Outcome uncertain')
 await button('Retry same recovery request').trigger('click');await flushPromises()
 const writes=api.mock.calls.filter(([,v])=>v?.method==='POST');expect(writes).toHaveLength(2);expect(writes[0]![1]?.body).toBe(writes[1]![1]?.body)
 expect(JSON.parse(String(writes[0]![1]?.body))).toMatchObject({expected_draft_revision:'9007199254740993',key_id:'key',target_id:'stage'})
 expect(wrapper.emitted('reload')).toHaveLength(1)
})
it('reads full keys in bounded fragments, preserves opaque cursors, and clears after access loss',async()=>{
 const {wrapper,client}=await setup()
 await wrapper.findAll('button').find(v=>v.text().startsWith('Show more'))!.trigger('click');await flushPromises();expect(wrapper.text()).toContain('full private source key')
 await wrapper.findAll('button').find(v=>v.text()==='More mappings')!.trigger('click');await flushPromises();expect(api.mock.calls.some(([url])=>url.includes('cursor=signed+%2B%2F'))).toBe(true)
 client.setQueryData(queryKeys.me,{...me,organization:{...me.organization,id:'other',role:'member'}});await flushPromises();expect(wrapper.text()).not.toContain('private source')
})
