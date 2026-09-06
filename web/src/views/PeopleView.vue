<script setup lang="ts">
// People filtering stays in the existing v1 URL/API representation.
// The page owns committed filters; FilterBar owns only its open editor.
import { computed, h, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useQueryClient } from '@tanstack/vue-query'
import { onBeforeRouteLeave, onBeforeRouteUpdate, RouterLink, useRoute, useRouter } from 'vue-router'
import { Copy, ListFilter, Plus, Save, Trash2, Users } from 'lucide-vue-next'
import type { ColumnDef } from '@tanstack/vue-table'
import PageHeader from '../components/PageHeader.vue'
import DataTable from '../components/DataTable.vue'
import PersonPreview from '../components/PersonPreview.vue'
import StageLabel from '../components/StageLabel.vue'
import FilterBar from '../components/FilterBar.vue'
import ConfirmDialog from '../components/ConfirmDialog.vue'
import SavedListDialog from '../components/SavedListDialog.vue'
import {
  useCreateSavedListMutation,
  useDeleteSavedListMutation,
  useInquirySources,
  useMe,
  useMembers,
  usePeople,
  useSavedList,
  useSavedListCount,
  useStages,
  useUpdateSavedListMutation,
  queryKeys,
} from '../api/queries'
import { ApiError } from '../api/client'
import type { CreateSavedListRequest, FilterClause, FilterDefinition, MeResponse, PersonSummary, SavedListDetailResponse } from '../api/types'
import { formatAbsoluteTime, formatRelativeTime, initials } from '../lib/format'
import { buttonClasses } from '../lib/controls'
import { describeApiError } from '../lib/errors'
import { canonicalFilterDefinition, committedClauses, parseFilter, serializeFilter } from '../lib/filter'

const props = withDefaults(defineProps<{ savedListId?: string }>(), { savedListId: '' })

const { data: me } = useMe()
const queryClient = useQueryClient()
const orgId = computed(() => me.value?.organization?.id ?? '')
const actorId = computed(() => me.value?.user.id ?? '')

const stagesQuery = useStages(orgId)
const stages = computed(() => stagesQuery.data.value?.stages ?? [])
const membersQuery = useMembers(orgId)
const members = computed(() => membersQuery.data.value?.members ?? [])
const sourcesQuery = useInquirySources(orgId)
const sources = computed(() => sourcesQuery.data.value?.sources ?? [])

function retryOptions(kind: 'stage' | 'assigned_to' | 'source') {
  const query = kind === 'stage' ? stagesQuery : kind === 'assigned_to' ? membersQuery : sourcesQuery
  void query.refetch()
}

const route = useRoute()
const router = useRouter()
const clauses = ref<FilterClause[]>([])
const filterOrigin = ref<'url' | null>(null)
const editorRevision = ref(0)
const isNamedList = computed(() => props.savedListId !== '')

interface SavedListBaseline {
  id: string
  revision: number
  name: string
  filter: FilterDefinition
}

function cloneFilter(filter: FilterDefinition): FilterDefinition {
  return JSON.parse(JSON.stringify(filter)) as FilterDefinition
}

// API decoding can preserve a different object-key insertion order at every
// level of a typed clause. Compare a recursive canonical form, while keeping
// clause/value-array order and the established People URL serialization.
function filtersEqual(left: FilterDefinition, right: FilterDefinition) {
  return canonicalFilterDefinition(left) === canonicalFilterDefinition(right)
}

const savedListQuery = useSavedList(orgId, actorId, computed(() => props.savedListId))
const savedBaseline = ref<SavedListBaseline | null>(null)
const nameDraft = ref('')

function installSavedDetail(detail: SavedListDetailResponse, preserveLocalDraft = false) {
  if (detail.filter === null) {
    savedBaseline.value = null
    clauses.value = []
    nameDraft.value = detail.list.name
    return
  }
  savedBaseline.value = {
    id: detail.list.id,
    revision: detail.list.revision,
    name: detail.list.name,
    filter: cloneFilter(detail.filter),
  }
  if (preserveLocalDraft) return
  nameDraft.value = detail.list.name
  clauses.value = committedClauses(cloneFilter(detail.filter).clauses)
  filterOrigin.value = null
  editorRevision.value++
}

function applySavedDetail(detail: SavedListDetailResponse, force = false) {
  if (detail.list.id !== props.savedListId) return
  const hasDraftForThisList = isDirty.value && savedBaseline.value?.id === detail.list.id
  if (!force && hasDraftForThisList) return
  installSavedDetail(detail)
}

const workingFilter = computed<FilterDefinition>(() => ({
  version: 1,
  clauses: committedClauses(clauses.value),
}))
const serializedFilter = computed(() => {
  // A saved empty definition is explicit JSON, not the ordinary /people
  // absence sentinel. That distinction keeps an unreadable list from ever
  // falling back to all People while still allowing a valid empty list.
  if (isNamedList.value) {
    return savedBaseline.value?.id === props.savedListId
      ? serializeFilter(workingFilter.value.clauses)
      : undefined
  }
  return workingFilter.value.clauses.length > 0 ? serializeFilter(workingFilter.value.clauses) : undefined
})
const hasFilter = computed(() => serializedFilter.value !== undefined)
const isDirty = computed(() => {
  if (!isNamedList.value || !savedBaseline.value) return false
  return nameDraft.value.trim() !== savedBaseline.value.name ||
    !filtersEqual(workingFilter.value, savedBaseline.value.filter)
})
const workingFilterChanged = computed(() => savedBaseline.value !== null &&
  !filtersEqual(workingFilter.value, savedBaseline.value.filter),
)

