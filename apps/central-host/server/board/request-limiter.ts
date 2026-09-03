type RequestWindow = {
  count: number
  startedAt: number
}

export class CoachAppRequestLimiter {
  readonly #exclusiveTails = new Map<string, Promise<void>>()
  readonly #windows = new Map<string, RequestWindow>()

  constructor(
    readonly maximumRequests: number,
    readonly windowMilliseconds = 60_000,
    readonly now: () => number = Date.now,
  ) {}

  admit(principal: string, category: string): number | undefined {
    const retryAfterSeconds = this.retryAfter(principal, category)
    if (retryAfterSeconds !== undefined) return retryAfterSeconds
    this.charge(principal, category, 1)
    return undefined
  }

  retryAfter(principal: string, category: string): number | undefined {
    const now = this.now()
    const key = `${category}\u0000${principal}`
    const current = this.#windows.get(key)
    if (!current || now - current.startedAt >= this.windowMilliseconds)
      return undefined
    return current.count >= this.maximumRequests
      ? Math.max(
          1,
          Math.ceil(
            (current.startedAt + this.windowMilliseconds - now) / 1_000,
          ),
        )
      : undefined
  }

  charge(principal: string, category: string, units: number) {
    if (units === 0) return
    const now = this.now()
    const key = `${category}\u0000${principal}`
    const current = this.#windows.get(key)
    if (!current || now - current.startedAt >= this.windowMilliseconds) {
      this.#windows.set(key, { count: units, startedAt: now })
      this.compact(now)
      return
    }
    current.count += units
  }

  async runExclusive<T>(
    principal: string,
    category: string,
    operation: () => Promise<T>,
  ): Promise<T> {
    const key = `${category}\u0000${principal}`
    const previous = this.#exclusiveTails.get(key) ?? Promise.resolve()
    let release!: () => void
    const gate = new Promise<void>((resolve) => {
      release = resolve
    })
    const tail = previous.then(() => gate)
    this.#exclusiveTails.set(key, tail)
    await previous
    try {
      return await operation()
    } finally {
      release()
      if (this.#exclusiveTails.get(key) === tail) {
        this.#exclusiveTails.delete(key)
      }
    }
  }

  private compact(now: number) {
    if (this.#windows.size < 10_000) return
    for (const [key, window] of this.#windows) {
      if (now - window.startedAt >= this.windowMilliseconds) {
        this.#windows.delete(key)
      }
    }
  }
}
