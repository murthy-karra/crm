<script setup lang="ts">
// SLICE_019.md §7: `/manage/fields`, admin-only (D-058 §2 — unlike Tags,
// there is no creator-while-unused carve-out; a definition is Organization
// schema). A live-fields table (label, type, people with a value) with
// inline rename, up/down reorder (PUT order with the full live set),
// Archive behind ConfirmDialog naming the count; an options editor for
// choice fields (add, rename, archive, restore; creation order, no
// reorder); an Archived section with Restore; a "New field" form.
import { computed, h, nextTick, ref } from 'vue'
import { useQueryClient } from '@tanstack/vue-query'
import type { ColumnDef } from '@tanstack/vue-table'
import Select from 'primevue/select'
import { ArrowDown, ArrowUp, Plus, X } from 'lucide-vue-next'
import PageHeader from '../components/PageHeader.vue'
import Card from '../components/Card.vue'
import DataTable from '../components/DataTable.vue'
import ConfirmDialog from '../components/ConfirmDialog.vue'
import FormField from '../components/FormField.vue'
import {
  queryKeys,
  useAddCustomFieldOptionMutation,
  useCreateCustomFieldMutation,
  useCustomFieldsQuery,
  useMe,
  useReorderCustomFieldsMutation,
  useUpdateCustomFieldMutation,
  useUpdateCustomFieldOptionMutation,
} from '../api/queries'
import type { CustomField, CustomFieldOption, CustomFieldType } from '../api/types'
import { ApiError } from '../api/client'
import { buttonClasses, INPUT_CLASSES, LABEL_CLASSES, selectPt } from '../lib/controls'
import { describeApiError } from '../lib/errors'

const { data: me } = useMe()
const orgId = computed(() => me.value?.organization?.id ?? '')
const queryClient = useQueryClient()

const { data, isPending, isError, error } = useCustomFieldsQuery(orgId)
const fields = computed(() => data.value?.fields ?? [])
const liveFields = computed(() => fields.value.filter((f) => f.archived_at === null))
const archivedFields = computed(() => fields.value.filter((f) => f.archived_at !== null))

const FIELD_TYPE_LABEL: Record<CustomFieldType, string> = {
  text: 'Text',
  number: 'Number',
  date: 'Date',
  choice: 'Single choice',
}
const FIELD_TYPE_OPTIONS: { value: CustomFieldType; label: string }[] = [
  { value: 'text', label: 'Text' },
  { value: 'number', label: 'Number' },
  { value: 'date', label: 'Date' },
  { value: 'choice', label: 'Single choice' },
]

// ---- New field form ---------------------------------------------------

const createMutation = useCreateCustomFieldMutation(orgId)
const newLabel = ref('')
const newType = ref<CustomFieldType>('text')
const newOptions = ref<string[]>([''])
const createError = ref<string | null>(null)

function addNewOptionRow() {
  newOptions.value.push('')
}
function removeNewOptionRow(index: number) {
  if (newOptions.value.length <= 1) {
    newOptions.value[0] = ''
    return
  }
  newOptions.value.splice(index, 1)
}

const canCreate = computed(() => {
  if (newLabel.value.trim() === '' || createMutation.isPending.value) return false
  if (newType.value === 'choice') return newOptions.value.some((o) => o.trim() !== '')
  return true
})

function submitCreate() {
  if (!canCreate.value) return
  createError.value = null
  const body: { label: string; field_type: CustomFieldType; options?: string[] } = {
    label: newLabel.value.trim(),
    field_type: newType.value,
  }
  if (newType.value === 'choice') {
    body.options = newOptions.value.map((o) => o.trim()).filter((o) => o !== '')
  }
  createMutation.mutate(body, {
    onSuccess: () => {
      newLabel.value = ''
      newType.value = 'text'
      newOptions.value = ['']
    },
    onError: (err) => {
      if (err instanceof ApiError && err.code === 'custom_field_label_taken') {
        createError.value = `"${body.label}" is already in use.`
      } else if (err instanceof ApiError && err.code === 'custom_field_limit_reached') {
        createError.value = 'This Organization already has 50 fields.'
      } else {
        createError.value = describeApiError(err, 'Could not create this field.')
      }
    },
  })
}

// ---- Inline rename (Enter saves, Escape cancels) — the TagsView pattern --

