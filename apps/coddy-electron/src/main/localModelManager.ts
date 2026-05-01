import { execFile, spawn } from 'child_process'
import type { ChildProcess } from 'child_process'

export type LocalModelRef = {
  provider: string
  name: string
}

export type LocalModelPrepareStatus =
  | 'skipped'
  | 'ready'
  | 'starting'
  | 'error'

export type LocalModelPrepareResult = {
  status: LocalModelPrepareStatus
  provider: string
  model: string
  message: string
  code?: string
}

export type LocalModelProviderPreference = 'auto' | 'ollama' | 'hf' | 'vllm'

export type LocalCommandResult = {
  stdout: string
  stderr: string
}

export type LocalCommandRunner = (
  command: string,
  args: string[],
  options: LocalCommandOptions,
) => Promise<LocalCommandResult>

export type LocalProcessStarter = (
  command: string,
  args: string[],
  options: LocalProcessOptions,
) => Promise<{ pid?: number }>

export type LocalCommandOptions = {
  timeoutMs: number
  maxBufferBytes: number
}

export type LocalProcessOptions = {
  detached: boolean
  stdio: 'ignore'
}

export type LocalModelManagerOptions = {
  runner?: LocalCommandRunner
  starter?: LocalProcessStarter
  preferredProvider?: LocalModelProviderPreference
}

const COMMAND_TIMEOUT_MS = 30_000
const DOWNLOAD_TIMEOUT_MS = 30 * 60_000
const MAX_BUFFER_BYTES = 128 * 1024
const VLLM_HOST = '127.0.0.1'
const VLLM_PORT = '8000'
const managedVllmModels = new Set<string>()

export async function ensureLocalModelReady(
  model: LocalModelRef,
  options: LocalModelManagerOptions = {},
): Promise<LocalModelPrepareResult> {
  const runner = options.runner ?? runLocalCommand
  const starter = options.starter ?? startLocalProcess
  const provider = model.provider.trim().toLowerCase()
  const modelName = model.name.trim()

  if (!modelName) {
    return errorResult(
      provider || model.provider,
      model.name,
      'LOCAL_MODEL_INVALID',
      'Local model name is empty.',
    )
  }

  const resolvedProvider = await resolveLocalProvider(provider, runner, options)

  switch (resolvedProvider) {
    case 'ollama':
      return ensureOllamaModel(modelName, runner)
    case 'hf':
      return ensureHuggingFaceModel(resolvedProvider, modelName, runner)
    case 'vllm':
      return ensureVllmModel(modelName, runner, starter)
    case 'missing':
      return errorResult(
        provider || 'local',
        model.name,
        'LOCAL_MODEL_TOOL_MISSING',
        'No supported local model provider was found. Install Ollama, vLLM or the Hugging Face hf CLI, or choose a remote API provider.',
      )
    default:
      return {
        status: 'skipped',
        provider: model.provider,
        model: model.name,
        message: 'Model provider does not require local preload.',
      }
  }
}

type ResolvedLocalProvider =
  | Exclude<LocalModelProviderPreference, 'auto'>
  | 'missing'
  | 'skip'

async function resolveLocalProvider(
  provider: string,
  runner: LocalCommandRunner,
  options: LocalModelManagerOptions,
): Promise<ResolvedLocalProvider> {
  const preference = options.preferredProvider ?? 'auto'
  if (preference !== 'auto' && isLocalProviderAlias(provider)) {
    return preference
  }
  if (preference !== 'auto' && provider === 'local') {
    return preference
  }
  if (provider === 'ollama' || provider === 'vllm') return provider
  if (provider === 'hf' || provider === 'huggingface' || provider === 'hugging-face') {
    return 'hf'
  }
  if (provider !== 'local') return 'skip'

  return detectInstalledLocalProvider(runner)
}

async function detectInstalledLocalProvider(
  runner: LocalCommandRunner,
): Promise<Exclude<LocalModelProviderPreference, 'auto'> | 'missing'> {
  const candidates: Array<{
    provider: Exclude<LocalModelProviderPreference, 'auto'>
    command: string
    args: string[]
  }> = [
    { provider: 'ollama', command: 'ollama', args: ['--version'] },
    { provider: 'vllm', command: 'vllm', args: ['--version'] },
    { provider: 'hf', command: 'hf', args: ['--version'] },
  ]

  for (const candidate of candidates) {
    try {
      await runner(
        candidate.command,
        candidate.args,
        commandOptions(COMMAND_TIMEOUT_MS),
      )
      return candidate.provider
    } catch {
      // Try the next local provider.
    }
  }

  return 'missing'
}

function isLocalProviderAlias(provider: string): boolean {
  return (
    provider === 'ollama'
    || provider === 'hf'
    || provider === 'huggingface'
    || provider === 'hugging-face'
    || provider === 'vllm'
  )
}

