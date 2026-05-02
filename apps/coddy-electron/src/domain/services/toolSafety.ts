import type {
  ReplToolCatalogItem,
  ToolRiskLevel,
} from '../types/tools'

export interface ToolSafetySummary {
  total: number
  autoApproved: number
  approvalRequired: number
  denied: number
  lowRisk: number
  mediumRisk: number
  highRisk: number
  highestRisk: ToolRiskLevel | null
  highRiskAutoApproved: number
  highRiskGuarded: number
  hasApprovalControls: boolean
  hasHighRiskTools: boolean
  hasAutoApprovedHighRiskTools: boolean
}

export function summarizeToolSafety(
  tools: ReplToolCatalogItem[],
): ToolSafetySummary {
  const summary: ToolSafetySummary = {
    total: tools.length,
    autoApproved: 0,
    approvalRequired: 0,
    denied: 0,
    lowRisk: 0,
    mediumRisk: 0,
    highRisk: 0,
    highestRisk: null,
    highRiskAutoApproved: 0,
    highRiskGuarded: 0,
    hasApprovalControls: false,
    hasHighRiskTools: false,
    hasAutoApprovedHighRiskTools: false,
  }

  for (const tool of tools) {
    if (tool.approval_policy === 'AutoApprove') {
      summary.autoApproved += 1
    } else if (
      tool.approval_policy === 'AskOnUse'
      || tool.approval_policy === 'AlwaysAsk'
    ) {
      summary.approvalRequired += 1
    } else if (tool.approval_policy === 'Deny') {
      summary.denied += 1
    }

    if (tool.risk_level === 'Low') {
      summary.lowRisk += 1
    } else if (tool.risk_level === 'Medium') {
      summary.mediumRisk += 1
    } else {
      summary.highRisk += 1
    }

    summary.highestRisk = maxRisk(summary.highestRisk, tool.risk_level)

    if (isHighRisk(tool.risk_level)) {
      if (tool.approval_policy === 'AutoApprove') {
        summary.highRiskAutoApproved += 1
      } else {
        summary.highRiskGuarded += 1
      }
    }
  }

  summary.hasApprovalControls =
    summary.approvalRequired > 0 || summary.denied > 0
  summary.hasHighRiskTools = summary.highRisk > 0
  summary.hasAutoApprovedHighRiskTools = summary.highRiskAutoApproved > 0

  return summary
}

function maxRisk(
  current: ToolRiskLevel | null,
  next: ToolRiskLevel,
): ToolRiskLevel {
  if (!current) return next
  return riskRank(next) > riskRank(current) ? next : current
}

function riskRank(risk: ToolRiskLevel): number {
  switch (risk) {
    case 'Low':
      return 1
    case 'Medium':
      return 2
    case 'High':
      return 3
    case 'Critical':
      return 4
  }
}

function isHighRisk(risk: ToolRiskLevel): boolean {
  return risk === 'High' || risk === 'Critical'
}
