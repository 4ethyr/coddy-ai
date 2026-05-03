export interface MultiagentEvalRequest {
  baseline?: string
  writeBaseline?: string
}

export interface EvalBaselineRequest {
  baseline?: string
  writeBaseline?: string
}

export type PromptBatteryEvalRequest = EvalBaselineRequest

export interface EvalBaselineComparison {
  status: 'passed' | 'failed'
  previousScore: number
  currentScore: number
  scoreDelta: number
  previousPromptCount?: number
  currentPromptCount?: number
  promptCountDelta?: number
  regressions: string[]
  improvements: string[]
}

export type MultiagentEvalComparison = EvalBaselineComparison

export interface MultiagentExecutionMetrics {
  total: number
  completed: number
  failed: number
  blocked: number
  awaitingApproval: number
  acceptedOutputs: number
  rejectedOutputs: number
  missingOutputs: number
  unexpectedOutputs: string[]
}

export interface MultiagentEvalReport {
  caseName: string
  status: 'passed' | 'failed'
  score: number
  failures: string[]
  executionMetrics?: MultiagentExecutionMetrics | null
}

export interface MultiagentEvalSuiteSummary {
  score: number
  passed: number
  failed: number
  reports: MultiagentEvalReport[]
}

export interface MultiagentEvalResult {
  suite: MultiagentEvalSuiteSummary
  baselineWritten: string | null
  comparison?: MultiagentEvalComparison
}

export interface PromptBatteryFailure {
  id: string
  stack: string
  knowledgeArea: string
  failures: string[]
}

export interface PromptBatteryResult {
  baselineWritten?: string | null
  comparison?: EvalBaselineComparison
  promptCount: number
  stackCount: number
  knowledgeAreaCount: number
  passed: number
  failed: number
  score: number
  memberCoverage: Record<string, number>
  failures: PromptBatteryFailure[]
}

export interface GroundedResponseFailure {
  id: string
  ungroundedPaths: string[]
}

export interface GroundedResponseResult {
  kind: 'coddy.groundedResponseEval'
  caseCount: number
  passed: number
  failed: number
  score: number
  failures: GroundedResponseFailure[]
}

export interface QualityEvalCheck {
  name: string
  status: 'passed' | 'failed'
  score: number
  passed?: number
  failed?: number
  promptCount?: number
  caseCount?: number
}

export interface QualityEvalResult {
  kind: 'coddy.qualityEval'
  version: number
  status: 'passed' | 'failed'
  passed: boolean
  score: number
  checks: QualityEvalCheck[]
  multiagent: MultiagentEvalSuiteSummary
  promptBattery: PromptBatteryResult
  groundedResponse?: GroundedResponseResult
}