function hasResolvableReferences(filter: FilterDefinition) {
  const knownStages = new Set(stages.value.map((stage) => stage.id))
  const knownMembers = new Set(members.value.map((member) => member.user_id))
  for (const clause of filter.clauses) {
    if (clause.kind === 'stage' && clause.stage_ids.some((id) => !knownStages.has(id))) return false
    if (clause.kind === 'assigned_to' && clause.assignees.some((assignee) =>
      typeof assignee === 'object' && !knownMembers.has(assignee.user_id),
    )) return false
  }
  return true
}

const namedDefinitionReadable = computed(() => {
  const detail = savedListQuery.data.value
  return isNamedList.value && !savedListQuery.isError.value &&
    detail?.list.id === props.savedListId && detail.filter !== null &&
    savedBaseline.value?.id === props.savedListId
})
const workingReferencesResolvable = computed(() => {
  const detail = savedListQuery.data.value
  if (!detail?.filter_error) return true
  if (detail.filter_error === 'unsupported_filter') return false
  return workingFilterChanged.value && hasResolvableReferences(workingFilter.value)
})
const namedDefinitionUsable = computed(() =>
  namedDefinitionReadable.value && workingReferencesResolvable.value,
)
const namedPeopleEnabled = computed(() => {
  if (!isNamedList.value) return true
  // A failed detail request can retain old TanStack data. It is never an
  // authorization grant: pause People until the current definition is again
  // readable (or a writer deliberately repairs a known reference error).
  if (savedListQuery.isError.value) return false
  // A stale reference disables the baseline table. A local repair is allowed
  // only after its actual stage/member references resolve; changing a list
  // name alone cannot turn an invalid definition into a People request.
  return namedDefinitionUsable.value
})
watch(
  () => [isNamedList.value, props.savedListId, savedListQuery.data.value] as const,
  ([named, id, detail]) => {
    if (!named) {
      savedBaseline.value = null
      return
    }
    if (detail?.list.id === id) applySavedDetail(detail)
  },
  { immediate: true },
)
let pendingWrite: { value: string | undefined } | null = null

function writeFilterParam(value: string | undefined) {
  if (isNamedList.value) return
  // Compare the raw value: empty, bare, and repeated params still need
  // removal even though none represents an applied filter.
  // Even if the URL already matches, a newer clear must supersede an
  // earlier navigation that has not finished yet.
  if (value === route.query.filter && !pendingWrite) return
  const query = { ...route.query }
  if (value === undefined) delete query.filter
  else query.filter = value
  const write = { value }
  pendingWrite = write
  void router.replace({ query, hash: route.hash }).finally(() => {
    if (pendingWrite === write) pendingWrite = null
  })
}

function onClausesUpdate(next: FilterClause[]) {
  clauses.value = committedClauses(next)
  filterOrigin.value = null
  if (!isNamedList.value) writeFilterParam(serializedFilter.value)
}

// Run before usePeople so a cached session and a shared URL issue only
// the filtered request. A router echo of our own edit keeps its origin;
// actual navigation (including to /people without a filter) restores it.
watch(
  () => route.query.filter,
  (raw) => {
    if (isNamedList.value || route.path !== '/people' || raw === serializedFilter.value) return
    const parsed = typeof raw === 'string' && raw !== '' ? parseFilter(raw) : null
    clauses.value = parsed ?? []
    filterOrigin.value = hasFilter.value ? 'url' : null
    editorRevision.value++
    if (!hasFilter.value) writeFilterParam(undefined)
  },
  { immediate: true },
)

const {
  data: peopleData, isPending, isFetching, isPlaceholderData, isError, error, refetch,
} = usePeople(orgId, serializedFilter, namedPeopleEnabled, isNamedList)
const people = computed(() => peopleData.value?.people ?? [])

// Preserve the existing §6 policy: only a URL-origin 400/422 drops the
// filter. User-created filters and all 5xx failures remain recoverable.
const filterWillDegrade = computed(
  () => !isNamedList.value && isError.value && filterOrigin.value === 'url' && hasFilter.value &&
    error.value instanceof ApiError && [400, 422].includes(error.value.status),
)
watch(filterWillDegrade, (degrade) => {
  if (degrade) {
    editorRevision.value++
    onClausesUpdate([])
  }
})

const resultLabel = computed(() => {
  if (isNamedList.value && !namedPeopleEnabled.value) return ''
  if (isPlaceholderData.value) return people.value.length > 0
    ? 'Updating results… Previous results are shown below.'
    : 'Updating results…'
  if (isPending.value || filterWillDegrade.value) return 'Loading people…'
  if (!peopleData.value) return ''
  const count = people.value.length
  const noun = hasFilter.value ? (count === 1 ? 'match' : 'matches') : (count === 1 ? 'person' : 'people')
  return `${count}${peopleData.value.truncated ? '+' : ''} ${noun}`
})

// ---- Saved-definition workspace -----------------------------------------

const createMutation = useCreateSavedListMutation(orgId, actorId)
const updateMutation = useUpdateSavedListMutation(orgId, actorId)
const deleteMutation = useDeleteSavedListMutation(orgId, actorId)
const namedCount = useSavedListCount(
  orgId,
  actorId,
  computed(() => savedBaseline.value?.id ?? ''),
  computed(() => savedBaseline.value?.revision ?? 0),
  computed(() => isNamedList.value && namedPeopleEnabled.value && savedBaseline.value !== null),
)

async function refreshNamedWorkspace() {
  const identity = captureViewIdentity()
  const response = await savedListQuery.refetch()
  if (response.isError || !response.data || !currentIdentityMatches(identity) || !namedDefinitionUsable.value) return
  // Detail is the authority for readable criteria. Refresh both dependent
  // reads even when the revision/filter string is unchanged: relative-time
  // filters and cached counts may have crossed a boundary.
  await Promise.allSettled([refetch(), namedCount.refetch()])
}

