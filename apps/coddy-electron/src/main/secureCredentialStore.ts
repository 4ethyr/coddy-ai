import { promises as fs } from 'fs'
import * as path from 'path'
import type { ModelProviderId } from './modelProviders'

export interface ProviderCredentialRecord {
  apiKey: string
  endpoint?: string
}

export interface CredentialStorageResult {
  persisted: boolean
  message: string
}

interface SecureCredentialStoreOptions {
  filePath: string
  isEncryptionAvailable: () => boolean
  encryptString: (value: string) => Buffer
  decryptString: (value: Buffer) => string
  readFile?: typeof fs.readFile
  writeFile?: typeof fs.writeFile
  mkdir?: typeof fs.mkdir
  chmod?: typeof fs.chmod
}

type StoredCredentialFile = Partial<Record<ModelProviderId, string>>

export class SecureCredentialStore {
  private readonly filePath: string
  private readonly isEncryptionAvailable: () => boolean
  private readonly encryptString: (value: string) => Buffer
  private readonly decryptString: (value: Buffer) => string
  private readonly readFile: typeof fs.readFile
  private readonly writeFile: typeof fs.writeFile
  private readonly mkdir: typeof fs.mkdir
  private readonly chmod: typeof fs.chmod

  constructor(options: SecureCredentialStoreOptions) {
    this.filePath = options.filePath
    this.isEncryptionAvailable = options.isEncryptionAvailable
    this.encryptString = options.encryptString
    this.decryptString = options.decryptString
    this.readFile = options.readFile ?? fs.readFile
    this.writeFile = options.writeFile ?? fs.writeFile
    this.mkdir = options.mkdir ?? fs.mkdir
    this.chmod = options.chmod ?? fs.chmod
  }

  async get(
    provider: ModelProviderId,
  ): Promise<ProviderCredentialRecord | null> {
    if (!this.isEncryptionAvailable()) return null

    const file = await this.readStoreFile()
    const encrypted = file[provider]
    if (!encrypted) return null

    try {
      const decrypted = this.decryptString(Buffer.from(encrypted, 'base64'))
      return normalizeCredentialRecord(JSON.parse(decrypted))
    } catch {
      return null
    }
  }

  async save(
    provider: ModelProviderId,
    record: ProviderCredentialRecord,
  ): Promise<CredentialStorageResult> {
    if (!this.isEncryptionAvailable()) {
      return {
        persisted: false,
        message: 'Secure OS credential storage is unavailable; token was not saved.',
      }
    }

    const normalized = normalizeCredentialRecord(record)
    if (!normalized) {
      return {
        persisted: false,
        message: 'Credential was empty and was not saved.',
      }
    }

    const file = await this.readStoreFile()
    file[provider] = this.encryptString(JSON.stringify(normalized)).toString(
      'base64',
    )

    await this.mkdir(path.dirname(this.filePath), { recursive: true })
    await this.writeFile(this.filePath, JSON.stringify(file, null, 2), {
      mode: 0o600,
    })
    await this.chmod(this.filePath, 0o600).catch(() => undefined)

    return {
      persisted: true,
      message: 'Credential saved with secure OS encryption.',
    }
  }

  private async readStoreFile(): Promise<StoredCredentialFile> {
    try {
      const raw = await this.readFile(this.filePath, 'utf8')
      const parsed = JSON.parse(raw) as StoredCredentialFile
      return parsed && typeof parsed === 'object' ? parsed : {}
    } catch {
      return {}
    }
  }
}

function normalizeCredentialRecord(
  value: unknown,
): ProviderCredentialRecord | null {
  if (!value || typeof value !== 'object') return null

  const record = value as Partial<ProviderCredentialRecord>
  const apiKey = record.apiKey?.trim()
  const endpoint = record.endpoint?.trim()
  if (!apiKey) return null

  return {
    apiKey,
    ...(endpoint ? { endpoint } : {}),
  }
}
