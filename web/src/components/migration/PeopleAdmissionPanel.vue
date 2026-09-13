<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { useQuery } from '@tanstack/vue-query'
import Card from '../Card.vue'
import FormField from '../FormField.vue'
import ConfirmDialog from '../ConfirmDialog.vue'
import { buttonClasses, INPUT_CLASSES } from '../../lib/controls'
import { describeApiError } from '../../lib/errors'
import { fetchImport, fetchImports, importAccessError, uncertainImportError } from '../../api/imports'
import { fetchCoreChangeReports } from '../../api/coreChangeReports'
import { admissionActive, admissionDispositions, admissionLabel, cancelPeopleAdmission, confirmPeopleAdmission, fetchAdmissionContacts, fetchAdmissionField, fetchAdmissionItem, fetchAdmissionItems, fetchAdmissionResults, fetchPeopleAdmission, fetchPeopleAdmissions, preparePeopleAdmission, repreviewPeopleAdmission, retryPeopleAdmission, usePeopleAdmissionAccess, type AdmissionConfirm, type AdmissionItem } from '../../api/peopleAdmissions'
import { snapshotTime } from './format'

const props=defineProps<{refreshWorkspace:()=>Promise<void>}>()
const access=usePeopleAdmissionAccess(); const parentId=ref(''); const reportId=ref(''); const admissionId=ref('')
const parentCursor=ref(''); const reportCursor=ref(''); const listCursor=ref(''); const itemCursor=ref(''); const resultCursor=ref(''); const contactCursor=ref(''); const fieldCursor=ref('')
const disposition=ref(''); const selected=ref<AdmissionItem>(); const selectedField=ref(''); const acknowledgement=ref(false); const mappings=ref(false); const distinct=ref(false); const hold=ref(false)
const action=ref<'confirm'|'cancel'|null>(null); const error=ref(''); const pending=ref(false); const now=ref(Date.now()); const timer=setInterval(()=>{now.value=Date.now()},1000)
type Intent={body:string;kind:'prepare'|'confirm'|'plans'|'retry'|'cancel';id?:string;identity:string;parent:string}
const intent=ref<Intent|null>(null); let disposed=false; let identityEpoch=0; let authorityEpoch=0; let selectionEpoch=0
const key=(...v:unknown[])=>[...access.prefix.value,...v]
const parents=useQuery({queryKey:computed(()=>key('parents',parentCursor.value)),enabled:access.enabled,retry:false,gcTime:0,queryFn:({signal})=>access.read(key('parents',parentCursor.value),()=>key('parents',parentCursor.value),()=>fetchImports(parentCursor.value||undefined,signal))})
const parent=useQuery({queryKey:computed(()=>key('parent',parentId.value)),enabled:computed(()=>access.enabled.value&&!!parentId.value),retry:false,gcTime:0,queryFn:({signal})=>access.read(key('parent',parentId.value),()=>key('parent',parentId.value),()=>fetchImport(parentId.value,signal))})
const reports=useQuery({queryKey:computed(()=>key('reports',parentId.value,reportCursor.value)),enabled:computed(()=>access.enabled.value&&!!parentId.value),retry:false,gcTime:0,queryFn:({signal})=>access.read(key('reports',parentId.value,reportCursor.value),()=>key('reports',parentId.value,reportCursor.value),()=>fetchCoreChangeReports(parentId.value,reportCursor.value||undefined,signal))})
const listing=useQuery({queryKey:computed(()=>key('list',parentId.value,listCursor.value)),enabled:computed(()=>access.enabled.value&&!!parentId.value),retry:false,gcTime:0,queryFn:({signal})=>access.read(key('list',parentId.value,listCursor.value),()=>key('list',parentId.value,listCursor.value),()=>fetchPeopleAdmissions(parentId.value,listCursor.value||undefined,signal))})
const detail=useQuery({queryKey:computed(()=>key('detail',admissionId.value)),enabled:computed(()=>access.enabled.value&&!!admissionId.value),retry:false,gcTime:0,queryFn:({signal})=>access.read(key('detail',admissionId.value),()=>key('detail',admissionId.value),()=>fetchPeopleAdmission(admissionId.value,signal)),refetchInterval:q=>admissionActive(q.state.data)?2000:false})
const current=computed(()=>detail.data.value?.parent_import_id===parentId.value?detail.data.value:undefined); const plan=computed(()=>current.value?.plan)
const items=useQuery({queryKey:computed(()=>key('items',admissionId.value,plan.value?.id,disposition.value,itemCursor.value)),enabled:computed(()=>access.enabled.value&&!!admissionId.value&&!!plan.value),retry:false,gcTime:0,queryFn:async({signal})=>access.read(key('items',admissionId.value,plan.value?.id,disposition.value,itemCursor.value),()=>key('items',admissionId.value,plan.value?.id,disposition.value,itemCursor.value),async()=>{const v=await fetchAdmissionItems(admissionId.value,plan.value!.id,disposition.value as never||undefined,itemCursor.value||undefined,signal);if(v.plan_id!==plan.value!.id||v.plan_revision!==plan.value!.revision)throw Error('Preview changed');return v})})
const item=useQuery({queryKey:computed(()=>key('item',admissionId.value,selected.value?.id)),enabled:computed(()=>access.enabled.value&&!!selected.value),retry:false,gcTime:0,queryFn:async({signal})=>access.read(key('item',admissionId.value,selected.value?.id),()=>key('item',admissionId.value,selected.value?.id),async()=>{const v=await fetchAdmissionItem(admissionId.value,selected.value!.id,signal);if(v.plan_id!==plan.value?.id||v.plan_revision!==plan.value?.revision)throw Error('Preview changed');return v})})
const contacts=useQuery({queryKey:computed(()=>key('contacts',admissionId.value,selected.value?.id,contactCursor.value)),enabled:computed(()=>access.enabled.value&&!!selected.value&&!!item.data.value),retry:false,gcTime:0,queryFn:async({signal})=>access.read(key('contacts',admissionId.value,selected.value?.id,contactCursor.value),()=>key('contacts',admissionId.value,selected.value?.id,contactCursor.value),async()=>{const v=await fetchAdmissionContacts(admissionId.value,selected.value!.id,contactCursor.value||undefined,signal);if(v.plan_id!==plan.value?.id||v.plan_revision!==plan.value?.revision)throw Error('Preview changed');return v})})
const field=useQuery({queryKey:computed(()=>key('field',admissionId.value,selected.value?.id,selectedField.value,fieldCursor.value)),enabled:computed(()=>access.enabled.value&&!!selected.value&&!!selectedField.value),retry:false,gcTime:0,queryFn:async({signal})=>access.read(key('field',admissionId.value,selected.value?.id,selectedField.value,fieldCursor.value),()=>key('field',admissionId.value,selected.value?.id,selectedField.value,fieldCursor.value),async()=>{const v=await fetchAdmissionField(admissionId.value,selected.value!.id,selectedField.value,fieldCursor.value||undefined,signal);if(v.plan_id!==plan.value?.id||v.plan_revision!==plan.value?.revision||v.field!==selectedField.value)throw Error('Field changed');return v})})
const results=useQuery({queryKey:computed(()=>key('results',admissionId.value,resultCursor.value)),enabled:computed(()=>access.enabled.value&&!!admissionId.value&&['running','paused','completed','cancelled'].includes(current.value?.state??'')),retry:false,gcTime:0,queryFn:({signal})=>access.read(key('results',admissionId.value,resultCursor.value),()=>key('results',admissionId.value,resultCursor.value),()=>fetchAdmissionResults(admissionId.value,resultCursor.value||undefined,signal))})
const fieldSummaries=computed(()=>item.data.value?.fields??{}); const expired=computed(()=>!!plan.value?.expires_at&&Date.parse(plan.value.expires_at)<=now.value); const canConfirm=computed(()=>!!current.value?.actions.confirm&&!!plan.value&&plan.value.counts.eligible!=='0'&&!expired.value&&acknowledgement.value&&mappings.value&&distinct.value&&hold.value&&!pending.value)
function resetDetail(){itemCursor.value='';resultCursor.value='';contactCursor.value='';fieldCursor.value='';selected.value=undefined;selectedField.value='';acknowledgement.value=mappings.value=distinct.value=hold.value=false;action.value=null}
watch(parentId,()=>{selectionEpoch++;reportId.value='';admissionId.value='';reportCursor.value='';listCursor.value='';resetDetail()}); watch(reportId,()=>{selectionEpoch++},{flush:'sync'}); watch([admissionId,disposition],()=>{selectionEpoch++;resetDetail()}); watch(() => `${plan.value?.id}:${plan.value?.revision}:${plan.value?.digest}`,()=>resetDetail(),{flush:'sync'}); watch(access.scope,()=>{authorityEpoch++},{flush:'sync'}); watch(access.identity,()=>{identityEpoch++;intent.value=null;pending.value=false;error.value='';parentId.value='';reportId.value='';admissionId.value='';resetDetail()},{flush:'sync'})
watch([parents.error,parent.error,reports.error,listing.error,detail.error,items.error,item.error,contacts.error,field.error,results.error],v=>{if(v.some(importAccessError)){access.denied.value=true;access.remove(access.prefix.value);void props.refreshWorkspace()}})
function reload(){for(const q of [parents,parent,reports,listing,detail,items,item,contacts,field,results])void q.refetch()}
async function dispatch(value:Intent, replay=false){
  if(pending.value||!access.enabled.value||value.identity!==access.identity.value||(!replay&&intent.value))return
  const identity=identityEpoch;const authority=authorityEpoch;const selection=selectionEpoch
  const sameIdentity=()=>!disposed&&identity===identityEpoch&&value.identity===access.identity.value
  const sameRequest=()=>sameIdentity()&&access.enabled.value&&authority===authorityEpoch&&selection===selectionEpoch
  if(!replay)intent.value=value; pending.value=true; error.value=''; action.value=null
  try{const b=JSON.parse(value.body) as never;const receipt=value.kind==='prepare'?await preparePeopleAdmission((b as {report_id:string}).report_id,(b as {request_id:string}).request_id):value.kind==='confirm'?await confirmPeopleAdmission(value.id??'',b as AdmissionConfirm):value.kind==='plans'?await repreviewPeopleAdmission(value.id??'',(b as {request_id:string}).request_id,(b as {expected_plan_revision:string}).expected_plan_revision):value.kind==='retry'?await retryPeopleAdmission(value.id??'',(b as {request_id:string}).request_id,(b as {expected_lifecycle_revision:string}).expected_lifecycle_revision):await cancelPeopleAdmission(value.id??'',(b as {request_id:string}).request_id,(b as {expected_lifecycle_revision:string}).expected_lifecycle_revision)
    if(!sameRequest()){if(sameIdentity())error.value='Access or selection changed while the request was pending. Verify the current scope, then retry the same request to recover its receipt.';return}
    admissionId.value=receipt.admission_id;intent.value=null;reload()
  }catch(e){if(!sameIdentity())return;if(importAccessError(e)){access.denied.value=true;access.remove(access.prefix.value);intent.value=null;error.value='Administrator access is required.';return}if(uncertainImportError(e)){error.value='The outcome is uncertain. Retry the same request to recover its saved receipt.'}else{intent.value=null;error.value=describeApiError(e,'Could not complete this People admission request.')}}finally{if(sameIdentity())pending.value=false}
}
function send(kind:Intent['kind'],body:Record<string,string|boolean>,id=admissionId.value){void dispatch({kind,body:JSON.stringify(body),id,identity:access.identity.value,parent:parentId.value})}
function replay(){const value=intent.value;if(value)void dispatch(value,true)}
function prepare(){if(reportId.value)void send('prepare',{request_id:crypto.randomUUID(),report_id:reportId.value})}
function confirm(){if(plan.value&&canConfirm.value&&action.value==='confirm')void send('confirm',{request_id:crypto.randomUUID(),plan_id:plan.value.id,plan_revision:plan.value.revision,plan_digest:plan.value.digest,eligible_count:plan.value.counts.eligible,acknowledged_coverage:true,acknowledged_mappings:true,acknowledged_distinct_contacts:true,acknowledged_review_hold:true})}
function lifecycle(kind:'plans'|'retry'|'cancel'){if(!current.value)return;const revision=kind==='plans'?plan.value?.revision:current.value.lifecycle_revision;if(!revision||(kind==='cancel'&&action.value!=='cancel'))return;void send(kind,{request_id:crypto.randomUUID(),[kind==='plans'?'expected_plan_revision':'expected_lifecycle_revision']:revision})}
onBeforeUnmount(()=>{disposed=true;clearInterval(timer);intent.value=null;access.remove(access.prefix.value)})
</script>
<template>
  <Card
    class="mb-6 min-w-0"
    data-testid="people-admission-panel"
  >
    <div class="flex flex-wrap justify-between gap-3">
      <div>
        <h2 class="text-section font-medium">
          Add newly observed People
        </h2><p class="mt-1 max-w-3xl text-small text-text-muted">
          Admit qualified new People from a completed retained change report. These core-only People remain in administrator migration review; notes, tasks, metadata and history are not covered.
        </p>
      </div><button
        type="button"
        :class="buttonClasses()"
        :disabled="!access.enabled.value"
        @click="reload"
      >
        Reload admissions
      </button>
    </div><template v-if="access.enabled.value">
      <div class="mt-4 grid gap-4 lg:grid-cols-2">
        <FormField
          v-slot="{id}"
          label="Completed parent import"
          bare
        >
          <select
            :id="id"
            v-model="parentId"
            :class="INPUT_CLASSES"
          >
            <option value="">
              Choose completed import
            </option><option
              v-for="p in parents.data.value?.imports.filter(v=>v.state==='completed'&&v.confirmed_plan_id)??[]"
              :key="p.id"
              :value="p.id"
            >
              {{ snapshotTime(p.created_at) }} · {{ p.id }}
            </option>
          </select><button
            :class="buttonClasses('ghost')"
            :disabled="!parents.data.value?.next_cursor"
            @click="parentCursor=parents.data.value?.next_cursor??''"
          >
            More parent imports
          </button>
        </FormField><FormField
          v-slot="{id}"
          label="Completed report for newly observed People"
          bare
        >
          <select
            :id="id"
            v-model="reportId"
            :class="INPUT_CLASSES"
            :disabled="!parentId"
          >
            <option value="">
              Choose sealed change report
            </option><option
              v-for="r in reports.data.value?.reports.filter(v=>v.state==='completed'&&v.output_revision)??[]"
              :key="r.id"
              :value="r.id"
            >
              {{ snapshotTime(r.created_at) }} · {{ r.id }}
            </option>
          </select><div class="mt-2 flex flex-wrap gap-2">
            <button
              type="button"
              :class="buttonClasses('ghost')"
              :disabled="!reports.data.value?.next_cursor||pending"
              @click="reportCursor=reports.data.value?.next_cursor??''"
            >
              More completed reports
            </button><button
              type="button"
              :class="buttonClasses('primary')"
              :disabled="!reportId||pending"
              @click="prepare"
            >
              Prepare admission preview
            </button>
          </div>
        </FormField>
      </div><FormField
        v-if="parentId"
        v-slot="{id}"
        class="mt-4"
        label="Saved People admission"
        bare
      >
        <select
          :id="id"
          v-model="admissionId"
          :class="INPUT_CLASSES"
        >
          <option value="">
            Choose saved admission
          </option><option
            v-for="a in listing.data.value?.items??[]"
            :key="a.id"
            :value="a.id"
          >
            {{ admissionLabel(a.state) }} · {{ snapshotTime(a.created_at) }}
          </option>
        </select><button
          :class="buttonClasses('ghost')"
          :disabled="!listing.data.value?.next_cursor"
          @click="listCursor=listing.data.value?.next_cursor??''"
        >
          More saved admissions
        </button>
      </FormField><section
        v-if="current"
        class="mt-5 space-y-3 rounded border border-border p-3"
      >
        <h3 class="font-medium">
          {{ admissionLabel(current.state) }}
        </h3><dl class="grid gap-1 text-small sm:grid-cols-2">
          <div>
            <dt class="text-text-muted">
              Original snapshot boundary
            </dt><dd>{{ current.source_boundary.original.snapshot_id }} · sequence {{ current.source_boundary.original.sequence }} · started {{ snapshotTime(current.source_boundary.original.started_at) }} · completed {{ snapshotTime(current.source_boundary.original.completed_at) }}</dd>
          </div>
          <div>
            <dt class="text-text-muted">
              Newer snapshot boundary
            </dt><dd>{{ current.source_boundary.newer.snapshot_id }} · sequence {{ current.source_boundary.newer.sequence }} · started {{ snapshotTime(current.source_boundary.newer.started_at) }} · completed {{ snapshotTime(current.source_boundary.newer.completed_at) }}</dd>
          </div>
        </dl><p class="text-small">
          {{ current.retained_bytes }} retained bytes · {{ current.progress.settled_items }} settled.
        </p><p class="text-small">
          Covered: {{ current.coverage.covered_families.join(', ') || 'none' }}. Deferred: {{ current.coverage.deferred_families.join(', ') || 'none' }}. {{ current.coverage.review_hold ? 'Administrator review hold remains active.' : 'No review hold is reported.' }}
        </p><p
          v-if="current.pause_reason"
          class="text-small text-danger"
        >
          {{ current.pause_reason }}
        </p><template v-if="plan">
          <dl class="grid gap-x-4 gap-y-1 text-small sm:grid-cols-2">
            <template
              v-for="(count, name) in plan.counts"
              :key="name"
            >
              <dt class="text-text-muted">
                {{ String(name).replaceAll('_', ' ') }}
              </dt><dd>{{ count }}</dd>
            </template>
          </dl><p class="text-small">
            Inherited stage and assignment mappings are frozen. Equal contacts do not merge People; each qualified source identity creates its own Person.
          </p><p
            v-if="expired"
            class="text-danger"
          >
            This preview expired. Prepare a new preview before confirmation.
          </p><div
            v-if="current.actions.confirm"
            class="space-y-2 text-small"
          >
            <label class="flex gap-2"><input
              v-model="acknowledgement"
              type="checkbox"
            > I understand only core Person/contact/stage/assignment values are covered.</label><label class="flex gap-2"><input
              v-model="mappings"
              type="checkbox"
            > I acknowledge inherited mapping targets are frozen and rechecked.</label><label class="flex gap-2"><input
              v-model="distinct"
              type="checkbox"
            > I acknowledge shared contacts create distinct People.</label><label class="flex gap-2"><input
              v-model="hold"
              type="checkbox"
            > I acknowledge the continuing administrator review hold.</label><button
              :class="buttonClasses('primary')"
              :disabled="!canConfirm"
              @click="action='confirm'"
            >
              Review exact admission
            </button>
          </div>
        </template><div class="flex flex-wrap gap-2">
          <button
            v-if="current.actions.repreview"
            :class="buttonClasses('secondary')"
            @click="lifecycle('plans')"
          >
            Prepare a new preview
          </button><button
            v-if="current.actions.retry"
            :class="buttonClasses('secondary')"
            @click="lifecycle('retry')"
          >
            Retry admission
          </button><button
            v-if="current.actions.cancel"
            :class="buttonClasses('ghost')"
            @click="action='cancel'"
          >
            Cancel admission
          </button>
        </div><FormField
          v-if="plan"
          v-slot="{id}"
          label="Admission disposition"
          bare
        >
          <select
            :id="id"
            v-model="disposition"
            :class="INPUT_CLASSES"
          >
            <option value="">
              All dispositions
            </option><option
              v-for="v in admissionDispositions"
              :key="v"
              :value="v"
            >
              {{ admissionLabel(v) }}
            </option>
          </select>
        </FormField><article
          v-for="v in items.data.value?.items??[]"
          :key="v.id"
          class="rounded border border-border p-3 text-small"
        >
          <p>Source Person {{ v.source_id ?? 'unavailable' }} · {{ admissionLabel(v.disposition) }}</p><p>Planned Person {{ v.prospective_person_id }}</p><button
            :class="buttonClasses('secondary')"
            @click="selected=selected?.id===v.id?undefined:v"
          >
            {{ selected?.id===v.id?'Close details':'Inspect values and contacts' }}
          </button>
        </article><p
          v-if="items.data.value?.items?.length===0"
          class="text-small"
        >
          No items on this page.
        </p><button
          :class="buttonClasses('ghost')"
          :disabled="!items.data.value?.next_cursor"
          @click="itemCursor=items.data.value?.next_cursor??''"
        >
          More admission items
        </button><section
          v-if="item.data.value"
          class="space-y-2 border-t pt-3 text-small"
        >
          <h4 class="font-medium">
            Proposed core provenance values
          </h4><dl>
            <template
              v-for="(summary, name) in fieldSummaries"
              :key="name"
            >
              <dt class="text-text-muted">
                {{ name }}
              </dt><dd class="whitespace-pre-wrap break-all">
                {{ summary.prefix }}
              </dd><dd>{{ summary.total_bytes }} UTF-8 bytes{{ summary.truncated ? '; prefix is truncated' : '' }}</dd>
            </template>
          </dl><p
            v-if="Object.keys(fieldSummaries).length===0"
            class="text-text-muted"
          >
            No retained core provenance fields were published for this item.
          </p><p
            v-if="item.data.value.held_reasons.length"
            class="text-danger"
          >
            Held reason: {{ item.data.value.held_reasons.join(', ') }}.
          </p><button
            v-for="(summary, name) in fieldSummaries"
            v-show="summary.truncated"
            :key="`field-${name}`"
            :class="buttonClasses('ghost')"
            @click="selectedField=String(name);fieldCursor=''"
          >
            Read complete {{ name }} in pages
          </button><section
            v-if="selectedField"
            class="rounded border p-2"
          >
            <p>{{ field.data.value?.offset }} / {{ field.data.value?.total_bytes }} UTF-8 bytes</p><p class="max-h-64 overflow-auto whitespace-pre-wrap break-all">
              {{ field.data.value?.fragment }}
            </p><button
              :class="buttonClasses('ghost')"
              :disabled="!field.data.value?.next_cursor"
              @click="fieldCursor=field.data.value?.next_cursor??''"
            >
              More field text
            </button><button
              :class="buttonClasses('ghost')"
              @click="selectedField=''"
            >
              Close field
            </button>
          </section><h4 class="font-medium">
            Ordered contacts
          </h4><p
            v-for="c in contacts.data.value?.contacts??[]"
            :key="c.id"
            class="break-all"
          >
            {{ c.kind }} · {{ c.import_order }} · {{ c.primary?'primary':'secondary' }} · {{ c.value.value }}
          </p><button
            :class="buttonClasses('ghost')"
            :disabled="!contacts.data.value?.next_cursor"
            @click="contactCursor=contacts.data.value?.next_cursor??''"
          >
            More contacts
          </button>
        </section><section
          v-if="results.data.value"
          class="border-t pt-3 text-small"
        >
          <h4 class="font-medium">
            Settled results
          </h4><p
            v-for="r in results.data.value.results"
            :key="r.id"
          >
            {{ r.source_id }} · {{ admissionLabel(r.disposition) }} <RouterLink
              v-if="r.person_id"
              :to="`/people/${encodeURIComponent(r.person_id)}`"
              class="text-accent underline"
            >
              Open Person
            </RouterLink>
          </p><button
            :class="buttonClasses('ghost')"
            :disabled="!results.data.value.next_cursor"
            @click="resultCursor=results.data.value?.next_cursor??''"
          >
            More settled results
          </button>
        </section>
      </section><p
        v-if="error"
        role="alert"
        class="mt-3 text-danger"
      >
        {{ error }} <button
          v-if="intent"
          :class="buttonClasses('ghost')"
          @click="replay"
        >
          Retry the same request
        </button>
      </p><p
        v-if="!access.enabled.value"
        class="mt-3 text-danger"
      >
        This review requires the current Organization administrator session.
      </p><ConfirmDialog
        :visible="action==='confirm'"
        title="Confirm exact People admission"
        message="Only the frozen eligible count and core coverage will be admitted. The review hold continues."
        confirm-label="Confirm admission"
        :is-pending="pending"
        @update:visible="action=null"
        @confirm="confirm"
      /><ConfirmDialog
        :visible="action==='cancel'"
        title="Cancel admission"
        message="Settled identities remain consumed. A later same-report remainder requires a new preview."
        confirm-label="Cancel future admission writes"
        confirm-variant="danger"
        :is-pending="pending"
        @update:visible="action=null"
        @confirm="lifecycle('cancel')"
      />
    </template>
  </Card>
</template>
