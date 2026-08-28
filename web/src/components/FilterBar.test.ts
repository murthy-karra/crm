// SLICE_011a review R1 fix: `replaceClause` (FilterBar.vue) must
// APPEND-IF-MISSING, not silently no-op when its `kind` is no longer in
// `props.clauses` while its editor is still open. The bug this guards:
// the old `.map()`-only version replaced nothing when the array didn't
// contain `kind`, so a checkbox tick after `clauses` was cleared out from
// under an open editor (e.g. a URL-origin degrade — see PeopleView.test.ts
// for the end-to-end scenarios) produced no `update:clauses` event at
// all, wedging the editor permanently. This file pins the fix directly at
// the component boundary, since the other two review fixes (draft
// clauses never reach the server; the degrade is now origin-gated) make
// the full end-to-end repro unreachable through the UI — the underlying
// class of bug (`clauses` and an open editor disagreeing) is still worth
// guarding structurally.
import { mount } from '@vue/test-utils'
import PrimeVue from 'primevue/config'
import Select from 'primevue/select'
import { describe, expect, it } from 'vitest'
import FilterBar from './FilterBar.vue'
import type { FilterClause, Member, Stage } from '../api/types'

const STAGE: Stage = { id: 'stage-1', name: 'Lead', position: 1 }
const MEMBERS: Member[] = []

function mountBar(clauses: FilterClause[]) {
  return mount(FilterBar, {
    props: { clauses, stages: [STAGE], members: MEMBERS, sources: [] },
    global: { plugins: [[PrimeVue, { unstyled: true }]] },
    attachTo: document.body,
  })
}

describe('FilterBar — replaceClause append-if-missing (review R1)', () => {
  it('ticking a checkbox after clauses was cleared externally still creates the clause (no wedge)', async () => {
    const wrapper = mountBar([])

    // Open the stage editor via "Add filter".
    const addSelect = wrapper.findComponent(Select)
    await addSelect.vm.$emit('update:model-value', 'stage')
    await wrapper.vm.$nextTick()
    expect(wrapper.emitted('update:clauses')).toBeTruthy()

    // Simulate the parent clearing the filter out from under the still-
    // open editor (e.g. a URL-origin degrade) — `clauses` goes back to
    // empty, but FilterBar's own `editingKind` (internal, not a prop)
    // stays open, exactly like PeopleView's real clear does.
    await wrapper.setProps({ clauses: [] })
    expect(wrapper.find('[data-testid="filter-editor-stage"]').exists()).toBe(true)

    // Tick the (only) stage checkbox.
    await wrapper.get('input[type="checkbox"]').setValue(true)

    const emissions = wrapper.emitted('update:clauses') as [FilterClause[]][]
    const last = emissions[emissions.length - 1][0]
    const stageClause = last.find((c) => c.kind === 'stage')
    expect(stageClause).toBeDefined()
    expect(stageClause && stageClause.kind === 'stage' ? stageClause.stage_ids : []).toEqual([STAGE.id])
  })

  it('the normal (non-wedged) path still replaces in place rather than duplicating', async () => {
    const initial: FilterClause[] = [{ kind: 'stage', stage_ids: [] }]
    const wrapper = mountBar(initial)

    // 'stage' is already present — open its editor via the chip.
    await wrapper.get('[data-testid="filter-chip-stage"] button').trigger('click')
    await wrapper.get('input[type="checkbox"]').setValue(true)

    const emissions = wrapper.emitted('update:clauses') as [FilterClause[]][]
    const last = emissions[emissions.length - 1][0]
    expect(last.filter((c) => c.kind === 'stage')).toHaveLength(1)
  })
})
