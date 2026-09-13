<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import AdmittedPeopleRefreshFieldFragment from './AdmittedPeopleRefreshFieldFragment.vue'
import { useQuery } from '@tanstack/vue-query'
import { buttonClasses } from '../../lib/controls'
import { describeApiError } from '../../lib/errors'
import { importAccessError } from '../../api/imports'
import { fetchAdmittedPeopleRefreshContacts, fetchAdmittedPeopleRefreshItem, refreshLabel, useAdmittedPeopleRefreshAccess, type RefreshPlan, type RefreshProjection } from '../../api/admittedPeopleRefreshes'
const props = defineProps<{ refreshId: string; itemId: string; plan: RefreshPlan }>()
const emit = defineEmits<{ accessDenied: [] }>()
const access = useAdmittedPeopleRefreshAccess(); const pages = ref([''])
const detailKey = computed(() => [...access.prefix.value, 'item', props.refreshId, props.plan.id, props.plan.revision, props.itemId])
const contactKey = computed(() => [...detailKey.value, 'contacts', pages.value.at(-1)])
function verified<T extends { plan_id: string; plan_revision: string }>(value: T) {
  if (value.plan_id !== props.plan.id || value.plan_revision !== props.plan.revision) throw new Error('The preview revision did not match. Reload the People refresh.')
  return value
}
const detail = useQuery({ queryKey: detailKey, enabled: access.enabled, retry: false, gcTime: 0, staleTime: Infinity, refetchOnWindowFocus: false, refetchOnReconnect: false, queryFn: ({ signal }) => access.read(detailKey.value, () => detailKey.value, async () => verified(await fetchAdmittedPeopleRefreshItem(props.refreshId, props.itemId, signal))) })
const contacts = useQuery({ queryKey: contactKey, enabled: computed(() => access.enabled.value && !!detail.data.value), retry: false, gcTime: 0, staleTime: Infinity, refetchOnWindowFocus: false, refetchOnReconnect: false, queryFn: ({ signal }) => access.read(contactKey.value, () => contactKey.value, async () => verified(await fetchAdmittedPeopleRefreshContacts(props.refreshId, props.itemId, pages.value.at(-1) || undefined, signal))) })
watch([detail.error, contacts.error], errors => { if (errors.some(importAccessError)) emit('accessDenied') })
watch([() => props.itemId, () => props.plan.id, access.scope], () => { pages.value = [''] }, { flush: 'sync' })
const sides = ['baseline', 'current', 'proposed'] as const
const selectedField = ref<{ side: typeof sides[number]; field: 'first_name' | 'last_name' } | null>(null)
watch([() => props.refreshId, () => props.itemId, () => props.plan.id, () => props.plan.revision, access.scope], () => { selectedField.value = null }, { flush: 'sync' })
function openField(side: typeof sides[number], field: string) { if (field === 'first_name' || field === 'last_name') selectedField.value = { side, field } }
const sideLabel = { baseline: 'Last settled baseline', current: 'CRM at preview', proposed: 'Proposed after refresh' }
const fields = ['first_name', 'last_name', 'stage_id', 'assigned_user_id'] as const
function field(value: RefreshProjection, key: typeof fields[number]) { return value[key] === undefined ? 'Unavailable' : value[key] === null ? 'Empty / unassigned' : value[key] }
function clears(key: typeof fields[number]) { const value = detail.data.value; return value && value.disposition === 'eligible' && value.baseline[key] != null && value.proposed[key] === null }
</script>
<template>
  <section
    v-if="access.enabled.value"
    class="space-y-3 border-t border-border pt-3"
    aria-label="Frozen Person value comparison"
  >
    <template v-if="detail.data.value">
      <p
        v-if="detail.data.value.disposition !== 'eligible'"
        class="text-text-muted"
      >
        This Person has no eligible update in this plan. No fields or contacts will be changed.
      </p>
      <div class="grid gap-3 lg:grid-cols-3">
        <section
          v-for="side in sides"
          :key="side"
          class="min-w-0 space-y-2 rounded bg-surface-1 p-3"
        >
          <h5 class="font-medium">
            {{ sideLabel[side] }}
          </h5><dl class="space-y-2">
            <div
              v-for="key in fields"
              :key="key"
            >
              <dt class="text-text-muted">
                {{ refreshLabel(key) }}<strong
                  v-if="side === 'proposed' && clears(key)"
                  class="ml-1 text-danger"
                >Will clear</strong>
              </dt><dd class="whitespace-pre-wrap break-all">
                {{ field(detail.data.value[side], key) }}
                <template v-if="detail.data.value[side].truncated_fields?.includes(key)">
                  <p class="mt-1 text-text-muted">
                    Display prefix; the complete retained name is available in pages.
                  </p>
                  <button
                    type="button"
                    :class="buttonClasses('ghost')"
                    @click="openField(side, key)"
                  >
                    Read complete name in pages
                  </button>
                </template>
              </dd>
            </div><div>
              <dt class="text-text-muted">
                Contacts
              </dt><dd>{{ detail.data.value[side].contact_counts?.email ?? 'Unknown' }} emails · {{ detail.data.value[side].contact_counts?.phone ?? 'Unknown' }} phones</dd>
            </div>
          </dl>
        </section>
      </div>
      <p
        v-if="detail.data.value.no_instruction.length"
        class="text-text-muted"
      >
        No new source instruction for {{ detail.data.value.no_instruction.map(refreshLabel).join(', ') }}; the owned baseline and its provenance are preserved.
      </p>
      <AdmittedPeopleRefreshFieldFragment
        v-if="selectedField"
        :key="`${itemId}:${selectedField.side}:${selectedField.field}`"
        :refresh-id="refreshId"
        :item-id="itemId"
        :plan="plan"
        :side="selectedField.side"
        :field="selectedField.field"
        @access-denied="emit('accessDenied')"
        @close="selectedField = null"
      />
      <h5 class="font-medium">
        Owned contact comparison
      </h5><p class="text-text-muted">
        Rows show their baseline, CRM-at-preview or proposed side. Proposed additions and removals are labeled explicitly. Existing contact identities are preserved when their normalized value is unchanged.
      </p>
      <article
        v-for="contact in contacts.data.value?.contacts ?? []"
        :key="contact.id"
        class="min-w-0 space-y-1 rounded border border-border p-3"
      >
        <p class="font-medium">
          {{ sideLabel[contact.side] }} · {{ contact.kind === 'email' ? 'Email' : 'Phone' }} · order {{ contact.import_order }}
          <strong
            v-if="detail.data.value.disposition === 'eligible' && contact.change === 'remove'"
            class="ml-1 text-danger"
          >Proposed removal</strong>
          <strong
            v-if="detail.data.value.disposition === 'eligible' && contact.change === 'add'"
            class="ml-1"
          >Proposed addition</strong>
        </p><p class="whitespace-pre-wrap break-all">
          {{ contact.value.value }}
        </p><p class="break-all text-text-muted">
          Normalized: {{ contact.value.normalized_value }}
        </p><p class="break-all text-text-muted">
          Contact identity: {{ contact.contact_id ?? 'Unavailable' }}
        </p>
      </article>
      <p v-if="contacts.data.value?.contacts.length === 0">
        No contact rows on this page.
      </p>
      <div class="flex flex-wrap gap-2">
        <button
          type="button"
          :class="buttonClasses('ghost')"
          :disabled="pages.length < 2 || contacts.isFetching.value"
          @click="pages.pop()"
        >
          Previous contact comparison
        </button><button
          type="button"
          :class="buttonClasses('ghost')"
          :disabled="!contacts.data.value?.next_cursor || contacts.isFetching.value"
          @click="contacts.data.value?.next_cursor && pages.push(contacts.data.value.next_cursor)"
        >
          More contact comparison
        </button>
      </div>
    </template>
    <p
      v-if="detail.isFetching.value || contacts.isFetching.value"
      role="status"
    >
      Loading owned values…
    </p><p
      v-if="detail.error.value || contacts.error.value"
      role="alert"
      class="text-danger"
    >
      {{ describeApiError(detail.error.value ?? contacts.error.value, 'The value preview could not be verified. Reload the People refresh.') }}
    </p>
  </section>
</template>
