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

  it('Show me opens a wide walkthrough dialog with the video, poster and described steps', async () => {
    const { wrapper } = await mountAt('/people?guide=create-list')
    await wrapper.get('button:not([aria-label])').trigger('click')
    await flushPromises()
    const dialog = document.body.querySelector('[role="dialog"]')
    expect(dialog).not.toBeNull()
    expect(dialog!.className).toContain('max-w-[min(96vw,1240px,calc((100vh-14rem)*1.6))]')
    const video = dialog!.querySelector('video') as HTMLVideoElement
    expect(video.getAttribute('src')).toBe('/guides/create-list.mp4')
    expect(video.getAttribute('poster')).toBe('/guides/create-list-poster.png')
    expect(video.hasAttribute('controls')).toBe(true)
    expect(video.hasAttribute('autoplay')).toBe(true)
    expect(video.getAttribute('aria-label')).toMatch(/Recorded walkthrough/)
    expect(dialog!.querySelectorAll('li')).toHaveLength(3)
  })

  it('does not autoplay when reduced motion is preferred, leaving the native controls', async () => {
    const { wrapper } = await mountAt('/people?guide=create-list', true)
    await wrapper.get('button:not([aria-label])').trigger('click')
    await flushPromises()
    const video = document.body.querySelector('[role="dialog"] video') as HTMLVideoElement
    expect(video.hasAttribute('autoplay')).toBe(false)
    expect(video.hasAttribute('controls')).toBe(true)
    expect(video.getAttribute('poster')).toBe('/guides/create-list-poster.png')
  })
})
