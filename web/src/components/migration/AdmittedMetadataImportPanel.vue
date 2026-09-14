<script setup lang="ts">
import { computed, ref } from 'vue'
import { useMutation, useQuery, useQueryClient } from '@tanstack/vue-query'
import Card from '../Card.vue'
import FormField from '../FormField.vue'
import { buttonClasses, INPUT_CLASSES } from '../../lib/controls'
import { describeApiError } from '../../lib/errors'
import { fetchAdmittedMetadataImport, fetchAdmittedMetadataImports, prepareAdmittedMetadataImport, type AdmittedMetadataImport } from '../../api/admittedMetadataImports'
import { useMe } from '../../api/queries'

const { data: me } = useMe()
const client = useQueryClient()
const admissionId = ref('')
const reportId = ref('')
const selectedId = ref('')
const canAdmin = computed(() => me.value?.organization?.role === 'admin')
const key = computed(() => ['admitted-metadata-imports', me.value?.organization?.id ?? '', me.value?.user.id ?? ''])
const listing = useQuery({ queryKey: key, queryFn: ({ signal }) => fetchAdmittedMetadataImports(undefined, undefined, signal), enabled: canAdmin, retry: false })
const detail = useQuery({ queryKey: computed(() => [...key.value, selectedId.value]), queryFn: ({ signal }) => fetchAdmittedMetadataImport(selectedId.value, signal), enabled: computed(() => canAdmin.value && selectedId.value !== ''), retry: false })
const prepare = useMutation({ mutationFn: prepareAdmittedMetadataImport, onSuccess: value => { selectedId.value = value.import.id; void client.invalidateQueries({ queryKey: key.value }) } })
function submit() { if (!canAdmin.value || !admissionId.value || !reportId.value || prepare.isPending.value) return; prepare.mutate({ request_id: crypto.randomUUID(), admission_id: admissionId.value, source_report_id: reportId.value }) }
function choose(row: AdmittedMetadataImport) { selectedId.value = row.id }
</script>
<template>
  <section data-testid="admitted-metadata-import-panel" class="min-w-0 space-y-5">
    <Card>
      <h2 class="text-section font-semibold">Admitted People metadata</h2>
      <p class="mt-1 text-small text-text-muted">Prepare first tag and custom-field coverage for one terminal admission cohort. The workspace remains under administrator review.</p>
      <p v-if="!canAdmin" role="status" class="mt-3 text-small text-text-muted">A verified administrator session is required.</p>
      <template v-else>
        <FormField v-slot="{ id }" label="Terminal admission ID" bare class="mt-4"><input :id="id" v-model.trim="admissionId" :class="INPUT_CLASSES" autocomplete="off"></FormField>
        <FormField v-slot="{ id }" label="Qualified core-change report ID" bare class="mt-3"><input :id="id" v-model.trim="reportId" :class="INPUT_CLASSES" autocomplete="off"></FormField>
        <button type="button" class="mt-3" :class="buttonClasses('primary')" :disabled="!admissionId || !reportId || prepare.isPending.value" @click="submit">Prepare admitted metadata plan</button>
        <p v-if="prepare.error.value" role="alert" class="mt-3 text-small text-danger">{{ describeApiError(prepare.error.value, 'Could not prepare the admitted metadata cohort.') }}</p>
      </template>
    </Card>
    <Card v-if="listing.data.value?.imports.length">
      <h3 class="text-section font-semibold">Prepared cohorts</h3>
      <ul class="mt-3 space-y-2"><li v-for="row in listing.data.value.imports" :key="row.id"><button type="button" :class="buttonClasses('ghost')" @click="choose(row)">{{ row.admission_id }} · {{ row.cohort_counts.settled_people }} settled People · {{ row.state }}</button></li></ul>
    </Card>
    <Card v-if="detail.data.value">
      <h3 class="text-section font-semibold">Cohort status</h3>
      <p class="mt-2 text-small text-text-muted">{{ detail.data.value.cohort_counts.settled_people }} settled People. Shared catalog claims are {{ detail.data.value.shared_claims_ready ? 'ready' : 'not ready' }}.</p>
      <p v-if="detail.data.value.remainder.available" class="mt-2 text-small text-text-muted">A cancelled cohort has exact unprocessed work available for its immutable successor.</p>
    </Card>
  </section>
</template>
