import type { ChildProcess } from 'child_process'
import { redactSensitiveLogText } from './sensitiveLogRedaction'

const CHILD_KILL_GRACE_MS = 1_500

export async function readJson(child: ChildProcess): Promise<unknown> {
  return new Promise((resolve, reject) => {
    let stdout = ''
    let stderr = ''

    child.stdout?.on('data', (chunk: Buffer) => {
      stdout += chunk.toString()
    })
    child.stderr?.on('data', (chunk: Buffer) => {
      stderr += chunk.toString()
    })

    child.on('close', (code) => {
      if (code !== 0) {
        const detail = redactSensitiveLogText(stderr.trim())
        reject(
          new Error(
            detail
              ? `coddy exited ${code}: ${detail}`
              : `coddy exited ${code}`,
          ),
        )
        return
      }

      try {
        resolve(JSON.parse(stdout.trim()))
      } catch {
        resolve(stdout.trim())
      }
    })

    child.on('error', reject)
  })
}

export async function readJsonWithTimeout(
  child: ChildProcess,
  timeoutMs: number,
  timeoutMessage: string,
  terminate: (child: ChildProcess) => void = terminateChild,
): Promise<unknown> {
  let timer: ReturnType<typeof setTimeout> | null = null

  try {
    return await Promise.race([
      readJson(child),
      new Promise<never>((_resolve, reject) => {
        timer = setTimeout(() => {
          terminate(child)
          reject(new Error(timeoutMessage))
        }, timeoutMs)
      }),
    ])
  } finally {
    if (timer) clearTimeout(timer)
  }
}

export function terminateChild(child: ChildProcess): void {
  if (child.killed) return

  const pid = child.pid
  try {
    if (process.platform !== 'win32' && pid) {
      process.kill(-pid, 'SIGTERM')
    } else {
      child.kill('SIGTERM')
    }
  } catch {
    child.kill('SIGTERM')
  }

  const killTimer = setTimeout(() => {
    if (child.killed) return
    try {
      if (process.platform !== 'win32' && pid) {
        process.kill(-pid, 'SIGKILL')
      } else {
        child.kill('SIGKILL')
      }
    } catch {
      child.kill('SIGKILL')
    }
  }, CHILD_KILL_GRACE_MS)
  child.once('exit', () => clearTimeout(killTimer))
  killTimer.unref?.()
}
