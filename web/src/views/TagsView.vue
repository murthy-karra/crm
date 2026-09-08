<script setup lang="ts">
// SLICE_011e.md §5: `/manage/tags`, a MEMBER route (rule 1/D-051 lets a
// creator manage their own unused tag, so this is not admin-only). A flat
// table: name, people (person_count), and — only where the row's
// `can_manage` is true — Rename (inline field, Enter/Escape) and Delete;
// other rows show the controls disabled with the "Only an admin..."
// tooltip. Delete reuses the existing ConfirmDialog.
import { computed, h, nextTick, ref } from 'vue'
import type { ColumnDef } from '@tanstack/vue-table'
import { Pencil, Trash2 } from 'lucide-vue-next'
import PageHeader from '../components/PageHeader.vue'
import DataTable from '../components/DataTable.vue'
import ConfirmDialog from '../components/ConfirmDialog.vue'
import { useDeleteTagMutation, useMe, useRenameTagMutation, useTagsQuery } from '../api/queries'
import type { Tag } from '../api/types'
import { ApiError } from '../api/client'
import { buttonClasses, INPUT_CLASSES } from '../lib/controls'
import { describeApiError, describeMutationError } from '../lib/errors'

const CANNOT_MANAGE_TITLE = 'Only an admin can change a tag that is in use'

const { data: me } = useMe()
const orgId = computed(() => me.value?.organization?.id ?? '')

const { data, isPending, isError, error } = useTagsQuery(orgId)
const tags = computed(() => data.value?.tags ?? [])

const renameMutation = useRenameTagMutation(orgId)
const deleteMutation = useDeleteTagMutation(orgId)

// A stale-reference explanation shown above the table (§5: "a 403 ... or a
// 404 refetches and explains" — the mutation hooks already refetch the
// index; this names why for the person acting on it).
const tableNotice = ref<string | null>(null)

function explainStaleAction(err: unknown): boolean {
  if (err instanceof ApiError && err.status === 403) {
    tableNotice.value = 'That tag is now in use, so only an admin can change it. The list has been refreshed.'
    return true
  }
  if (err instanceof ApiError && err.status === 404) {
    tableNotice.value = 'That tag no longer exists. The list has been refreshed.'
    return true
  }
  return false
}

// ---- Inline rename (Enter saves, Escape cancels) --------------------------

const editingId = ref<string | null>(null)
const editingName = ref('')
const renameError = ref<string | null>(null)

function startRename(tag: Tag) {
  if (!tag.can_manage || renameMutation.isPending.value) return
  renameMutation.reset()
  renameError.value = null
  tableNotice.value = null
  editingId.value = tag.id
  editingName.value = tag.name
}

function cancelRename() {
  editingId.value = null
  renameError.value = null
}

function saveRename(tag: Tag) {
  const name = editingName.value.trim()
  if (name === '' || renameMutation.isPending.value) return
  renameError.value = null
  renameMutation.mutate(
    { tagId: tag.id, body: { name } },
    {
      onSuccess: () => {
        editingId.value = null
      },
      onError: (err) => {
        if (explainStaleAction(err)) {
          editingId.value = null
          return
        }
        renameError.value = err instanceof ApiError && err.code === 'tag_name_taken'
          ? `"${name}" is already in use.`
          : describeMutationError(err, 'Could not rename this tag.')
      },
    },
  )
}

// ---- Delete -----------------------------------------------------------

const pendingDelete = ref<Tag | null>(null)

function openDelete(tag: Tag) {
  if (!tag.can_manage || deleteMutation.isPending.value) return
  deleteMutation.reset()
  tableNotice.value = null
  pendingDelete.value = tag
}

function closeDelete() {
  if (deleteMutation.isPending.value) return
  pendingDelete.value = null
}

function confirmDelete() {
  const tag = pendingDelete.value
  if (!tag) return
  deleteMutation.mutate(tag.id, {
    onSuccess: () => {
      pendingDelete.value = null
    },
    onError: (err) => {
      if (explainStaleAction(err)) pendingDelete.value = null
    },
  })
}

