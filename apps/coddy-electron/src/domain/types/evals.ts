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

export interface MultiagentEvalSuiteSummary {
  score: number
  passed: number
  failed: number
  reports: unknown[]
}

export interface MultiagentEvalResult {
  suite: MultiagentEvalSuiteSummary
  baselineWritten: string | null
  comparison?: MultiagentEvalComparison
}
