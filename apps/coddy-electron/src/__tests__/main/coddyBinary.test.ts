import { describe, expect, it } from 'vitest'
import { resolveCoddyBinaryPath } from '../../main/coddyBinary'

describe('coddyBinary', () => {
  it('honors an explicit CODDY_BIN override', () => {
    expect(
      resolveCoddyBinaryPath({
        env: { CODDY_BIN: '/opt/coddy/bin/coddy' },
        exists: () => false,
      }),
    ).toBe('/opt/coddy/bin/coddy')
  })

  it('prefers the packaged backend binary from Electron resources', () => {
    expect(
      resolveCoddyBinaryPath({
        cwd: '/repo/apps/coddy-electron',
        env: {},
        exists: (candidate) => candidate === '/opt/Coddy/resources/bin/coddy',
        platform: 'linux',
        resourcesPath: '/opt/Coddy/resources',
      }),
    ).toBe('/opt/Coddy/resources/bin/coddy')
  })

  it('falls back to the local release binary during development', () => {
    expect(
      resolveCoddyBinaryPath({
        cwd: '/repo/apps/coddy-electron',
        env: {},
        exists: (candidate) => candidate === '/repo/target/release/coddy',
        platform: 'linux',
      }),
    ).toBe('/repo/target/release/coddy')
  })

  it('returns the executable name when no candidate exists', () => {
    expect(
      resolveCoddyBinaryPath({
        cwd: '/repo/apps/coddy-electron',
        env: {},
        exists: () => false,
        platform: 'linux',
      }),
    ).toBe('coddy')
  })
})
