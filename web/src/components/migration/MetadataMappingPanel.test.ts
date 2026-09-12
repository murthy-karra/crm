import { DOMWrapper, flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import PrimeVue from 'primevue/config'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { apiFetch } from '../../api/client'
import { queryKeys } from '../../api/queries'
import type { MeResponse } from '../../api/types'
import type { MetadataKind, MetadataMapping, MetadataPage, MetadataPatch, MetadataSummary, MetadataTarget } from '../../api/metadataImports'
import MetadataMappingPanel from './MetadataMappingPanel.vue'

vi.mock('../../api/client', async original => ({
  ...await original<typeof import('../../api/client')>(),
  apiFetch: vi.fn(),
}))

const fetch = vi.mocked(apiFetch)
const cleanup: Array<() => void> = []
const root = '/migrations/fub/metadata-imports/child'

function identity(): MeResponse {
  return {
    user: { id: 'admin', email: 'admin@synthetic.test', display_name: 'Admin' },
    organization: { id: 'org', name: 'Org', role: 'admin', workspace_mode: 'migration_review', workspace_revision: '9007199254740993' },
    platform_admin: false,
  }
}

function summary(label: string, sourceType?: string, nameKey: 'label' | 'tag' | 'choice' = 'label'): MetadataSummary {
  const values = [[nameKey, label], ...(sourceType ? [['type', sourceType]] : [])]
  return {
    fields: values.map(([key, value]) => ({
      key: `source.${key}`, label: key!, label_abbreviated: false,
      label_full_utf8_bytes: String(key!.length), text: JSON.stringify(value),
      full_utf8_bytes: String(new TextEncoder().encode(JSON.stringify(value)).length), abbreviated: false,
    })),
    total_fields: String(values.length), abbreviated: false, field_key: 'source.all',
  }
}

function target(id: string, kind: MetadataKind = 'tag', fieldType: string | null = null): MetadataTarget {
  return { id, kind, field_id: kind === 'option' ? 'existing-field' : null, label: `Native ${id}`, field_type: fieldType, source_bound: false, archived: false }
}

function row(id: string, kind: MetadataKind = 'tag', overrides: Partial<MetadataMapping> = {}): MetadataMapping {
  return {
    id, kind, parent_mapping_id: kind === 'option' ? 'parent-field' : null,
    source_id: kind === 'tag' ? null : '900719925474099312345678901234567890',
    disposition: 'held', qualified: true, create_matching_available: true, choice: { kind: 'hold' }, target_id: null, field_id: null,
    reasons: ['mapping_choice_required'], suggestions: [target('suggested', kind, kind === 'field' ? 'text' : null)],
    dependent_count: '9007199254740993', source: summary(`Source ${id}`, kind === 'field' ? 'text' : undefined, kind === 'tag' ? 'tag' : kind === 'option' ? 'choice' : 'label'),
    added_byte_bound: '4096', alias_count: '0', target: null, ...overrides,
  }
}

function page(items: MetadataMapping[], next: string | null = null): MetadataPage<MetadataMapping> {
  return { items, next_cursor: next }
}

function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>(yes => { resolve = yes })
  return { promise, resolve }
}

async function setup(handle?: (url: string) => unknown) {
  const session = { current: identity() }
  fetch.mockImplementation(async url => {
    const value = handle?.(url)
    if (value !== undefined) return await value as never
    if (url === '/me') return session.current as never
    if (url.includes('/mappings?')) {
      const kind = new URL(url, 'https://synthetic.invalid').searchParams.get('kind') as MetadataKind
      return page([row(`${kind}-1`, kind)]) as never
    }
    if (url.includes('/targets?')) return { items: [], next_cursor: null } as never
    if (url.includes('/fields/')) {
      const text = '<script>exact retained source</script>'
      return { text, full_utf8_bytes: String(text.length), offset_bytes: '0', next_cursor: null, complete: true } as never
    }
    throw new Error(`Unexpected test request: ${url}`)
  })
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const wrapper = mount(MetadataMappingPanel, {
    props: { importId: 'child', planId: 'plan', revision: '1', canReplan: true },
    global: { plugins: [[VueQueryPlugin, { queryClient: client }], [PrimeVue, { unstyled: true }]] },
    attachTo: document.body,
  })
  cleanup.push(() => { wrapper.unmount(); client.clear() })
  await flushPromises()
  return { wrapper, client, session }
}

