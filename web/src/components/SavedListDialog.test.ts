import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import { ApiError } from '../api/client'
import SavedListDialog from './SavedListDialog.vue'

function dialog(error?: unknown, sortSummary?: string) {
  return mount(SavedListDialog, {
    props: {
      visible: true,
      title: 'Save as list',
      submitLabel: 'Save list',
      initialName: 'A list',
      allowShared: true,
      isPending: false,
      error,
      sortSummary,
    },
    global: {
      stubs: {
        Dialog: { template: '<section><slot name="header" /><slot /><slot name="footer" /></section>' },
      },
    },
  })
}

describe('SavedListDialog quota recovery', () => {
  it('explains the applicable personal or shared quota and a useful next action', async () => {
    const personal = dialog(new ApiError(409, 'saved_list_limit_reached'))
    expect(personal.text()).toContain('50 personal lists')
    expect(personal.text()).toContain('Shared lists')

    const shared = dialog()
    await shared.get('input[value="shared"]').setValue()
    await shared.setProps({ error: new ApiError(409, 'saved_list_limit_reached') })
    expect(shared.text()).toContain('200 shared lists')
    expect(shared.text()).toContain('Only me')
  })
})

// docs/specs/SLICE_011b_SORT.md §9: "one summary line ... omitted for the
// default order".
describe('SavedListDialog sort summary', () => {
  it('renders the summary line only when one is passed', () => {
    const withoutSort = dialog()
    expect(withoutSort.find('[data-testid="saved-list-sort-summary"]').exists()).toBe(false)

    const withSort = dialog(undefined, 'Sorted by Name (A–Z)')
    expect(withSort.get('[data-testid="saved-list-sort-summary"]').text()).toBe('Sorted by Name (A–Z)')
  })
})