const renameMutation = useUpdateCustomFieldMutation(orgId)
const editingId = ref<string | null>(null)
const editingLabel = ref('')
const renameError = ref<string | null>(null)

function startRename(field: CustomField) {
  if (renameMutation.isPending.value) return
  renameMutation.reset()
  renameError.value = null
  editingId.value = field.id
  editingLabel.value = field.label
}
function cancelRename() {
  editingId.value = null
  renameError.value = null
}
function saveRename(field: CustomField) {
  const label = editingLabel.value.trim()
  if (label === '' || renameMutation.isPending.value) return
  renameError.value = null
  renameMutation.mutate(
    { fieldId: field.id, body: { label, archived: false } },
    {
      onSuccess: () => {
        editingId.value = null
      },
      onError: (err) => {
        renameError.value = err instanceof ApiError && err.code === 'custom_field_label_taken'
          ? `"${label}" is already in use.`
          : describeApiError(err, 'Could not rename this field.')
      },
    },
  )
}

// ---- Reorder (up/down; PUT the full live order) ------------------------

const reorderMutation = useReorderCustomFieldsMutation(orgId)
const listError = ref<string | null>(null)
function moveField(fieldId: string, delta: -1 | 1) {
  if (reorderMutation.isPending.value) return
  listError.value = null
  const ids = liveFields.value.map((f) => f.id)
  const index = ids.indexOf(fieldId)
  const targetIndex = index + delta
  if (index === -1 || targetIndex < 0 || targetIndex >= ids.length) return
  const reordered = [...ids]
  ;[reordered[index], reordered[targetIndex]] = [reordered[targetIndex]!, reordered[index]!]
  reorderMutation.mutate(
    { field_ids: reordered },
    {
      onError: (err) => {
        listError.value = describeApiError(err, 'Could not reorder fields.')
        // A 422 means the live set changed under us (a field was archived
        // or restored elsewhere) — the list this ordering was built from is
        // stale, so refetch it rather than leaving the up/down arrows
        // pointed at an order the server just refused.
        if (err instanceof ApiError && err.status === 422) {
          void queryClient.invalidateQueries({ queryKey: queryKeys.customFields(orgId.value) })
        }
      },
    },
  )
}

// ---- Archive (ConfirmDialog naming the count) / restore ------------------

const archiveMutation = useUpdateCustomFieldMutation(orgId)
const pendingArchive = ref<CustomField | null>(null)

function openArchive(field: CustomField) {
  archiveMutation.reset()
  pendingArchive.value = field
}
function closeArchive() {
  if (archiveMutation.isPending.value) return
  pendingArchive.value = null
}
function confirmArchive() {
  const field = pendingArchive.value
  if (!field) return
  archiveMutation.mutate(
    { fieldId: field.id, body: { label: field.label, archived: true } },
    { onSuccess: () => { pendingArchive.value = null } },
  )
}
const archiveMessage = computed(() => {
  const field = pendingArchive.value
  if (!field) return ''
  return `Archive "${field.label}"? Values are kept and reappear if you restore it. Filters using this field will need repair or restoration.`
})

const restoreMutation = useUpdateCustomFieldMutation(orgId)
function restoreField(field: CustomField) {
  if (restoreMutation.isPending.value) return
  listError.value = null
  restoreMutation.mutate(
    { fieldId: field.id, body: { label: field.label, archived: false } },
    {
      onError: (err) => {
        listError.value = err instanceof ApiError && err.code === 'custom_field_label_taken'
          ? `"${field.label}" is already in use.`
          : describeApiError(err, 'Could not restore this field.')
      },
    },
  )
}
function isRestoring(fieldId: string): boolean {
  return restoreMutation.isPending.value && restoreMutation.variables.value?.fieldId === fieldId
}

// ---- Options editor (choice fields only) ---------------------------------

const optionsFieldId = ref<string | null>(null)
const optionsField = computed(() => fields.value.find((f) => f.id === optionsFieldId.value) ?? null)
function toggleOptionsEditor(field: CustomField) {
  optionsFieldId.value = optionsFieldId.value === field.id ? null : field.id
  newOptionLabel.value = ''
  addOptionError.value = null
}

