<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { useQuery } from '@tanstack/vue-query'
import Card from '../Card.vue'
import MetadataEvidence from './MetadataEvidence.vue'
import MetadataFieldViewer from './MetadataFieldViewer.vue'
import MetadataTargetPicker from './MetadataTargetPicker.vue'
import { fetchMetadataAliases, fetchMetadataMappings, metadataName, useMetadataAccess, type MetadataChoice, type MetadataKind, type MetadataMapping, type MetadataPatch, type MetadataTarget } from '../../api/metadataImports'
import { buttonClasses, INPUT_CLASSES } from '../../lib/controls'
import { describeApiError } from '../../lib/errors'
import { snapshotLabel } from './format'
const props = defineProps<{ importId: string; planId: string; revision: string; canReplan: boolean; progressVersion?: string }>()
const emit = defineEmits<{ apply: [mappings: MetadataPatch[]]; dirty: [value: boolean] }>()
const access = useMetadataAccess()
const kind = ref<MetadataKind>('tag')
const cursors = ref<string[]>([''])
const drafts = ref<Record<string, MetadataChoice>>({})
const draftTargets = ref<Record<string, MetadataTarget>>({})
const selected = ref<MetadataMapping | null>(null)
const picking = ref<MetadataMapping | null>(null)
const aliasMapping = ref<MetadataMapping | null>(null)
const aliasCursors = ref<string[]>([''])
const aliasRecord = ref<string | null>(null)
const message = ref('')
const branch = computed(() => [...access.prefix.value, 'mappings', props.importId, props.planId, props.revision])
const key = computed(() => [...branch.value, kind.value, cursors.value.at(-1) ?? '', props.progressVersion ?? ''])
const enabled = computed(() => access.enabled.value && !!props.importId && !!props.planId)
const reading = useQuery({ queryKey: key, enabled, retry: false, gcTime: 0, queryFn: ({ signal }) => access.read(key.value, () => key.value, () => fetchMetadataMappings(props.importId, props.planId, kind.value, cursors.value.at(-1) || undefined, signal)) })
const aliasKey = computed(() => [...branch.value, 'aliases', aliasMapping.value?.id ?? '', aliasCursors.value.at(-1) ?? '', props.progressVersion ?? ''])
const aliases = useQuery({ queryKey: aliasKey, enabled: computed(() => enabled.value && !!aliasMapping.value), retry: false, gcTime: 0, queryFn: ({ signal }) => access.read(aliasKey.value, () => aliasKey.value, () => fetchMetadataAliases(props.importId, props.planId, aliasMapping.value!.id, aliasCursors.value.at(-1) || undefined, signal)) })
const page = computed(() => enabled.value ? reading.data.value : undefined)
const aliasPage = computed(() => enabled.value ? aliases.data.value : undefined)
const changes = computed(() => Object.keys(drafts.value).length)
watch(() => changes.value > 0, value => emit('dirty', value), { immediate: true, flush: 'sync' })
function clearInspection() { selected.value = null; picking.value = null; aliasMapping.value = null; aliasRecord.value = null; aliasCursors.value = [''] }
watch(() => JSON.stringify([access.scope.value, props.importId, props.planId, props.revision]), () => { drafts.value = {}; draftTargets.value = {}; cursors.value = ['']; message.value = ''; clearInspection() }, { flush: 'sync' })
watch(kind, () => { cursors.value = ['']; clearInspection() })
watch(() => cursors.value.at(-1), clearInspection)
watch(() => aliasCursors.value.at(-1), () => { aliasRecord.value = null })
watch(() => props.canReplan, value => { if (!value) picking.value = null })
onBeforeUnmount(() => { emit('dirty', false); access.remove(branch.value) })
function choice(row: MetadataMapping) { return drafts.value[row.id] ?? row.choice }
function encoded(value: MetadataChoice) { return value.kind === 'map_existing' ? `map_existing:${value.target_id}` : value.kind }
function targetLabel(row: MetadataMapping) { return draftTargets.value[row.id]?.label ?? row.target?.label ?? row.suggestions.find(s => s.id === (choice(row).kind === 'map_existing' ? (choice(row) as { target_id: string }).target_id : row.target_id))?.label ?? 'Selected existing target' }
function change(row: MetadataMapping, value: MetadataChoice, target?: MetadataTarget) {
  if (!enabled.value || !props.canReplan) return
  if (!drafts.value[row.id] && changes.value >= 50 && encoded(value) !== encoded(row.choice)) { message.value = 'Apply these 50 changes before choosing more mappings.'; return }
  if (encoded(value) === encoded(row.choice)) { delete drafts.value[row.id]; delete draftTargets.value[row.id] }
  else { drafts.value[row.id] = value; if (target) draftTargets.value[row.id] = target; else delete draftTargets.value[row.id] }
  message.value = ''
}
function select(row: MetadataMapping, event: Event) { const input = event.target as HTMLSelectElement; change(row, input.value === 'create_matching' ? { kind: 'create_matching' } : { kind: 'hold' }); input.value = encoded(choice(row)) }
function picked(target: MetadataTarget) { if (picking.value) change(picking.value, { kind: 'map_existing', target_id: target.id }, target); picking.value = null }
function apply() { if (enabled.value && props.canReplan && changes.value > 0 && changes.value <= 50) emit('apply', Object.entries(drafts.value).sort(([a], [b]) => a.localeCompare(b)).map(([mapping_id, value]) => ({ mapping_id, choice: { ...value } }))) }
function discard() { if (enabled.value && props.canReplan) { drafts.value = {}; draftTargets.value = {}; message.value = '' } }
function showAliases(row: MetadataMapping) { aliasMapping.value = row; aliasCursors.value = ['']; aliasRecord.value = null }
</script>
<template>
  <Card
    class="min-w-0"
    data-testid="metadata-mappings"
  >
    <h2 class="text-section font-semibold">
      Tag and field mappings
    </h2>
    <p class="mt-1 text-small text-text-muted">
      Choose matching creation or an existing target. Unchosen items stay held. Field choices and option choices require separate approval.
    </p>
    <div
      class="mt-3 flex flex-wrap gap-2"
      aria-label="Metadata mapping kind"
    >
      <button
        v-for="value in (['tag', 'field', 'option'] as const)"
        :key="value"
        type="button"
        :class="buttonClasses(kind === value ? 'primary' : 'secondary')"
        :aria-pressed="kind === value"
        @click="kind = value"
      >
        {{ value === 'tag' ? 'Tags' : value === 'field' ? 'Fields' : 'Choice options' }}
      </button>
    </div>
    <p
      v-if="!enabled"
      role="alert"
      class="mt-3 text-small text-danger"
    >
      Mappings require a current administrator session.
    </p>
    <p
      v-if="reading.isFetching.value && enabled"
      role="status"
      class="mt-3 text-small text-text-muted"
    >
      Loading mappings…
    </p>
    <p
      v-if="reading.error.value"
      role="alert"
      class="mt-3 text-small text-danger"
    >
      {{ describeApiError(reading.error.value, 'Could not load mapping choices.') }}
    </p>
    <div
      v-if="page"
      class="mt-3 min-w-0 overflow-x-auto"
    >
      <table class="w-full min-w-[640px] text-left text-small">
        <thead class="text-text-muted">
          <tr>
            <th class="p-2 font-medium">
              Source
            </th><th class="p-2 font-medium">
              Choice
            </th><th class="p-2 font-medium">
              Review
            </th>
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="row in page.items"
            :key="row.id"
            class="border-t border-border align-top"
          >
            <td class="max-w-64 p-2">
              <p class="whitespace-pre-wrap break-all font-medium">
                {{ metadataName(row.source) }}
              </p>
              <p class="text-text-muted">
                {{ row.dependent_count }} dependent items<span v-if="row.source_id"> · FUB {{ row.source_id }}</span>
              </p>
              <p
                v-if="row.source.abbreviated"
                class="text-text-muted"
              >
                Source summary abbreviated.
              </p>
              <button
                type="button"
                :class="buttonClasses('ghost')"
                @click="selected = row"
              >
                Inspect {{ metadataName(row.source) }}
              </button>
              <button
                v-if="row.kind === 'tag' && BigInt(row.alias_count) > 0n"
                type="button"
                :class="buttonClasses('ghost')"
                @click="showAliases(row)"
              >
                Review {{ row.alias_count }} tag occurrences
              </button>
            </td>
            <td class="min-w-64 max-w-80 p-2">
              <template v-if="canReplan">
                <select
                  :value="encoded(choice(row))"
                  :aria-label="`${snapshotLabel(row.kind)} choice for ${metadataName(row.source)}`"
                  :class="INPUT_CLASSES"
                  :disabled="!enabled"
                  @change="select(row, $event)"
                >
                  <option value="hold">
                    Hold for later
                  </option>
                  <option
                    v-if="row.create_matching_available"
                    value="create_matching"
                  >
                    Create matching {{ row.kind }}
                  </option>
                  <option
                    v-if="choice(row).kind === 'map_existing'"
                    :value="encoded(choice(row))"
                  >
                    Use {{ targetLabel(row) }}
                  </option>
                </select>
                <button
                  v-if="row.qualified"
                  type="button"
                  class="mt-2"
                  :class="buttonClasses('secondary')"
                  :disabled="!enabled || row.kind === 'option' && !row.field_id"
                  @click="picking = row"
                >
                  Choose existing {{ row.kind }}
                </button>
                <p
                  v-if="row.kind === 'option' && !row.field_id"
                  class="mt-1 text-text-muted"
                >
                  For existing options, apply the parent field mapping first. New fields use matching new options.
                </p>
              </template>
              <p
                v-else
                class="break-all"
              >
                {{ snapshotLabel(row.disposition) }}{{ row.target ? ` · ${row.target.label}` : '' }}
              </p>
              <p
                v-for="suggestion in row.suggestions"
                :key="suggestion.id"
                class="mt-1 break-words text-text-muted"
              >
                Suggested: {{ suggestion.label }}
              </p>
            </td>
            <td class="max-w-72 p-2">
              <p
                v-if="!row.qualified"
                class="text-danger"
              >
                Source evidence cannot authorize a write. A mapping cannot resolve this hold.
              </p>
              <ul class="space-y-1">
                <li
                  v-for="reason in row.reasons"
                  :key="reason"
                >
                  {{ snapshotLabel(reason) }}
                </li>
              </ul>
            </td>
          </tr>
        </tbody>
      </table>
      <p
        v-if="!page.items.length"
        class="py-3 text-small text-text-muted"
      >
        No mappings in this page.
      </p>
    </div>
    <div
      v-if="enabled"
      class="mt-3 flex flex-wrap gap-2"
    >
      <button
        type="button"
        :class="buttonClasses('secondary')"
        :disabled="cursors.length < 2 || reading.isFetching.value"
        @click="cursors.pop()"
      >
        Previous mappings
      </button>
      <button
        type="button"
        :class="buttonClasses('secondary')"
        :disabled="!page?.next_cursor || reading.isFetching.value"
        @click="page?.next_cursor && cursors.push(page.next_cursor)"
      >
        Next mappings
      </button>
      <button
        type="button"
        :class="buttonClasses('ghost')"
        @click="reading.refetch()"
      >
        Refresh mappings
      </button>
      <button
        v-if="canReplan"
        type="button"
        :class="buttonClasses('primary')"
        :disabled="!changes || changes > 50"
        @click="apply"
      >
        Apply {{ changes }} mapping changes
      </button>
      <button
        v-if="canReplan && changes"
        type="button"
        :class="buttonClasses('secondary')"
        @click="discard"
      >
        Discard mapping changes
      </button>
    </div>
    <p
      v-if="message"
      role="alert"
      class="mt-2 text-small text-danger"
    >
      {{ message }}
    </p>
    <section
      v-if="aliasMapping && enabled"
      class="mt-4 border-t border-border pt-4"
      aria-label="Tag source occurrences"
    >
      <h3 class="break-all text-body font-medium">
        Source occurrences of {{ metadataName(aliasMapping.source) }}
      </h3>
      <p
        v-if="aliases.error.value"
        role="alert"
        class="mt-2 text-small text-danger"
      >
        {{ describeApiError(aliases.error.value, 'Could not load source occurrences.') }}
      </p>
      <p
        v-if="aliases.isFetching.value"
        role="status"
        class="mt-2 text-small"
      >
        Loading source occurrences…
      </p>
      <ul class="mt-2 space-y-2 text-small">
        <li
          v-for="alias in aliasPage?.items ?? []"
          :key="`${alias.source_id}:${alias.ordinal}`"
          class="min-w-0 break-all"
        >
          FUB Person {{ alias.source_id }} · source position {{ alias.ordinal }}
          <p
            v-for="entry in alias.source.fields"
            :key="entry.key"
            class="whitespace-pre-wrap"
          >
            {{ entry.label }}: {{ entry.text }}{{ entry.abbreviated ? ' (abbreviated)' : '' }}
          </p>
          <button
            v-if="alias.record_id"
            type="button"
            class="max-w-full"
            :class="buttonClasses('ghost')"
            @click="aliasRecord = alias.record_id"
          >
            <span class="min-w-0 truncate">Inspect exact source {{ alias.source_id }}</span>
          </button>
        </li>
      </ul>
      <div class="mt-3 flex flex-wrap gap-2">
        <button
          type="button"
          :class="buttonClasses('secondary')"
          :disabled="aliasCursors.length < 2 || aliases.isFetching.value"
          @click="aliasCursors.pop()"
        >
          Previous occurrences
        </button>
        <button
          type="button"
          :class="buttonClasses('secondary')"
          :disabled="!aliasPage?.next_cursor || aliases.isFetching.value"
          @click="aliasPage?.next_cursor && aliasCursors.push(aliasPage.next_cursor)"
        >
          Next occurrences
        </button>
        <button
          v-if="aliases.error.value"
          type="button"
          :class="buttonClasses('secondary')"
          @click="aliases.refetch()"
        >
          Retry occurrences
        </button>
      </div>
      <MetadataFieldViewer
        v-if="aliasRecord"
        :request="{ kind: 'record', importId, planId, recordId: aliasRecord, fieldKey: 'source.all' }"
        title="Exact tag source fields"
      />
    </section>
    <MetadataEvidence
      v-if="selected && enabled"
      :source="selected.source"
      :request="{ kind: 'mapping', importId, planId, mappingId: selected.id, fieldKey: 'all' }"
      :title="`Source mapping for ${metadataName(selected.source)}`"
    />
    <MetadataTargetPicker
      v-if="picking && enabled"
      :import-id="importId"
      :plan-id="planId"
      :mapping="picking"
      :disabled="!canReplan"
      @close="picking = null"
      @select="picked"
    />
  </Card>
</template>
