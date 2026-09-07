// SLICE_011d.md §1 rule 2, §6: the typed-confirmation gate for disabling
// the unanswered-inquiry feed. Pins the exact-match requirement: case
// variation and a wrong-text Enter both must not confirm.
import { DOMWrapper, flushPromises, mount } from '@vue/test-utils'
import PrimeVue from 'primevue/config'
import { afterEach, describe, expect, it } from 'vitest'
import TypedConfirmDialog from './TypedConfirmDialog.vue'

const cleanups: Array<() => void> = []

afterEach(() => {
  cleanups.splice(0).forEach((cleanup) => cleanup())
  document.body.innerHTML = ''
})

// PrimeVue's Dialog teleports its content to document.body by default, so
// its content is queried there, not inside the component's own (now-empty)
// mount root — same pattern FilterBar.test.ts uses for its own Popover.
// PrimeVue's Dialog defers mounting its Portal content by a tick even when
// `visible` is already `true` at creation, so callers must await a flush
// before querying — every other Dialog in this codebase is opened by a
// reactive `true` after mount for exactly this reason; this helper mounts
// already-visible, so it flushes itself instead.
async function mountDialog() {
  const wrapper = mount(TypedConfirmDialog, {
    props: {
      visible: true,
      title: 'Turn off Unanswered inquiry?',
      message: 'This is the core Today rule...',
      confirmText: 'Unanswered inquiry',
      confirmLabel: 'Turn off',
      isPending: false,
    },
    global: { plugins: [[PrimeVue, { unstyled: true }]] },
    attachTo: document.body,
  })
  cleanups.push(() => wrapper.unmount())
  await flushPromises()
  return { wrapper, body: new DOMWrapper(document.body) }
}

describe('TypedConfirmDialog exact-match gate', () => {
  it('rejects a case-variant match: the submit button stays disabled', async () => {
    const { wrapper, body } = await mountDialog()
    const input = body.get('[data-testid="typed-confirm-input"]')
    await input.setValue('unanswered inquiry')

    expect(body.get('[data-testid="typed-confirm-submit"]').attributes('disabled')).toBeDefined()
    expect(wrapper.emitted('confirm')).toBeUndefined()
  })

  it('does not confirm on Enter when the typed text is wrong, even close to correct', async () => {
    const { wrapper, body } = await mountDialog()
    const input = body.get('[data-testid="typed-confirm-input"]')
    await input.setValue('Unanswered inquiry ')
    await input.trigger('keydown', { key: 'Enter', code: 'Enter' })
    await flushPromises()

    expect(wrapper.emitted('confirm')).toBeUndefined()
  })

  it('confirms on Enter only for the exact text', async () => {
    const { wrapper, body } = await mountDialog()
    const input = body.get('[data-testid="typed-confirm-input"]')
    await input.setValue('Unanswered inquiry')
    await input.trigger('keydown', { key: 'Enter', code: 'Enter' })
    await flushPromises()

    expect(wrapper.emitted('confirm')).toHaveLength(1)
    expect(body.get('[data-testid="typed-confirm-submit"]').attributes('disabled')).toBeUndefined()
  })

  it('resets the typed text when the dialog is reopened', async () => {
    const { wrapper, body } = await mountDialog()
    await body.get('[data-testid="typed-confirm-input"]').setValue('Unanswered inquiry')
    await wrapper.setProps({ visible: false })
    await wrapper.setProps({ visible: true })

    expect((body.get('[data-testid="typed-confirm-input"]').element as HTMLInputElement).value).toBe('')
    expect(body.get('[data-testid="typed-confirm-submit"]').attributes('disabled')).toBeDefined()
  })
})
