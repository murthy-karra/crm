import { beforeEach } from 'vitest'

// Node's happy-dom intentionally has no persistent storage unless Vitest is
// launched with a localstorage file. Browser session-lifecycle tests need the
// normal readable-storage path by default; individual tests replace this
// shim when exercising the unavailable-storage privacy boundary.
const values = new Map<string, string>()

Object.defineProperty(window, 'localStorage', {
  configurable: true,
  value: {
    get length() { return values.size },
    key: (index: number) => [...values.keys()][index] ?? null,
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => { values.set(key, value) },
    removeItem: (key: string) => { values.delete(key) },
    clear: () => { values.clear() },
  },
})

beforeEach(() => values.clear())