const deleteMessage = computed(() => {
  const tag = pendingDelete.value
  if (!tag) return ''
  const count = tag.person_count
  const base = count > 0
    ? `Delete "${tag.name}"? ${count} ${count === 1 ? 'person' : 'people'} will lose this tag.`
    : `Delete "${tag.name}"? It is not applied to anyone.`
  return count > 0
    ? `${base} Saved lists and Today rules that use this tag will show an invalid-filter notice until they are edited.`
    : base
})

// ---- Table columns ------------------------------------------------------

const columns: ColumnDef<Tag>[] = [
  {
    id: 'name',
    header: 'Name',
    cell: (info) => {
      const tag = info.row.original
      if (editingId.value === tag.id && tag.can_manage) {
        return h('div', [
          h('input', {
            class: INPUT_CLASSES,
            value: editingName.value,
            'aria-label': `Rename ${tag.name}`,
            'data-testid': 'rename-tag-input',
            onInput: (event: Event) => {
              editingName.value = (event.target as HTMLInputElement).value
            },
            onKeydown: (event: KeyboardEvent) => {
              if (event.key === 'Enter') {
                event.preventDefault()
                saveRename(tag)
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
            ? h('p', { role: 'alert', class: 'mt-1 text-small text-danger', 'data-testid': 'rename-tag-error' }, renameError.value)
            : null,
        ])
      }
      return h('span', { class: 'text-body text-text' }, tag.name)
    },
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
      const tag = info.row.original
      if (editingId.value === tag.id && tag.can_manage) {
        return h('div', { class: 'flex justify-end gap-2' }, [
          h('button', {
            type: 'button',
            class: buttonClasses('secondary'),
            disabled: renameMutation.isPending.value,
            'data-testid': 'save-tag-rename',
            onClick: (event: MouseEvent) => {
              event.stopPropagation()
              saveRename(tag)
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
      const title = tag.can_manage ? undefined : CANNOT_MANAGE_TITLE
      return h('div', { class: 'flex justify-end gap-2' }, [
        h('button', {
          type: 'button',
          class: buttonClasses('secondary'),
          disabled: !tag.can_manage,
          title,
          'data-testid': 'rename-tag',
          onClick: (event: MouseEvent) => {
            event.stopPropagation()
            startRename(tag)
          },
        }, [h(Pencil, { class: 'h-4 w-4', 'aria-hidden': 'true' }), ' Rename']),
        h('button', {
          type: 'button',
          class: buttonClasses('secondary'),
          disabled: !tag.can_manage,
          title,
          'data-testid': 'delete-tag',
          onClick: (event: MouseEvent) => {
            event.stopPropagation()
            openDelete(tag)
          },
        }, [h(Trash2, { class: 'h-4 w-4', 'aria-hidden': 'true' }), ' Delete']),
      ])
    },
  },
]
</script>

<template>
  <div>
    <PageHeader
      title="Tags"
      subtitle="Free-form tags applied to People. Anyone can create, apply, and remove them."
    />

    <p
      v-if="tableNotice"
      role="status"
      class="mb-4 rounded-xl border border-border bg-surface-0 p-4 text-body text-text-muted"
      data-testid="tags-notice"
    >
      {{ tableNotice }}
    </p>

    <div
      v-if="isError"
      class="rounded-xl border border-border bg-surface-0 p-5 text-body text-danger"
    >
      {{ describeApiError(error, 'Could not load tags.') }}
    </div>
    <div
      v-else-if="isPending"
      class="rounded-xl border border-border bg-surface-0 p-5 text-body text-text-muted"
    >
      Loading…
    </div>
    <DataTable
      v-else
      :data="tags"
      :columns="columns"
      :row-key="(tag) => tag.id"
      count-noun="tags"
      count-noun-singular="tag"
      empty-message="No tags yet."
    />

    <ConfirmDialog
      :visible="pendingDelete !== null"
      title="Delete tag"
      :message="deleteMessage"
      confirm-label="Delete"
      confirm-variant="danger"
      :is-pending="deleteMutation.isPending.value"
      :error="deleteMutation.error.value"
      @update:visible="(value) => !value && closeDelete()"
      @confirm="confirmDelete"
    />
  </div>
</template>
