import { EventEmitter } from 'events'
import { describe, expect, it, vi } from 'vitest'
import { readJson, readJsonWithTimeout } from '../../main/childProcessJson'
import type { ChildProcess } from 'child_process'

function fakeChild(): ChildProcess {
  const child = new EventEmitter() as ChildProcess
  Object.assign(child, {
    stdout: new EventEmitter(),
    stderr: new EventEmitter(),
    killed: false,
    kill: vi.fn(() => true),
  })
  return child
}

describe('childProcessJson', () => {
  it('parses a single JSON stdout payload without duplicating chunks', async () => {
    const child = fakeChild()
    const result = readJson(child)

    child.stdout?.emit('data', Buffer.from('{"text":"hello"}'))
    child.emit('close', 0)

    await expect(result).resolves.toEqual({ text: 'hello' })
  })

  it('terminates a child process when JSON reading times out', async () => {
    vi.useFakeTimers()
    const child = fakeChild()
    const terminate = vi.fn()

    const result = readJsonWithTimeout(
      child,
      50,
      'voice capture timed out',
      terminate,
    )
    const expectation = expect(result).rejects.toThrow('voice capture timed out')
    await vi.advanceTimersByTimeAsync(50)

    await expectation
    expect(terminate).toHaveBeenCalledWith(child)
    vi.useRealTimers()
  })
})
