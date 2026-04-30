use serde_json::{json, Value};

use crate::{
    APPLY_EDIT_TOOL, LIST_FILES_TOOL, PREVIEW_EDIT_TOOL, READ_FILE_TOOL, SEARCH_FILES_TOOL,
    SHELL_RUN_TOOL,
};

pub const SUBAGENT_LIST_TOOL: &str = "subagent.list";
pub const SUBAGENT_PREPARE_TOOL: &str = "subagent.prepare";
pub const SUBAGENT_ROUTE_TOOL: &str = "subagent.route";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubagentMode {
    ReadOnly,
    WorkspaceWrite,
    Evaluation,
}

impl SubagentMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ReadOnly => "read-only",
            Self::WorkspaceWrite => "workspace-write",
            Self::Evaluation => "evaluation",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "read-only" => Some(Self::ReadOnly),
            "workspace-write" => Some(Self::WorkspaceWrite),
            "evaluation" => Some(Self::Evaluation),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SubagentDefinition {
    pub name: String,
    pub description: String,
    pub mode: SubagentMode,
    pub allowed_tools: Vec<String>,
    pub routing_signals: Vec<String>,
    pub timeout_ms: u64,
    pub max_context_tokens: u32,
    pub output_schema: Value,
}

impl SubagentDefinition {
    pub fn public_metadata(&self) -> Value {
        json!({
            "name": self.name,
            "description": self.description,
            "mode": self.mode.as_str(),
            "allowedTools": self.allowed_tools,
            "routingSignals": self.routing_signals,
            "timeoutMs": self.timeout_ms,
            "maxContextTokens": self.max_context_tokens,
            "outputSchema": self.output_schema,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SubagentRecommendation {
    pub name: String,
    pub score: u8,
    pub mode: SubagentMode,
    pub matched_signals: Vec<String>,
    pub rationale: String,
    pub allowed_tools: Vec<String>,
    pub timeout_ms: u64,
    pub max_context_tokens: u32,
    pub output_schema: Value,
}

impl SubagentRecommendation {
    pub fn public_metadata(&self) -> Value {
        json!({
            "name": self.name,
            "score": self.score,
            "mode": self.mode.as_str(),
            "matchedSignals": self.matched_signals,
            "rationale": self.rationale,
            "allowedTools": self.allowed_tools,
            "timeoutMs": self.timeout_ms,
            "maxContextTokens": self.max_context_tokens,
            "outputSchema": self.output_schema,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SubagentHandoffPlan {
    pub name: String,
    pub description: String,
    pub mode: SubagentMode,
    pub goal: String,
    pub allowed_tools: Vec<String>,
    pub timeout_ms: u64,
    pub max_context_tokens: u32,
    pub approval_required: bool,
    pub handoff_prompt: String,
    pub validation_checklist: Vec<String>,
    pub safety_notes: Vec<String>,
    pub readiness_score: u8,
    pub readiness_issues: Vec<String>,
    pub output_schema: Value,
}

impl SubagentHandoffPlan {
    pub fn public_metadata(&self) -> Value {
        json!({
            "name": self.name,
            "description": self.description,
            "mode": self.mode.as_str(),
            "goal": self.goal,
            "allowedTools": self.allowed_tools,
            "timeoutMs": self.timeout_ms,
            "maxContextTokens": self.max_context_tokens,
            "approvalRequired": self.approval_required,
            "handoffPrompt": self.handoff_prompt,
            "validationChecklist": self.validation_checklist,
            "safetyNotes": self.safety_notes,
            "readinessScore": self.readiness_score,
            "readinessIssues": self.readiness_issues,
            "outputSchema": self.output_schema,
        })
    }
}

#[derive(Debug, Clone)]
pub struct SubagentRegistry {
    definitions: Vec<SubagentDefinition>,
}

impl Default for SubagentRegistry {
    fn default() -> Self {
        Self {
            definitions: vec![
                subagent(
                    "explorer",
                    "Explore repository structure, entrypoints, tests, dependencies and risks in read-only mode",
                    SubagentMode::ReadOnly,
                    &[LIST_FILES_TOOL, READ_FILE_TOOL, SEARCH_FILES_TOOL],
                    &[
                        "explore",
                        "inspect",
                        "map",
                        "entrypoint",
                        "structure",
                        "dependency",
                        "risk",
                        "analisar",
                        "mapear",
                    ],
                    structured_report_schema(&[
                        "summary",
                        "importantFiles",
                        "entrypoints",
                        "testFiles",
                        "commands",
                        "risks",
                        "recommendations",
                    ]),
                ),
                subagent(
                    "planner",
                    "Create technical plans, divide work, identify risks and validation criteria",
                    SubagentMode::ReadOnly,
                    &[READ_FILE_TOOL, SEARCH_FILES_TOOL],
                    &[
                        "plan",
                        "roadmap",
                        "strategy",
                        "decompose",
                        "phase",
                        "validation",
                        "architecture",
                        "planejar",
                        "estrategia",
                    ],
                    structured_report_schema(&[
                        "goal",
                        "plan",
                        "risks",
                        "requiredTools",
                        "validation",
                        "approvalNeeded",
                    ]),
                ),
                subagent(
                    "coder",
                    "Implement small scoped changes while preserving compatibility and approval boundaries",
                    SubagentMode::WorkspaceWrite,
                    &[
                        READ_FILE_TOOL,
                        SEARCH_FILES_TOOL,
                        PREVIEW_EDIT_TOOL,
                        APPLY_EDIT_TOOL,
                        SHELL_RUN_TOOL,
                    ],
                    &[
                        "implement",
                        "fix",
                        "build",
                        "refactor",
                        "change",
                        "edit",
                        "feature",
                        "bug",
                        "code",
                        "continue",
                        "implementar",
                        "corrigir",
                    ],
                    structured_report_schema(&[
                        "changedFiles",
                        "summary",
                        "testsAdded",
                        "risks",
                        "nextSteps",
                    ]),
                ),
                subagent(
                    "reviewer",
                    "Review diffs, bugs, regressions, maintainability and architecture adherence",
                    SubagentMode::ReadOnly,
                    &[READ_FILE_TOOL, SEARCH_FILES_TOOL],
                    &[
                        "review",
                        "regression",
                        "quality",
                        "maintainability",
                        "diff",
                        "architecture",
                        "revise",
                        "revisar",
                    ],
                    structured_report_schema(&[
                        "approved",
                        "issues",
                        "suggestions",
                        "blockingProblems",
                        "nonBlockingProblems",
                    ]),
                ),
                subagent(
                    "security-reviewer",
                    "Assess command safety, filesystem policy, secrets exposure, MCP and prompt-injection risks",
                    SubagentMode::ReadOnly,
                    &[READ_FILE_TOOL, SEARCH_FILES_TOOL],
                    &[
                        "security",
                        "secret",
                        "sandbox",
                        "approval",
                        "prompt injection",
                        "path traversal",
                        "command",
                        "filesystem",
                        "mcp",
                        "token",
                        "seguranca",
                        "segurança",
                    ],
                    structured_report_schema(&[
                        "riskLevel",
                        "findings",
                        "requiredFixes",
                        "recommendations",
                    ]),
                ),
                subagent(
                    "test-writer",
                    "Create focused tests, fixtures and edge-case coverage for the current change",
                    SubagentMode::WorkspaceWrite,
                    &[
                        READ_FILE_TOOL,
                        SEARCH_FILES_TOOL,
                        PREVIEW_EDIT_TOOL,
                        APPLY_EDIT_TOOL,
                        SHELL_RUN_TOOL,
                    ],
                    &[
                        "test",
                        "fixture",
                        "coverage",
                        "edge case",
                        "tdd",
                        "unit",
                        "integration",
                        "regression",
                        "teste",
                        "testar",
                    ],
                    structured_report_schema(&[
                        "testsCreated",
                        "coverageFocus",
                        "edgeCases",
                        "commandsToRun",
                    ]),
                ),
                subagent(
                    "eval-runner",
                    "Run deterministic evaluations, compare scores and report regressions",
                    SubagentMode::Evaluation,
                    &[READ_FILE_TOOL, SEARCH_FILES_TOOL, SHELL_RUN_TOOL],
                    &[
                        "eval",
                        "benchmark",
                        "score",
                        "baseline",
                        "metric",
                        "regression",
                        "swe bench",
                        "harness",
                        "hardness",
                        "gate",
                        "metrica",
                        "métrica",
                    ],
                    structured_report_schema(&[
                        "score",
                        "passed",
                        "failedChecks",
                        "metrics",
                        "recommendations",
                    ]),
                ),
                subagent(
                    "docs-writer",
                    "Update README, architecture, commands, policies and usage examples",
                    SubagentMode::WorkspaceWrite,
                    &[READ_FILE_TOOL, SEARCH_FILES_TOOL, PREVIEW_EDIT_TOOL, APPLY_EDIT_TOOL],
                    &[
                        "docs",
                        "readme",
                        "documentation",
                        "guide",
                        "usage",
                        "architecture",
                        "commands",
                        "documentacao",
                        "documentação",
                    ],
                    structured_report_schema(&["docsUpdated", "sectionsAdded", "missingDocs"]),
                ),
            ],
        }
    }
}

impl SubagentRegistry {
    pub fn definitions(&self) -> &[SubagentDefinition] {
        &self.definitions
    }

    pub fn get(&self, name: &str) -> Option<&SubagentDefinition> {
        self.definitions
            .iter()
            .find(|definition| definition.name == name)
    }

    pub fn public_definitions(&self, mode: Option<SubagentMode>) -> Vec<Value> {
        self.definitions
            .iter()
            .filter(|definition| mode.is_none_or(|mode| definition.mode == mode))
            .map(SubagentDefinition::public_metadata)
            .collect()
    }

    pub fn recommend(
        &self,
        goal: &str,
        mode: Option<SubagentMode>,
        limit: usize,
    ) -> Vec<SubagentRecommendation> {
        let normalized_goal = normalize_text(goal);
        let haystack = format!(" {normalized_goal} ");
        let mut recommendations = self
            .definitions
            .iter()
            .filter(|definition| mode.is_none_or(|mode| definition.mode == mode))
            .filter_map(|definition| {
                let mut matched_signals = definition
                    .routing_signals
                    .iter()
                    .filter(|signal| {
                        let normalized_signal = normalize_text(signal);
                        !normalized_signal.is_empty()
                            && haystack.contains(&format!(" {normalized_signal} "))
                    })
                    .cloned()
                    .collect::<Vec<_>>();

                let normalized_name = normalize_text(&definition.name);
                if !normalized_name.is_empty() && haystack.contains(&format!(" {normalized_name} "))
                {
                    matched_signals.push(definition.name.clone());
                }

                if matched_signals.is_empty() {
                    return None;
                }

                Some(recommendation_from_definition(
                    definition,
                    routing_score(matched_signals.len()),
                    matched_signals,
                    "Matched the task intent against the subagent routing signals.".to_string(),
                ))
            })
            .collect::<Vec<_>>();

        recommendations.sort_by(|left, right| {
            right
                .score
                .cmp(&left.score)
                .then_with(|| left.name.cmp(&right.name))
        });

        if recommendations.is_empty() && !normalized_goal.is_empty() {
            if let Some(definition) = self.fallback_definition(mode) {
                recommendations.push(recommendation_from_definition(
                    definition,
                    20,
                    vec!["fallback:ambiguous-goal".to_string()],
                    "No strong signal matched; defaulting to the safest available planning role."
                        .to_string(),
                ));
            }
        }

        recommendations.truncate(limit.max(1).min(self.definitions.len()));
        recommendations
    }

    pub fn prepare_handoff(&self, name: &str, goal: &str) -> Option<SubagentHandoffPlan> {
        let definition = self.get(name)?;
        let goal = goal.trim();
        if goal.is_empty() {
            return None;
        }

        Some(handoff_plan_from_definition(definition, goal))
    }

    fn fallback_definition(&self, mode: Option<SubagentMode>) -> Option<&SubagentDefinition> {
        self.get("planner")
            .filter(|definition| mode.is_none_or(|mode| definition.mode == mode))
            .or_else(|| {
                self.definitions
                    .iter()
                    .find(|definition| mode.is_none_or(|mode| definition.mode == mode))
            })
    }
}

fn handoff_plan_from_definition(
    definition: &SubagentDefinition,
    goal: &str,
) -> SubagentHandoffPlan {
    let validation_checklist = validation_checklist(definition);
    let safety_notes = safety_notes(definition);
    let output_fields = required_schema_fields(&definition.output_schema);
    let readiness = handoff_readiness(
        definition,
        &validation_checklist,
        &safety_notes,
        &output_fields,
    );
    let approval_required = definition.mode != SubagentMode::ReadOnly
        || definition
            .allowed_tools
            .iter()
            .any(|tool| tool == APPLY_EDIT_TOOL || tool == SHELL_RUN_TOOL);
    let handoff_prompt = [
        format!("You are the `{}` Coddy subagent.", definition.name),
        format!("Purpose: {}", definition.description),
        format!("Mode: {}", definition.mode.as_str()),
        format!("Goal: {goal}"),
        format!("Allowed tools: {}", definition.allowed_tools.join(", ")),
        format!(
            "Context budget: {} tokens. Timeout: {} ms.",
            definition.max_context_tokens, definition.timeout_ms
        ),
        "Rules: use only the allowed tools; preserve least privilege; do not claim side effects without a successful tool observation; return structured output only.".to_string(),
        format!("Required output fields: {}", output_fields.join(", ")),
    ]
    .join("\n");

    SubagentHandoffPlan {
        name: definition.name.clone(),
        description: definition.description.clone(),
        mode: definition.mode,
        goal: goal.to_string(),
        allowed_tools: definition.allowed_tools.clone(),
        timeout_ms: definition.timeout_ms,
        max_context_tokens: definition.max_context_tokens,
        approval_required,
        handoff_prompt,
        validation_checklist,
        safety_notes,
        readiness_score: readiness.score,
        readiness_issues: readiness.issues,
        output_schema: definition.output_schema.clone(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HandoffReadiness {
    score: u8,
    issues: Vec<String>,
}

fn handoff_readiness(
    definition: &SubagentDefinition,
    validation_checklist: &[String],
    safety_notes: &[String],
    output_fields: &[String],
) -> HandoffReadiness {
    let mut issues = Vec::new();

    if definition.allowed_tools.is_empty() {
        issues.push("no allowed tools configured".to_string());
    }
    if definition.timeout_ms == 0 {
        issues.push("timeout must be greater than zero".to_string());
    }
    if definition.max_context_tokens < 1_000 {
        issues.push("context budget is below the minimum useful threshold".to_string());
    }
    if output_fields.is_empty() {
        issues.push("output schema has no required fields".to_string());
    }
    if validation_checklist.len() < 3 {
        issues.push("validation checklist is underspecified".to_string());
    }
    if safety_notes.is_empty() {
        issues.push("safety notes are missing".to_string());
    }
    if definition.mode == SubagentMode::WorkspaceWrite
        && !definition
            .allowed_tools
            .iter()
            .any(|tool| tool == PREVIEW_EDIT_TOOL)
    {
        issues.push("workspace-write handoff must include preview edit capability".to_string());
    }

    HandoffReadiness {
        score: readiness_score(issues.len()),
        issues,
    }
}

fn readiness_score(issue_count: usize) -> u8 {
    100_u8.saturating_sub((issue_count.min(5) as u8) * 20)
}

fn validation_checklist(definition: &SubagentDefinition) -> Vec<String> {
    let mut checklist = vec![
        "Confirm the task scope and constraints before acting.".to_string(),
        "Use only tools listed in allowedTools.".to_string(),
        "Return output that matches the subagent outputSchema.".to_string(),
    ];

    match definition.mode {
        SubagentMode::ReadOnly => checklist.extend([
            "Do not report file modifications as completed.".to_string(),
            "Ground findings in repository evidence when available.".to_string(),
        ]),
        SubagentMode::WorkspaceWrite => checklist.extend([
            "Read relevant files before preparing edits.".to_string(),
            "Preview edits before applying workspace changes.".to_string(),
            "Run focused validation or report why it was not run.".to_string(),
        ]),
        SubagentMode::Evaluation => checklist.extend([
            "Use deterministic checks and report score, pass/fail counts and regressions."
                .to_string(),
            "Treat command execution as guarded by the runtime approval policy.".to_string(),
        ]),
    }

    checklist
}

fn safety_notes(definition: &SubagentDefinition) -> Vec<String> {
    let mut notes = vec![
        "Do not expose secrets, credentials, tokens or hidden configuration values.".to_string(),
        "Stop and report if required context is missing or redacted.".to_string(),
    ];

    if definition
        .allowed_tools
        .iter()
        .any(|tool| tool == SHELL_RUN_TOOL)
    {
        notes.push(
            "Shell commands must remain workspace-scoped and pass the command guard.".to_string(),
        );
    }
    if definition
        .allowed_tools
        .iter()
        .any(|tool| tool == APPLY_EDIT_TOOL)
    {
        notes.push(
            "Workspace writes require preview/apply flow and explicit approval boundaries."
                .to_string(),
        );
    }

    notes
}

fn required_schema_fields(schema: &Value) -> Vec<String> {
    schema
        .get("required")
        .and_then(Value::as_array)
        .map(|fields| {
            fields
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn subagent(
    name: &str,
    description: &str,
    mode: SubagentMode,
    allowed_tools: &[&str],
    routing_signals: &[&str],
    output_schema: Value,
) -> SubagentDefinition {
    SubagentDefinition {
        name: name.to_string(),
        description: description.to_string(),
        mode,
        allowed_tools: allowed_tools
            .iter()
            .map(|tool| (*tool).to_string())
            .collect(),
        routing_signals: routing_signals
            .iter()
            .map(|signal| (*signal).to_string())
            .collect(),
        timeout_ms: 60_000,
        max_context_tokens: 8_000,
        output_schema,
    }
}

fn structured_report_schema(required_fields: &[&str]) -> Value {
    let properties = required_fields
        .iter()
        .map(|field| {
            (
                (*field).to_string(),
                json!({
                    "description": "Structured subagent response field"
                }),
            )
        })
        .collect::<serde_json::Map<_, _>>();

    json!({
        "type": "object",
        "additionalProperties": false,
        "required": required_fields,
        "properties": properties,
    })
}

fn recommendation_from_definition(
    definition: &SubagentDefinition,
    score: u8,
    matched_signals: Vec<String>,
    rationale: String,
) -> SubagentRecommendation {
    SubagentRecommendation {
        name: definition.name.clone(),
        score,
        mode: definition.mode,
        matched_signals,
        rationale,
        allowed_tools: definition.allowed_tools.clone(),
        timeout_ms: definition.timeout_ms,
        max_context_tokens: definition.max_context_tokens,
        output_schema: definition.output_schema.clone(),
    }
}

fn routing_score(matched_signal_count: usize) -> u8 {
    (40 + matched_signal_count.saturating_mul(12)).min(100) as u8
}

fn normalize_text(value: &str) -> String {
    let mut normalized = String::with_capacity(value.len());

    for character in value.chars() {
        if character.is_alphanumeric() {
            for lowercase in character.to_lowercase() {
                normalized.push(lowercase);
            }
        } else {
            normalized.push(' ');
        }
    }

    normalized.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recommends_security_reviewer_for_security_sensitive_tasks() {
        let registry = SubagentRegistry::default();

        let recommendations = registry.recommend(
            "revise seguranca, sandbox, secrets e prompt injection",
            None,
            2,
        );

        assert_eq!(recommendations[0].name, "security-reviewer");
        assert!(recommendations[0].score >= 70);
        assert!(recommendations[0]
            .allowed_tools
            .contains(&READ_FILE_TOOL.to_string()));
        assert!(!recommendations[0]
            .allowed_tools
            .contains(&APPLY_EDIT_TOOL.to_string()));
    }

    #[test]
    fn recommends_eval_runner_with_mode_filter_for_harness_metrics() {
        let registry = SubagentRegistry::default();

        let recommendations = registry.recommend(
            "run harness evals, baseline score and regression gates",
            Some(SubagentMode::Evaluation),
            3,
        );

        assert_eq!(recommendations.len(), 1);
        assert_eq!(recommendations[0].name, "eval-runner");
        assert_eq!(recommendations[0].mode, SubagentMode::Evaluation);
        assert!(recommendations[0]
            .matched_signals
            .iter()
            .any(|signal| signal == "harness"));
    }

    #[test]
    fn falls_back_to_planner_for_ambiguous_goals() {
        let registry = SubagentRegistry::default();

        let recommendations = registry.recommend("think deeply about this", None, 1);

        assert_eq!(recommendations[0].name, "planner");
        assert_eq!(recommendations[0].score, 20);
        assert_eq!(
            recommendations[0].matched_signals,
            vec!["fallback:ambiguous-goal".to_string()]
        );
    }

    #[test]
    fn prepares_handoff_with_role_tools_and_safety_contract() {
        let registry = SubagentRegistry::default();

        let handoff = registry
            .prepare_handoff("coder", "implement a focused parser fix")
            .expect("handoff");

        assert_eq!(handoff.name, "coder");
        assert_eq!(handoff.mode, SubagentMode::WorkspaceWrite);
        assert!(handoff.approval_required);
        assert_eq!(handoff.readiness_score, 100);
        assert!(handoff.readiness_issues.is_empty());
        assert!(handoff.allowed_tools.contains(&APPLY_EDIT_TOOL.to_string()));
        assert!(handoff
            .handoff_prompt
            .contains("You are the `coder` Coddy subagent."));
        assert!(handoff
            .validation_checklist
            .iter()
            .any(|item| item.contains("Preview edits")));
        assert!(handoff
            .safety_notes
            .iter()
            .any(|item| item.contains("Workspace writes require")));
        assert_eq!(
            required_schema_fields(&handoff.output_schema),
            vec![
                "changedFiles".to_string(),
                "summary".to_string(),
                "testsAdded".to_string(),
                "risks".to_string(),
                "nextSteps".to_string()
            ]
        );
    }

    #[test]
    fn rejects_unknown_or_empty_handoff_requests() {
        let registry = SubagentRegistry::default();

        assert!(registry
            .prepare_handoff("unknown", "inspect code")
            .is_none());
        assert!(registry.prepare_handoff("explorer", "   ").is_none());
    }

    #[test]
    fn scores_incomplete_handoff_contracts_as_not_ready() {
        let definition = SubagentDefinition {
            name: "unsafe-coder".to_string(),
            description: "Incomplete workspace writer".to_string(),
            mode: SubagentMode::WorkspaceWrite,
            allowed_tools: vec![APPLY_EDIT_TOOL.to_string()],
            routing_signals: vec!["unsafe".to_string()],
            timeout_ms: 0,
            max_context_tokens: 500,
            output_schema: json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {}
            }),
        };

        let handoff = handoff_plan_from_definition(&definition, "change files");

        assert_eq!(handoff.readiness_score, 20);
        assert!(handoff
            .readiness_issues
            .contains(&"timeout must be greater than zero".to_string()));
        assert!(handoff
            .readiness_issues
            .contains(&"context budget is below the minimum useful threshold".to_string()));
        assert!(handoff
            .readiness_issues
            .contains(&"output schema has no required fields".to_string()));
        assert!(handoff
            .readiness_issues
            .contains(&"workspace-write handoff must include preview edit capability".to_string()));
    }
}
