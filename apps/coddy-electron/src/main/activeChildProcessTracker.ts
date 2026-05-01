import type { ChildProcess } from 'child_process'

export type ChildProcessTerminator = (child: ChildProcess) => void

export class ActiveChildProcessTracker {
  #current: ChildProcess | null = null
  #terminated = new WeakSet<ChildProcess>()

  constructor(private readonly terminate: ChildProcessTerminator) {}

  get current(): ChildProcess | null {
    return this.#current
  }

  async track<T>(
    child: ChildProcess,
    operation: (child: ChildProcess) => Promise<T>,
  ): Promise<T> {
    this.#current = child

    try {
      return await operation(child)
    } finally {
      if (this.#current === child) {
        this.#current = null
      }
    }
  }

  terminateActive(): boolean {
    const child = this.#current
    if (!child || child.killed) return false
    if (this.#terminated.has(child)) return false

    this.#terminated.add(child)
    this.terminate(child)
    return true
  }

  wasTerminated(child: ChildProcess): boolean {
    return this.#terminated.has(child)
  }

  clear(): void {
    this.#current = null
  }
}
