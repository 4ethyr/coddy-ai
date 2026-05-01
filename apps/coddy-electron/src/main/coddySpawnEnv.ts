export const CODDY_SKIP_DESKTOP_LAUNCH_ENV = 'CODDY_SKIP_DESKTOP_LAUNCH'

export function buildCoddySpawnEnv(
  baseEnv: NodeJS.ProcessEnv,
  workspaceEnv: Record<string, string>,
  requestEnv: Record<string, string>,
): NodeJS.ProcessEnv {
  return {
    ...baseEnv,
    ...workspaceEnv,
    ...requestEnv,
    [CODDY_SKIP_DESKTOP_LAUNCH_ENV]: '1',
  }
}
