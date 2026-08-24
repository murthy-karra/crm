// The app-level call host (docs/specs/SLICE_006b.md §6): ONE call session
// and ONE docked CallPanel for the whole app, fed by both the Person
// page's Call button and the Ask drawer's Confirm. Created and provided
// by AppShell (components/CallHostPanel.vue renders the panel); views and
// the drawer reach it with `useCallHost()`.
//
// What moved here from PersonDetailView (SLICE_006c §5a preserved
// verbatim): the post-call forced-outcome prompt and its save mutation.
// What did NOT move: the History Set/Change-outcome dialog and the
// `?outcome=` deep link — person-page concerns.
import { computed, inject, provide, ref, type ComputedRef, type InjectionKey, type MaybeRefOrGetter, type Ref } from 'vue'
import { useConfirmProposal, useCorrectCallOutcome } from '../api/queries'
import type { CallOutcomeCorrection, OperatorProposal } from '../api/types'
import { ApiError } from '../api/client'
import { describeOutcomeError } from './errors'
import { showsOutcomePrompt } from './format'
import { useCall, type CallRoomFactory, type UseCallResult } from './useCall'
import { queryKeys } from '../api/queries'
import { useQueryClient, type QueryClient } from '@tanstack/vue-query'

export interface CallHost {
  call: UseCallResult
  /** The callee's name as it was when the call started — the panel keeps
   * naming that Person even if the route changes mid-call. */
  calleeName: Ref<string>
  /** SLICE_006c §5a: the panel stays until Save succeeds. */
  outcomePromptOpen: ComputedRef<boolean>
  outcomeSaving: ComputedRef<boolean>
  outcomeSaved: Ref<CallOutcomeCorrection | null>
  outcomeError: Ref<string | null>
  saveOutcome(outcome: CallOutcomeCorrection): void
  /** The Person page's Call button. */
  startFromPerson(personId: string, personName: string, contactMethodId: string): void
  /** The drawer's Confirm (SLICE_006b §6): mic first, then the confirm
   * POST, then the ordinary join → dial path. Resolves to the terminal
   * error code when the attempt failed before ringing (e.g.
   * `proposal_expired`, `microphone_denied`), or null. */
  startFromProposal(proposal: OperatorProposal): Promise<string | null>
  dismissCall(): void
}

export const CALL_HOST_KEY: InjectionKey<CallHost> = Symbol('call-host')

export interface CallHostOptions {
  orgId: MaybeRefOrGetter<string>
  createRoom: CallRoomFactory
  queryClient?: QueryClient
}

export function createCallHost(options: CallHostOptions): CallHost {
  const call = useCall({ orgId: options.orgId, createRoom: options.createRoom, queryClient: options.queryClient })
  const qc = options.queryClient ?? useQueryClient()
  const confirmMutation = useConfirmProposal(options.orgId, options.queryClient)
  const calleeName = ref('')

  // ---- Post-call outcome (SLICE_006c §10, §5a — moved verbatim) ----------
  const panelOutcome = useCorrectCallOutcome(options.orgId, options.queryClient)
  const outcomeSaved = ref<CallOutcomeCorrection | null>(null)
  const outcomeError = ref<string | null>(null)
  const saving = ref(false)

  const outcomePromptOpen = computed(() =>
    showsOutcomePrompt(call.phase.value, call.error.value !== null, call.call.value, outcomeSaved.value !== null),
  )
  const outcomeSaving = computed(() => saving.value || panelOutcome.isPending.value)

  function resetOutcome() {
    outcomeSaved.value = null
    outcomeError.value = null
    saving.value = false
    panelOutcome.reset()
  }

  function saveOutcome(outcome: CallOutcomeCorrection) {
    if (saving.value || panelOutcome.isPending.value) return
    const callId = call.callId.value
    const personId = call.personId.value
    if (callId === '' || personId === '') return
    saving.value = true
    outcomeError.value = null
    panelOutcome.mutate(
      { callId, personId, outcome },
      {
        // §5a: the panel stays until Save succeeds, then shows "Outcome
        // saved — <label>" → Done. `changed: false` is a success too.
        onSuccess: () => {
          outcomeSaved.value = outcome
        },
        onError: (failure) => {
          outcomeError.value = describeOutcomeError(failure)
          if (failure instanceof ApiError && failure.code === 'correction_conflict') {
            const orgId = typeof options.orgId === 'function' ? options.orgId() : options.orgId
            void qc.invalidateQueries({
              queryKey: queryKeys.person(typeof orgId === 'string' ? orgId : orgId.value, personId),
            })
          }
        },
        onSettled: () => {
          saving.value = false
        },
      },
    )
  }

  function startFromPerson(personId: string, personName: string, contactMethodId: string): void {
    calleeName.value = personName
    resetOutcome()
    void call.start(personId, contactMethodId)
  }

  async function startFromProposal(proposal: OperatorProposal): Promise<string | null> {
    // Local pre-checks: nothing is POSTed, the proposal is not consumed,
    // and — unlike the Call button — an unsaved D-033 outcome prompt is
    // never silently discarded (SLICE_006c §5a).
    if (call.active.value) return 'call_in_progress'
    if (outcomePromptOpen.value) return 'outcome_pending'
    calleeName.value = proposal.person.display_name
    resetOutcome()
    await call.startProposed(proposal.person.id, async () => {
      // The response holds the join token: read it out and reset at once,
      // so it never outlives this call in the MutationCache (the same rule
      // useStartCall documents; gcTime: 0 alone does not cover an
      // app-lifetime observer).
      const response = await confirmMutation.mutateAsync(proposal.id)
      confirmMutation.reset()
      return response
    })
    return call.phase.value === 'failed' ? (call.error.value?.code ?? 'unknown_error') : null
  }

  function dismissCall(): void {
    call.dismiss()
    resetOutcome()
  }

  return {
    call,
    calleeName,
    outcomePromptOpen,
    outcomeSaving,
    outcomeSaved,
    outcomeError,
    saveOutcome,
    startFromPerson,
    startFromProposal,
    dismissCall,
  }
}

export function provideCallHost(options: CallHostOptions): CallHost {
  const host = createCallHost(options)
  provide(CALL_HOST_KEY, host)
  return host
}

export function useCallHost(): CallHost {
  const host = inject(CALL_HOST_KEY, null)
  if (!host) throw new Error('useCallHost() requires provideCallHost() in an ancestor (AppShell)')
  return host
}
