// TanStack Table's documented extension point: `ColumnMeta` is an
// intentionally empty interface in @tanstack/table-core, meant to be
// augmented by the consuming app via declaration merging (works the same
// whether columns are authored against '@tanstack/table-core' directly or
// through the '@tanstack/vue-table' re-export DataTable.vue uses).
import '@tanstack/table-core'

declare module '@tanstack/table-core' {
  // Both type parameters are required so this declaration matches the
  // upstream interface exactly (declaration merging), even though this
  // particular augmentation doesn't need either of them.
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  interface ColumnMeta<TData extends RowData, TValue> {
    /** Right-align numeric columns per UI_STYLE.md §4 ("tabular-nums, right-aligned when numeric"). */
    align?: 'left' | 'right'
  }
}
