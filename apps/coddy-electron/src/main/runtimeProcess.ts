import { spawn, ChildProcess } from 'child_process'
import { resolveCoddyBinaryPath } from './coddyBinary'
import { redactSensitiveLogText } from './sensitiveLogRedaction'

export type RuntimeSpawnPlan = {
  command: string
  args: string[]
  env: NodeJS.ProcessEnv
}

export type CoddyRuntimeProcessOptions = {
  appPath: string
  env?: NodeJS.ProcessEnv
  exists?: (candidate: string) => boolean
  resourcesPath?: string
  spawnProcess?: typeof spawn
}

let runtimeProcess: ChildProcess | null = null

export function coddyRuntimeSpawnPlan(
  options: CoddyRuntimeProcessOptions,
): RuntimeSpawnPlan {
  const env = options.env ?? process.env
  return {
    command: resolveCoddyBinaryPath({
      appPath: options.appPath,
      env,
      exists: options.exists,
      resourcesPath: options.resourcesPath,
    }),
    args: ['runtime', 'serve'],
    env,
  }
}

export function startCoddyRuntimeProcess(
  options: CoddyRuntimeProcessOptions,
): ChildProcess | null {
  if (runtimeProcess && !runtimeProcess.killed) return runtimeProcess

  const plan = coddyRuntimeSpawnPlan(options)
  const spawnProcess = options.spawnProcess ?? spawn
  const child = spawnProcess(plan.command, plan.args, {
    env: plan.env,
    stdio: ['ignore', 'ignore', 'pipe'],
  })
  runtimeProcess = child

  child.stderr?.on('data', (chunk: Buffer) => {
    console.error(
      `[coddy runtime] ${redactSensitiveLogText(chunk.toString().trim())}`,
    )
  })
  child.on('exit', () => {
    if (runtimeProcess === child) runtimeProcess = null
  })
  child.on('error', (error) => {
    console.error(`[coddy runtime] failed to start: ${error.message}`)
    if (runtimeProcess === child) runtimeProcess = null
  })

  return child
}

export function stopCoddyRuntimeProcess(): void {
  if (runtimeProcess && !runtimeProcess.killed) {
    runtimeProcess.kill()
  }
  runtimeProcess = null
}

export function restartCoddyRuntimeProcess(
  options: CoddyRuntimeProcessOptions,
): ChildProcess | null {
  stopCoddyRuntimeProcess()
  return startCoddyRuntimeProcess(options)
}
