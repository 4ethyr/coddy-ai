import { describe, expect, it } from 'vitest'
import {
  buildCoddySpawnEnv,
  CODDY_SKIP_DESKTOP_LAUNCH_ENV,
} from '@/main/coddySpawnEnv'

describe('buildCoddySpawnEnv', () => {
  it('marks Electron-spawned CLI commands so ui open does not relaunch Desktop', () => {
    const env = buildCoddySpawnEnv(
      {
        PATH: '/usr/bin',
        CODDY_SKIP_DESKTOP_LAUNCH: '0',
      },
      { CODDY_WORKSPACE: '/home/user/project' },
      { CODDY_EPHEMERAL_MODEL_CREDENTIAL: '{"provider":"openrouter"}' },
    )

    expect(env.PATH).toBe('/usr/bin')
    expect(env.CODDY_WORKSPACE).toBe('/home/user/project')
    expect(env.CODDY_EPHEMERAL_MODEL_CREDENTIAL).toBe(
      '{"provider":"openrouter"}',
    )
    expect(env[CODDY_SKIP_DESKTOP_LAUNCH_ENV]).toBe('1')
  })
})
