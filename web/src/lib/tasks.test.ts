import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { TASK_KIND_LABEL, clipTitle, groupTasks, panelDueCellText, tomorrowLocalEndOfDay } from './tasks'

describe('TASK_KIND_LABEL', () => {
  it('covers all five closed kinds', () => {
    expect(TASK_KIND_LABEL).toEqual({
      follow_up: 'Follow up',
      call: 'Call',
      email: 'Email',
      text: 'Text',
      other: 'Other',
    })
  })
})

describe('clipTitle', () => {
  it('returns a short title unchanged', () => {
    expect(clipTitle('Call the client')).toBe('Call the client')
  })

  it('clips a long title to the default 40 code points with an ellipsis', () => {
    const long = 'a'.repeat(60)
    const clipped = clipTitle(long)
    expect(Array.from(clipped.replace('…', '')).length).toBe(40)
    expect(clipped.endsWith('…')).toBe(true)
  })

  it('counts Unicode code points, not UTF-16 units', () => {
    const emojiTitle = '\u{1F600}'.repeat(50)
    const clipped = clipTitle(emojiTitle, 10)
    expect(Array.from(clipped.replace('…', '')).length).toBe(10)
  })

  it('respects an explicit maxChars', () => {
    expect(clipTitle('Follow up with the seller', 10)).toBe('Follow up …')
  })
})

// Fixed America/New_York clock (the PersonDetailView.test.ts §12.8
// precedent) — `process` is a real Node global at Vitest runtime, not
// browser-typed by this app's tsconfig, hence the cast. A mid-July instant
// stays clear of any DST transition, unlike the Person-page date-only
// boundary tests, which deliberately target the spring/fall edges.
const nodeProcess = (globalThis as unknown as { process: { env: Record<string, string | undefined> } }).process

describe('tomorrowLocalEndOfDay', () => {
  let originalTz: string | undefined
  beforeEach(() => {
    originalTz = nodeProcess.env.TZ
    nodeProcess.env.TZ = 'America/New_York'
    vi.useFakeTimers()
    vi.setSystemTime(new Date('2026-07-15T14:30:00.000Z')) // 10:30 EDT
  })
  afterEach(() => {
    vi.useRealTimers()
    nodeProcess.env.TZ = originalTz
  })

  it('returns tomorrow at 23:59:59 local time as the equivalent UTC instant', () => {
    expect(tomorrowLocalEndOfDay()).toBe('2026-07-17T03:59:59.000Z')
  })
})

// Round-2 review fix 3: `groupTasks` compares with `Date.parse`, never a
// raw ISO-string `<` — a whole-second `due_at` sorts AFTER a
// fractional-second `generated_at` lexically ('.' is 0x2E, 'Z' is 0x5A),
// which would misclassify a task due at exactly `generated_at` (minus a
// fractional remainder) as "Due soon" instead of "Overdue".
describe('groupTasks', () => {
  function taskDueAt(due_at: string | null) {
    return { due_at }
  }

  it('a task due exactly at generated_at is Due soon (not strictly before)', () => {
    const t = taskDueAt('2026-09-09T12:00:00.000Z')
    const { overdue, dueSoon } = groupTasks([t], '2026-09-09T12:00:00.000Z')
    expect(overdue).toEqual([])
    expect(dueSoon).toEqual([t])
  })

  it('a task due one second before generated_at is Overdue', () => {
    const t = taskDueAt('2026-09-09T11:59:59.000Z')
    const { overdue, dueSoon } = groupTasks([t], '2026-09-09T12:00:00.000Z')
    expect(overdue).toEqual([t])
    expect(dueSoon).toEqual([])
  })

  it('a whole-second due_at is Overdue against a fractional-second generated_at, despite sorting "after" it as a raw string', () => {
    const dueAt = '2026-09-09T12:00:00Z'
    const generatedAt = '2026-09-09T12:00:00.500000Z'
    // The raw-string comparison this fix replaces would have been wrong:
    // '.' (0x2E) sorts below 'Z' (0x5A), so the shorter due_at string is
    // lexically GREATER than the fractional generated_at string, even
    // though the instant it names is chronologically earlier.
    expect(dueAt > generatedAt).toBe(true)
    const t = taskDueAt(dueAt)
    const { overdue, dueSoon } = groupTasks([t], generatedAt)
    expect(overdue).toEqual([t])
    expect(dueSoon).toEqual([])
  })

  it('a null due_at is never Overdue', () => {
    const t = taskDueAt(null)
    const { overdue, dueSoon } = groupTasks([t], '2026-09-09T12:00:00.000Z')
    expect(overdue).toEqual([])
    expect(dueSoon).toEqual([t])
  })
})

describe('panelDueCellText', () => {
  let originalTz: string | undefined
  beforeEach(() => {
    originalTz = nodeProcess.env.TZ
    nodeProcess.env.TZ = 'America/New_York'
  })
  afterEach(() => {
    nodeProcess.env.TZ = originalTz
  })

  it('shows a time for a row due the same local calendar day as the reference', () => {
    const reference = '2026-07-15T14:00:00.000Z' // 10:00 EDT
    const dueAt = '2026-07-15T22:30:00.000Z' // 18:30 EDT, same local day
    expect(panelDueCellText(dueAt, reference)).toBe('6:30 PM')
  })

  it('shows a date for a row past local midnight relative to the reference', () => {
    const reference = '2026-07-15T14:00:00.000Z'
    const dueAt = '2026-07-21T18:30:00.000Z'
    expect(panelDueCellText(dueAt, reference)).toBe('Jul 21, 2026')
  })
})
