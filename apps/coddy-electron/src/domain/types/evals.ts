export interface MultiagentEvalRequest {
  baseline?: string
  writeBaseline?: string
}

export interface MultiagentEvalComparison {
  status: 'passed' | 'failed'
  previousScore: number
  currentScore: number
  scoreDelta: number
  regressions: string[]
  improvements: string[]
}

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
  promptCount: number
  stackCount: number
  knowledgeAreaCount: number
  passed: number
  failed: number
  score: number
  memberCoverage: Record<string, number>
  failures: PromptBatteryFailure[]
}
