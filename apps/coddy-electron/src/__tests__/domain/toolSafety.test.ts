import { describe, expect, it } from 'vitest'
import type { ReplToolCatalogItem } from '@/domain'
import { summarizeToolSafety } from '@/domain'

describe('summarizeToolSafety', () => {
  it('counts approval policies and risk levels for the current tool catalog', () => {
    const summary = summarizeToolSafety([
      tool({ name: 'filesystem.read_file', risk_level: 'Low' }),
      tool({
        name: 'filesystem.apply_edit',
        approval_policy: 'AlwaysAsk',
        risk_level: 'Medium',
      }),
      tool({
        name: 'shell.run',
        approval_policy: 'AskOnUse',
        risk_level: 'High',
      }),
      tool({
        name: 'network.http_post',
        approval_policy: 'Deny',
        risk_level: 'Critical',
      }),
    ])

    expect(summary).toEqual({
      total: 4,
      autoApproved: 1,
      approvalRequired: 2,
      denied: 1,
      lowRisk: 1,
      mediumRisk: 1,
      highRisk: 2,
      highestRisk: 'Critical',
      hasApprovalControls: true,
      hasHighRiskTools: true,
    })
  })

  it('returns a neutral summary for an empty tool catalog', () => {
    expect(summarizeToolSafety([])).toEqual({
      total: 0,
      autoApproved: 0,
      approvalRequired: 0,
      denied: 0,
      lowRisk: 0,
      mediumRisk: 0,
      highRisk: 0,
      highestRisk: null,
      hasApprovalControls: false,
      hasHighRiskTools: false,
    })
  })
})

function tool(
  overrides: Partial<ReplToolCatalogItem> & Pick<ReplToolCatalogItem, 'name'>,
): ReplToolCatalogItem {
  return {
    name: overrides.name,
    description: 'Test tool',
    category: overrides.category ?? 'Filesystem',
    input_schema: overrides.input_schema ?? { type: 'object' },
    output_schema: overrides.output_schema ?? { type: 'object' },
    risk_level: overrides.risk_level ?? 'Low',
    permissions: overrides.permissions ?? ['ReadWorkspace'],
    timeout_ms: overrides.timeout_ms ?? 10000,
    approval_policy: overrides.approval_policy ?? 'AutoApprove',
  }
}
