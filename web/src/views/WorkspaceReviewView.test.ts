import { flushPromises, mount } from '@vue/test-utils'
import { afterEach, expect, it, vi } from 'vitest'
import WorkspaceReviewView from './WorkspaceReviewView.vue'
import { refreshWorkspace } from '../workspaceLifecycle'
vi.mock('../workspaceLifecycle', () => ({ refreshWorkspace: vi.fn() }))
afterEach(() => vi.clearAllMocks())
it('offers status recovery without exposing business controls or an activation action', async () => {
  vi.mocked(refreshWorkspace).mockRejectedValueOnce(new Error('offline')).mockResolvedValueOnce()
  const wrapper = mount(WorkspaceReviewView)
  expect(wrapper.text()).toContain('Workspace under review')
  expect(wrapper.findAll('button')).toHaveLength(1)
  await wrapper.get('[data-testid="workspace-status-refresh"]').trigger('click'); await flushPromises()
  expect(wrapper.get('[role="alert"]').text()).toContain('Could not verify workspace status')
  await wrapper.get('[data-testid="workspace-status-refresh"]').trigger('click'); await flushPromises()
  expect(refreshWorkspace).toHaveBeenCalledTimes(2)
  expect(wrapper.find('[role="alert"]').exists()).toBe(false)
  wrapper.unmount()
})
