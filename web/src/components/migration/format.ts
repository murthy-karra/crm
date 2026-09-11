const GIB = 1_073_741_824n
const MAX_BYTES = 9_223_372_036_854_775_807n
export function gibToBytes(input: string): string | null {
  if (!/^\d{1,10}(\.\d{1,30})?$/.test(input.trim())) return null
  const [whole, fraction = ''] = input.trim().split('.')
  const divisor = 10n ** BigInt(fraction.length)
  const numerator = (BigInt(whole) * divisor + BigInt(fraction || '0')) * GIB
  if (numerator % divisor !== 0n) return null
  const bytes = numerator / divisor
  return bytes > 0n && bytes <= MAX_BYTES ? bytes.toString() : null
}
export function bytesToGiB(bytes: string): string {
  const n = BigInt(bytes)
  const fraction = ((n % GIB) * 10n ** 30n / GIB).toString().padStart(30, '0').replace(/0+$/, '')
  return `${n / GIB}${fraction ? `.${fraction}` : ''}`
}
export function formatBytes(bytes: string): string {
  const n = BigInt(bytes)
  for (const [size, label] of [[GIB, 'GiB'], [1_048_576n, 'MiB'], [1024n, 'KiB']] as const) {
    if (n >= size) return `${n / size}.${(n % size * 100n / size).toString().padStart(2, '0')} ${label}`
  }
  return `${bytes} bytes`
}
export function snapshotLabel(value: string): string {
  const names: Record<string, string> = {
    people: 'People', users: 'Users', stages: 'Stages', custom_fields: 'Custom fields', notes: 'Notes', tasks: 'Tasks',
    tasks_open: 'Open tasks', tasks_completed: 'Completed tasks', note_detail: 'Note details', identity: 'Source identity',
    needs_decision: 'Needs a decision', unsupported_value: 'Unsupported value', unresolved_reference: 'Unresolved reference',
    reviewable: 'Reviewable', complete_for_selected_query: 'Complete for selected query',
    completed_with_gaps: 'Completed with gaps', waiting_retry: 'Waiting to retry',
  }
  return names[value] ?? (value.charAt(0).toUpperCase() + value.slice(1).replaceAll('_', ' '))
}
export function snapshotTime(value: string | null): string {
  return value ? new Date(value).toLocaleString() : 'Not yet observed'
}
export function sourceActive(state?: string): boolean { return ['queued', 'running', 'waiting_retry'].includes(state ?? '') }
