import { flushPromises, mount } from '@vue/test-utils'
import PrimeVue from 'primevue/config'
import { createMemoryHistory, createRouter } from 'vue-router'
import { afterEach, describe, expect, it, vi } from 'vitest'
import CreateListGuide from './CreateListGuide.vue'

function makeRouter() {
  return createRouter({
    history: createMemoryHistory(),
    routes: [{ path: '/people', component: { template: '<p>People</p>' } }],
  })
}

async function mountAt(path: string, reducedMotion = false) {
  vi.stubGlobal('matchMedia', vi.fn().mockImplementation((query: string) => ({
    matches: reducedMotion && query.includes('reduce'),
    media: query,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
  })))
  const router = makeRouter()
  await router.push(path)
  await router.isReady()
  const wrapper = mount(CreateListGuide, {
    attachTo: document.body,
    global: { plugins: [router, [PrimeVue, {}]], stubs: { teleport: true } },
  })
  await flushPromises()
  return { wrapper, router }
}

afterEach(() => {
  vi.unstubAllGlobals()
  document.body.innerHTML = ''
})

describe('CreateListGuide', () => {
  it('stays hidden on an ordinary People visit', async () => {
    const { wrapper } = await mountAt('/people')
    expect(wrapper.find('[data-testid="create-list-guide"]').exists()).toBe(false)
  })

  it('shows the three steps when the URL carries the create-list guide flag', async () => {
    const { wrapper } = await mountAt('/people?guide=create-list')
    const guide = wrapper.find('[data-testid="create-list-guide"]')
    expect(guide.exists()).toBe(true)
    expect(guide.text()).toContain('Creating a list')
    expect(guide.findAll('li')).toHaveLength(3)
    expect(guide.text()).toContain('Save as list')
    expect(guide.text()).toContain('Today source')
  })

  it('dismiss removes only the guide flag and keeps the rest of the query', async () => {
    const { wrapper, router } = await mountAt('/people?guide=create-list&filter=%7B%22version%22%3A1%7D')
    await wrapper.get('button[aria-label="Dismiss the creating-a-list guide"]').trigger('click')
    await flushPromises()
    expect(router.currentRoute.value.query.guide).toBeUndefined()
    expect(router.currentRoute.value.query.filter).toBe('{"version":1}')
    expect(wrapper.find('[data-testid="create-list-guide"]').exists()).toBe(false)
  })

  it('Show me opens the walkthrough with the animated image and described steps', async () => {
    const { wrapper } = await mountAt('/people?guide=create-list')
    await wrapper.get('button:not([aria-label])').trigger('click')
    await flushPromises()
    const dialog = document.body.querySelector('[role="dialog"]')
    expect(dialog).not.toBeNull()
    const img = dialog!.querySelector('img') as HTMLImageElement
    expect(img.getAttribute('src')).toBe('/guides/create-list.gif')
    expect(img.getAttribute('alt')).toMatch(/Recorded walkthrough/)
    expect(dialog!.querySelectorAll('li')).toHaveLength(3)
  })

  it('offers the poster and an explicit Play choice when reduced motion is preferred', async () => {
    const { wrapper } = await mountAt('/people?guide=create-list', true)
    await wrapper.get('button:not([aria-label])').trigger('click')
    await flushPromises()
    const dialog = document.body.querySelector('[role="dialog"]')!
    expect((dialog.querySelector('img') as HTMLImageElement).getAttribute('src')).toBe('/guides/create-list-poster.png')
    const play = Array.from(dialog.querySelectorAll('button')).find((b) => b.textContent?.includes('Play the animation'))!
    play.click()
    await flushPromises()
    expect((dialog.querySelector('img') as HTMLImageElement).getAttribute('src')).toBe('/guides/create-list.gif')
  })
})