async function ensureOllamaModel(
  model: string,
  runner: LocalCommandRunner,
): Promise<LocalModelPrepareResult> {
  try {
    await runner('ollama', ['show', model], commandOptions(COMMAND_TIMEOUT_MS))
    return {
      status: 'ready',
      provider: 'ollama',
      model,
      message: `Ollama model ${model} is already available locally.`,
    }
  } catch (error) {
    if (isMissingExecutable(error)) {
      return missingToolResult('ollama', model, 'Ollama CLI')
    }
  }

  try {
    await runner('ollama', ['pull', model], commandOptions(DOWNLOAD_TIMEOUT_MS))
    return {
      status: 'ready',
      provider: 'ollama',
      model,
      message: `Ollama model ${model} was pulled successfully.`,
    }
  } catch (error) {
    if (isMissingExecutable(error)) {
      return missingToolResult('ollama', model, 'Ollama CLI')
    }
    return errorResult(
      'ollama',
      model,
      'LOCAL_MODEL_PRELOAD_FAILED',
      localToolErrorMessage('Ollama could not prepare this model', error),
    )
  }
}

async function ensureHuggingFaceModel(
  provider: string,
  model: string,
  runner: LocalCommandRunner,
): Promise<LocalModelPrepareResult> {
  try {
    await runner('hf', ['download', model], commandOptions(DOWNLOAD_TIMEOUT_MS))
    return {
      status: 'ready',
      provider,
      model,
      message: `Hugging Face model ${model} is available in the local hf cache.`,
    }
  } catch (error) {
    if (isMissingExecutable(error)) {
      return missingToolResult(provider, model, 'Hugging Face hf CLI')
    }
    return errorResult(
      provider,
      model,
      'LOCAL_MODEL_PRELOAD_FAILED',
      localToolErrorMessage('hf could not download this model', error),
    )
  }
}

async function ensureVllmModel(
  model: string,
  runner: LocalCommandRunner,
  starter: LocalProcessStarter,
): Promise<LocalModelPrepareResult> {
  if (managedVllmModels.has(model)) {
    return {
      status: 'ready',
      provider: 'vllm',
      model,
      message: `vLLM model ${model} is already managed by Coddy.`,
    }
  }

  try {
    await runner('vllm', ['--version'], commandOptions(COMMAND_TIMEOUT_MS))
  } catch (error) {
    if (isMissingExecutable(error)) {
      return missingToolResult('vllm', model, 'vLLM CLI')
    }
    return errorResult(
      'vllm',
      model,
      'LOCAL_MODEL_PRELOAD_FAILED',
      localToolErrorMessage('vLLM is not available', error),
    )
  }

  try {
    const child = await starter(
      'vllm',
      ['serve', model, '--host', VLLM_HOST, '--port', VLLM_PORT],
      {
        detached: true,
        stdio: 'ignore',
      },
    )
    managedVllmModels.add(model)
    return {
      status: 'starting',
      provider: 'vllm',
      model,
      message: `vLLM is starting ${model} on ${VLLM_HOST}:${VLLM_PORT}.`,
      ...(child.pid ? { code: `PID_${child.pid}` } : {}),
    }
  } catch (error) {
    if (isMissingExecutable(error)) {
      return missingToolResult('vllm', model, 'vLLM CLI')
    }
    return errorResult(
      'vllm',
      model,
      'LOCAL_MODEL_PRELOAD_FAILED',
      localToolErrorMessage('vLLM could not start this model', error),
    )
  }
}

function runLocalCommand(
  command: string,
  args: string[],
  options: LocalCommandOptions,
): Promise<LocalCommandResult> {
  return new Promise((resolve, reject) => {
    execFile(
      command,
      args,
      {
        timeout: options.timeoutMs,
        windowsHide: true,
        maxBuffer: options.maxBufferBytes,
      },
      (error, stdout, stderr) => {
        if (error) {
          reject(error)
          return
        }
        resolve({
          stdout: String(stdout),
          stderr: String(stderr),
        })
      },
    )
  })
}

function startLocalProcess(
  command: string,
  args: string[],
  options: LocalProcessOptions,
): Promise<{ pid?: number }> {
  return new Promise((resolve, reject) => {
    let child: ChildProcess
    try {
      child = spawn(command, args, {
        detached: options.detached,
        stdio: options.stdio,
      })
    } catch (error) {
      reject(error)
      return
    }

    child.once('error', reject)
    child.once('spawn', () => {
      child.unref()
      resolve({ pid: child.pid })
    })
  })
}

function commandOptions(timeoutMs: number): LocalCommandOptions {
  return {
    timeoutMs,
    maxBufferBytes: MAX_BUFFER_BYTES,
  }
}

function missingToolResult(
  provider: string,
  model: string,
  toolLabel: string,
): LocalModelPrepareResult {
  return errorResult(
    provider,
    model,
    'LOCAL_MODEL_TOOL_MISSING',
    `${toolLabel} was not found. Install it or choose a model from another configured provider.`,
  )
}

function errorResult(
  provider: string,
  model: string,
  code: string,
  message: string,
): LocalModelPrepareResult {
  return {
    status: 'error',
    provider,
    model,
    code,
    message,
  }
}

function isMissingExecutable(error: unknown): boolean {
  return (
    error !== null
    && typeof error === 'object'
    && 'code' in error
    && (error as { code?: unknown }).code === 'ENOENT'
  )
}

function localToolErrorMessage(prefix: string, error: unknown): string {
  if (error instanceof Error && error.message.trim()) {
    return `${prefix}: ${error.message}`
  }
  return `${prefix}.`
}
