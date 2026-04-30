// domain/types/tools.ts
// Mirrors public tool catalog items from crates/coddy-ipc.

export type ToolCategory =
  | 'Filesystem'
  | 'Search'
  | 'Shell'
  | 'Git'
  | 'Network'
  | 'Memory'
  | 'Eval'
  | 'Mcp'
  | 'Subagent'
  | 'Repl'
  | 'Other'

export type ToolRiskLevel = 'Low' | 'Medium' | 'High' | 'Critical'

export type ToolPermission =
  | 'ReadWorkspace'
  | 'WriteWorkspace'
  | 'ReadExternalPath'
  | 'WriteExternalPath'
  | 'ExecuteCommand'
  | 'AccessNetwork'
  | 'ManageMemory'
  | 'UseMcp'
  | 'DelegateSubagent'
  | 'RequestUserInput'

export type ApprovalPolicy =
  | 'AutoApprove'
  | 'AskOnUse'
  | 'AlwaysAsk'
  | 'Deny'

export interface ReplToolCatalogItem {
  name: string
  description: string
  category: ToolCategory
  input_schema: Record<string, unknown>
  output_schema: Record<string, unknown>
  risk_level: ToolRiskLevel
  permissions: ToolPermission[]
  timeout_ms: number
  approval_policy: ApprovalPolicy
}