watch(() => savedListQuery.dataUpdatedAt.value, (updatedAt, previous) => {
  if (updatedAt === 0 || updatedAt === previous) return
  void nextTick(() => {
    if (isNamedList.value && namedDefinitionUsable.value) {
      void refetch()
      void namedCount.refetch()
    }
  })
})

type CreateMode = 'save-as' | 'duplicate'
interface SavedListViewIdentity {
  orgId: string
  actorId: string
  session: MeResponse | undefined
  viewEpoch: number
  listId: string
}
interface CreateIntent extends SavedListViewIdentity {
  mode: CreateMode
  preserveOriginalDraft: boolean
  request: CreateSavedListRequest
}
interface DeleteIntent extends SavedListViewIdentity {
  revision: number
  name: string
  scope: 'personal' | 'shared'
}

const createDialogOpen = ref(false)
const createMode = ref<CreateMode>('save-as')
const pendingCreate = ref<CreateIntent | null>(null)
const createError = ref<unknown>()
const createUncertain = ref(false)
const createRetryUnavailable = ref(false)
const createdCopy = ref<{ id: string; name: string } | null>(null)
const saveError = ref<unknown>()
const saveNotice = ref('')
const deleteDialogOpen = ref(false)
const pendingDelete = ref<DeleteIntent | null>(null)
const deleteConflictMessage = ref('')
let activeView = true
let permitDirtyNavigation = false
let viewEpoch = 0

function captureViewIdentity(): SavedListViewIdentity {
  return {
    orgId: orgId.value,
    actorId: actorId.value,
    session: me.value,
    viewEpoch,
    listId: props.savedListId,
  }
}

function currentIdentityMatches(identity: SavedListViewIdentity) {
  return activeView && me.value === identity.session &&
    orgId.value === identity.orgId && actorId.value === identity.actorId &&
    viewEpoch === identity.viewEpoch && props.savedListId === identity.listId
}

function sameSavedDefinition(detail: SavedListDetailResponse, name: string, filter: FilterDefinition) {
  return detail.filter !== null && detail.list.name === name &&
    filtersEqual(detail.filter, filter)
}

async function reconcileSavedUpdate(
  identity: SavedListViewIdentity,
  submittedName: string,
  submittedFilter: FilterDefinition,
) {
  const response = await savedListQuery.refetch()
  const detail = response.data
  if (response.isError || !detail || !currentIdentityMatches(identity) || detail.list.id !== identity.listId) return
  const draftChangedDuringRequest = nameDraft.value.trim() !== submittedName ||
    !filtersEqual(workingFilter.value, submittedFilter)
  if (sameSavedDefinition(detail, submittedName, submittedFilter)) {
    installSavedDetail(detail, draftChangedDuringRequest)
    saveError.value = undefined
    saveNotice.value = draftChangedDuringRequest
      ? 'The saved version includes the submitted changes. Your newer local edits are still unsaved.'
      : 'The saved version already includes those changes.'
    return
  }
  // Rebase the draft on the reviewed current revision without changing its
  // local name/filter. The user decides whether to Save again; no force write.
  installSavedDetail(detail, true)
  saveNotice.value = 'The saved version changed. Your draft is still here; review it and choose Save again.'
}

// A definite failed write still needs a fresh authority read: a demotion or
// deletion can make retained detail data unsafe to keep rendering. Applying
// without `force` retains a dirty baseline, so this never silently rebases a
// 409 draft onto somebody else's revision.
async function refreshSavedListAuthority(identity: SavedListViewIdentity) {
  const response = await savedListQuery.refetch()
  if (response.isError || !response.data || !currentIdentityMatches(identity)) return
  applySavedDetail(response.data)
}

function requestId() {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') return crypto.randomUUID()
  // The fallback remains canonical UUID text for older/local test browsers.
  return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, (letter) => {
    const value = Math.floor(Math.random() * 16)
    return (letter === 'x' ? value : (value & 0x3) | 0x8).toString(16)
  })
}

function copyName(name: string) {
  const prefix = 'Copy of '
  return `${prefix}${Array.from(name).slice(0, 80 - Array.from(prefix).length).join('')}`
}

const createDialogTitle = computed(() => createMode.value === 'duplicate' ? 'Duplicate list' : 'Save as list')
const createDialogName = computed(() => {
  if ((createUncertain.value || createRetryUnavailable.value) && pendingCreate.value) return pendingCreate.value.request.name
  if (createMode.value === 'duplicate' && savedBaseline.value) return copyName(savedBaseline.value.name)
  return isNamedList.value && nameDraft.value.trim() ? nameDraft.value.trim() : 'New saved list'
})
const createDialogDescription = computed(() => {
  const filter = createMode.value === 'duplicate' ? savedBaseline.value?.filter : workingFilter.value
  const source = createMode.value === 'duplicate'
    ? 'This duplicate starts from the saved version of this list.'
    : 'This saves the criteria currently shown.'
  if (!filter || filter.clauses.length === 0) return `${source} It includes all people.`
  const includesMe = filter.clauses.some((clause) =>
    clause.kind === 'assigned_to' && clause.assignees.includes('me'),
  )
  return includesMe
    ? `${source} “Me” stays symbolic, so it follows the person using the list.`
    : source
})
const canChooseShared = computed(() => me.value?.organization?.role === 'admin')

function openSaveAs() {
  if (isNamedList.value && !namedDefinitionUsable.value) return
  createMutation.reset()
  createMode.value = 'save-as'
  pendingCreate.value = null
  createError.value = undefined
  createUncertain.value = false
  createRetryUnavailable.value = false
  createdCopy.value = null
  createDialogOpen.value = true
}

