import { describe, expect, it, vi } from 'vitest'
import {
  ensureLocalModelReady,
  type LocalCommandRunner,
  type LocalProcessStarter,
} from '../../main/localModelManager'

function commandError(code: string, message: string): Error & { code: string } {
  return Object.assign(new Error(message), { code })
}

describe('localModelManager', () => {
  it('skips remote API providers', async () => {
    const runner = vi.fn<LocalCommandRunner>()

    const result = await ensureLocalModelReady(
      { provider: 'openai', name: 'gpt-4.1' },
      { runner },
    )

    expect(result.status).toBe('skipped')
    expect(runner).not.toHaveBeenCalled()
  })

  it('checks whether an Ollama model is already available', async () => {
    const runner = vi.fn<LocalCommandRunner>().mockResolvedValue({
      stdout: 'Modelfile',
      stderr: '',
    })

    const result = await ensureLocalModelReady(
      { provider: 'ollama', name: 'qwen2.5:0.5b' },
      { runner },
    )

    expect(result).toMatchObject({
      status: 'ready',
      provider: 'ollama',
      model: 'qwen2.5:0.5b',
    })
    expect(runner).toHaveBeenCalledWith(
      'ollama',
      ['show', 'qwen2.5:0.5b'],
      expect.any(Object),
    )
  })

  it('pulls an absent Ollama model before selection', async () => {
    const runner = vi
      .fn<LocalCommandRunner>()
      .mockRejectedValueOnce(commandError('1', 'model not found'))
      .mockResolvedValueOnce({ stdout: 'success', stderr: '' })

    const result = await ensureLocalModelReady(
      { provider: 'ollama', name: 'qwen2.5:0.5b' },
      { runner },
    )

    expect(result.status).toBe('ready')
    expect(runner).toHaveBeenNthCalledWith(
      1,
      'ollama',
      ['show', 'qwen2.5:0.5b'],
      expect.any(Object),
    )
    expect(runner).toHaveBeenNthCalledWith(
      2,
      'ollama',
      ['pull', 'qwen2.5:0.5b'],
      expect.any(Object),
    )
  })

  it('uses the preferred installed local provider when auto-detecting', async () => {
    const runner = vi
      .fn<LocalCommandRunner>()
      .mockRejectedValueOnce(commandError('ENOENT', 'spawn ollama ENOENT'))
      .mockResolvedValueOnce({ stdout: 'vllm 0.10', stderr: '' })
      .mockResolvedValueOnce({ stdout: 'hf 1.0', stderr: '' })
    const starter = vi.fn<LocalProcessStarter>().mockResolvedValue({ pid: 456 })

    const result = await ensureLocalModelReady(
      { provider: 'local', name: 'Qwen/Qwen2.5-0.5B-Instruct' },
      { runner, starter, preferredProvider: 'auto' },
    )

    expect(result.status).toBe('starting')
    expect(result.provider).toBe('vllm')
    expect(starter).toHaveBeenCalledWith(
      'vllm',
      [
        'serve',
        'Qwen/Qwen2.5-0.5B-Instruct',
        '--host',
        '127.0.0.1',
        '--port',
        '8000',
      ],
      expect.any(Object),
    )
  })

  it('honors an explicit local provider preference', async () => {
    const runner = vi.fn<LocalCommandRunner>().mockResolvedValue({
      stdout: '/models/Qwen',
      stderr: '',
    })

    const result = await ensureLocalModelReady(
      { provider: 'local', name: 'Qwen/Qwen2.5-0.5B-Instruct' },
      { runner, preferredProvider: 'hf' },
    )

    expect(result.status).toBe('ready')
    expect(result.provider).toBe('hf')
    expect(runner).toHaveBeenCalledWith(
      'hf',
      ['download', 'Qwen/Qwen2.5-0.5B-Instruct'],
      expect.any(Object),
    )
  })

  it('returns a friendly error when the local CLI is missing', async () => {
    const runner = vi
      .fn<LocalCommandRunner>()
      .mockRejectedValue(commandError('ENOENT', 'spawn ollama ENOENT'))

    const result = await ensureLocalModelReady(
      { provider: 'ollama', name: 'qwen2.5:0.5b' },
      { runner },
    )

    expect(result).toMatchObject({
      status: 'error',
      code: 'LOCAL_MODEL_TOOL_MISSING',
    })
    expect(result.message).toContain('Ollama CLI')
  })

  it('downloads Hugging Face models with the hf CLI', async () => {
    const runner = vi.fn<LocalCommandRunner>().mockResolvedValue({
      stdout: '/models/Qwen',
      stderr: '',
    })

    const result = await ensureLocalModelReady(
      { provider: 'hf', name: 'Qwen/Qwen2.5-0.5B-Instruct' },
      { runner },
    )

    expect(result.status).toBe('ready')
    expect(runner).toHaveBeenCalledWith(
      'hf',
      ['download', 'Qwen/Qwen2.5-0.5B-Instruct'],
      expect.any(Object),
    )
  })

  it('starts vLLM with an OpenAI-compatible local server command', async () => {
    const runner = vi.fn<LocalCommandRunner>().mockResolvedValue({
      stdout: 'vllm 0.10',
      stderr: '',
    })
    const starter = vi.fn<LocalProcessStarter>().mockResolvedValue({ pid: 123 })

    const result = await ensureLocalModelReady(
      { provider: 'vllm', name: 'Meta/Llama-3.2-1B-Instruct' },
      { runner, starter },
    )

    expect(result).toMatchObject({
      status: 'starting',
      provider: 'vllm',
      model: 'Meta/Llama-3.2-1B-Instruct',
    })
    expect(runner).toHaveBeenCalledWith('vllm', ['--version'], expect.any(Object))
    expect(starter).toHaveBeenCalledWith(
      'vllm',
      [
        'serve',
        'Meta/Llama-3.2-1B-Instruct',
        '--host',
        '127.0.0.1',
        '--port',
        '8000',
      ],
      expect.any(Object),
    )
  })

  it('does not start duplicate vLLM processes for a model already managed by Coddy', async () => {
    const runner = vi.fn<LocalCommandRunner>().mockResolvedValue({
      stdout: 'vllm 0.10',
      stderr: '',
    })
    const starter = vi.fn<LocalProcessStarter>().mockResolvedValue({ pid: 123 })

    await ensureLocalModelReady(
      { provider: 'vllm', name: 'Qwen/Qwen2.5-1.5B-Instruct' },
      { runner, starter },
    )
    const result = await ensureLocalModelReady(
      { provider: 'vllm', name: 'Qwen/Qwen2.5-1.5B-Instruct' },
      { runner, starter },
    )

    expect(result.status).toBe('ready')
    expect(starter).toHaveBeenCalledTimes(1)
  })
})
