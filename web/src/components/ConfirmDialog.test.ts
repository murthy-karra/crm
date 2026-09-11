import { DOMWrapper, flushPromises, mount } from '@vue/test-utils'
import PrimeVue from 'primevue/config'
import { afterEach, describe, expect, it } from 'vitest'
import ConfirmDialog from './ConfirmDialog.vue'

const cleanup: Array<() => void> = []

afterEach(() => {
  cleanup.splice(0).forEach((unmount) => unmount())
  document.body.innerHTML = ''
})

describe('ConfirmDialog accessible title', () => {
  it.each([
    'Confirm People import',
    'Increase snapshot allowances',
    'Cancel remaining import',
  ])('names the real PrimeVue dialog %s through its visible heading', async (title) => {
    const wrapper = mount(ConfirmDialog, {
      props: { visible: true, title, message: 'Review this action.', confirmLabel: 'Confirm', isPending: false },
      global: { plugins: [[PrimeVue, { unstyled: true }]] },
      attachTo: document.body,
    })
    cleanup.push(() => wrapper.unmount())
    await flushPromises()

    const dialog = new DOMWrapper(document.body).get('[role="dialog"]')
    const headingId = dialog.attributes('aria-labelledby')
    expect(headingId).toBeTruthy()
    const heading = document.getElementById(headingId!)
    expect(heading?.tagName).toBe('H2')
    expect(heading?.textContent?.trim()).toBe(title)
    expect(dialog.element.contains(heading)).toBe(true)
    expect(wrapper.emitted('confirm')).toBeUndefined()
  })
})
