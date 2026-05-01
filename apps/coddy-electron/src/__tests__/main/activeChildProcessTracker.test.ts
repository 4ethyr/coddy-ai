import { EventEmitter } from 'events'
import type { ChildProcess } from 'child_process'
import { describe, expect, it, vi } from 'vitest'
import { ActiveChildProcessTracker } from '../../main/activeChildProcessTracker'

function fakeChild(): ChildProcess {
  const child = new EventEmitter() as ChildProcess
  Object.assign(child, {
    killed: false,
    kill: vi.fn(() => true),
  })
  return child
}

describe('ActiveChildProcessTracker', () => {
  it('tracks and clears the active child when the operation completes', async () => {
    const tracker = new ActiveChildProcessTracker(vi.fn())
    const child = fakeChild()

    await expect(
      tracker.track(child, async () => {
        expect(tracker.current).toBe(child)
        return 'done'
      }),
    ).resolves.toBe('done')

    expect(tracker.current).toBeNull()
  })

  it('terminates the active child on request', async () => {
    const terminate = vi.fn()
    const tracker = new ActiveChildProcessTracker(terminate)
    const child = fakeChild()
    const deferredOperation = deferred()
    const operation = tracker.track(child, () => deferredOperation.promise)

    expect(tracker.terminateActive()).toBe(true)
    expect(tracker.wasTerminated(child)).toBe(true)
    expect(tracker.terminateActive()).toBe(false)
    expect(terminate).toHaveBeenCalledWith(child)
    expect(terminate).toHaveBeenCalledTimes(1)

    deferredOperation.resolve()
    await operation
  })

  it('does not let an older child clear a newer active child', async () => {
    const tracker = new ActiveChildProcessTracker(vi.fn())
    const first = fakeChild()
    const second = fakeChild()
    const firstDeferred = deferred()
    const secondDeferred = deferred()
    const firstOperation = tracker.track(first, () => firstDeferred.promise)

    const secondOperation = tracker.track(second, () => secondDeferred.promise)
    expect(tracker.current).toBe(second)

    firstDeferred.resolve()
    await firstOperation

    expect(tracker.current).toBe(second)

    secondDeferred.resolve()
    await secondOperation
    expect(tracker.current).toBeNull()
  })
})

function deferred(): { promise: Promise<void>; resolve: () => void } {
  let resolve!: () => void
  const promise = new Promise<void>((resolvePromise) => {
    resolve = resolvePromise
  })
  return { promise, resolve }
}