function openDuplicate() {
  if (!savedBaseline.value || !namedDefinitionUsable.value) return
  createMutation.reset()
  createMode.value = 'duplicate'
  pendingCreate.value = null
  createError.value = undefined
  createUncertain.value = false
  createRetryUnavailable.value = false
  createdCopy.value = null
  createDialogOpen.value = true
}

function closeCreateDialog(visible: boolean) {
  if (visible) {
    createDialogOpen.value = true
    return
  }
  if (createUncertain.value && !window.confirm(
    'The server may have saved this list. Closing will not retry it; reload Lists before creating another copy.',
  )) return
  createDialogOpen.value = false
  if (createUncertain.value || createRetryUnavailable.value) pendingCreate.value = null
  createUncertain.value = false
  createRetryUnavailable.value = false
}

function uncertainCreate(error: unknown) {
  return !(error instanceof ApiError) || error.status === 0 || error.status >= 500
}

// A 409 is an authoritative answer: retain the original baseline until the
// user explicitly Reloads or Save-as. Only a lost/5xx write can have reached
// the server despite its failed response, so only those outcomes reconcile.
function uncertainUpdate(error: unknown) {
  return !(error instanceof ApiError) || error.status === 0 || error.status >= 500
}

function terminalCreateRetry(error: unknown) {
  return error instanceof ApiError &&
    (error.code === 'saved_list_request_conflict' || error.code === 'saved_list_deleted')
}

async function submitCreate(intent: CreateIntent) {
  if (createMutation.isPending.value || createRetryUnavailable.value) return
  const retryingUncertainCreate = createUncertain.value && pendingCreate.value === intent
  createError.value = undefined
  try {
    const result = await createMutation.mutateAsync(intent.request)
    if (!currentIdentityMatches(intent)) return
    createUncertain.value = false
    createRetryUnavailable.value = false
    pendingCreate.value = null
    createDialogOpen.value = false
    createdCopy.value = { id: result.list.id, name: result.list.name }
    // A clean Duplicate can open its new list. A dirty source remains in
    // place so the local draft is never lost; its explicit link still passes
    // through the normal dirty-navigation guard.
    if (intent.mode === 'save-as' || !intent.preserveOriginalDraft) {
      permitDirtyNavigation = true
      try {
        await router.push(`/lists/${encodeURIComponent(result.list.id)}`)
      } finally {
        permitDirtyNavigation = false
      }
    }
  } catch (error) {
    if (!currentIdentityMatches(intent)) return
    createError.value = error
    if (uncertainCreate(error)) {
      createUncertain.value = true
      createRetryUnavailable.value = false
      pendingCreate.value = intent
    } else if (retryingUncertainCreate && terminalCreateRetry(error)) {
      // The original token conclusively points to a conflicting/deleted
      // attempt. Keep it frozen until the user explicitly closes/abandons
      // this dialog; never turn a second Retry click into a new token.
      createUncertain.value = false
      createRetryUnavailable.value = true
      pendingCreate.value = intent
    } else {
      pendingCreate.value = null
      createUncertain.value = false
      createRetryUnavailable.value = false
    }
  }
}

function onCreateSubmit(payload: { name: string; scope: 'personal' | 'shared' }) {
  if (createRetryUnavailable.value) return
  if (createUncertain.value && pendingCreate.value) {
    void submitCreate(pendingCreate.value)
    return
  }
  const filter = createMode.value === 'duplicate'
    ? savedBaseline.value?.filter
    : workingFilter.value
  if (!filter || orgId.value === '' || actorId.value === '') return
  const intent: CreateIntent = {
    ...captureViewIdentity(),
    mode: createMode.value,
    preserveOriginalDraft: createMode.value === 'duplicate' && isDirty.value,
    request: {
      request_id: requestId(),
      scope: payload.scope,
      name: payload.name,
      filter: cloneFilter(filter),
    },
  }
  pendingCreate.value = intent
  void submitCreate(intent)
}

function resetSavedDraft() {
  const identity = captureViewIdentity()
  saveError.value = undefined
  saveNotice.value = ''
  void savedListQuery.refetch().then((response) => {
    if (!response.isError && response.data && currentIdentityMatches(identity)) applySavedDetail(response.data, true)
  })
}

async function saveSavedList() {
  const baseline = savedBaseline.value
  const identity = captureViewIdentity()
  if (!baseline || !namedCanEdit.value || !namedDefinitionUsable.value || !currentIdentityMatches(identity)) return
  const submittedName = nameDraft.value.trim()
  const submittedFilter = cloneFilter(workingFilter.value)
  saveError.value = undefined
  saveNotice.value = ''
  try {
    const result = await updateMutation.mutateAsync({
      listId: baseline.id,
      body: { expected_revision: baseline.revision, name: submittedName, filter: submittedFilter },
    })
    if (!currentIdentityMatches(identity)) return
    const draftChangedDuringRequest = nameDraft.value.trim() !== submittedName ||
      !filtersEqual(workingFilter.value, submittedFilter)
    savedBaseline.value = {
      id: result.list.id,
      revision: result.list.revision,
      name: result.list.name,
      filter: submittedFilter,
    }
    if (!draftChangedDuringRequest) nameDraft.value = result.list.name
    void savedListQuery.refetch()
  } catch (error) {
    if (!currentIdentityMatches(identity)) return
    saveError.value = error
    if (!uncertainUpdate(error)) {
      try {
        await refreshSavedListAuthority(identity)
      } catch {
        // Preserve the original error and dirty draft if the authority read
        // itself is unavailable. It is never an automatic write retry.
      }
      return
    }
    try {
      await reconcileSavedUpdate(identity, submittedName, submittedFilter)
    } catch {
      // Keep the submitted draft and original error if the reconciliation
      // read is also unavailable. It is never an automatic retry.
    }
  }
}

