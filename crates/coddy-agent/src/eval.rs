use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    path::Path,
};

use coddy_core::PermissionReply;
use thiserror::Error;
use uuid::Uuid;

use crate::{
    AgentError, DeterministicPlanExecutor, DeterministicPlanItem, DeterministicPlanReport,
    DeterministicPlanStatus, SubagentExecutionCoordinator, SubagentExecutionSummary,
    SubagentHandoffPlan, SubagentRegistry, SubagentTeamPlan,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvalStatus {
    Passed,
    Failed,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EvalCase {
    pub name: String,
    pub goal: String,
    pub plan: Vec<DeterministicPlanItem>,
    pub approvals: Vec<PermissionReply>,
    pub expectations: EvalExpectations,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvalExpectations {
    pub final_status: DeterministicPlanStatus,
    pub approvals_requested: usize,
    pub required_observation_substrings: Vec<String>,
    pub required_error_codes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EvalReport {
    pub case_name: String,
    pub status: EvalStatus,
    pub score: u8,
    pub final_plan_status: DeterministicPlanStatus,
    pub approvals_requested: usize,
    pub failures: Vec<String>,
    pub plan_report: DeterministicPlanReport,
}

#[derive(Debug, Clone)]
pub struct EvalRunner {
    executor: DeterministicPlanExecutor,
}

impl EvalRunner {
    pub fn new(workspace_root: impl AsRef<Path>) -> Result<Self, AgentError> {
        Ok(Self {
            executor: DeterministicPlanExecutor::new(workspace_root)?,
        })
    }

    pub fn run_case(&self, case: &EvalCase) -> EvalReport {
        let session_id = Uuid::new_v4();
        let mut approvals_requested = 0_usize;
        let mut approval_cursor = 0_usize;
        let mut report = self
            .executor
            .execute(session_id, case.goal.clone(), &case.plan);

        while report.status == DeterministicPlanStatus::AwaitingApproval {
            approvals_requested += 1;
            let Some(request) = report.pending_permission.clone() else {
                break;
            };
            let Some(reply) = case.approvals.get(approval_cursor).copied() else {
                break;
            };
            approval_cursor += 1;
            report = self.executor.resume_after_permission(
                report.state,
                request.id,
                reply,
                &case.plan,
                report.next_item_index,
            );
        }

        let evaluation = evaluate_expectations(case, &report, approvals_requested);
        EvalReport {
            case_name: case.name.clone(),
            status: if evaluation.failures.is_empty() {
                EvalStatus::Passed
            } else {
                EvalStatus::Failed
            },
            score: evaluation.score(),
            final_plan_status: report.status,
            approvals_requested,
            failures: evaluation.failures,
            plan_report: report,
        }
    }

    pub fn run_suite(&self, cases: &[EvalCase]) -> EvalSuiteReport {
        let reports = cases.iter().map(|case| self.run_case(case)).collect();
        EvalSuiteReport::new(reports)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EvalSuiteReport {
    pub reports: Vec<EvalReport>,
    pub passed: usize,
    pub failed: usize,
    pub score: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvalGateStatus {
    Passed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvalQualityGate {
    pub minimum_score: u8,
    pub max_failed_cases: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvalGateReport {
    pub status: EvalGateStatus,
    pub suite_score: u8,
    pub minimum_score: u8,
    pub failed_cases: usize,
    pub max_failed_cases: usize,
    pub failures: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultiagentEvalCase {
    pub name: String,
    pub goal: String,
    pub expected_members: Vec<String>,
    pub min_hardness_score: u8,
    pub max_blocked: usize,
    pub max_awaiting_approval: usize,
    pub validate_execution_reducer: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultiagentEvalReport {
    pub case_name: String,
    pub status: EvalStatus,
    pub score: u8,
    pub failures: Vec<String>,
    pub team_plan: SubagentTeamPlan,
    pub execution_metrics: Option<MultiagentExecutionMetrics>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultiagentEvalSuiteReport {
    pub reports: Vec<MultiagentEvalReport>,
    pub passed: usize,
    pub failed: usize,
    pub score: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultiagentExecutionMetrics {
    pub total: usize,
    pub completed: usize,
    pub failed: usize,
    pub blocked: usize,
    pub awaiting_approval: usize,
    pub accepted_outputs: usize,
    pub rejected_outputs: usize,
    pub missing_outputs: usize,
    pub unexpected_outputs: Vec<String>,
}

#[derive(Debug, Error)]
pub enum MultiagentEvalBaselineError {
    #[error("io error for {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("json error for {path}: {source}")]
    Json {
        path: String,
        #[source]
        source: serde_json::Error,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultiagentEvalBaselineComparison {
    pub status: EvalGateStatus,
    pub previous_score: u8,
    pub current_score: u8,
    pub regressions: Vec<String>,
    pub improvements: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptBatteryCase {
    pub id: String,
    pub stack: String,
    pub knowledge_area: String,
    pub prompt: String,
    pub expected_members: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptBatteryFailure {
    pub id: String,
    pub stack: String,
    pub knowledge_area: String,
    pub failures: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptBatteryReport {
    pub prompt_count: usize,
    pub stack_count: usize,
    pub knowledge_area_count: usize,
    pub passed: usize,
    pub failed: usize,
    pub score: u8,
    pub member_coverage: BTreeMap<String, usize>,
    pub failures: Vec<PromptBatteryFailure>,
}

#[derive(Debug, Clone, Default)]
pub struct MultiagentEvalRunner {
    registry: SubagentRegistry,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PromptBatteryStack {
    key: &'static str,
    label: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PromptBatteryScenario {
    key: &'static str,
    knowledge_area: &'static str,
    template: &'static str,
    expected_members: &'static [&'static str],
}

const PROMPT_BATTERY_STACKS: &[PromptBatteryStack] = &[
    PromptBatteryStack {
        key: "rust",
        label: "Rust async services and CLI tooling",
    },
    PromptBatteryStack {
        key: "typescript-react",
        label: "TypeScript, React and Electron UI",
    },
    PromptBatteryStack {
        key: "nodejs",
        label: "Node.js API services",
    },
    PromptBatteryStack {
        key: "python-fastapi",
        label: "Python FastAPI backends",
    },
    PromptBatteryStack {
        key: "django",
        label: "Django monoliths",
    },
    PromptBatteryStack {
        key: "go",
        label: "Go microservices",
    },
    PromptBatteryStack {
        key: "java-spring",
        label: "Java Spring Boot systems",
    },
    PromptBatteryStack {
        key: "kotlin",
        label: "Kotlin JVM services",
    },
    PromptBatteryStack {
        key: "cpp",
        label: "C and C++ low-level code",
    },
    PromptBatteryStack {
        key: "dotnet",
        label: "C# and .NET applications",
    },
    PromptBatteryStack {
        key: "swift-ios",
        label: "Swift iOS applications",
    },
    PromptBatteryStack {
        key: "android",
        label: "Android Kotlin applications",
    },
    PromptBatteryStack {
        key: "flutter",
        label: "Flutter and Dart applications",
    },
    PromptBatteryStack {
        key: "postgres",
        label: "PostgreSQL and SQL data layers",
    },
    PromptBatteryStack {
        key: "redis",
        label: "Redis caching systems",
    },
    PromptBatteryStack {
        key: "kafka",
        label: "Kafka streaming platforms",
    },
    PromptBatteryStack {
        key: "kubernetes",
        label: "Kubernetes platform engineering",
    },
    PromptBatteryStack {
        key: "terraform",
        label: "Terraform infrastructure as code",
    },
    PromptBatteryStack {
        key: "aws",
        label: "AWS cloud applications",
    },
    PromptBatteryStack {
        key: "gcp",
        label: "Google Cloud and Vertex AI systems",
    },
    PromptBatteryStack {
        key: "azure",
        label: "Azure cloud applications",
    },
    PromptBatteryStack {
        key: "security",
        label: "cybersecurity and secure coding",
    },
    PromptBatteryStack {
        key: "ml",
        label: "machine learning services",
    },
    PromptBatteryStack {
        key: "data-engineering",
        label: "data engineering pipelines",
    },
    PromptBatteryStack {
        key: "computer-vision",
        label: "computer vision systems",
    },
    PromptBatteryStack {
        key: "embedded",
        label: "embedded and hardware-adjacent software",
    },
    PromptBatteryStack {
        key: "blockchain",
        label: "Solidity and blockchain services",
    },
    PromptBatteryStack {
        key: "elixir",
        label: "Elixir Phoenix applications",
    },
    PromptBatteryStack {
        key: "ruby-rails",
        label: "Ruby on Rails applications",
    },
    PromptBatteryStack {
        key: "php-laravel",
        label: "PHP Laravel applications",
    },
];

const PROMPT_BATTERY_SCENARIOS: &[PromptBatteryScenario] = &[
    PromptBatteryScenario {
        key: "architecture-map",
        knowledge_area: "architecture",
        template: "Para {stack}, explore o repo workspace code, map architecture entrypoint dependency risk e plan strategy incremental.",
        expected_members: &["explorer", "planner"],
    },
    PromptBatteryScenario {
        key: "implementation-tdd",
        knowledge_area: "implementation",
        template: "Para {stack}, implement fix bug no code com TDD test coverage, preview edit, revise quality e security sandbox.",
        expected_members: &[
            "explorer",
            "coder",
            "test-writer",
            "security-reviewer",
            "reviewer",
        ],
    },
    PromptBatteryScenario {
        key: "security-threat-model",
        knowledge_area: "security",
        template: "Para {stack}, revise security secrets auth sandbox prompt injection path traversal command filesystem token leakage no repo.",
        expected_members: &["explorer", "security-reviewer", "reviewer"],
    },
    PromptBatteryScenario {
        key: "integration-regression",
        knowledge_area: "testing",
        template: "Para {stack}, criar test integration e2e regression harness eval baseline metric no code workspace.",
        expected_members: &["explorer", "test-writer", "eval-runner"],
    },
    PromptBatteryScenario {
        key: "performance-debug",
        knowledge_area: "performance",
        template: "Para {stack}, inspect performance bug no code, implement fix, add regression test e review diff.",
        expected_members: &["explorer", "coder", "test-writer", "reviewer"],
    },
    PromptBatteryScenario {
        key: "docs-onboarding",
        knowledge_area: "documentation",
        template: "Para {stack}, atualizar docs readme documentation guide usage commands architecture do repo.",
        expected_members: &["explorer", "docs-writer"],
    },
    PromptBatteryScenario {
        key: "platform-gate",
        knowledge_area: "devops",
        template: "Para {stack}, plan CI build docker deploy guard security, implement pipeline, test gate eval metric.",
        expected_members: &["planner", "coder", "test-writer", "security-reviewer", "eval-runner"],
    },
    PromptBatteryScenario {
        key: "data-ai-quality",
        knowledge_area: "data-ai",
        template: "Para {stack}, plan strategy architecture for data AI quality, implement feature, test eval metric baseline e review.",
        expected_members: &["planner", "coder", "test-writer", "eval-runner", "reviewer"],
    },
    PromptBatteryScenario {
        key: "low-level-reliability",
        knowledge_area: "reliability",
        template: "Para {stack}, inspect memory concurrency crash risk, security command sandbox, fix code e test edge case.",
        expected_members: &["explorer", "coder", "test-writer", "security-reviewer"],
    },
    PromptBatteryScenario {
        key: "product-ux-config",
        knowledge_area: "product-ux",
        template: "Para {stack}, planejar UX API config, implement feature, teste fluxos e revise maintainability.",
        expected_members: &["planner", "coder", "test-writer", "reviewer"],
    },
];

const MULTIAGENT_EXECUTION_REDUCER_CHECKS: usize = 6;

impl EvalSuiteReport {
    fn new(reports: Vec<EvalReport>) -> Self {
        let passed = reports
            .iter()
            .filter(|report| report.status == EvalStatus::Passed)
            .count();
        let failed = reports.len().saturating_sub(passed);
        let score = suite_score(&reports);
        Self {
            reports,
            passed,
            failed,
            score,
        }
    }

    pub fn is_success(&self) -> bool {
        self.failed == 0
    }

    pub fn passes_score_threshold(&self, minimum_score: u8) -> bool {
        self.score >= minimum_score
    }

    pub fn evaluate_gate(&self, gate: EvalQualityGate) -> EvalGateReport {
        gate.evaluate(self)
    }
}

impl MultiagentEvalCase {
    pub fn new(name: impl Into<String>, goal: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            goal: goal.into(),
            expected_members: Vec::new(),
            min_hardness_score: 100,
            max_blocked: 0,
            max_awaiting_approval: usize::MAX,
            validate_execution_reducer: false,
        }
    }

    pub fn expected_members(mut self, members: &[&str]) -> Self {
        self.expected_members = members.iter().map(|member| (*member).to_string()).collect();
        self
    }

    pub fn min_hardness_score(mut self, score: u8) -> Self {
        self.min_hardness_score = score.min(100);
        self
    }

    pub fn max_blocked(mut self, count: usize) -> Self {
        self.max_blocked = count;
        self
    }

    pub fn max_awaiting_approval(mut self, count: usize) -> Self {
        self.max_awaiting_approval = count;
        self
    }

    pub fn validate_execution_reducer(mut self) -> Self {
        self.validate_execution_reducer = true;
        self
    }
}

impl MultiagentEvalRunner {
    pub fn new(registry: SubagentRegistry) -> Self {
        Self { registry }
    }

    pub fn run_case(&self, case: &MultiagentEvalCase) -> MultiagentEvalReport {
        let team_plan = self
            .registry
            .plan_team(&case.goal, self.registry.definitions().len())
            .unwrap_or_else(|| {
                self.registry
                    .plan_team(
                        "plan fallback multiagent evaluation",
                        self.registry.definitions().len(),
                    )
                    .expect("fallback team plan")
            });
        let failures = evaluate_multiagent_case(case, &team_plan);
        let (execution_metrics, execution_failures) = if case.validate_execution_reducer {
            let (summary, failures) = self.reduce_execution_for_team(&team_plan, &case.goal);
            (Some(MultiagentExecutionMetrics::from(&summary)), failures)
        } else {
            (None, Vec::new())
        };
        let mut failures = failures;
        failures.extend(execution_failures);
        let total_checks = 3
            + case.expected_members.len()
            + if case.validate_execution_reducer {
                MULTIAGENT_EXECUTION_REDUCER_CHECKS
            } else {
                0
            };
        let score = multiagent_case_score(&failures, total_checks);

        MultiagentEvalReport {
            case_name: case.name.clone(),
            status: if failures.is_empty() {
                EvalStatus::Passed
            } else {
                EvalStatus::Failed
            },
            score,
            failures,
            team_plan,
            execution_metrics,
        }
    }

    pub fn run_suite(&self, cases: &[MultiagentEvalCase]) -> MultiagentEvalSuiteReport {
        MultiagentEvalSuiteReport::new(cases.iter().map(|case| self.run_case(case)).collect())
    }

    fn reduce_execution_for_team(
        &self,
        team_plan: &SubagentTeamPlan,
        goal: &str,
    ) -> (SubagentExecutionSummary, Vec<String>) {
        let mut failures = Vec::new();
        let mut handoffs = Vec::<SubagentHandoffPlan>::new();

        for member in &team_plan.members {
            match self.registry.prepare_handoff(&member.name, goal) {
                Some(handoff) => handoffs.push(handoff),
                None => failures.push(format!(
                    "missing handoff definition for subagent member: {}",
                    member.name
                )),
            }
        }

        let outputs = handoffs
            .iter()
            .map(|handoff| {
                (
                    handoff.name.clone(),
                    synthetic_valid_subagent_output(handoff),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let approvals = handoffs
            .iter()
            .filter(|handoff| handoff.approval_required)
            .map(|handoff| handoff.name.clone())
            .collect::<std::collections::BTreeSet<_>>();
        let summary = SubagentExecutionCoordinator::default()
            .reduce_handoffs(&handoffs, &outputs, &approvals);

        failures.extend(evaluate_execution_summary(
            &summary,
            team_plan.members.len(),
        ));
        (summary, failures)
    }
}

impl MultiagentEvalReport {
    pub fn public_metadata(&self) -> serde_json::Value {
        let execution_metrics = self
            .execution_metrics
            .as_ref()
            .map(MultiagentExecutionMetrics::public_metadata);
        serde_json::json!({
            "caseName": self.case_name,
            "status": eval_status_name(&self.status),
            "score": self.score,
            "failures": self.failures,
            "teamPlan": self.team_plan.public_metadata(),
            "executionMetrics": execution_metrics,
        })
    }
}

impl MultiagentExecutionMetrics {
    pub fn public_metadata(&self) -> serde_json::Value {
        serde_json::json!({
            "total": self.total,
            "completed": self.completed,
            "failed": self.failed,
            "blocked": self.blocked,
            "awaitingApproval": self.awaiting_approval,
            "acceptedOutputs": self.accepted_outputs,
            "rejectedOutputs": self.rejected_outputs,
            "missingOutputs": self.missing_outputs,
            "unexpectedOutputs": self.unexpected_outputs,
        })
    }
}

impl From<&SubagentExecutionSummary> for MultiagentExecutionMetrics {
    fn from(summary: &SubagentExecutionSummary) -> Self {
        Self {
            total: summary.total,
            completed: summary.completed,
            failed: summary.failed,
            blocked: summary.blocked,
            awaiting_approval: summary.awaiting_approval,
            accepted_outputs: summary.accepted_outputs,
            rejected_outputs: summary.rejected_outputs,
            missing_outputs: summary.missing_outputs,
            unexpected_outputs: summary.unexpected_outputs.clone(),
        }
    }
}

impl MultiagentEvalSuiteReport {
    fn new(reports: Vec<MultiagentEvalReport>) -> Self {
        let passed = reports
            .iter()
            .filter(|report| report.status == EvalStatus::Passed)
            .count();
        let failed = reports.len().saturating_sub(passed);
        let score = multiagent_suite_score(&reports);

        Self {
            reports,
            passed,
            failed,
            score,
        }
    }

    pub fn public_metadata(&self) -> serde_json::Value {
        serde_json::json!({
            "passed": self.passed,
            "failed": self.failed,
            "score": self.score,
            "reports": self.reports.iter().map(MultiagentEvalReport::public_metadata).collect::<Vec<_>>(),
        })
    }

    pub fn baseline_json(&self) -> serde_json::Value {
        serde_json::json!({
            "kind": "coddy.multiagentEvalBaseline",
            "version": 1,
            "suite": self.public_metadata(),
        })
    }

    pub fn write_baseline(
        &self,
        path: impl AsRef<Path>,
    ) -> Result<(), MultiagentEvalBaselineError> {
        let path = path.as_ref();
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent).map_err(|source| MultiagentEvalBaselineError::Io {
                path: parent.display().to_string(),
                source,
            })?;
        }
        let json = serde_json::to_string_pretty(&self.baseline_json()).map_err(|source| {
            MultiagentEvalBaselineError::Json {
                path: path.display().to_string(),
                source,
            }
        })?;
        fs::write(path, format!("{json}\n")).map_err(|source| MultiagentEvalBaselineError::Io {
            path: path.display().to_string(),
            source,
        })
    }

    pub fn read_baseline(
        path: impl AsRef<Path>,
    ) -> Result<serde_json::Value, MultiagentEvalBaselineError> {
        let path = path.as_ref();
        let text = fs::read_to_string(path).map_err(|source| MultiagentEvalBaselineError::Io {
            path: path.display().to_string(),
            source,
        })?;
        serde_json::from_str(&text).map_err(|source| MultiagentEvalBaselineError::Json {
            path: path.display().to_string(),
            source,
        })
    }

    pub fn compare_to_baseline(
        &self,
        baseline: &serde_json::Value,
    ) -> MultiagentEvalBaselineComparison {
        compare_multiagent_suite_to_baseline(self, baseline)
    }

    pub fn compare_to_baseline_file(
        &self,
        path: impl AsRef<Path>,
    ) -> Result<MultiagentEvalBaselineComparison, MultiagentEvalBaselineError> {
        let baseline = Self::read_baseline(path)?;
        Ok(self.compare_to_baseline(&baseline))
    }

    pub fn is_success(&self) -> bool {
        self.failed == 0
    }
}

impl MultiagentEvalBaselineComparison {
    pub fn public_metadata(&self) -> serde_json::Value {
        serde_json::json!({
            "status": eval_gate_status_name(self.status),
            "previousScore": self.previous_score,
            "currentScore": self.current_score,
            "scoreDelta": i16::from(self.current_score) - i16::from(self.previous_score),
            "regressions": self.regressions,
            "improvements": self.improvements,
        })
    }
}

impl PromptBatteryCase {
    fn to_multiagent_eval_case(&self) -> MultiagentEvalCase {
        MultiagentEvalCase {
            name: self.id.clone(),
            goal: self.prompt.clone(),
            expected_members: self.expected_members.clone(),
            min_hardness_score: 100,
            max_blocked: 0,
            max_awaiting_approval: usize::MAX,
            validate_execution_reducer: false,
        }
    }

    pub fn public_metadata(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id,
            "stack": self.stack,
            "knowledgeArea": self.knowledge_area,
            "prompt": self.prompt,
            "expectedMembers": self.expected_members,
        })
    }
}

impl PromptBatteryReport {
    pub fn is_success(&self) -> bool {
        self.failed == 0
    }

    pub fn public_metadata(&self) -> serde_json::Value {
        serde_json::json!({
            "promptCount": self.prompt_count,
            "stackCount": self.stack_count,
            "knowledgeAreaCount": self.knowledge_area_count,
            "passed": self.passed,
            "failed": self.failed,
            "score": self.score,
            "memberCoverage": self.member_coverage,
            "failures": self.failures.iter().map(PromptBatteryFailure::public_metadata).collect::<Vec<_>>(),
        })
    }
}

impl PromptBatteryFailure {
    pub fn public_metadata(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id,
            "stack": self.stack,
            "knowledgeArea": self.knowledge_area,
            "failures": self.failures,
        })
    }
}

pub fn default_prompt_battery_cases() -> Vec<PromptBatteryCase> {
    let mut cases =
        Vec::with_capacity(PROMPT_BATTERY_STACKS.len() * PROMPT_BATTERY_SCENARIOS.len());

    for stack in PROMPT_BATTERY_STACKS {
        for scenario in PROMPT_BATTERY_SCENARIOS {
            cases.push(PromptBatteryCase {
                id: format!("{}:{}", stack.key, scenario.key),
                stack: stack.key.to_string(),
                knowledge_area: scenario.knowledge_area.to_string(),
                prompt: scenario.template.replace("{stack}", stack.label),
                expected_members: scenario
                    .expected_members
                    .iter()
                    .map(|member| (*member).to_string())
                    .collect(),
            });
        }
    }

    cases
}

pub fn run_default_prompt_battery() -> PromptBatteryReport {
    let runner = MultiagentEvalRunner::default();
    let cases = default_prompt_battery_cases();
    run_prompt_battery(&runner, &cases)
}

pub fn run_prompt_battery(
    runner: &MultiagentEvalRunner,
    cases: &[PromptBatteryCase],
) -> PromptBatteryReport {
    let mut passed = 0_usize;
    let mut failures = Vec::new();
    let mut score_total = 0_usize;
    let mut member_coverage = BTreeMap::<String, usize>::new();

    for case in cases {
        let report = runner.run_case(&case.to_multiagent_eval_case());
        score_total += usize::from(report.score);

        for member in &report.team_plan.members {
            *member_coverage.entry(member.name.clone()).or_default() += 1;
        }

        if report.status == EvalStatus::Passed {
            passed += 1;
        } else {
            failures.push(PromptBatteryFailure {
                id: case.id.clone(),
                stack: case.stack.clone(),
                knowledge_area: case.knowledge_area.clone(),
                failures: report.failures,
            });
        }
    }

    let prompt_count = cases.len();
    let failed = prompt_count.saturating_sub(passed);
    let score = if prompt_count == 0 {
        100
    } else {
        (score_total / prompt_count) as u8
    };

    PromptBatteryReport {
        prompt_count,
        stack_count: unique_case_field_count(cases, |case| &case.stack),
        knowledge_area_count: unique_case_field_count(cases, |case| &case.knowledge_area),
        passed,
        failed,
        score,
        member_coverage,
        failures,
    }
}

impl EvalQualityGate {
    pub fn strict() -> Self {
        Self {
            minimum_score: 100,
            max_failed_cases: 0,
        }
    }

    pub fn new(minimum_score: u8, max_failed_cases: usize) -> Self {
        Self {
            minimum_score: minimum_score.min(100),
            max_failed_cases,
        }
    }

    pub fn evaluate(&self, suite: &EvalSuiteReport) -> EvalGateReport {
        let mut failures = Vec::new();

        if suite.score < self.minimum_score {
            failures.push(format!(
                "suite score {} is below required minimum {}",
                suite.score, self.minimum_score
            ));
        }

        if suite.failed > self.max_failed_cases {
            failures.push(format!(
                "suite has {} failed cases, above allowed maximum {}",
                suite.failed, self.max_failed_cases
            ));

            for report in suite
                .reports
                .iter()
                .filter(|report| report.status == EvalStatus::Failed)
            {
                failures.push(format!(
                    "{} failed with score {}: {}",
                    report.case_name,
                    report.score,
                    report.failures.join("; ")
                ));
            }
        }

        EvalGateReport {
            status: if failures.is_empty() {
                EvalGateStatus::Passed
            } else {
                EvalGateStatus::Failed
            },
            suite_score: suite.score,
            minimum_score: self.minimum_score,
            failed_cases: suite.failed,
            max_failed_cases: self.max_failed_cases,
            failures,
        }
    }
}

impl EvalCase {
    pub fn new(
        name: impl Into<String>,
        goal: impl Into<String>,
        plan: Vec<DeterministicPlanItem>,
        approvals: Vec<PermissionReply>,
        expectations: EvalExpectations,
    ) -> Self {
        Self {
            name: name.into(),
            goal: goal.into(),
            plan,
            approvals,
            expectations,
        }
    }
}

impl EvalExpectations {
    pub fn final_status(final_status: DeterministicPlanStatus) -> Self {
        Self {
            final_status,
            approvals_requested: 0,
            required_observation_substrings: Vec::new(),
            required_error_codes: Vec::new(),
        }
    }

    pub fn approvals_requested(mut self, approvals_requested: usize) -> Self {
        self.approvals_requested = approvals_requested;
        self
    }

    pub fn observation_contains(mut self, value: impl Into<String>) -> Self {
        self.required_observation_substrings.push(value.into());
        self
    }

    pub fn error_code(mut self, value: impl Into<String>) -> Self {
        self.required_error_codes.push(value.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EvalCaseEvaluation {
    failures: Vec<String>,
    total_checks: usize,
}

impl EvalCaseEvaluation {
    fn score(&self) -> u8 {
        if self.total_checks == 0 {
            return 100;
        }
        let passed = self.total_checks.saturating_sub(self.failures.len());
        ((passed * 100) / self.total_checks) as u8
    }
}

fn evaluate_expectations(
    case: &EvalCase,
    report: &DeterministicPlanReport,
    approvals_requested: usize,
) -> EvalCaseEvaluation {
    let mut failures = Vec::new();
    let mut total_checks = 0_usize;

    total_checks += 1;
    if report.status != case.expectations.final_status {
        failures.push(format!(
            "expected final status {:?}, got {:?}",
            case.expectations.final_status, report.status
        ));
    }

    total_checks += 1;
    if approvals_requested != case.expectations.approvals_requested {
        failures.push(format!(
            "expected {} approval requests, got {approvals_requested}",
            case.expectations.approvals_requested
        ));
    }

    for expected in &case.expectations.required_observation_substrings {
        total_checks += 1;
        if !report
            .state
            .observations
            .iter()
            .any(|observation| observation.text.contains(expected))
        {
            failures.push(format!("missing observation substring: {expected}"));
        }
    }

    for expected in &case.expectations.required_error_codes {
        total_checks += 1;
        if !report
            .state
            .observations
            .iter()
            .any(|observation| observation.error_code.as_deref() == Some(expected.as_str()))
        {
            failures.push(format!("missing error code: {expected}"));
        }
    }

    EvalCaseEvaluation {
        failures,
        total_checks,
    }
}

fn suite_score(reports: &[EvalReport]) -> u8 {
    if reports.is_empty() {
        return 100;
    }
    let total = reports
        .iter()
        .map(|report| usize::from(report.score))
        .sum::<usize>();
    (total / reports.len()) as u8
}

fn evaluate_multiagent_case(
    case: &MultiagentEvalCase,
    team_plan: &SubagentTeamPlan,
) -> Vec<String> {
    let mut failures = Vec::new();
    let member_names = team_plan
        .members
        .iter()
        .map(|member| member.name.as_str())
        .collect::<Vec<_>>();

    for expected in &case.expected_members {
        if !member_names.iter().any(|name| *name == expected) {
            failures.push(format!("missing expected subagent member: {expected}"));
        }
    }

    if team_plan.metrics.hardness_score < case.min_hardness_score {
        failures.push(format!(
            "hardness score {} is below required minimum {}",
            team_plan.metrics.hardness_score, case.min_hardness_score
        ));
    }
    if team_plan.metrics.blocked > case.max_blocked {
        failures.push(format!(
            "blocked members {} exceed allowed maximum {}",
            team_plan.metrics.blocked, case.max_blocked
        ));
    }
    if team_plan.metrics.awaiting_approval > case.max_awaiting_approval {
        failures.push(format!(
            "awaiting approval members {} exceed allowed maximum {}",
            team_plan.metrics.awaiting_approval, case.max_awaiting_approval
        ));
    }

    failures
}

fn evaluate_execution_summary(
    summary: &SubagentExecutionSummary,
    expected_total: usize,
) -> Vec<String> {
    let mut failures = Vec::new();

    if summary.total != expected_total {
        failures.push(format!(
            "execution reducer total {} does not match expected {}",
            summary.total, expected_total
        ));
    }
    if summary.completed != expected_total {
        failures.push(format!(
            "execution reducer completed {} does not match expected {}",
            summary.completed, expected_total
        ));
    }
    if summary.failed > 0 {
        failures.push(format!(
            "execution reducer reported {} failed subagent outputs",
            summary.failed
        ));
    }
    if summary.blocked > 0 || summary.awaiting_approval > 0 {
        failures.push(format!(
            "execution reducer left {} blocked and {} awaiting approval handoffs",
            summary.blocked, summary.awaiting_approval
        ));
    }
    if summary.rejected_outputs > 0 || summary.missing_outputs > 0 {
        failures.push(format!(
            "execution reducer rejected {} outputs and missed {} outputs",
            summary.rejected_outputs, summary.missing_outputs
        ));
    }
    if summary.accepted_outputs != expected_total || !summary.unexpected_outputs.is_empty() {
        failures.push(format!(
            "execution reducer accepted {} outputs for expected {} with unexpected outputs: {}",
            summary.accepted_outputs,
            expected_total,
            summary.unexpected_outputs.join(", ")
        ));
    }

    failures
}

fn synthetic_valid_subagent_output(handoff: &SubagentHandoffPlan) -> serde_json::Value {
    let fields = handoff
        .output_schema
        .get("required")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str);
    let mut object = serde_json::Map::new();

    for field in fields {
        object.insert(
            field.to_string(),
            serde_json::Value::String("ok".to_string()),
        );
    }

    serde_json::Value::Object(object)
}

fn multiagent_case_score(failures: &[String], total_checks: usize) -> u8 {
    if total_checks == 0 {
        return 100;
    }
    let passed = total_checks.saturating_sub(failures.len());
    ((passed * 100) / total_checks) as u8
}

fn multiagent_suite_score(reports: &[MultiagentEvalReport]) -> u8 {
    if reports.is_empty() {
        return 100;
    }
    let total = reports
        .iter()
        .map(|report| usize::from(report.score))
        .sum::<usize>();
    (total / reports.len()) as u8
}

fn unique_case_field_count<'a>(
    cases: &'a [PromptBatteryCase],
    field: impl Fn(&'a PromptBatteryCase) -> &'a str,
) -> usize {
    cases.iter().map(field).collect::<HashSet<_>>().len()
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MultiagentCaseSummary {
    status: EvalStatus,
    score: u8,
}

fn compare_multiagent_suite_to_baseline(
    current: &MultiagentEvalSuiteReport,
    baseline: &serde_json::Value,
) -> MultiagentEvalBaselineComparison {
    let baseline_suite = baseline.get("suite").unwrap_or(baseline);
    let previous_score = u8_field(baseline_suite, "score").unwrap_or(0);
    let current_score = current.score;
    let previous_cases = baseline_case_summaries(baseline_suite);
    let current_cases = current_case_summaries(current);
    let mut regressions = Vec::new();
    let mut improvements = Vec::new();

    if current_score < previous_score {
        regressions.push(format!(
            "suite score dropped from {previous_score} to {current_score}"
        ));
    } else if current_score > previous_score {
        improvements.push(format!(
            "suite score improved from {previous_score} to {current_score}"
        ));
    }

    for report in &current.reports {
        let Some(previous) = previous_cases.get(&report.case_name) else {
            improvements.push(format!(
                "new case `{}` was added with status {} and score {}",
                report.case_name,
                eval_status_name(&report.status),
                report.score
            ));
            continue;
        };

        if previous.status == EvalStatus::Passed && report.status == EvalStatus::Failed {
            regressions.push(format!(
                "`{}` regressed from passed to failed",
                report.case_name
            ));
        } else if previous.status == EvalStatus::Failed && report.status == EvalStatus::Passed {
            improvements.push(format!(
                "`{}` improved from failed to passed",
                report.case_name
            ));
        }

        if report.score < previous.score {
            regressions.push(format!(
                "`{}` score dropped from {} to {}",
                report.case_name, previous.score, report.score
            ));
        } else if report.score > previous.score {
            improvements.push(format!(
                "`{}` score improved from {} to {}",
                report.case_name, previous.score, report.score
            ));
        }
    }

    let current_names = current_cases.keys().cloned().collect::<HashSet<_>>();
    for name in previous_cases.keys() {
        if !current_names.contains(name) {
            regressions.push(format!(
                "baseline case `{name}` is missing from current suite"
            ));
        }
    }

    MultiagentEvalBaselineComparison {
        status: if regressions.is_empty() {
            EvalGateStatus::Passed
        } else {
            EvalGateStatus::Failed
        },
        previous_score,
        current_score,
        regressions,
        improvements,
    }
}

fn baseline_case_summaries(value: &serde_json::Value) -> HashMap<String, MultiagentCaseSummary> {
    value
        .get("reports")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|report| {
            let name = report.get("caseName")?.as_str()?.to_string();
            let status = eval_status_from_name(report.get("status")?.as_str()?)?;
            let score = u8_field(report, "score").unwrap_or(0);
            Some((name, MultiagentCaseSummary { status, score }))
        })
        .collect()
}

fn current_case_summaries(
    suite: &MultiagentEvalSuiteReport,
) -> HashMap<String, MultiagentCaseSummary> {
    suite
        .reports
        .iter()
        .map(|report| {
            (
                report.case_name.clone(),
                MultiagentCaseSummary {
                    status: report.status.clone(),
                    score: report.score,
                },
            )
        })
        .collect()
}

fn u8_field(value: &serde_json::Value, field: &str) -> Option<u8> {
    value
        .get(field)?
        .as_u64()
        .and_then(|value| u8::try_from(value).ok())
}

fn eval_status_from_name(value: &str) -> Option<EvalStatus> {
    match value {
        "passed" => Some(EvalStatus::Passed),
        "failed" => Some(EvalStatus::Failed),
        _ => None,
    }
}

fn eval_status_name(status: &EvalStatus) -> &'static str {
    match status {
        EvalStatus::Passed => "passed",
        EvalStatus::Failed => "failed",
    }
}

fn eval_gate_status_name(status: EvalGateStatus) -> &'static str {
    match status {
        EvalGateStatus::Passed => "passed",
        EvalGateStatus::Failed => "failed",
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use coddy_core::{ToolName, ToolResultStatus};
    use serde_json::json;

    use crate::{PREVIEW_EDIT_TOOL, READ_FILE_TOOL, SHELL_RUN_TOOL};

    use super::*;

    struct TempWorkspace {
        path: PathBuf,
    }

    impl TempWorkspace {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("coddy-eval-test-{}", Uuid::new_v4()));
            fs::create_dir_all(&path).expect("create temp workspace");
            Self { path }
        }

        fn write(&self, relative_path: &str, content: &str) {
            let path = self.path.join(relative_path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("create parent directory");
            }
            fs::write(path, content).expect("write fixture file");
        }
    }

    impl Drop for TempWorkspace {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn item(description: &str, tool_name: &str, input: serde_json::Value) -> DeterministicPlanItem {
        DeterministicPlanItem::new(
            description,
            ToolName::new(tool_name).expect("tool name"),
            input,
        )
    }

    #[test]
    fn read_only_eval_passes() {
        let workspace = TempWorkspace::new();
        workspace.write("README.md", "# Coddy\n");
        let runner = EvalRunner::new(&workspace.path).expect("runner");
        let case = EvalCase::new(
            "read-only",
            "read project docs",
            vec![item(
                "Read README",
                READ_FILE_TOOL,
                json!({ "path": "README.md" }),
            )],
            Vec::new(),
            EvalExpectations::final_status(DeterministicPlanStatus::Completed)
                .observation_contains("# Coddy"),
        );

        let report = runner.run_case(&case);

        assert_eq!(report.status, EvalStatus::Passed);
        assert_eq!(report.score, 100);
        assert_eq!(report.approvals_requested, 0);
    }

    #[test]
    fn shell_approval_eval_passes_after_reply() {
        let workspace = TempWorkspace::new();
        let runner = EvalRunner::new(&workspace.path).expect("runner");
        let case = EvalCase::new(
            "shell-approval",
            "run approved shell",
            vec![item(
                "Print marker",
                SHELL_RUN_TOOL,
                json!({ "command": "printf coddy" }),
            )],
            vec![PermissionReply::Once],
            EvalExpectations::final_status(DeterministicPlanStatus::Completed)
                .approvals_requested(1)
                .observation_contains("coddy"),
        );

        let report = runner.run_case(&case);

        assert_eq!(report.status, EvalStatus::Passed);
        assert_eq!(report.approvals_requested, 1);
    }

    #[test]
    fn shell_block_eval_passes_with_denied_observation() {
        let workspace = TempWorkspace::new();
        let runner = EvalRunner::new(&workspace.path).expect("runner");
        let case = EvalCase::new(
            "shell-block",
            "block destructive shell",
            vec![item(
                "Try destructive command",
                SHELL_RUN_TOOL,
                json!({ "command": "rm -rf target" }),
            )],
            Vec::new(),
            EvalExpectations::final_status(DeterministicPlanStatus::Failed)
                .error_code("command_blocked"),
        );

        let report = runner.run_case(&case);

        assert_eq!(report.status, EvalStatus::Passed);
        assert_eq!(
            report
                .plan_report
                .state
                .observations
                .last()
                .expect("observation")
                .status,
            ToolResultStatus::Denied
        );
    }

    #[test]
    fn edit_approval_eval_applies_change() {
        let workspace = TempWorkspace::new();
        workspace.write("README.md", "Coddy REPL\n");
        let runner = EvalRunner::new(&workspace.path).expect("runner");
        let case = EvalCase::new(
            "edit-approval",
            "edit project docs",
            vec![
                item(
                    "Read README before edit",
                    READ_FILE_TOOL,
                    json!({ "path": "README.md" }),
                ),
                item(
                    "Preview README edit",
                    PREVIEW_EDIT_TOOL,
                    json!({
                        "path": "README.md",
                        "old_string": "Coddy",
                        "new_string": "Coddy Agent"
                    }),
                ),
            ],
            vec![PermissionReply::Once],
            EvalExpectations::final_status(DeterministicPlanStatus::Completed)
                .approvals_requested(1)
                .observation_contains("Edit applied: README.md"),
        );

        let report = runner.run_case(&case);

        assert_eq!(report.status, EvalStatus::Passed);
        assert_eq!(
            fs::read_to_string(workspace.path.join("README.md")).expect("read edited file"),
            "Coddy Agent REPL\n"
        );
    }

    #[test]
    fn edit_reject_eval_preserves_file() {
        let workspace = TempWorkspace::new();
        workspace.write("README.md", "Coddy REPL\n");
        let runner = EvalRunner::new(&workspace.path).expect("runner");
        let case = EvalCase::new(
            "edit-reject",
            "reject project docs edit",
            vec![
                item(
                    "Read README before edit",
                    READ_FILE_TOOL,
                    json!({ "path": "README.md" }),
                ),
                item(
                    "Preview README edit",
                    PREVIEW_EDIT_TOOL,
                    json!({
                        "path": "README.md",
                        "old_string": "Coddy",
                        "new_string": "Coddy Agent"
                    }),
                ),
            ],
            vec![PermissionReply::Reject],
            EvalExpectations::final_status(DeterministicPlanStatus::Failed)
                .approvals_requested(1)
                .error_code("permission_rejected"),
        );

        let report = runner.run_case(&case);

        assert_eq!(report.status, EvalStatus::Passed);
        assert_eq!(
            fs::read_to_string(workspace.path.join("README.md")).expect("read preserved file"),
            "Coddy REPL\n"
        );
    }

    #[test]
    fn suite_counts_passed_and_failed_cases() {
        let workspace = TempWorkspace::new();
        workspace.write("README.md", "# Coddy\n");
        let runner = EvalRunner::new(&workspace.path).expect("runner");
        let passing = EvalCase::new(
            "passing",
            "read docs",
            vec![item(
                "Read README",
                READ_FILE_TOOL,
                json!({ "path": "README.md" }),
            )],
            Vec::new(),
            EvalExpectations::final_status(DeterministicPlanStatus::Completed),
        );
        let failing = EvalCase::new(
            "failing",
            "expect wrong status",
            vec![item(
                "Read README",
                READ_FILE_TOOL,
                json!({ "path": "README.md" }),
            )],
            Vec::new(),
            EvalExpectations::final_status(DeterministicPlanStatus::Failed),
        );

        let suite = runner.run_suite(&[passing, failing]);

        assert_eq!(suite.passed, 1);
        assert_eq!(suite.failed, 1);
        assert_eq!(suite.score, 75);
        assert!(!suite.is_success());
        assert!(suite.passes_score_threshold(75));
        assert!(!suite.passes_score_threshold(76));
    }

    #[test]
    fn quality_gate_reports_score_and_case_regressions() {
        let workspace = TempWorkspace::new();
        workspace.write("README.md", "# Coddy\n");
        let runner = EvalRunner::new(&workspace.path).expect("runner");
        let passing = EvalCase::new(
            "passing",
            "read docs",
            vec![item(
                "Read README",
                READ_FILE_TOOL,
                json!({ "path": "README.md" }),
            )],
            Vec::new(),
            EvalExpectations::final_status(DeterministicPlanStatus::Completed),
        );
        let failing = EvalCase::new(
            "failing",
            "expect wrong status",
            vec![item(
                "Read README",
                READ_FILE_TOOL,
                json!({ "path": "README.md" }),
            )],
            Vec::new(),
            EvalExpectations::final_status(DeterministicPlanStatus::Failed),
        );
        let suite = runner.run_suite(&[passing, failing]);

        let report = suite.evaluate_gate(EvalQualityGate::strict());

        assert_eq!(report.status, EvalGateStatus::Failed);
        assert_eq!(report.suite_score, 75);
        assert_eq!(report.minimum_score, 100);
        assert_eq!(report.failed_cases, 1);
        assert_eq!(report.max_failed_cases, 0);
        assert!(report
            .failures
            .iter()
            .any(|failure| failure.contains("suite score 75 is below required minimum 100")));
        assert!(report
            .failures
            .iter()
            .any(|failure| failure.contains("suite has 1 failed cases, above allowed maximum 0")));
        assert!(report
            .failures
            .iter()
            .any(|failure| failure.contains("failing failed with score 50")));
    }

    #[test]
    fn quality_gate_can_allow_non_blocking_known_failures() {
        let workspace = TempWorkspace::new();
        workspace.write("README.md", "# Coddy\n");
        let runner = EvalRunner::new(&workspace.path).expect("runner");
        let passing = EvalCase::new(
            "passing",
            "read docs",
            vec![item(
                "Read README",
                READ_FILE_TOOL,
                json!({ "path": "README.md" }),
            )],
            Vec::new(),
            EvalExpectations::final_status(DeterministicPlanStatus::Completed),
        );
        let known_gap = EvalCase::new(
            "known-gap",
            "documented non-blocking gap",
            vec![item(
                "Read README",
                READ_FILE_TOOL,
                json!({ "path": "README.md" }),
            )],
            Vec::new(),
            EvalExpectations::final_status(DeterministicPlanStatus::Failed),
        );
        let suite = runner.run_suite(&[passing, known_gap]);

        let report = suite.evaluate_gate(EvalQualityGate::new(75, 1));

        assert_eq!(report.status, EvalGateStatus::Passed);
        assert!(report.failures.is_empty());
    }

    #[test]
    fn multiagent_eval_runner_scores_expected_team_plan() {
        let runner = MultiagentEvalRunner::default();
        let case = MultiagentEvalCase::new(
            "hardness-multiagent",
            "revise, aprimore e teste multiagents, harness, prompts e metricas",
        )
        .expected_members(&[
            "explorer",
            "coder",
            "test-writer",
            "eval-runner",
            "reviewer",
        ])
        .min_hardness_score(100)
        .max_blocked(0);

        let report = runner.run_case(&case);

        assert_eq!(report.status, EvalStatus::Passed);
        assert_eq!(report.score, 100);
        assert_eq!(report.team_plan.metrics.hardness_score, 100);
        assert_eq!(report.team_plan.metrics.blocked, 0);
        assert!(report.failures.is_empty());
        assert_eq!(
            report.public_metadata()["teamPlan"]["metrics"]["hardnessScore"],
            json!(100)
        );
    }

    #[test]
    fn multiagent_eval_runner_validates_execution_reducer_contracts() {
        let runner = MultiagentEvalRunner::default();
        let case = MultiagentEvalCase::new(
            "execution-reducer-contracts",
            "revise, aprimore e teste multiagents, harness, prompts e metricas",
        )
        .expected_members(&[
            "explorer",
            "coder",
            "test-writer",
            "eval-runner",
            "reviewer",
        ])
        .min_hardness_score(100)
        .max_blocked(0)
        .validate_execution_reducer();

        let report = runner.run_case(&case);
        let execution_metrics = report
            .execution_metrics
            .as_ref()
            .expect("execution metrics");

        assert_eq!(report.status, EvalStatus::Passed);
        assert_eq!(report.score, 100);
        assert_eq!(execution_metrics.total, report.team_plan.members.len());
        assert_eq!(execution_metrics.completed, report.team_plan.members.len());
        assert_eq!(
            execution_metrics.accepted_outputs,
            report.team_plan.members.len()
        );
        assert_eq!(execution_metrics.failed, 0);
        assert_eq!(execution_metrics.rejected_outputs, 0);
        assert_eq!(execution_metrics.missing_outputs, 0);
        assert!(execution_metrics.unexpected_outputs.is_empty());
        assert_eq!(
            report.public_metadata()["executionMetrics"]["acceptedOutputs"],
            json!(report.team_plan.members.len())
        );
    }

    #[test]
    fn multiagent_eval_runner_reports_missing_members() {
        let runner = MultiagentEvalRunner::default();
        let case = MultiagentEvalCase::new("security-only", "revise seguranca, secrets e sandbox")
            .expected_members(&["security-reviewer", "docs-writer"])
            .min_hardness_score(100)
            .max_blocked(0)
            .max_awaiting_approval(0);

        let report = runner.run_case(&case);

        assert_eq!(report.status, EvalStatus::Failed);
        assert!(report.score < 100);
        assert!(report
            .failures
            .iter()
            .any(|failure| failure.contains("missing expected subagent member: docs-writer")));
    }

    #[test]
    fn multiagent_eval_suite_aggregates_scores_and_metadata() {
        let runner = MultiagentEvalRunner::default();
        let passing = MultiagentEvalCase::new(
            "hardness-multiagent",
            "revise, aprimore e teste multiagents, harness, prompts e metricas",
        )
        .expected_members(&["explorer", "coder", "test-writer", "eval-runner"])
        .min_hardness_score(100)
        .max_blocked(0);
        let failing = MultiagentEvalCase::new("missing-docs-writer", "revise seguranca")
            .expected_members(&["docs-writer"])
            .max_awaiting_approval(0);

        let suite = runner.run_suite(&[passing, failing]);

        assert_eq!(suite.passed, 1);
        assert_eq!(suite.failed, 1);
        assert!(suite.score < 100);
        assert!(!suite.is_success());
        assert_eq!(suite.public_metadata()["passed"], json!(1));
        assert_eq!(suite.public_metadata()["failed"], json!(1));
    }

    #[test]
    fn multiagent_baseline_persistence_serializes_and_deserializes() {
        let workspace = TempWorkspace::new();
        let suite = passing_multiagent_suite();
        let baseline_path = workspace.path.join("evals/multiagent-baseline.json");

        suite
            .write_baseline(&baseline_path)
            .expect("write baseline");
        let baseline =
            MultiagentEvalSuiteReport::read_baseline(&baseline_path).expect("read baseline");
        let comparison = suite.compare_to_baseline(&baseline);

        assert_eq!(baseline["kind"], json!("coddy.multiagentEvalBaseline"));
        assert_eq!(baseline["version"], json!(1));
        assert_eq!(baseline["suite"]["score"], json!(100));
        assert_eq!(comparison.status, EvalGateStatus::Passed);
        assert!(comparison.regressions.is_empty());
        assert!(comparison.improvements.is_empty());
    }

    #[test]
    fn multiagent_baseline_comparison_detects_score_drop_and_failed_case() {
        let baseline = passing_multiagent_suite().baseline_json();
        let current = failing_multiagent_suite_with_same_case_name();

        let comparison = current.compare_to_baseline(&baseline);

        assert_eq!(comparison.status, EvalGateStatus::Failed);
        assert_eq!(comparison.previous_score, 100);
        assert!(comparison.current_score < 100);
        assert!(comparison
            .regressions
            .iter()
            .any(|regression| regression.contains("suite score dropped from 100")));
        assert!(comparison
            .regressions
            .iter()
            .any(|regression| regression.contains("`hardness-multiagent` regressed")));
        assert!(comparison
            .regressions
            .iter()
            .any(|regression| regression.contains("`hardness-multiagent` score dropped")));
    }

    #[test]
    fn multiagent_baseline_comparison_reports_missing_baseline_cases() {
        let baseline = passing_multiagent_suite().baseline_json();
        let runner = MultiagentEvalRunner::default();
        let current = runner.run_suite(&[]);

        let comparison = current.compare_to_baseline(&baseline);

        assert_eq!(comparison.status, EvalGateStatus::Failed);
        assert!(comparison
            .regressions
            .iter()
            .any(|regression| regression.contains("baseline case `hardness-multiagent`")));
    }

    #[test]
    fn multiagent_baseline_comparison_reports_improvements() {
        let baseline = failing_multiagent_suite_with_same_case_name().baseline_json();
        let current = passing_multiagent_suite();

        let comparison = current.compare_to_baseline(&baseline);

        assert_eq!(comparison.status, EvalGateStatus::Passed);
        assert!(comparison.regressions.is_empty());
        assert!(comparison
            .improvements
            .iter()
            .any(|improvement| improvement.contains("suite score improved")));
        assert!(comparison
            .improvements
            .iter()
            .any(|improvement| improvement.contains("`hardness-multiagent` improved")));
    }

    #[test]
    fn multiagent_baseline_comparison_projects_frontend_metadata() {
        let baseline = passing_multiagent_suite().baseline_json();
        let current = failing_multiagent_suite_with_same_case_name();

        let metadata = current.compare_to_baseline(&baseline).public_metadata();

        assert_eq!(metadata["status"], json!("failed"));
        assert_eq!(metadata["previousScore"], json!(100));
        assert!(metadata["currentScore"].as_u64().expect("current score") < 100);
        assert!(metadata["scoreDelta"].as_i64().expect("score delta") < 0);
        assert!(metadata["regressions"]
            .as_array()
            .expect("regressions")
            .iter()
            .any(|regression| regression
                .as_str()
                .expect("regression")
                .contains("hardness-multiagent")));
    }

    #[test]
    fn default_prompt_battery_contains_300_diverse_prompts() {
        let cases = default_prompt_battery_cases();
        let stack_count = cases
            .iter()
            .map(|case| case.stack.as_str())
            .collect::<HashSet<_>>()
            .len();
        let knowledge_area_count = cases
            .iter()
            .map(|case| case.knowledge_area.as_str())
            .collect::<HashSet<_>>()
            .len();

        assert_eq!(cases.len(), 300);
        assert_eq!(stack_count, 30);
        assert_eq!(knowledge_area_count, 10);
        assert!(cases
            .iter()
            .any(|case| case.id == "rust:implementation-tdd"));
        assert!(cases
            .iter()
            .any(|case| case.id == "gcp:security-threat-model"));
        assert!(cases
            .iter()
            .any(|case| case.id == "embedded:low-level-reliability"));
    }

    #[test]
    fn default_prompt_battery_routes_all_cases_successfully() {
        let report = run_default_prompt_battery();

        assert!(report.is_success());
        assert_eq!(report.prompt_count, 300);
        assert_eq!(report.stack_count, 30);
        assert_eq!(report.knowledge_area_count, 10);
        assert_eq!(report.passed, 300);
        assert_eq!(report.failed, 0);
        assert_eq!(report.score, 100);

        for member in [
            "explorer",
            "planner",
            "coder",
            "reviewer",
            "security-reviewer",
            "test-writer",
            "eval-runner",
            "docs-writer",
        ] {
            assert!(
                report
                    .member_coverage
                    .get(member)
                    .copied()
                    .unwrap_or_default()
                    > 0,
                "expected member coverage for {member}",
            );
        }
    }

    #[test]
    fn prompt_battery_projects_frontend_ready_metadata() {
        let report = run_default_prompt_battery();
        let metadata = report.public_metadata();

        assert_eq!(metadata["promptCount"], json!(300));
        assert_eq!(metadata["stackCount"], json!(30));
        assert_eq!(metadata["knowledgeAreaCount"], json!(10));
        assert_eq!(metadata["failed"], json!(0));
        assert_eq!(metadata["score"], json!(100));
        assert!(
            metadata["memberCoverage"]["coder"]
                .as_u64()
                .expect("coder coverage")
                > 0
        );
    }

    fn passing_multiagent_suite() -> MultiagentEvalSuiteReport {
        let runner = MultiagentEvalRunner::default();
        let case = MultiagentEvalCase::new(
            "hardness-multiagent",
            "revise, aprimore e teste multiagents, harness, prompts e metricas",
        )
        .expected_members(&[
            "explorer",
            "coder",
            "test-writer",
            "eval-runner",
            "reviewer",
        ])
        .min_hardness_score(100)
        .max_blocked(0);

        runner.run_suite(&[case])
    }

    fn failing_multiagent_suite_with_same_case_name() -> MultiagentEvalSuiteReport {
        let runner = MultiagentEvalRunner::default();
        let case = MultiagentEvalCase::new("hardness-multiagent", "revise seguranca")
            .expected_members(&["docs-writer"])
            .min_hardness_score(100)
            .max_blocked(0)
            .max_awaiting_approval(0);

        runner.run_suite(&[case])
    }
}
