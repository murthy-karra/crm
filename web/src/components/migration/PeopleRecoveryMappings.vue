<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { useQuery } from '@tanstack/vue-query'
import { buttonClasses, INPUT_CLASSES } from '../../lib/controls'
import { describeApiError } from '../../lib/errors'
import { fetchImportMembers, fetchImportStages, importAccessError, uncertainImportError } from '../../api/imports'
import { fetchRecoveryMappingField, fetchRecoveryMappings, postRecoveryMapping, sealRecoveryMappings, usePeopleAdmissionAccess, type PeopleAdmission, type RecoveryMapping } from '../../api/peopleAdmissions'
const props=defineProps<{current:PeopleAdmission;disabled:boolean}>()
const emit=defineEmits<{reload:[];denied:[];busy:[value:boolean]}>()
const access=usePeopleAdmissionAccess();const cursors=ref(['']);const error=ref('');const pending=ref(false)
const draft=ref<Record<string,string>>({})
const expanded=ref<Record<string,{text:string;next:string|null}>>({});const expanding=ref('')
async function expand(row:RecoveryMapping){
 if(expanding.value)return
 const expected=access.scope.value;const root=props.current.id;const revision=props.current.lifecycle_revision
 expanding.value=row.id
 try{const previous=expanded.value[row.id];const result=await fetchRecoveryMappingField(root,row.id,previous?.next??undefined)
 if(disposed||expected!==access.scope.value||root!==props.current.id||revision!==props.current.lifecycle_revision)return
 expanded.value[row.id]={text:(previous?.text??'')+result.fragment,next:result.next_cursor}
 }catch(e){if(importAccessError(e)){access.denied.value=true;access.remove(access.prefix.value);emit('denied')}else error.value=describeApiError(e,'Could not load the full source key.')}
 finally{expanding.value=''}
}
const intent=ref<{id:string;identity:string;kind:'choice'|'seal';body:unknown}|null>(null)
const branch=computed(()=>[...access.prefix.value,'recovery-mappings',props.current.id,props.current.lifecycle_revision])
const key=computed(()=>[...branch.value,cursors.value.at(-1)])
const enabled=computed(()=>access.enabled.value&&props.current.mode==='mapping_recovery')
const editing=computed(()=>enabled.value&&!props.current.confirmed_admission_plan_id&&['ready','paused'].includes(props.current.state))
const busy=computed(()=>props.disabled||pending.value||!!intent.value)
const mappings=useQuery({queryKey:key,enabled,retry:false,gcTime:0,queryFn:({signal})=>access.read(key.value,()=>key.value,()=>fetchRecoveryMappings(props.current.id,cursors.value.at(-1)||undefined,signal))})
const stageKey=computed(()=>[...branch.value,'stages']);const memberKey=computed(()=>[...branch.value,'members'])
const stages=useQuery({queryKey:stageKey,enabled:editing,retry:false,gcTime:0,queryFn:({signal})=>access.read(stageKey.value,()=>stageKey.value,()=>fetchImportStages(signal))})
const members=useQuery({queryKey:memberKey,enabled:editing,retry:false,gcTime:0,queryFn:({signal})=>access.read(memberKey.value,()=>memberKey.value,()=>fetchImportMembers(signal))})
let disposed=false
watch(()=>pending.value||!!intent.value,v=>emit('busy',v),{flush:'sync'})
watch(()=>[access.scope.value,props.current.id,props.current.lifecycle_revision],()=>{cursors.value=[''];draft.value={};expanded.value={};error.value=''}, {flush:'sync'})
watch(access.identity,()=>{intent.value=null;pending.value=false},{flush:'sync'})
watch([mappings.error,stages.error,members.error],errors=>{if(errors.some(importAccessError)){access.denied.value=true;access.remove(access.prefix.value);emit('denied')}})
onBeforeUnmount(()=>{disposed=true;emit('busy',false);access.remove(branch.value)})
const selected=(r:RecoveryMapping)=>draft.value[r.id]??(r.disposition?`${r.disposition}${r.target_id?`:${r.target_id}`:''}`:'')
async function send(value:NonNullable<typeof intent.value>){
  if(!access.enabled.value||pending.value||value.identity!==access.identity.value||value.id!==props.current.id)return
  const scope=access.scope.value;intent.value=value;pending.value=true;error.value=''
  try{
    await (value.kind==='choice'?postRecoveryMapping(value.id,value.body):sealRecoveryMappings(value.id,value.body))
    if(disposed||value.identity!==access.identity.value)return
    if(scope!==access.scope.value||value.id!==props.current.id){error.value='Selection changed. Return to this recovery and retry the same request to recover its receipt.';return}
    intent.value=null;draft.value={};emit('reload');await access.client.invalidateQueries({queryKey:branch.value})
  }catch(e){if(disposed||value.identity!==access.identity.value)return;if(importAccessError(e)){intent.value=null;access.denied.value=true;access.remove(access.prefix.value);emit('denied');return}if(!uncertainImportError(e))intent.value=null;error.value=uncertainImportError(e)?'Outcome uncertain. Retry the same request.':describeApiError(e,'Could not save recovery mappings.')}
  finally{if(!disposed&&value.identity===access.identity.value)pending.value=false}
}
function save(row:RecoveryMapping){if(!editing.value||busy.value||!mappings.data.value)return;const value=selected(row);if(!value)return;const [disposition,target]=value.split(':');void send({id:props.current.id,identity:access.identity.value,kind:'choice',body:{request_id:crypto.randomUUID(),expected_draft_revision:mappings.data.value.draft_revision,key_id:row.id,disposition,target_id:target??null}})}
function preview(){if(!editing.value||busy.value||!mappings.data.value?.candidates_complete||Object.keys(draft.value).length)return;void send({id:props.current.id,identity:access.identity.value,kind:'seal',body:{request_id:crypto.randomUUID(),expected_draft_revision:mappings.data.value.draft_revision}})}
</script>
<template>
  <section
    v-if="enabled"
    class="my-4 space-y-3 rounded border border-border p-4"
  >
    <h3 class="font-semibold">
      Mappings for People never imported
    </h3>
    <p class="text-small text-text-muted">
      Choose each stage and agent explicitly. Unresolved choices stay held. Saving a mapping does not create People; the complete preview comes next.
    </p>
    <p v-if="!mappings.data.value?.candidates_complete">
      Checking retained mapping holds…
    </p>
    <div
      v-for="row in mappings.data.value?.items??[]"
      :key="row.id"
      class="space-y-2 rounded border border-border p-3"
    >
      <label
        :for="`recovery-${row.id}`"
        class="block break-words"
      >{{ expanded[row.id]?.text??row.source.key??'No source stage' }} · {{ row.dependent_count }} People</label>
      <button
        v-if="row.source_key_truncated&&(!expanded[row.id]||expanded[row.id]?.next)"
        :class="buttonClasses('ghost')"
        :disabled="!!expanding"
        @click="expand(row)"
      >
        Show more of this source key ({{ row.source_key_bytes }} bytes)
      </button>
      <select
        :id="`recovery-${row.id}`"
        :value="selected(row)"
        :class="INPUT_CLASSES"
        :disabled="!editing||busy"
        @change="draft[row.id]=($event.target as HTMLSelectElement).value"
      >
        <option value="">
          No choice saved
        </option><option value="hold">
          Keep held
        </option>
        <option
          v-if="row.kind==='assignee'"
          value="unassigned"
        >
          Explicitly unassigned
        </option>
        <option
          v-if="row.target_id"
          :value="`${row.disposition}:${row.target_id}`"
        >
          Saved target · {{ row.target_id }}
        </option>
        <template v-if="row.kind==='stage'">
          <option
            v-for="stage in stages.data.value?.stages??[]"
            :key="stage.id"
            :value="`existing:${stage.id}`"
          >
            {{ stage.name }}
          </option>
        </template>
        <template v-else>
          <option
            v-for="member in members.data.value?.members.filter(v=>v.status==='active')??[]"
            :key="member.user_id"
            :value="`member:${member.user_id}`"
          >
            {{ member.display_name }} · {{ member.email }}
          </option>
        </template>
      </select>
      <button
        :class="buttonClasses()"
        :disabled="!editing||busy||!selected(row)"
        @click="save(row)"
      >
        Save mapping
      </button>
    </div>
    <div class="flex flex-wrap gap-2">
      <button
        :class="buttonClasses('ghost')"
        :disabled="busy||cursors.length<2"
        @click="cursors.pop()"
      >
        Previous mappings
      </button>
      <button
        :class="buttonClasses('ghost')"
        :disabled="busy||!mappings.data.value?.next_cursor"
        @click="cursors.push(mappings.data.value?.next_cursor??'')"
      >
        More mappings
      </button>
      <button
        v-if="editing"
        :class="buttonClasses('primary')"
        :disabled="busy||!mappings.data.value?.candidates_complete||Object.keys(draft).length>0"
        @click="preview"
      >
        Prepare full recovery preview
      </button>
    </div>
    <p
      v-if="error"
      role="alert"
      class="text-danger"
    >
      {{ error }}
    </p>
    <button
      v-if="intent"
      :class="buttonClasses()"
      :disabled="pending"
      @click="send(intent)"
    >
      Retry same recovery request
    </button>
  </section>
</template>
