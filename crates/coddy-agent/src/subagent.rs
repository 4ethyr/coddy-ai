use serde_json::{json, Value};

use crate::{
    APPLY_EDIT_TOOL, LIST_FILES_TOOL, PREVIEW_EDIT_TOOL, READ_FILE_TOOL, SEARCH_FILES_TOOL,
    SHELL_RUN_TOOL,
};

pub const SUBAGENT_LIST_TOOL: &str = "subagent.list";
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
}