function button(wrapper: VueWrapper, text: string) {
  const found = wrapper.findAll('button').find(candidate => candidate.text() === text)
  expect(found, `Button ${text}`).toBeDefined()
  return found!
}

function dialogButton(text: string) {
  const found = Array.from(document.querySelectorAll<HTMLButtonElement>('[role="dialog"] button'))
    .find(candidate => candidate.textContent?.trim() === text)
  expect(found, `Dialog button ${text}`).toBeDefined()
  return new DOMWrapper(found!)
}

function useTarget(label: string) {
  const found = Array.from(document.querySelectorAll<HTMLButtonElement>('[role="dialog"] button'))
    .find(candidate => candidate.getAttribute('aria-label') === `Use ${label}`)
  expect(found, `Use target ${label}`).toBeDefined()
  return new DOMWrapper(found!)
}

beforeEach(() => { fetch.mockReset() })
afterEach(() => {
  cleanup.splice(0).forEach(dispose => dispose())
  document.body.innerHTML = ''
})

describe('MetadataMappingPanel', () => {
  it('keeps existing mapping available when valid source evidence cannot create a native field', async () => {
    const { wrapper } = await setup(url => {
      if (url.includes('/mappings?kind=field')) return page([row('oversized-key', 'field', {
        qualified: true, create_matching_available: false,
        source: summary('Oversized source key', 'text'), reasons: ['native_source_key_too_large'],
      })])
      if (url.includes('/targets?kind=field')) return { items: [target('compatible-field', 'field', 'text')], next_cursor: null }
      return undefined
    })
    await button(wrapper, 'Fields').trigger('click')
    await flushPromises()
    expect(wrapper.get('select').element.value).toBe('hold')
    expect(wrapper.find('option[value="create_matching"]').exists()).toBe(false)
    expect(button(wrapper, 'Choose existing field').element.disabled).toBe(false)
    expect(wrapper.text()).not.toContain('Source evidence cannot authorize a write.')
    await button(wrapper, 'Choose existing field').trigger('click')
    await flushPromises()
    await useTarget('Native compatible-field').trigger('click')
    await flushPromises()
    expect(wrapper.get('select').element.value).toBe('map_existing:compatible-field')
    await button(wrapper, 'Apply 1 mapping changes').trigger('click')
    expect(wrapper.emitted('apply')![0]).toEqual([[
      { mapping_id: 'oversized-key', choice: { kind: 'map_existing', target_id: 'compatible-field' } },
    ]])
  })

  it('refreshes an initially empty mapping page when the same plan makes progress', async () => {
    let ready = false
    const { wrapper } = await setup(url => url.includes('/mappings?') ? page(ready ? [row('newly-prepared')] : []) : undefined)
    expect(wrapper.text()).toContain('No mappings in this page.')
    ready = true
    await wrapper.setProps({ progressVersion: 'catalog-ready' })
    await flushPromises()

    expect(wrapper.props('planId')).toBe('plan')
    expect(wrapper.props('revision')).toBe('1')
    expect(wrapper.text()).toContain('Source newly-prepared')
    expect(wrapper.text()).not.toContain('No mappings in this page.')
    expect(fetch.mock.calls.filter(([url]) => url === `${root}/plans/plan/mappings?kind=tag&limit=50`)).toHaveLength(2)
  })

  it('refreshes progress on the current kind and page without losing its mapping draft', async () => {
    const { wrapper } = await setup(url => url.includes('/mappings?kind=field')
      ? url.includes('cursor=next') ? page([row('field-page-two', 'field')])
        : page([row('field-page-one', 'field')], 'next')
      : undefined)
    await button(wrapper, 'Fields').trigger('click')
    await flushPromises()
    await button(wrapper, 'Next mappings').trigger('click')
    await flushPromises()
    await wrapper.get('select').setValue('create_matching')
    const requests = fetch.mock.calls.length
    await wrapper.setProps({ progressVersion: 'people-progressed' })
    await flushPromises()

    expect(fetch.mock.calls.slice(requests).map(([url]) => url)).toEqual([
      `${root}/plans/plan/mappings?kind=field&cursor=next&limit=50`,
    ])
    expect(button(wrapper, 'Fields').attributes('aria-pressed')).toBe('true')
    expect(button(wrapper, 'Previous mappings').element.disabled).toBe(false)
    expect(wrapper.get('select').element.value).toBe('create_matching')
    expect(wrapper.text()).toContain('Source field-page-two')
    expect(wrapper.emitted('dirty')).toEqual([[false], [true]])
    await button(wrapper, 'Apply 1 mapping changes').trigger('click')
    expect(wrapper.emitted('apply')![0]).toEqual([[{ mapping_id: 'field-page-two', choice: { kind: 'create_matching' } }]])
  })

  it('keeps explicit drafts across pages and kinds without choosing suggestions', async () => {
    const { wrapper } = await setup(url => {
      if (!url.includes('/mappings?kind=tag')) return undefined
      return url.includes('cursor=next') ? page([row('tag-2')]) : page([
        row('tag-1', 'tag', { source: summary('Untrusted <script>tag</script>', undefined, 'tag') }),
        row('inherited', 'tag', { choice: { kind: 'map_existing', target_id: 'prior' }, target: target('prior') }),
      ], 'next')
    })
    expect(wrapper.emitted('apply')).toBeUndefined()
    expect(wrapper.find('script').exists()).toBe(false)
    expect(wrapper.text()).toContain('Suggested: Native suggested')
    expect(wrapper.findAll('select')[0]!.element.value).toBe('hold')
    expect(wrapper.findAll('select')[1]!.element.value).toBe('map_existing:prior')
    expect(fetch.mock.calls.some(([url]) => url.includes('/targets?'))).toBe(false)

    await wrapper.findAll('select')[0]!.setValue('create_matching')
    await button(wrapper, 'Next mappings').trigger('click')
    await flushPromises()
    await wrapper.get('select').setValue('create_matching')
    await button(wrapper, 'Fields').trigger('click')
    await flushPromises()
    await wrapper.get('select').setValue('create_matching')
    await button(wrapper, 'Tags').trigger('click')
    await flushPromises()
    expect(wrapper.findAll('select')[0]!.element.value).toBe('create_matching')
    expect(wrapper.findAll('select')[1]!.element.value).toBe('map_existing:prior')
    await button(wrapper, 'Apply 3 mapping changes').trigger('click')

    expect(wrapper.emitted('apply')![0]).toEqual([[
      { mapping_id: 'field-1', choice: { kind: 'create_matching' } },
      { mapping_id: 'tag-1', choice: { kind: 'create_matching' } },
      { mapping_id: 'tag-2', choice: { kind: 'create_matching' } },
    ]])
    expect(wrapper.emitted('dirty')).toEqual([[false], [true]])
    expect(fetch.mock.calls.every(([, options]) => options?.method !== 'POST')).toBe(true)
  })

  it('caps drafts at 50 across pages and frees capacity when an inherited choice is restored', async () => {
    const { wrapper } = await setup(url => url.includes('/mappings?')
      ? url.includes('cursor=next') ? page([row('tag-51')])
        : page(Array.from({ length: 50 }, (_, index) => row(`tag-${String(index + 1).padStart(2, '0')}`)), 'next')
      : undefined)
    for (const select of wrapper.findAll('select')) await select.setValue('create_matching')
    await button(wrapper, 'Next mappings').trigger('click')
    await flushPromises()
    await wrapper.get('select').setValue('create_matching')
    expect(wrapper.text()).toContain('Apply these 50 changes before choosing more mappings.')
    expect(wrapper.get('select').element.value).toBe('hold')
    await button(wrapper, 'Previous mappings').trigger('click')
    await flushPromises()
    await wrapper.findAll('select')[0]!.setValue('hold')
    await button(wrapper, 'Next mappings').trigger('click')
    await flushPromises()
    await wrapper.get('select').setValue('create_matching')
    await button(wrapper, 'Apply 50 mapping changes').trigger('click')

    const patches = wrapper.emitted('apply')![0]![0] as MetadataPatch[]
    expect(patches).toHaveLength(50)
    expect(patches.some(patch => patch.mapping_id === 'tag-01')).toBe(false)
    expect(patches).toContainEqual({ mapping_id: 'tag-51', choice: { kind: 'create_matching' } })
    expect(wrapper.text()).not.toContain('Apply these 50 changes before choosing more mappings.')
  })

  it('paginates existing targets and requires an explicit compatible choice', async () => {
    const { wrapper } = await setup(url => {
      if (url.includes('/mappings?kind=field')) return page([row('choice-field', 'field', { source: summary('Preferred area', 'dropdown') })])
      if (url.includes('/targets?')) return url.includes('cursor=more')
        ? { items: [target('choice-second', 'field', 'choice')], next_cursor: null }
        : { items: [target('text-first', 'field', 'text'), target('choice-first', 'field', 'choice')], next_cursor: 'more +/' }
      return undefined
    })
    await button(wrapper, 'Fields').trigger('click')
    await flushPromises()
    await button(wrapper, 'Choose existing field').trigger('click')
    await flushPromises()
    expect(useTarget('Native text-first').element.disabled).toBe(true)
    expect(useTarget('Native choice-first').element.disabled).toBe(false)
    expect(wrapper.emitted('apply')).toBeUndefined()
    expect(wrapper.get('select').element.value).toBe('hold')
    await dialogButton('Next choices').trigger('click')
    await flushPromises()
    expect(fetch.mock.calls.some(([url]) => url === `${root}/plans/plan/targets?kind=field&cursor=more+%2B%2F&limit=50`)).toBe(true)
    await useTarget('Native choice-second').trigger('click')
    await flushPromises()
    expect(wrapper.get('select').element.value).toBe('map_existing:choice-second')
    expect(wrapper.text()).toContain('Use Native choice-second')
    expect(wrapper.emitted('apply')).toBeUndefined()
    await button(wrapper, 'Apply 1 mapping changes').trigger('click')
    expect(wrapper.emitted('apply')![0]).toEqual([[{ mapping_id: 'choice-field', choice: { kind: 'map_existing', target_id: 'choice-second' } }]])
  })

  it('waits for an applied parent field mapping before looking up its existing options', async () => {
    const { wrapper } = await setup(url => {
      if (url.includes('/mappings?kind=field')) return page([row('parent-field', 'field', { source: summary('Preferred area', 'dropdown') })])
      if (url.includes('/mappings?kind=option')) return page([row('none-option', 'option', {
        source: summary('None', undefined, 'choice'), field_id: url.includes('/plans/applied/') ? 'existing-field' : null,
      })])
      if (url.includes('/targets?kind=field')) return { items: [target('existing-field', 'field', 'choice')], next_cursor: null }
      if (url.includes('/targets?kind=option')) return { items: [target('literal-none', 'option')], next_cursor: null }
      return undefined
    })
    await button(wrapper, 'Fields').trigger('click')
    await flushPromises()
    await button(wrapper, 'Choose existing field').trigger('click')
    await flushPromises()
    await useTarget('Native existing-field').trigger('click')
    await flushPromises()
    await button(wrapper, 'Choice options').trigger('click')
    await flushPromises()
    expect(button(wrapper, 'Choose existing option').element.disabled).toBe(true)
    expect(wrapper.text()).toContain('apply the parent field mapping first')
    expect(fetch.mock.calls.some(([url]) => url.includes('/targets?kind=option'))).toBe(false)
    await button(wrapper, 'Apply 1 mapping changes').trigger('click')
    expect(wrapper.emitted('apply')![0]).toEqual([[{ mapping_id: 'parent-field', choice: { kind: 'map_existing', target_id: 'existing-field' } }]])

    await wrapper.setProps({ planId: 'applied', revision: '2' })
    await flushPromises()
    expect(button(wrapper, 'Choose existing option').element.disabled).toBe(false)
    expect(button(wrapper, 'Apply 0 mapping changes').element.disabled).toBe(true)
    await button(wrapper, 'Choose existing option').trigger('click')
    await flushPromises()
    expect(fetch.mock.calls.some(([url]) => url === `${root}/plans/applied/targets?kind=option&field_id=existing-field&limit=50`)).toBe(true)
    await useTarget('Native literal-none').trigger('click')
    await flushPromises()
    await button(wrapper, 'Apply 1 mapping changes').trigger('click')
    expect(wrapper.emitted('apply')![1]).toEqual([[{ mapping_id: 'none-option', choice: { kind: 'map_existing', target_id: 'literal-none' } }]])
  })

  it('inspects an alias through its exact source record and clears inspection when occurrences change', async () => {
    const sourceId = '9'.repeat(128)
    const { wrapper } = await setup(url => {
      if (url.includes('/mappings?')) return page([row('buyer-tag', 'tag', { alias_count: '2' })])
      if (url.includes('/aliases?')) return url.includes('cursor=next')
        ? { items: [{ source_id: '2', ordinal: '0', record_id: null, source: summary('Buyer', undefined, 'tag') }], next_cursor: null }
        : { items: [{ source_id: sourceId, ordinal: '17', record_id: 'exact-record', source: summary('  Buyer  ', undefined, 'tag') }], next_cursor: 'next' }
      return undefined
    })
    await button(wrapper, 'Review 2 tag occurrences').trigger('click')
    await flushPromises()
    expect(wrapper.text()).toContain(`FUB Person ${sourceId} · source position 17`)
    await button(wrapper, `Inspect exact source ${sourceId}`).trigger('click')
    await flushPromises()
    expect(fetch.mock.calls.some(([url]) => url === `${root}/plans/plan/records/exact-record/fields/source.all?limit=65536`)).toBe(true)
    expect(wrapper.get('pre').text()).toBe('<script>exact retained source</script>')
    expect(wrapper.find('script').exists()).toBe(false)
    await button(wrapper, 'Next occurrences').trigger('click')
    await flushPromises()
    expect(wrapper.find('pre').exists()).toBe(false)
    expect(wrapper.text()).not.toContain(sourceId)
    expect(wrapper.findAll('button').some(candidate => candidate.text() === 'Inspect exact source 2')).toBe(false)
    expect(fetch.mock.calls.some(([url]) => url === `${root}/plans/plan/mappings/buyer-tag/aliases?cursor=next&limit=50`)).toBe(true)
  })

  it('discards a late page from a superseded plan', async () => {
    const stale = deferred<MetadataPage<MetadataMapping>>()
    const { wrapper } = await setup(url => url.includes('/plans/plan/mappings?') ? stale.promise
      : url.includes('/plans/new/mappings?') ? page([row('current-plan')]) : undefined)
    await wrapper.setProps({ planId: 'new', revision: '2' })
    await flushPromises()
    stale.resolve(page([row('stale-plan')]))
    await flushPromises()
    expect(wrapper.text()).toContain('Source current-plan')
    expect(wrapper.text()).not.toContain('Source stale-plan')
    expect(wrapper.emitted('apply')).toBeUndefined()
  })

  it.each(['actor', 'organization', 'workspace revision', 'role'] as const)('fences late pages and clears drafts after the %s changes', async change => {
    const stale = deferred<MetadataPage<MetadataMapping>>()
    let changed = false
    const { wrapper, client, session } = await setup(url => {
      if (!url.includes('/mappings?')) return undefined
      if (url.includes('cursor=next')) return stale.promise
      return page([row(changed ? 'current-scope' : 'old-scope')], changed ? null : 'next')
    })
    await wrapper.get('select').setValue('create_matching')
    await button(wrapper, 'Next mappings').trigger('click')
    await flushPromises()
    const pendingSignal = fetch.mock.calls.find(([url]) => url.includes('cursor=next'))![1]!.signal!
    changed = true
    const next = identity()
    if (change === 'actor') next.user.id = 'other-admin'
    if (change === 'organization') next.organization!.id = 'other-org'
    if (change === 'workspace revision') next.organization!.workspace_revision = '9007199254740994'
    if (change === 'role') next.organization!.role = 'member'
    session.current = next
    client.setQueryData(queryKeys.me, next)
    await flushPromises()
    expect(pendingSignal.aborted).toBe(true)
    stale.resolve(page([row('late-private-data')]))
    await flushPromises()

    expect(wrapper.text()).not.toContain('Source old-scope')
    expect(wrapper.text()).not.toContain('Source late-private-data')
    expect(wrapper.emitted('dirty')?.at(-1)).toEqual([false])
    expect(wrapper.emitted('apply')).toBeUndefined()
    if (change === 'role') {
      expect(wrapper.find('table').exists()).toBe(false)
      expect(wrapper.text()).toContain('Mappings require a current administrator session.')
    } else {
      expect(wrapper.text()).toContain('Source current-scope')
      expect(button(wrapper, 'Apply 0 mapping changes').element.disabled).toBe(true)
    }
  })

  it('closes the picker and discards its late targets when the administrator loses access', async () => {
    const stale = deferred<{ items: MetadataTarget[]; next_cursor: null }>()
    const { wrapper, client, session } = await setup(url => url.includes('/targets?') ? stale.promise : undefined)
    await button(wrapper, 'Choose existing tag').trigger('click')
    await flushPromises()
    expect(document.querySelector('[role="dialog"]')).not.toBeNull()
    session.current = { ...identity(), organization: { ...identity().organization!, role: 'member' } }
    client.setQueryData(queryKeys.me, session.current)
    await flushPromises()
    stale.resolve({ items: [target('late-private-target')], next_cursor: null })
    await flushPromises()
    expect(document.querySelector('[role="dialog"]')).toBeNull()
    expect(document.body.textContent).not.toContain('Native late-private-target')
    expect(wrapper.emitted('apply')).toBeUndefined()
  })
})
