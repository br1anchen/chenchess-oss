import { installAstryxTestShims } from "@chenchess/ui/vitest/astryx"

// Bun exposes experimental `localStorage` and `sessionStorage` globals that
// stay `undefined` unless the runtime is started with `--localstorage-file`.
// Those getters shadow the jsdom-backed storage the browser journeys rely on,
// so the suite would otherwise depend on the runtime version the maintainer
// happens to have installed. Install a spec-shaped in-memory Storage only when
// the environment left one missing.

function createMemoryStorage(): Storage {
  const entries = new Map<string, string>()
  // SAFETY: DOM Storage also has a string index signature this in-memory Map does not need.
  return {
    get length() {
      return entries.size
    },
    clear() {
      entries.clear()
    },
    getItem(key: string) {
      return entries.get(String(key)) ?? null
    },
    key(index: number) {
      return [...entries.keys()][index] ?? null
    },
    removeItem(key: string) {
      entries.delete(String(key))
    },
    setItem(key: string, value: string) {
      entries.set(String(key), String(value))
    },
  } as Storage
}

installAstryxTestShims()

for (const name of ["localStorage", "sessionStorage"] as const) {
  if (Object.getOwnPropertyDescriptor(globalThis, name) !== undefined) continue
  const storage = createMemoryStorage()
  Object.defineProperty(globalThis, name, {
    configurable: true,
    get: () => storage,
  })
}
