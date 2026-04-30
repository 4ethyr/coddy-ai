use serde_json::{json, Value};

use crate::{
    APPLY_EDIT_TOOL, LIST_FILES_TOOL, PREVIEW_EDIT_TOOL, READ_FILE_TOOL, SEARCH_FILES_TOOL,
    SHELL_RUN_TOOL,
};

pub const SUBAGENT_LIST_TOOL: &str = "subagent.list";

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
}

fn subagent(
    name: &str,
    description: &str,
    mode: SubagentMode,
    allowed_tools: &[&str],
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
