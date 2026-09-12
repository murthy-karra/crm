import { beforeEach, describe, expect, it, vi } from 'vitest'
import { apiFetch } from './client'
import { activityReviewCoreKey, fetchActivityReviewCore, fetchReviewNote, fetchReviewNotes, fetchReviewTasks } from './activityReview'
vi.mock('./client', async original => ({ ...await original<typeof import('./client')>(), apiFetch: vi.fn() }))
const api = vi.mocked(apiFetch)
beforeEach(() => api.mockReset())
describe('bounded native review transport', () => {
  it('uses distinct bounded no-store reads and never follows response URLs', async () => {
    const signal = new AbortController().signal
    await fetchActivityReviewCore('person/one', signal)
    await fetchReviewNotes('person', 'cursor +', signal)
    await fetchReviewTasks('person', 'open', undefined, signal)
    await fetchReviewTasks('person', 'completed', 'later', signal)
    await fetchReviewNote('person', 'note/id', signal)
    expect(api.mock.calls.map(([url]) => url)).toEqual([
      '/people/person%2Fone/migration-review', '/people/person/migration-review/notes?limit=50&cursor=cursor+%2B',
      '/people/person/migration-review/tasks?limit=50&state=open', '/people/person/migration-review/tasks?limit=50&state=completed&cursor=later',
      '/people/person/migration-review/notes/note%2Fid',
    ])
    expect(api.mock.calls.every(([, init]) => init?.signal === signal && init.cache === 'no-store' && !init.method)).toBe(true)
  })
  it('keeps core separate from complete Person cache and scoped to identity and workspace', () => {
    const prefix = ['org', 'org', 'activity-review', 'actor', 4, '9007199254740993', 'admin']
    expect(activityReviewCoreKey(prefix, 'person')).toEqual([...prefix, 'person', 'person', 'core'])
    expect(activityReviewCoreKey(prefix, 'person')).not.toEqual(['org', 'org', 'person', 'person'])
    for (const index of [1, 3, 4, 5, 6]) {
      const changed = [...prefix]; changed[index] = 'changed'
      expect(activityReviewCoreKey(changed, 'person')).not.toEqual(activityReviewCoreKey(prefix, 'person'))
    }
  })
})