const addOptionMutation = useAddCustomFieldOptionMutation(orgId)
const newOptionLabel = ref('')
const addOptionError = ref<string | null>(null)
function submitAddOption() {
  const field = optionsField.value
  const label = newOptionLabel.value.trim()
  if (!field || label === '' || addOptionMutation.isPending.value) return
  addOptionError.value = null
  addOptionMutation.mutate(
    { fieldId: field.id, body: { label } },
    {
      onSuccess: () => {
        newOptionLabel.value = ''
      },
      onError: (err) => {
        if (err instanceof ApiError && err.code === 'option_label_taken') {
          addOptionError.value = `"${label}" is already an option on this field.`
        } else if (err instanceof ApiError && err.code === 'option_limit_reached') {
          addOptionError.value = 'This field already has 50 options.'
        } else {
          addOptionError.value = describeApiError(err, 'Could not add this option.')
        }
      },
    },
  )
}

const updateOptionMutation = useUpdateCustomFieldOptionMutation(orgId)
const editingOptionId = ref<string | null>(null)
const editingOptionLabel = ref('')
const optionRenameError = ref<string | null>(null)

function startOptionRename(option: CustomFieldOption) {
  if (updateOptionMutation.isPending.value) return
  updateOptionMutation.reset()
  optionRenameError.value = null
  editingOptionId.value = option.id
  editingOptionLabel.value = option.label
}
function cancelOptionRename() {
  editingOptionId.value = null
  optionRenameError.value = null
}
function saveOptionRename(option: CustomFieldOption) {
  const field = optionsField.value
  const label = editingOptionLabel.value.trim()
  if (!field || label === '' || updateOptionMutation.isPending.value) return
  optionRenameError.value = null
  updateOptionMutation.mutate(
    { fieldId: field.id, optionId: option.id, body: { label, archived: false } },
    {
      onSuccess: () => {
        editingOptionId.value = null
      },
      onError: (err) => {
        optionRenameError.value = err instanceof ApiError && err.code === 'option_label_taken'
          ? `"${label}" is already an option on this field.`
          : describeApiError(err, 'Could not rename this option.')
      },
    },
  )
}
const optionActionError = ref<string | null>(null)
function archiveOption(option: CustomFieldOption) {
  const field = optionsField.value
  if (!field || updateOptionMutation.isPending.value) return
  optionActionError.value = null
  updateOptionMutation.mutate(
    { fieldId: field.id, optionId: option.id, body: { label: option.label, archived: true } },
    {
      onError: (err) => {
        optionActionError.value = describeApiError(err, 'Could not archive this option.')
      },
    },
  )
}
function restoreOption(option: CustomFieldOption) {
  const field = optionsField.value
  if (!field || updateOptionMutation.isPending.value) return
  optionActionError.value = null
  updateOptionMutation.mutate(
    { fieldId: field.id, optionId: option.id, body: { label: option.label, archived: false } },
    {
      onError: (err) => {
        optionActionError.value = err instanceof ApiError && err.code === 'option_label_taken'
          ? `"${option.label}" is already an option on this field.`
          : describeApiError(err, 'Could not restore this option.')
      },
    },
  )
}
function isOptionPending(optionId: string): boolean {
  return updateOptionMutation.isPending.value && updateOptionMutation.variables.value?.optionId === optionId
}

const liveOptions = computed(() => optionsField.value?.options.filter((o) => o.archived_at === null) ?? [])
const archivedOptions = computed(() => optionsField.value?.options.filter((o) => o.archived_at !== null) ?? [])

// ---- Live fields table columns -------------------------------------------

