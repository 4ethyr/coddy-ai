export const CODDY_SKIP_DESKTOP_LAUNCH_ENV = 'CODDY_SKIP_DESKTOP_LAUNCH'

const BASE_ENV_ALLOWLIST = new Set([
  'CODDY_CLIENT_REQUEST_TIMEOUT_MS',
  'CODDY_CONFIG',
  'CODDY_DAEMON_SOCKET',
  'CODDY_DESKTOP_BIN',
  'CODDY_WORKSPACE',
  'DBUS_SESSION_BUS_ADDRESS',
  'DISPLAY',
  'HOME',
  'LANG',
  'LC_ALL',
  'LC_CTYPE',
  'LOGNAME',
  'OLLAMA_HOST',
  'PATH',
  'RUST_LOG',
  'SHELL',
  'TERM',
  'TMPDIR',
  'TZ',
  'USER',
  'WAYLAND_DISPLAY',
  'XDG_CACHE_HOME',
  'XDG_CONFIG_HOME',
  'XDG_CURRENT_DESKTOP',
  'XDG_DATA_HOME',
  'XDG_RUNTIME_DIR',
  'XDG_SESSION_TYPE',
])

const SENSITIVE_ENV_KEY_PATTERN =
  /(auth|credential|cookie|key|password|secret|session|token)/i

export function buildCoddySpawnEnv(
  baseEnv: NodeJS.ProcessEnv,
  workspaceEnv: Record<string, string>,
  requestEnv: Record<string, string>,
): NodeJS.ProcessEnv {
  return {
    ...sanitizedBaseEnv(baseEnv),
    ...workspaceEnv,
    ...requestEnv,
    [CODDY_SKIP_DESKTOP_LAUNCH_ENV]: '1',
  }
}

function sanitizedBaseEnv(baseEnv: NodeJS.ProcessEnv): NodeJS.ProcessEnv {
  return Object.fromEntries(
    Object.entries(baseEnv).filter(([key, value]) => {
      if (typeof value !== 'string' || value.length === 0) return false
      if (SENSITIVE_ENV_KEY_PATTERN.test(key)) return false
      return BASE_ENV_ALLOWLIST.has(key) || key.startsWith('LC_')
    }),
  )
}
