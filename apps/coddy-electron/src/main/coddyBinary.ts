import * as fs from 'fs'
import * as path from 'path'

export interface CoddyBinaryResolutionOptions {
  appPath?: string
  cwd?: string
  env?: { CODDY_BIN?: string }
  exists?: (candidate: string) => boolean
  platform?: NodeJS.Platform
  resourcesPath?: string
}

export function resolveCoddyBinaryPath(
  options: CoddyBinaryResolutionOptions = {},
): string {
  const env = options.env ?? process.env
  const override = env.CODDY_BIN?.trim()
  if (override) return override

  const platform = options.platform ?? process.platform
  const executable = platform === 'win32' ? 'coddy.exe' : 'coddy'
  const exists = options.exists ?? fs.existsSync
  const candidates = coddyBinaryCandidates({
    ...options,
    platform,
    executable,
  })

  return candidates.find((candidate) => exists(candidate)) ?? executable
}

function coddyBinaryCandidates(
  options: CoddyBinaryResolutionOptions & {
    executable: string
    platform: NodeJS.Platform
  },
): string[] {
  const cwd = options.cwd ?? process.cwd()
  const candidates: string[] = []

  if (options.resourcesPath) {
    candidates.push(path.join(options.resourcesPath, 'bin', options.executable))
  }

  if (options.appPath) {
    candidates.push(path.resolve(options.appPath, '..', 'bin', options.executable))
    candidates.push(
      path.resolve(options.appPath, '..', '..', '..', '..', 'target', 'release', options.executable),
    )
    candidates.push(
      path.resolve(options.appPath, '..', '..', '..', '..', 'target', 'debug', options.executable),
    )
  }

  candidates.push(path.resolve(cwd, '..', '..', 'target', 'release', options.executable))
  candidates.push(path.resolve(cwd, '..', '..', 'target', 'debug', options.executable))

  return [...new Set(candidates)]
}