const liveColumns: ColumnDef<CustomField>[] = [
  {
    id: 'label',
    header: 'Label',
    cell: (info) => {
      const field = info.row.original
      if (editingId.value === field.id) {
        return h('div', [
          h('input', {
            class: INPUT_CLASSES,
            value: editingLabel.value,
            'aria-label': `Rename ${field.label}`,
            'data-testid': 'rename-field-input',
            onInput: (event: Event) => {
              editingLabel.value = (event.target as HTMLInputElement).value
            },
            onKeydown: (event: KeyboardEvent) => {
              if (event.key === 'Enter') {
                event.preventDefault()
                saveRename(field)
              } else if (event.key === 'Escape') {
                event.preventDefault()
                cancelRename()
              }
            },
            onClick: (event: MouseEvent) => event.stopPropagation(),
            onVnodeMounted: (vnode) => {
              void nextTick(() => (vnode.el as HTMLInputElement | null)?.focus())
            },
          }),
          renameError.value
            ? h('p', { role: 'alert', class: 'mt-1 text-small text-danger', 'data-testid': 'rename-field-error' }, renameError.value)
            : null,
        ])
      }
      return h('span', { class: 'text-body text-text' }, field.label)
    },
  },
  {
    id: 'field_type',
    header: 'Type',
    cell: (info) => FIELD_TYPE_LABEL[info.row.original.field_type],
  },
  {
    id: 'person_count',
    header: 'People',
    meta: { align: 'right' },
    cell: (info) => String(info.row.original.person_count),
  },
  {
    id: 'order',
    header: 'Order',
    cell: (info) => {
      const field = info.row.original
      const index = liveFields.value.findIndex((f) => f.id === field.id)
      return h('div', { class: 'flex justify-end gap-1' }, [
        h('button', {
          type: 'button',
          class: buttonClasses('ghost'),
          disabled: index <= 0 || reorderMutation.isPending.value,
          'aria-label': `Move ${field.label} up`,
          'data-testid': 'move-field-up',
          onClick: (event: MouseEvent) => {
            event.stopPropagation()
            moveField(field.id, -1)
          },
        }, [h(ArrowUp, { class: 'h-4 w-4', 'aria-hidden': 'true' })]),
        h('button', {
          type: 'button',
          class: buttonClasses('ghost'),
          disabled: index === -1 || index >= liveFields.value.length - 1 || reorderMutation.isPending.value,
          'aria-label': `Move ${field.label} down`,
          'data-testid': 'move-field-down',
          onClick: (event: MouseEvent) => {
            event.stopPropagation()
            moveField(field.id, 1)
          },
        }, [h(ArrowDown, { class: 'h-4 w-4', 'aria-hidden': 'true' })]),
      ])
    },
  },
  {
    id: 'actions',
    header: '',
    cell: (info) => {
      const field = info.row.original
      if (editingId.value === field.id) {
        return h('div', { class: 'flex justify-end gap-2' }, [
          h('button', {
            type: 'button',
            class: buttonClasses('secondary'),
            disabled: renameMutation.isPending.value,
            'data-testid': 'save-field-rename',
            onClick: (event: MouseEvent) => {
              event.stopPropagation()
              saveRename(field)
            },
          }, renameMutation.isPending.value ? 'Saving…' : 'Save'),
          h('button', {
            type: 'button',
            class: buttonClasses('ghost'),
            disabled: renameMutation.isPending.value,
            onClick: (event: MouseEvent) => {
              event.stopPropagation()
              cancelRename()
            },
          }, 'Cancel'),
        ])
      }
      const buttons = [
        h('button', {
          type: 'button',
          class: buttonClasses('secondary'),
          'data-testid': 'rename-field',
          onClick: (event: MouseEvent) => {
            event.stopPropagation()
            startRename(field)
          },
        }, 'Rename'),
      ]
      if (field.field_type === 'choice') {
        buttons.push(h('button', {
          type: 'button',
          class: buttonClasses('secondary'),
          'data-testid': 'manage-field-options',
          onClick: (event: MouseEvent) => {
            event.stopPropagation()
            toggleOptionsEditor(field)
          },
        }, optionsFieldId.value === field.id ? 'Close options' : 'Options'))
      }
      buttons.push(h('button', {
        type: 'button',
        class: buttonClasses('secondary'),
        'data-testid': 'archive-field',
        onClick: (event: MouseEvent) => {
          event.stopPropagation()
          openArchive(field)
        },
      }, 'Archive'))
      return h('div', { class: 'flex flex-wrap justify-end gap-2' }, buttons)
    },
  },
]