// A failed detail refetch must not leave cached private metadata/content on
// screen while a 404/tenant change is being recovered.
const currentSavedMetadata = computed(() =>
  savedListQuery.isError.value || savedListQuery.data.value?.list.id !== props.savedListId
    ? undefined
    : savedListQuery.data.value.list,
)
const namedCanEdit = computed(() => currentSavedMetadata.value?.can_edit === true)
const namedCanDelete = computed(() => currentSavedMetadata.value?.can_delete === true)
const namedFilterUnavailable = computed(() =>
  isNamedList.value && savedListQuery.data.value?.list.id === props.savedListId && savedListQuery.data.value.filter === null,
)
const namedReferenceError = computed(() =>
  isNamedList.value && savedListQuery.data.value?.list.id === props.savedListId &&
    savedListQuery.data.value.filter_error && !workingReferencesResolvable.value,
)

watch(
  () => [isNamedList.value, savedListQuery.isError.value, savedListQuery.error.value] as const,
  ([named, isError, error]) => {
    if (!named || !isError) return
    if (error instanceof ApiError && error.status === 404) {
      savedBaseline.value = null
      clauses.value = []
      const key = queryKeys.savedList(orgId.value, actorId.value, props.savedListId)
      void queryClient.cancelQueries({ queryKey: key, exact: true })
      queryClient.removeQueries({ queryKey: key, exact: true })
    }
  },
)

async function deleteSavedList() {
  const intent = pendingDelete.value
  if (!intent || !currentIdentityMatches(intent)) return
  try {
    await deleteMutation.mutateAsync({
      listId: intent.listId,
      body: { expected_revision: intent.revision },
    })
    if (!currentIdentityMatches(intent)) return
    pendingDelete.value = null
    deleteDialogOpen.value = false
    permitDirtyNavigation = true
    try {
      await router.push('/lists')
    } finally {
      permitDirtyNavigation = false
    }
  } catch (error) {
    if (!currentIdentityMatches(intent)) return
    // The user reviewed a particular revision. A conflict means that review
    // is no longer valid, so refresh and require an explicit new confirmation.
    if (error instanceof ApiError && error.status === 409) {
      pendingDelete.value = null
      deleteDialogOpen.value = false
      deleteMutation.reset()
      deleteConflictMessage.value = 'This list changed before it was deleted. The latest version is ready to review; choose Delete again to confirm it.'
      void savedListQuery.refetch()
    }
  }
}

function openDeleteDialog() {
  const list = currentSavedMetadata.value
  if (!list) return
  pendingDelete.value = { ...captureViewIdentity(), revision: list.revision, name: list.name, scope: list.scope }
  deleteMutation.reset()
  deleteConflictMessage.value = ''
  deleteDialogOpen.value = true
}

function closeDeleteDialog(visible: boolean) {
  if (visible) {
    deleteDialogOpen.value = true
    return
  }
  if (deleteMutation.isPending.value) return
  deleteDialogOpen.value = false
  pendingDelete.value = null
  deleteMutation.reset()
}

const deleteDialogMessage = computed(() => {
  const intent = pendingDelete.value
  if (!intent) return 'Delete this saved definition? People are not changed.'
  const access = intent.scope === 'shared'
    ? ' Everyone in this Organization will lose access to this shared list.'
    : ' This personal list is only visible to you.'
  return `Delete “${intent.name}”? People are not changed.${access}`
})

const workspaceCountLabel = computed(() => {
  if (!isNamedList.value || !savedBaseline.value) return ''
  const prefix = isDirty.value ? 'Saved version: ' : ''
  if (!namedDefinitionUsable.value) return `${prefix}Match count paused`
  if (namedCount.isPending.value || namedCount.isFetching.value) return `${prefix}Updating match count…`
  if (namedCount.data.value) return `${prefix}${namedCount.data.value.truncated ? '500+ matches' :
    `${namedCount.data.value.count} ${namedCount.data.value.count === 1 ? 'match' : 'matches'}`}`
  if (namedCount.isError.value) return `${prefix}Match count unavailable`
  return ''
})
const savedDescription = computed(() => {
  const description = savedListQuery.data.value?.list.id === props.savedListId
    ? savedListQuery.data.value.description.join(' · ')
    : ''
  return isDirty.value && description ? `Saved version: ${description}` : description
})

function confirmDiscardDraft() {
  return !isDirty.value || permitDirtyNavigation || window.confirm('Discard unsaved changes to this saved list?')
}

onBeforeRouteLeave(() => confirmDiscardDraft())
onBeforeRouteUpdate((to, from) => {
  if (to.params.savedListId !== from.params.savedListId) return confirmDiscardDraft()
  return true
})

