import { describe, expect, it } from 'vitest'
import { coddyRuntimeSpawnPlan } from '../../main/runtimeProcess'

describe('runtimeProcess', () => {
  it('starts the packaged Coddy backend as the local runtime server', () => {
    const plan = coddyRuntimeSpawnPlan({
      appPath: '/opt/Coddy/resources/app.asar',
      env: {},
      exists: (candidate) => candidate === '/opt/Coddy/resources/bin/coddy',
      resourcesPath: '/opt/Coddy/resources',
    })

    expect(plan.command).toBe('/opt/Coddy/resources/bin/coddy')
    expect(plan.args).toEqual(['runtime', 'serve'])
  })

  it('honors CODDY_BIN for development runtime launches', () => {
    const plan = coddyRuntimeSpawnPlan({
      appPath: '/repo/apps/coddy-electron/dist/main',
      env: { CODDY_BIN: '/repo/target/debug/coddy' },
    })

    expect(plan.command).toBe('/repo/target/debug/coddy')
    expect(plan.args).toEqual(['runtime', 'serve'])
  })

  it('keeps CODDY_WORKSPACE in the runtime server environment', () => {
    const plan = coddyRuntimeSpawnPlan({
      appPath: '/repo/apps/coddy-electron/dist/main',
      env: {
        CODDY_BIN: '/repo/target/debug/coddy',
        CODDY_WORKSPACE: '/home/user/project',
      },
    })

    expect(plan.env.CODDY_WORKSPACE).toBe('/home/user/project')
  })

  it('does not pass provider API keys to the runtime server process environment', () => {
    const plan = coddyRuntimeSpawnPlan({
      appPath: '/repo/apps/coddy-electron/dist/main',
      env: {
        CODDY_BIN: '/repo/target/debug/coddy',
        HOME: '/home/user',
        OPENROUTER_API_KEY: 'sk-or-secret',
      },
    })

    expect(plan.env.HOME).toBe('/home/user')
    expect(plan.env.OPENROUTER_API_KEY).toBeUndefined()
  })
})