const archivedColumns: ColumnDef<CustomField>[] = [
  {
    id: 'label',
    header: 'Label',
    cell: (info) => h('span', { class: 'text-body text-text' }, info.row.original.label),
  },
  {
    id: 'field_type',
    header: 'Type',
    cell: (info) => FIELD_TYPE_LABEL[info.row.original.field_type],
  },
  {
    id: 'person_count',
    header: 'People',
    meta: { align: 'right' },
    cell: (info) => String(info.row.original.person_count),
  },
  {
    id: 'actions',
    header: '',
    cell: (info) => {
      const field = info.row.original
      return h('div', { class: 'flex justify-end' }, [
        h('button', {
          type: 'button',
          class: buttonClasses('secondary'),
          disabled: isRestoring(field.id),
          'data-testid': 'restore-field',
          onClick: (event: MouseEvent) => {
            event.stopPropagation()
            restoreField(field)
          },
        }, isRestoring(field.id) ? 'Restoring…' : 'Restore'),
      ])
    },
  },
]
</script>

<template>
  <div>
    <PageHeader
      title="Fields"
      subtitle="Typed custom fields on People. Definitions and options are archived, never deleted."
    />

    <Card class="mb-6">
      <h2 class="mb-4 text-section font-semibold text-text">
        New field
      </h2>
      <div class="flex flex-wrap gap-4">
        <FormField
          label="Label"
          bare
          class="w-64"
        >
          <input
            v-model="newLabel"
            :class="INPUT_CLASSES"
            :disabled="createMutation.isPending.value"
            data-testid="new-field-label"
          >
        </FormField>
        <FormField
          label="Type"
          bare
        >
          <Select
            v-model="newType"
            :options="FIELD_TYPE_OPTIONS"
            option-label="label"
            option-value="value"
            aria-label="Type"
            :disabled="createMutation.isPending.value"
            :pt="selectPt()"
            class="w-48"
            data-testid="new-field-type"
          />
        </FormField>
      </div>

      <div
        v-if="newType === 'choice'"
        class="mt-4"
      >
        <label :class="LABEL_CLASSES">Options</label>
        <div
          v-for="(_option, index) in newOptions"
          :key="index"
          class="mb-2 flex items-center gap-2"
        >
          <input
            v-model="newOptions[index]"
            :class="INPUT_CLASSES"
            :aria-label="`Option ${index + 1}`"
            :disabled="createMutation.isPending.value"
            data-testid="new-field-option"
          >
          <button
            type="button"
            class="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg text-text-muted hover:bg-surface-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-focus disabled:opacity-50"
            aria-label="Remove option"
            :disabled="createMutation.isPending.value"
            data-testid="remove-new-field-option"
            @click="removeNewOptionRow(index)"
          >
            <X
              class="h-4 w-4"
              aria-hidden="true"
            />
          </button>
        </div>
        <button
          type="button"
          :class="buttonClasses('ghost')"
          :disabled="createMutation.isPending.value"
          data-testid="add-new-field-option"
          @click="addNewOptionRow"
        >
          <Plus
            class="h-4 w-4"
            aria-hidden="true"
          /> Add option
        </button>
      </div>

      <div class="mt-4 flex items-center gap-3">
        <button
          type="button"
          :class="buttonClasses('primary')"
          :disabled="!canCreate"
          data-testid="create-field"
          @click="submitCreate"
        >
          {{ createMutation.isPending.value ? 'Creating…' : 'Create field' }}
        </button>
        <p
          v-if="createError"
          role="alert"
          class="text-small text-danger"
          data-testid="create-field-error"
        >
          {{ createError }}
        </p>
      </div>
    </Card>

    <div
      v-if="isError"
      class="rounded-xl border border-border bg-surface-0 p-5 text-body text-danger"
    >
      {{ describeApiError(error, 'Could not load fields.') }}
    </div>
    <div
      v-else-if="isPending"
      class="rounded-xl border border-border bg-surface-0 p-5 text-body text-text-muted"
    >
      Loading…
    </div>
    <template v-else>
      <DataTable
        :data="liveFields"
        :columns="liveColumns"
        :row-key="(field) => field.id"
        count-noun="fields"
        count-noun-singular="field"
        empty-message="No custom fields yet."
      />
      <p
        v-if="listError"
        role="alert"
        class="mt-2 text-small text-danger"
        data-testid="fields-list-error"
      >
        {{ listError }}
      </p>

      <Card
        v-if="optionsField"
        class="mt-6"
      >
        <h2 class="mb-4 text-section font-semibold text-text">
          Options for {{ optionsField.label }}
        </h2>
        <p
          v-if="optionActionError"
          role="alert"
          class="mb-3 text-small text-danger"
          data-testid="option-action-error"
        >
          {{ optionActionError }}
        </p>
        <ul class="mb-4 divide-y divide-border">
          <li
            v-for="option in liveOptions"
            :key="option.id"
            class="flex items-center justify-between gap-3 py-2.5 first:pt-0"
            data-testid="field-option-row"
          >
            <template v-if="editingOptionId === option.id">
              <input
                v-model="editingOptionLabel"
                :class="INPUT_CLASSES"
                :aria-label="`Rename option ${option.label}`"
                data-testid="rename-option-input"
                @keydown.enter.prevent="saveOptionRename(option)"
                @keydown.escape.prevent="cancelOptionRename"
              >
              <div class="flex shrink-0 gap-2">
                <button
                  type="button"
                  :class="buttonClasses('secondary')"
                  :disabled="updateOptionMutation.isPending.value"
                  data-testid="save-option-rename"
                  @click="saveOptionRename(option)"
                >
                  Save
                </button>
                <button
                  type="button"
                  :class="buttonClasses('ghost')"
                  :disabled="updateOptionMutation.isPending.value"
                  @click="cancelOptionRename"
                >
                  Cancel
                </button>
              </div>
            </template>
            <template v-else>
              <span class="text-body text-text">{{ option.label }}</span>
              <div class="flex shrink-0 gap-2">
                <button
                  type="button"
                  :class="buttonClasses('secondary')"
                  data-testid="rename-option"
                  @click="startOptionRename(option)"
                >
                  Rename
                </button>
                <button
                  type="button"
                  :class="buttonClasses('secondary')"
                  :disabled="isOptionPending(option.id)"
                  data-testid="archive-option"
                  @click="archiveOption(option)"
                >
                  Archive
                </button>
              </div>
            </template>
          </li>
        </ul>
        <p
          v-if="optionRenameError"
          role="alert"
          class="mb-3 text-small text-danger"
          data-testid="option-rename-error"
        >
          {{ optionRenameError }}
        </p>

        <div
          v-if="archivedOptions.length > 0"
          class="mb-4"
        >
          <p class="mb-2 text-small font-medium text-text-muted">
            Archived
          </p>
          <ul class="divide-y divide-border">
            <li
              v-for="option in archivedOptions"
              :key="option.id"
              class="flex items-center justify-between gap-3 py-2.5 first:pt-0"
              data-testid="archived-field-option-row"
            >
              <span class="text-body text-text-muted">{{ option.label }}</span>
              <button
                type="button"
                :class="buttonClasses('secondary')"
                :disabled="isOptionPending(option.id)"
                data-testid="restore-option"
                @click="restoreOption(option)"
              >
                Restore
              </button>
            </li>
          </ul>
        </div>

        <div class="flex items-center gap-2">
          <input
            v-model="newOptionLabel"
            :class="INPUT_CLASSES"
            aria-label="New option"
            placeholder="New option"
            :disabled="addOptionMutation.isPending.value"
            data-testid="add-option-input"
            @keydown.enter.prevent="submitAddOption"
          >
          <button
            type="button"
            :class="buttonClasses('secondary')"
            :disabled="addOptionMutation.isPending.value || newOptionLabel.trim() === ''"
            data-testid="add-option-submit"
            @click="submitAddOption"
          >
            Add
          </button>
        </div>
        <p
          v-if="addOptionError"
          role="alert"
          class="mt-2 text-small text-danger"
          data-testid="add-option-error"
        >
          {{ addOptionError }}
        </p>
      </Card>

      <div
        v-if="archivedFields.length > 0"
        class="mt-6"
      >
        <h2 class="mb-3 text-section font-semibold text-text">
          Archived
        </h2>
        <DataTable
          :data="archivedFields"
          :columns="archivedColumns"
          :row-key="(field) => field.id"
          count-noun="archived fields"
          count-noun-singular="archived field"
          empty-message="No archived fields."
        />
      </div>
    </template>

    <ConfirmDialog
      :visible="pendingArchive !== null"
      title="Archive field"
      :message="archiveMessage"
      confirm-label="Archive"
      confirm-variant="danger"
      :is-pending="archiveMutation.isPending.value"
      :error="archiveMutation.error.value"
      @update:visible="(value) => !value && closeArchive()"
      @confirm="confirmArchive"
    />
  </div>
</template>
