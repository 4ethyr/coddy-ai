import { describe, expect, it, vi } from 'vitest'
import { SecureCredentialStore } from '../../main/secureCredentialStore'

describe('SecureCredentialStore', () => {
  it('encrypts credentials before writing them to disk', async () => {
    let written = ''
    const store = new SecureCredentialStore({
      filePath: '/tmp/coddy-secure-credentials.json',
      isEncryptionAvailable: () => true,
      encryptString: (value) => Buffer.from(`encrypted:${value}`),
      decryptString: (value) =>
        value.toString('utf8').replace(/^encrypted:/, ''),
      readFile: vi.fn().mockResolvedValue('{}'),
      writeFile: vi.fn(async (_path, value) => {
        written = String(value)
      }),
      mkdir: vi.fn().mockResolvedValue(undefined),
      chmod: vi.fn().mockResolvedValue(undefined),
    })

    const result = await store.save('openai', { apiKey: 'sk-secret' })

    expect(result.persisted).toBe(true)
    expect(written).not.toContain('sk-secret')
    const parsed = JSON.parse(written) as { openai: string }
    expect(Buffer.from(parsed.openai, 'base64').toString('utf8')).toContain(
      'encrypted:',
    )
  })

  it('does not persist credentials when OS encryption is unavailable', async () => {
    const writeFile = vi.fn()
    const store = new SecureCredentialStore({
      filePath: '/tmp/coddy-secure-credentials.json',
      isEncryptionAvailable: () => false,
      encryptString: (value) => Buffer.from(value),
      decryptString: (value) => value.toString('utf8'),
      writeFile,
    })

    const result = await store.save('openai', { apiKey: 'sk-secret' })

    expect(result.persisted).toBe(false)
    expect(writeFile).not.toHaveBeenCalled()
    await expect(store.get('openai')).resolves.toBeNull()
  })
})
