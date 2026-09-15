<script setup lang="ts">
import { computed, ref, watch, onBeforeUnmount } from 'vue'
import { useQuery } from '@tanstack/vue-query'
import Card from '../Card.vue'
import ConfirmDialog from '../ConfirmDialog.vue'
import FormField from '../FormField.vue'
import { buttonClasses, INPUT_CLASSES } from '../../lib/controls'
import { describeApiError } from '../../lib/errors'
import { fetchPeopleAdmissions } from '../../api/peopleAdmissions'
import { fetchHistoryCaptures } from '../../api/historyCaptures'
import { createAdmittedHistoryRemainder, confirmAdmittedHistory, fetchAdmittedHistoryImport, fetchAdmittedHistoryImports, fetchAdmittedHistoryPage, historyAction, historyActive, increaseAdmittedHistoryBudget, prepareAdmittedHistory, useAdmittedHistoryAccess, type Root } from '../../api/admittedHistoryImports'
defineProps<{refreshWorkspace:()=>Promise<void>}>(); const access=useAdmittedHistoryAccess(); const admissionId=ref(''); const captureId=ref(''); const selectedId=ref(''); const pending=ref(false); const uncertain=ref(false); const error=ref(''); const confirmOpen=ref(false); const held=ref(false); const coverage=ref(false); const budget=ref(''); const mode=ref<'manifests'|'results'|'issues'>('manifests'); let epoch=0; let dead=false; let replay: (()=>Promise<void>)|null=null
const admissions=useQuery({queryKey:computed(()=>[...access.prefix.value,'admissions']),enabled:access.enabled,retry:false,queryFn:({signal})=>access.read([],()=>[],()=>fetchPeopleAdmissions('',undefined,signal))})
const captures=useQuery({queryKey:computed(()=>[...access.prefix.value,'history-captures']),enabled:access.enabled,retry:false,queryFn:({signal})=>access.read([],()=>[],()=>fetchHistoryCaptures(undefined,undefined,signal))})
const roots=useQuery({queryKey:computed(()=>[...access.prefix.value,'admitted-history',admissionId.value]),enabled:computed(()=>access.enabled.value&&!!admissionId.value),retry:false,queryFn:({signal})=>access.read([],()=>[],()=>fetchAdmittedHistoryImports(admissionId.value,undefined,signal)),refetchInterval:q=>q.state.data?.imports.some(historyActive)?2000:false,refetchOnWindowFocus:'always',refetchOnReconnect:'always'})
const detail=useQuery({queryKey:computed(()=>[...access.prefix.value,'admitted-history-detail',selectedId.value]),enabled:computed(()=>access.enabled.value&&!!selectedId.value),retry:false,queryFn:({signal})=>access.read([],()=>[],()=>fetchAdmittedHistoryImport(selectedId.value,signal)),refetchInterval:q=>historyActive(q.state.data)?2000:false,refetchOnWindowFocus:'always',refetchOnReconnect:'always'})
const current=computed(()=>detail.data.value); const plan=computed(()=>current.value?.latest_plan); const pages=useQuery({queryKey:computed(()=>[...access.prefix.value,'admitted-history-page',selectedId.value,plan.value?.id,mode.value]),enabled:computed(()=>!!selectedId.value&&!!plan.value?.id),retry:false,queryFn:({signal})=>fetchAdmittedHistoryPage(selectedId.value,plan.value!.id,mode.value,{},undefined,signal)})
const qualified=computed(()=>captures.data.value?.captures.filter(c=>c.state==='completed_with_gaps'&&['events','calls','text_messages'].every(f=>c.streams.some(s=>s.family===f&&s.state==='completed')))||[])
const allowedAdmission=computed(()=>admissions.data.value?.items.filter(a=>['completed','cancelled'].includes(a.state)&&BigInt(a.progress.settled_items)>0)||[])
const canPrepare=computed(()=>!!admissionId.value&&!!captureId.value&&!pending.value); const canConfirm=computed(()=>!!current.value?.actions.confirm&&held.value&&coverage.value&&!pending.value)
watch(allowedAdmission,v=>{if(!admissionId.value&&v?.[0])admissionId.value=v[0].id}); watch(qualified,v=>{if(!captureId.value&&v?.[0])captureId.value=v[0].id}); watch(roots.data,v=>{if(!selectedId.value&&v?.imports[0])selectedId.value=v.imports[0].id}); watch([access.identity,access.scope,admissionId,captureId,selectedId],()=>{epoch++;confirmOpen.value=false;held.value=false;coverage.value=false}); onBeforeUnmount(()=>{dead=true})
async function run(work:()=>Promise<{import:Root}>){const e=epoch; pending.value=true;error.value='';try{const v=await work();if(dead||e!==epoch){uncertain.value=true;return} selectedId.value=v.import.id ?? '';uncertain.value=false;void roots.refetch();void detail.refetch()}catch(x){error.value=describeApiError(x,'Could not complete admitted history action.');uncertain.value=true;replay=()=>run(work)}finally{if(e===epoch)pending.value=false}}
function prepare(){if(canPrepare.value)run(()=>prepareAdmittedHistory({request_id:crypto.randomUUID(),admission_id:admissionId.value,history_capture_id:captureId.value}))} function confirm(){const r=current.value,p=plan.value;if(r&&p&&canConfirm.value)run(()=>confirmAdmittedHistory(r.id,{request_id:crypto.randomUUID(),plan_id:p.id,expected_revision:r.revision,acknowledge_held:true,acknowledge_coverage:true}))} function action(a:'resume'|'cancel'){const r=current.value;if(r)run(()=>historyAction(r.id,a,{request_id:crypto.randomUUID(),expected_revision:r.revision}))} function remainder(){const r=current.value;if(r?.current_attempt_id)run(()=>createAdmittedHistoryRemainder(r.id,{request_id:crypto.randomUUID(),attempt_id:r.current_attempt_id!,expected_revision:r.revision}))} function increase(){const r=current.value;if(r&&budget.value)run(()=>increaseAdmittedHistoryBudget(r.id,{request_id:crypto.randomUUID(),expected_revision:r.revision,run_byte_limit:budget.value}))}
</script>
<template>
  <Card data-testid="admitted-history-import-panel">
    <h2 class="text-section font-medium">
      Historical activity for admitted People
    </h2><p class="mt-1 text-small text-text-muted">
      Imports retained event, call and text metadata only. It never imports bodies, subjects, phone endpoints or raw source data.
    </p><p
      v-if="error"
      role="alert"
      class="mt-3 text-small text-danger"
    >
      {{ error }}
    </p><button
      v-if="uncertain&&replay"
      :class="buttonClasses()"
      @click="replay"
    >
      Retry the same request
    </button><template v-if="access.enabled.value">
      <div class="mt-4 grid gap-3 sm:grid-cols-2">
        <FormField label="Terminal People admission">
          <select
            v-model="admissionId"
            :class="INPUT_CLASSES"
          >
            <option
              v-for="a in allowedAdmission"
              :key="a.id"
              :value="a.id"
            >
              {{ a.id }}
            </option>
          </select>
        </FormField><FormField label="Qualified history capture">
          <select
            v-model="captureId"
            :class="INPUT_CLASSES"
          >
            <option
              v-for="c in qualified"
              :key="c.id"
              :value="c.id"
            >
              {{ c.id }} · {{ c.started_at }}–{{ c.completed_at }}
            </option>
          </select>
        </FormField>
      </div><button
        :class="buttonClasses('primary')"
        :disabled="!canPrepare"
        @click="prepare"
      >
        Prepare retained history
      </button><template v-if="current">
        <p class="mt-4 text-small">
          {{ current.state }} · plan {{ plan?.state }} · {{ current.retained_bytes }} retained bytes
        </p><p class="text-small">
          Coverage: <span
            v-for="s in current.coverage.streams"
            :key="s.family"
          >{{ s.family }} {{ s.state }} </span>
        </p><div class="mt-3 flex gap-2">
          <button
            v-for="m in ['manifests','results','issues']"
            :key="m"
            :class="buttonClasses()"
            @click="mode=m as typeof mode"
          >
            {{ m }}
          </button>
        </div><pre class="mt-2 max-h-48 overflow-auto text-small">{{ pages.data.value }}</pre><div
          v-if="current.actions.confirm"
          class="mt-3"
        >
          <label><input
            v-model="held"
            type="checkbox"
          > I reviewed held outcomes</label><label><input
            v-model="coverage"
            type="checkbox"
          > I acknowledge retained coverage limits</label><button
            :class="buttonClasses('primary')"
            :disabled="!canConfirm"
            @click="confirmOpen=true"
          >
            Confirm metadata import
          </button>
        </div><div class="mt-3 flex gap-2">
          <button
            v-if="current.actions.resume"
            :class="buttonClasses()"
            @click="action('resume')"
          >
            Resume
          </button><button
            v-if="current.actions.cancel"
            :class="buttonClasses('danger')"
            @click="action('cancel')"
          >
            Cancel
          </button><button
            v-if="current.actions.remainder"
            :class="buttonClasses('primary')"
            @click="remainder"
          >
            Continue never-settled remainder
          </button>
        </div><div class="mt-3">
          <input
            v-model="budget"
            :class="INPUT_CLASSES"
            inputmode="numeric"
            placeholder="New run byte limit"
          ><button
            :class="buttonClasses()"
            @click="increase"
          >
            Increase budget
          </button>
        </div>
      </template>
    </template><p
      v-else
      class="mt-3 text-small text-text-muted"
    >
      A verified Organization administrator session is required.
    </p>
  </Card><ConfirmDialog
    :visible="confirmOpen"
    title="Import admitted history metadata"
    message="Committed facts remain after cancellation; only never-settled units can continue in one exact remainder."
    :is-pending="pending"
    confirm-label="Confirm import"
    @update:visible="v=>confirmOpen=v"
    @confirm="confirm"
  />
</template>