// Vue reuses this component for /lists/A → /lists/B. Every saved-list
// completion captures this epoch, so an old mutation/reset cannot navigate
// or alter the new workspace even when the actor and Organization match.
watch(
  () => [props.savedListId, orgId.value, actorId.value, me.value] as const,
  ([nextListId, nextOrgId, nextActorId, nextSession], [previousListId, previousOrgId, previousActorId, previousSession]) => {
    viewEpoch++
    const listChanged = nextListId !== previousListId
    const publicIdentityChanged = previousOrgId !== nextOrgId || previousActorId !== nextActorId
    const loggedOut = previousSession !== undefined && nextSession === undefined
    // A route switch closes view-local controls even when A and B serialize
    // identically. A same-actor `/me` refresh still advances the epoch (late
    // mutations are suppressed), but deliberately preserves an accessible
    // reader's draft and an ordinary /people URL filter.
    createDialogOpen.value = false
    pendingCreate.value = null
    createUncertain.value = false
    createRetryUnavailable.value = false
    createError.value = undefined
    createdCopy.value = null
    deleteDialogOpen.value = false
    pendingDelete.value = null
    saveError.value = undefined
    saveNotice.value = ''
    deleteConflictMessage.value = ''
    if (listChanged || (isNamedList.value && previousSession !== undefined && (publicIdentityChanged || loggedOut))) {
      savedBaseline.value = null
      nameDraft.value = ''
      clauses.value = []
      filterOrigin.value = null
      editorRevision.value++
    }
    if ((publicIdentityChanged || loggedOut) && previousOrgId !== '' && previousActorId !== '') {
      const listPrefix = queryKeys.savedLists(previousOrgId, previousActorId)
      const countPrefix = queryKeys.savedListCountsForActor(previousOrgId, previousActorId)
      void queryClient.cancelQueries({ queryKey: listPrefix })
      queryClient.removeQueries({ queryKey: listPrefix })
      void queryClient.cancelQueries({ queryKey: countPrefix })
      queryClient.removeQueries({ queryKey: countPrefix })
      if (isNamedList.value && orgId.value !== '' && actorId.value !== '') {
        void nextTick(() => savedListQuery.refetch())
      }
    }
  },
  { flush: 'sync' },
)

function beforeUnload(event: BeforeUnloadEvent) {
  if (!isDirty.value) return
  event.preventDefault()
  event.returnValue = ''
}
function refreshOnWindowFocus() {
  if (isNamedList.value) void refreshNamedWorkspace()
}
onMounted(() => {
  window.addEventListener('beforeunload', beforeUnload)
  // query-core's visibility listener does not observe every desktop-window
  // focus transition. Keep this UI-owned recovery path explicit.
  window.addEventListener('focus', refreshOnWindowFocus)
})
onBeforeUnmount(() => {
  activeView = false
  window.removeEventListener('beforeunload', beforeUnload)
  window.removeEventListener('focus', refreshOnWindowFocus)
})

const selectedId = ref('')
const preview = ref<InstanceType<typeof PersonPreview> | null>(null)
let previewTrigger: HTMLElement | null = null
const selectedPerson = computed(() => people.value.find((person) => person.id === selectedId.value))

function selectPerson(person: PersonSummary) {
  previewTrigger = document.querySelector<HTMLElement>(`a[href="/people/${CSS.escape(person.id)}"]`) ?? null
  selectedId.value = person.id
  void nextTick(() => preview.value?.focus())
}
function closePreview() {
  selectedId.value = ''
  void nextTick(() => {
    if (previewTrigger?.isConnected) previewTrigger.focus()
  })
}
watch([orgId, serializedFilter, () => props.savedListId], () => { selectedId.value = '' }, { flush: 'sync' })
watch(selectedPerson, (person) => { if (!person) selectedId.value = '' })

const myPeople = computed(() => clauses.value.length === 1 &&
  clauses.value[0]?.kind === 'assigned_to' && clauses.value[0].assignees.length === 1 &&
  clauses.value[0].assignees[0] === 'me')

const columns: ColumnDef<PersonSummary>[] = [
  {
    id: 'name',
    header: 'Name',
    cell: (info) => {
      const person = info.row.original
      return h('div', { class: 'flex min-w-[150px] items-center gap-2.5', title: person.primary_email ?? undefined }, [
        h('span', { class: 'avatar-surface h-7 w-7', 'aria-hidden': 'true' }, initials(person.display_name)),
        h('span', { class: 'text-body text-text' }, person.display_name),
      ])
    },
  },
  {
    id: 'stage',
    header: 'Stage',
    cell: (info) =>
      h(StageLabel, { stage: info.row.original.stage, badge: true }),
  },
  {
    id: 'assignee',
    header: 'Assignee',
    cell: (info) => {
      const assignee = info.row.original.assigned_user
      return assignee
        ? h('span', { class: 'text-text-muted' }, assignee.display_name)
        : h('span', { class: 'text-text-muted' }, 'Unassigned')
    },
  },
  {
    id: 'inquiry_count',
    header: 'Inquiries',
    meta: { align: 'right' },
    cell: (info) => String(info.row.original.inquiry_count),
  },
  {
    id: 'last_inquiry_at',
    header: 'Last inquiry',
    cell: (info) => {
      const value = info.row.original.last_inquiry_at
      if (!value) return h('span', { class: 'text-text-muted' }, '—')
      return h('span', { title: formatAbsoluteTime(value) }, formatRelativeTime(value))
    },
  },
]
</script>

