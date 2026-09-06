import type { InjectionKey } from 'vue'

// Internal UI coordination only. Opening a Person's Operator first navigates
// to their full profile, preserving the existing route-derived wire context.
export const OPERATOR_LAUNCHER: InjectionKey<(personId?: string) => void> = Symbol('operator-launcher')
