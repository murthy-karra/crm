<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { useQuery } from '@tanstack/vue-query'
import { ApiError } from '../../api/client'
import { fetchPersonProvenance, useImportAccess, type ImportFieldRequest } from '../../api/imports'
import { buttonClasses } from '../../lib/controls'
import { describeApiError } from '../../lib/errors'
import { snapshotTime } from './format'
import ImportFieldViewer from './ImportFieldViewer.vue'
const props = defineProps<{ personId: string }>()
const access = useImportAccess()
const selected = ref<{ request: ImportFieldRequest; title: string } | null>(null)
const key = computed(() => [...access.prefix.value, 'provenance', props.personId])
const reading = useQuery({ queryKey: key, queryFn: ({ signal }) => access.read(key.value, () => key.value, () => fetchPersonProvenance(props.personId, signal)), enabled: computed(() => access.enabled.value && !!props.personId), retry: false, gcTime: 0 })
const detail = computed(() => access.enabled.value ? reading.data.value : undefined)
const absent = computed(() => reading.error.value instanceof ApiError && reading.error.value.status === 404)
watch(() => JSON.stringify([access.scope.value, props.personId]), () => { selected.value = null }, { flush: 'sync' })
onBeforeUnmount(() => access.remove(key.value))
function inspect(fieldKey: string, title: string) { selected.value = { request: { kind: 'provenance', personId: props.personId, fieldKey }, title } }
</script>
<template>
  <section
    v-if="access.enabled.value && !absent"
    class="min-w-0"
    aria-label="Imported source information"
  >
    <h2 class="text-section font-semibold text-text">
      Imported source information
    </h2>
    <p
      v-if="reading.isPending.value"
      class="mt-2 text-small text-text-muted"
    >
      Loading imported source information…
    </p>
    <p
      v-if="reading.error.value"
      role="alert"
      class="mt-2 text-small text-danger"
    >
      {{ describeApiError(reading.error.value, 'Could not load imported source information.') }}
    </p>
    <button
      v-if="reading.error.value"
      type="button"
      :class="buttonClasses('secondary')"
      @click="reading.refetch()"
    >
      Retry source information
    </button>
    <template v-if="detail">
      <p class="mt-2 break-all text-small text-text-muted">
        FUB Person {{ detail.source_id }} · Imported {{ snapshotTime(detail.committed_at) }}
      </p>
      <p class="mt-1 text-small text-text-muted">
        Original source information is separate from Inquiries and activity. This workspace remains for administrator review.
      </p>
      <dl class="mt-3 grid min-w-0 gap-3 sm:grid-cols-2">
        <div
          v-for="field in detail.source.provenance.fields"
          :key="field.field_key"
          class="min-w-0"
        >
          <dt class="break-all text-small font-medium">
            {{ field.label }}{{ field.label_abbreviated ? ' (label abbreviated)' : '' }}
          </dt>
          <dd class="whitespace-pre-wrap break-all text-small">
            {{ field.value }}
          </dd>
          <dd
            v-if="field.abbreviated"
            class="text-small text-text-muted"
          >
            Abbreviated · {{ field.full_utf8_bytes }} UTF-8 bytes
          </dd>
          <dd>
            <button
              type="button"
              :class="buttonClasses('ghost')"
              @click="inspect(field.field_key, field.label)"
            >
              Inspect {{ field.label }}
            </button>
          </dd>
        </div>
      </dl>
      <p
        v-if="detail.source.provenance.abbreviated"
        class="mt-2 text-small text-text-muted"
      >
        Showing part of {{ detail.source.provenance.total_count }} retained source fields.
      </p>
      <div class="mt-3 flex flex-wrap gap-2">
        <button
          type="button"
          :class="buttonClasses('secondary')"
          @click="inspect('provenance', 'All retained source fields')"
        >
          Inspect all source fields
        </button>
        <button
          v-if="detail.source.contacts"
          type="button"
          :class="buttonClasses('secondary')"
          @click="inspect('contacts', 'Imported contact values')"
        >
          Inspect imported contacts
        </button>
      </div>
      <details class="mt-3 text-small text-text-muted">
        <summary class="cursor-pointer">
          Import references
        </summary><dl class="mt-2 break-all">
          <dt>Import</dt><dd>{{ detail.import_id }}</dd><dt>Plan</dt><dd>{{ detail.plan_id }}</dd><dt>Snapshot</dt><dd>{{ detail.snapshot_id }}</dd><dt>Source record</dt><dd>{{ detail.source_record_id }}</dd><dt>Capture</dt><dd>{{ detail.capture_id }}</dd>
        </dl>
      </details>
      <ImportFieldViewer
        v-if="selected"
        :request="selected.request"
        :title="selected.title"
      />
    </template>
  </section>
</template>