<template>
  <div class="people-layout">
    <div class="people-list">
      <PageHeader :title="isNamedList ? (currentSavedMetadata?.name ?? 'Saved list') : 'People'">
        <template #action>
          <div class="flex flex-wrap justify-end gap-2">
            <template v-if="isNamedList">
              <button
                v-if="currentSavedMetadata"
                type="button"
                :class="buttonClasses('secondary')"
                :disabled="savedListQuery.isFetching.value"
                @click="refreshNamedWorkspace"
              >
                Refresh
              </button>
              <button
                v-if="namedDefinitionUsable"
                type="button"
                :class="buttonClasses('secondary')"
                @click="openSaveAs"
              >
                <Copy
                  class="h-4 w-4"
                  stroke-width="1.5"
                />
                Save as
              </button>
              <button
                v-if="savedBaseline && namedDefinitionUsable"
                type="button"
                :class="buttonClasses('secondary')"
                @click="openDuplicate"
              >
                <Copy
                  class="h-4 w-4"
                  stroke-width="1.5"
                />
                Duplicate
              </button>
              <button
                v-if="namedCanEdit"
                type="button"
                :class="buttonClasses('primary')"
                :disabled="!isDirty || !namedDefinitionUsable || updateMutation.isPending.value"
                @click="saveSavedList"
              >
                <Save
                  class="h-4 w-4"
                  stroke-width="1.5"
                />
                {{ updateMutation.isPending.value ? 'Saving…' : 'Save' }}
              </button>
              <button
                v-if="namedCanDelete"
                type="button"
                :class="buttonClasses('danger')"
                :disabled="deleteMutation.isPending.value"
                @click="openDeleteDialog"
              >
                <Trash2
                  class="h-4 w-4"
                  stroke-width="1.5"
                />
                Delete
              </button>
            </template>
            <template v-else>
              <button
                type="button"
                :class="buttonClasses('secondary')"
                @click="openSaveAs"
              >
                <ListFilter
                  class="h-4 w-4"
                  stroke-width="1.5"
                />
                Save as list
              </button>
              <RouterLink
                to="/intake/new"
                :class="buttonClasses('primary')"
              >
                <Plus
                  class="h-4 w-4"
                  stroke-width="1.5"
                />
                New lead
              </RouterLink>
            </template>
          </div>
        </template>
      </PageHeader>

      <div
        v-if="createdCopy"
        class="mb-4 flex flex-wrap items-center justify-between gap-3 rounded-xl border border-border bg-surface-0 p-4 text-body text-text"
      >
        <span>Created <strong class="font-medium">{{ createdCopy.name }}</strong>.</span>
        <RouterLink
          :to="`/lists/${createdCopy.id}`"
          class="font-medium text-accent hover:underline"
        >
          Open list
        </RouterLink>
      </div>

      <section
        v-if="isNamedList && savedListQuery.isPending.value && !savedListQuery.data.value"
        class="rounded-xl border border-border bg-surface-0 p-5 text-body text-text-muted"
      >
        Loading saved list…
      </section>
      <section
        v-else-if="isNamedList && savedListQuery.isError.value"
        role="alert"
        class="rounded-xl border border-border bg-surface-0 p-5 text-body text-danger"
      >
        <p>This saved list is not available.</p>
        <div class="mt-3 flex gap-2">
          <button
            type="button"
            :class="buttonClasses('secondary')"
            @click="refreshNamedWorkspace"
          >
            Try again
          </button>
          <RouterLink
            to="/lists"
            :class="buttonClasses('secondary')"
          >
            Back to lists
          </RouterLink>
        </div>
      </section>
      <section
        v-else-if="namedFilterUnavailable"
        role="alert"
        class="rounded-xl border border-border bg-surface-0 p-5 text-body text-danger"
      >
        <h2 class="text-section font-semibold text-text">
          This list’s criteria are unavailable
        </h2>
        <p class="mt-1">
          People are not loaded because this saved definition cannot be evaluated safely.
        </p>
        <div class="mt-3 flex gap-2">
          <button
            type="button"
            :class="buttonClasses('secondary')"
            @click="refreshNamedWorkspace"
          >
            Refresh definition
          </button>
          <RouterLink
            to="/lists"
            :class="buttonClasses('secondary')"
          >
            Back to lists
          </RouterLink>
        </div>
      </section>

      <template v-else>
        <div
          v-if="isNamedList"
          class="mb-4 rounded-xl border border-border bg-surface-0 p-4"
        >
          <div class="flex flex-wrap items-center justify-between gap-3">
            <label
              v-if="namedCanEdit"
              class="min-w-0 flex-1"
            >
              <span class="sr-only">List name</span>
              <input
                v-model="nameDraft"
                class="w-full bg-transparent text-section font-semibold text-text outline-none placeholder:text-text-muted"
                aria-label="List name"
                :disabled="updateMutation.isPending.value"
              >
            </label>
            <p
              v-else
              class="text-section font-semibold text-text"
            >
              {{ currentSavedMetadata?.name }}
            </p>
            <p
              v-if="workspaceCountLabel"
              class="text-small text-text-muted"
              role="status"
              aria-live="polite"
            >
              {{ workspaceCountLabel }}
            </p>
          </div>
          <p
            v-if="savedDescription"
            class="mt-2 text-small text-text-muted"
          >
            {{ savedDescription }}
          </p>
          <p
            v-if="currentSavedMetadata"
            class="mt-2 text-small text-text-muted"
          >
            {{ currentSavedMetadata.scope === 'shared'
              ? 'Shared with everyone in this Organization'
              : 'Personal list — only you can use it' }}
          </p>
          <div
            v-if="isDirty"
            class="mt-3 flex flex-wrap items-center gap-2"
          >
            <span class="text-small text-text-muted">Unsaved changes</span>
            <button
              type="button"
              class="text-small font-medium text-accent hover:underline"
              @click="resetSavedDraft"
            >
              Reset changes
            </button>
          </div>
          <div
            v-if="saveError"
            role="alert"
            class="mt-3 flex flex-wrap items-center gap-3 text-small text-danger"
          >
            <span>{{ describeApiError(saveError, 'This list changed or could not be saved. Review it and try again.') }}</span>
            <button
              type="button"
              class="font-medium text-accent hover:underline"
              @click="resetSavedDraft"
            >
              Reload saved version
            </button>
            <button
              type="button"
              class="font-medium text-accent hover:underline"
              @click="openSaveAs"
            >
              Save as a new list
            </button>
          </div>
          <p
            v-if="saveNotice"
            role="status"
            class="mt-3 text-small text-text-muted"
          >
            {{ saveNotice }}
          </p>
          <p
            v-if="deleteConflictMessage"
            role="alert"
            class="mt-3 text-small text-danger"
          >
            {{ deleteConflictMessage }}
          </p>
        </div>

        <div
          v-if="namedReferenceError"
          role="alert"
          class="mb-3 rounded-xl border border-border bg-surface-0 p-4 text-body text-danger"
        >
          <p>This list references an item that is no longer available. People are paused until the criteria are repaired.</p>
          <p
            v-if="!namedCanEdit"
            class="mt-1 text-small text-text-muted"
          >
            You can change the criteria locally, then Save as a new list.
          </p>
        </div>

        <div
          v-if="!isNamedList"
          class="people-tabs"
          aria-label="People views"
        >
          <button
            type="button"
            class="people-tab"
            :aria-pressed="!hasFilter"
            @click="onClausesUpdate([])"
          >
            All people
          </button>
          <button
            type="button"
            class="people-tab"
            :aria-pressed="myPeople"
            @click="onClausesUpdate([{ kind: 'assigned_to', assignees: ['me'] }])"
          >
            My people
          </button>
          <span
            v-if="hasFilter && !myPeople"
            class="flex items-center text-small text-text-muted"
          >Filtered view</span>
        </div>

        <div class="people-tools">
          <FilterBar
            v-if="!isNamedList || namedDefinitionReadable"
            :key="editorRevision"
            :clauses="clauses"
            :stages="stages"
            :members="members"
            :sources="sources"
            :stages-pending="stagesQuery.isPending.value"
            :stages-error="stagesQuery.isError.value"
            :members-pending="membersQuery.isPending.value"
            :members-error="membersQuery.isError.value"
            :sources-pending="sourcesQuery.isPending.value"
            :sources-error="sourcesQuery.isError.value"
            :sources-truncated="sourcesQuery.data.value?.truncated ?? false"
            @update:clauses="onClausesUpdate"
            @retry-options="retryOptions"
          />
          <div
            v-else
            class="min-h-10 flex-1 text-small text-text-muted"
          >
            This list’s criteria are unavailable.
          </div>

          <div class="flex min-h-10 flex-col items-end justify-center gap-1 text-small text-text-muted">
            <span v-if="clauses.length > 1">Match all filters</span>
            <p
              data-testid="people-result-count"
              role="status"
              aria-live="polite"
            >
              {{ resultLabel }}
              <span v-if="peopleData?.truncated && !isPlaceholderData"> · Showing the first {{ people.length }}</span>
              <span v-if="isFetching && !isPending && !isPlaceholderData"> · Refreshing…</span>
            </p>
          </div>
        </div>

        <template v-if="!isNamedList || namedPeopleEnabled">
          <div
            v-if="isError && !filterWillDegrade"
            role="alert"
            class="mb-3 flex flex-wrap items-center justify-between gap-3 rounded-xl border border-border bg-surface-0 p-5 text-body text-danger"
          >
            <span>{{ describeApiError(error, 'Could not load people.') }}</span>
            <button
              type="button"
              :class="buttonClasses('secondary')"
              @click="refetch()"
            >
              Try again
            </button>
          </div>
          <div
            v-if="((isPending || filterWillDegrade) && !peopleData) || (isPlaceholderData && people.length === 0)"
            class="rounded-xl border border-border bg-surface-0 p-5 text-body text-text-muted"
          >
            {{ isPlaceholderData ? 'Updating results…' : 'Loading…' }}
          </div>
          <div
            v-else-if="peopleData"
            :aria-busy="isFetching"
            :inert="isPlaceholderData"
            :class="{ 'opacity-60': isPlaceholderData }"
          >
            <DataTable
              class="people-table"
              :on-row-click="selectPerson"
              :selected-row-key="selectedId"
              :data="people"
              :columns="columns"
              :row-key="(person) => person.id"
              :row-to="(person) => `/people/${person.id}`"
              count-noun="people"
              count-noun-singular="person"
              :truncated="peopleData.truncated"
              :empty-title="hasFilter ? 'No people match these filters' : 'No people yet'"
              :empty-message="hasFilter ? 'Change or clear your filters to see more people.' : 'Leads you add or receive will appear here.'"
              :empty-icon="Users"
              :empty-action-label="hasFilter ? undefined : 'Add a lead'"
              :empty-action-to="hasFilter ? undefined : '/intake/new'"
            />
          </div>
        </template>
      </template>
    </div>
    <PersonPreview
      v-if="selectedPerson"
      :key="`${orgId}:${selectedPerson.id}`"
      ref="preview"
      :org-id="orgId"
      :summary="selectedPerson"
      @close="closePreview"
    />
    <SavedListDialog
      :visible="createDialogOpen"
      :title="createDialogTitle"
      :submit-label="createMode === 'duplicate' ? 'Duplicate list' : 'Save list'"
      :initial-name="createDialogName"
      :description="createDialogDescription"
      initial-scope="personal"
      :allow-shared="canChooseShared"
      :is-pending="createMutation.isPending.value"
      :freeze-payload="createUncertain || createRetryUnavailable"
      :retry-unavailable="createRetryUnavailable"
      :error="createError"
      @update:visible="closeCreateDialog"
      @submit="onCreateSubmit"
    />
    <ConfirmDialog
      :visible="deleteDialogOpen"
      title="Delete saved list"
      :message="deleteDialogMessage"
      confirm-label="Delete list"
      confirm-variant="danger"
      :is-pending="deleteMutation.isPending.value"
      :error="deleteMutation.error.value"
      error-fallback="Could not delete this list. Refresh it and try again."
      @update:visible="closeDeleteDialog"
      @confirm="deleteSavedList"
    />
  </div>
</template>
