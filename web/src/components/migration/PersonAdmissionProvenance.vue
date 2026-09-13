<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { useQuery } from '@tanstack/vue-query'
import { ApiError } from '../../api/client'
import { fetchAdmissionProvenance, fetchAdmissionProvenanceContacts, fetchAdmissionProvenanceField, usePeopleAdmissionAccess, type ProvenanceFieldSummary } from '../../api/peopleAdmissions'
import { buttonClasses } from '../../lib/controls'
import { describeApiError } from '../../lib/errors'
const props=defineProps<{personId:string}>(); const access=usePeopleAdmissionAccess(); const cursor=ref(''); const selected=ref(''); const fieldCursor=ref('')
const key=computed(()=>[...access.prefix.value,'provenance',props.personId]); const reading=useQuery({queryKey:key,enabled:computed(()=>access.enabled.value&&!!props.personId),retry:false,gcTime:0,queryFn:({signal})=>access.read(key.value,()=>key.value,()=>fetchAdmissionProvenance(props.personId,signal))})
const contacts=useQuery({queryKey:computed(()=>[...key.value,'contacts',cursor.value]),enabled:computed(()=>!!reading.data.value&&access.enabled.value),retry:false,gcTime:0,queryFn:({signal})=>access.read([...key.value,'contacts',cursor.value],()=>[...key.value,'contacts',cursor.value],()=>fetchAdmissionProvenanceContacts(props.personId,cursor.value||undefined,signal))})
const field=useQuery({queryKey:computed(()=>[...key.value,'field',selected.value,fieldCursor.value]),enabled:computed(()=>access.enabled.value&&!!selected.value),retry:false,gcTime:0,queryFn:({signal})=>access.read([...key.value,'field',selected.value,fieldCursor.value],()=>[...key.value,'field',selected.value,fieldCursor.value],()=>fetchAdmissionProvenanceField(props.personId,selected.value,fieldCursor.value||undefined,signal))})
const absent=computed(()=>reading.error.value instanceof ApiError&&reading.error.value.status===404); const detail=computed(()=>reading.data.value); const fields=computed(()=>{const p=detail.value?.provenance;const value=(p?.fields??p?.source_fields) as Record<string,ProvenanceFieldSummary>|undefined;return Object.entries(value??{})})
watch([()=>props.personId,access.scope],()=>{cursor.value='';selected.value='';fieldCursor.value=''},{flush:'sync'}); onBeforeUnmount(()=>access.remove(key.value))
</script>
<template>
  <section
    v-if="access.enabled.value&&!absent"
    class="min-w-0"
    aria-label="Admission provenance"
  >
    <h2 class="text-section font-semibold">
      Admission provenance
    </h2><p class="mt-2 text-small text-text-muted">
      This Person was admitted from qualified retained evidence. Coverage is core-only; notes, tasks, metadata and history remain deferred.
    </p><p
      v-if="reading.isPending.value"
      class="text-small"
    >
      Loading admission provenance…
    </p><p
      v-if="reading.error.value"
      role="alert"
      class="text-danger"
    >
      {{ describeApiError(reading.error.value,'Could not load admission provenance.') }}
    </p><template v-if="detail">
      <p class="mt-2 break-all text-small">
        Admission {{ detail.admission_id }} · result {{ detail.result_id }}
      </p><p class="mt-2 text-small text-text-muted">
        These are the original admission values. Later core refreshes retain separate plans and results.
        <RouterLink
          to="/migration#admitted-people-refresh"
          class="text-accent underline"
        >
          Review later core refreshes in Migration
        </RouterLink>
        by choosing this admission cohort.
      </p><p class="text-small">
        {{ detail.coverage }}
      </p><dl class="mt-3 grid gap-3 sm:grid-cols-2">
        <div
          v-for="[name,value] in fields"
          :key="name"
        >
          <dt class="font-medium break-all">
            {{ name }}
          </dt><dd class="whitespace-pre-wrap break-all">
            {{ value.prefix }}
          </dd><dd class="text-text-muted">
            {{ value.total_bytes }} UTF-8 bytes{{ value.truncated?' · display prefix':'' }}
          </dd><button
            :class="buttonClasses('ghost')"
            @click="selected=name;fieldCursor=''"
          >
            Read full field
          </button>
        </div>
      </dl><section
        v-if="selected"
        class="mt-3 rounded border p-3"
      >
        <p>{{ field.data.value?.offset }} / {{ field.data.value?.total_bytes }} UTF-8 bytes</p><p class="max-h-72 overflow-auto whitespace-pre-wrap break-all">
          {{ field.data.value?.fragment }}
        </p><button
          :class="buttonClasses('ghost')"
          :disabled="!field.data.value?.next_cursor"
          @click="fieldCursor=field.data.value?.next_cursor??''"
        >
          More field text
        </button><button
          :class="buttonClasses('ghost')"
          @click="selected=''"
        >
          Close field
        </button>
      </section><h3 class="mt-4 font-medium">
        Ordered admitted contacts
      </h3><p
        v-for="c in contacts.data.value?.items??[]"
        :key="c.id"
        class="break-all text-small"
      >
        {{ c.kind }} · {{ c.import_order }} · {{ c.primary?'primary':'secondary' }} · {{ c.value.value }}
      </p><button
        :class="buttonClasses('ghost')"
        :disabled="!contacts.data.value?.next_cursor"
        @click="cursor=contacts.data.value?.next_cursor??''"
      >
        More contacts
      </button>
    </template>
  </section>
</template>
