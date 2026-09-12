<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useMetadataAccess, type MetadataFieldRequest, type MetadataSummary } from '../../api/metadataImports'
import MetadataFieldViewer from './MetadataFieldViewer.vue'
import { buttonClasses } from '../../lib/controls'
const props = defineProps<{ source: MetadataSummary; operations?: MetadataSummary; request: MetadataFieldRequest; title: string }>()
const access = useMetadataAccess()
const field = ref<{ key: string; title: string } | null>(null)
const sections = computed(() => [{ name: 'Source fields', value: props.source }, ...(props.operations ? [{ name: 'Planned or committed values', value: props.operations }] : [])])
watch(() => JSON.stringify([access.scope.value, props.request]), () => { field.value = null }, { flush: 'sync' })
</script>
<template>
  <section
    v-if="access.enabled.value"
    class="mt-4 min-w-0 border-t border-border pt-4"
    :aria-label="title"
  >
    <h3 class="break-words text-body font-medium text-text">
      {{ title }}
    </h3>
    <div
      v-for="section in sections"
      :key="section.name"
      class="mt-3 min-w-0"
    >
      <h4 class="text-small font-medium">
        {{ section.name }}
      </h4>
      <dl class="mt-2 grid min-w-0 gap-3 sm:grid-cols-2">
        <div
          v-for="entry in section.value.fields"
          :key="entry.key"
          class="min-w-0"
        >
          <dt class="break-all text-small font-medium">
            {{ entry.label }}{{ entry.label_abbreviated ? ' (label abbreviated)' : '' }}
          </dt>
          <dd class="whitespace-pre-wrap break-all text-small">
            {{ entry.text }}
          </dd>
          <dd
            v-if="entry.abbreviated"
            class="text-small text-text-muted"
          >
            Abbreviated · {{ entry.full_utf8_bytes }} UTF-8 bytes
          </dd>
          <dd>
            <button
              type="button"
              class="max-w-full"
              :class="buttonClasses('ghost')"
              @click="field = { key: entry.key, title: entry.label }"
            >
              <span class="min-w-0 truncate">Inspect {{ entry.label }}</span>
            </button>
          </dd>
        </div>
      </dl>
      <p
        v-if="section.value.abbreviated"
        class="mt-2 text-small text-text-muted"
      >
        This summary is abbreviated. All {{ section.value.total_fields }} fields remain available.
      </p>
      <button
        type="button"
        class="mt-2"
        :class="buttonClasses('secondary')"
        @click="field = { key: section.value.field_key, title: section.name }"
      >
        Inspect all {{ section.name.toLowerCase() }}
      </button>
    </div>
    <MetadataFieldViewer
      v-if="field"
      :request="{ ...request, fieldKey: field.key }"
      :title="field.title"
    />
  </section>
</template>
