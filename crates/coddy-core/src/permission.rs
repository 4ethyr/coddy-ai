use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

use crate::{ToolName, ToolPermission, ToolRiskLevel};

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum PermissionContractError {
    #[error("permission pattern cannot be empty")]
    EmptyPattern,

    #[error("permission request must include at least one pattern")]
    EmptyPatterns,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum PermissionAction {
    Allow,
    Deny,
    Ask,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum PermissionReply {
    Once,
    Always,
    Reject,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PermissionRule {
    pub permission: ToolPermission,
    pub pattern: String,
    pub action: PermissionAction,
}

impl PermissionRule {
    pub fn new(
        permission: ToolPermission,
        pattern: impl Into<String>,
        action: PermissionAction,
    ) -> Result<Self, PermissionContractError> {
        let pattern = pattern.into();
        if pattern.trim().is_empty() {
            return Err(PermissionContractError::EmptyPattern);
        }
        Ok(Self {
            permission,
            pattern,
            action,
        })
    }

    pub fn matches(&self, permission: ToolPermission, pattern: &str) -> bool {
        self.permission == permission && wildcard_match(&self.pattern, pattern)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PermissionEvaluation {
    pub permission: ToolPermission,
    pub pattern: String,
    pub action: PermissionAction,
    pub matched_rule: Option<PermissionRule>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PermissionRuleset {
    pub rules: Vec<PermissionRule>,
}

impl PermissionRuleset {
    pub fn new(rules: Vec<PermissionRule>) -> Self {
        Self { rules }
    }

    pub fn empty() -> Self {
        Self::default()
    }

    pub fn push(&mut self, rule: PermissionRule) {
        self.rules.push(rule);
    }

    pub fn evaluate(
        &self,
        permission: ToolPermission,
        pattern: impl Into<String>,
    ) -> PermissionEvaluation {
        let pattern = pattern.into();
        let matched_rule = self
            .rules
            .iter()
            .rev()
            .find(|rule| rule.matches(permission, &pattern))
            .cloned();
        let action = matched_rule
            .as_ref()
            .map(|rule| rule.action)
            .unwrap_or(PermissionAction::Ask);
        PermissionEvaluation {
            permission,
            pattern,
            action,
            matched_rule,
        }
    }

    pub fn evaluate_request(&self, request: &PermissionRequest) -> PermissionAction {
        let mut aggregate = PermissionAction::Allow;
        for pattern in &request.patterns {
            match self.evaluate(request.permission, pattern).action {
                PermissionAction::Deny => return PermissionAction::Deny,
                PermissionAction::Ask => aggregate = PermissionAction::Ask,
                PermissionAction::Allow => {}
            }
        }
        aggregate
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PermissionRequest {
    pub id: Uuid,
    pub session_id: Uuid,
    pub run_id: Uuid,
    pub tool_call_id: Option<Uuid>,
    pub tool_name: ToolName,
    pub permission: ToolPermission,
    pub patterns: Vec<String>,
    pub risk_level: ToolRiskLevel,
    #[serde(with = "crate::json_value_wire")]
    pub metadata: Value,
    pub requested_at_unix_ms: u64,
}

impl PermissionRequest {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        session_id: Uuid,
        run_id: Uuid,
        tool_call_id: Option<Uuid>,
        tool_name: ToolName,
        permission: ToolPermission,
        patterns: Vec<String>,
        risk_level: ToolRiskLevel,
        metadata: Value,
        requested_at_unix_ms: u64,
    ) -> Result<Self, PermissionContractError> {
        validate_patterns(&patterns)?;
        Ok(Self {
            id: Uuid::new_v4(),
            session_id,
            run_id,
            tool_call_id,
            tool_name,
            permission,
            patterns,
            risk_level,
            metadata,
            requested_at_unix_ms,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PermissionResponse {
    pub request_id: Uuid,
    pub reply: PermissionReply,
    pub message: Option<String>,
    pub responded_at_unix_ms: u64,
}

impl PermissionResponse {
    pub fn new(
        request_id: Uuid,
        reply: PermissionReply,
        message: Option<String>,
        responded_at_unix_ms: u64,
    ) -> Self {
        Self {
            request_id,
            reply,
            message,
            responded_at_unix_ms,
        }
    }
}

fn validate_patterns(patterns: &[String]) -> Result<(), PermissionContractError> {
    if patterns.is_empty() {
        return Err(PermissionContractError::EmptyPatterns);
    }
    if patterns.iter().any(|pattern| pattern.trim().is_empty()) {
        return Err(PermissionContractError::EmptyPattern);
    }
    Ok(())
}

fn wildcard_match(pattern: &str, value: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if !pattern.contains('*') {
        return pattern == value;
    }

    let starts_with_wildcard = pattern.starts_with('*');
    let ends_with_wildcard = pattern.ends_with('*');
    let parts: Vec<&str> = pattern.split('*').filter(|part| !part.is_empty()).collect();

    if parts.is_empty() {
        return true;
    }

    let mut offset = 0;
    for (index, part) in parts.iter().enumerate() {
        if index == 0 && !starts_with_wildcard {
            if !value.starts_with(part) {
                return false;
            }
            offset = part.len();
            continue;
        }

        let Some(relative_index) = value[offset..].find(part) else {
            return false;
        };
        offset += relative_index + part.len();
    }

    ends_with_wildcard || value.ends_with(parts.last().expect("parts are not empty"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_rule_rejects_empty_patterns() {
        assert_eq!(
            PermissionRule::new(ToolPermission::ReadWorkspace, "", PermissionAction::Allow),
            Err(PermissionContractError::EmptyPattern)
        );
    }

    #[test]
    fn ruleset_defaults_to_ask_without_a_match() {
        let ruleset = PermissionRuleset::empty();

        let evaluation = ruleset.evaluate(ToolPermission::ReadWorkspace, "src/lib.rs");

        assert_eq!(evaluation.action, PermissionAction::Ask);
        assert_eq!(evaluation.matched_rule, None);
    }

    #[test]
    fn ruleset_uses_last_matching_rule_as_override() {
        let ruleset = PermissionRuleset::new(vec![
            PermissionRule::new(ToolPermission::WriteWorkspace, "*", PermissionAction::Ask)
                .expect("valid rule"),
            PermissionRule::new(
                ToolPermission::WriteWorkspace,
                "docs/*",
                PermissionAction::Allow,
            )
            .expect("valid rule"),
            PermissionRule::new(
                ToolPermission::WriteWorkspace,
                "docs/private/*",
                PermissionAction::Deny,
            )
            .expect("valid rule"),
        ]);

        assert_eq!(
            ruleset
                .evaluate(ToolPermission::WriteWorkspace, "docs/repl/plan.md")
                .action,
            PermissionAction::Allow
        );
        assert_eq!(
            ruleset
                .evaluate(ToolPermission::WriteWorkspace, "docs/private/secret.md")
                .action,
            PermissionAction::Deny
        );
    }

    #[test]
    fn ruleset_evaluates_multi_pattern_requests_conservatively() {
        let ruleset = PermissionRuleset::new(vec![
            PermissionRule::new(
                ToolPermission::ReadWorkspace,
                "docs/*",
                PermissionAction::Allow,
            )
            .expect("valid rule"),
            PermissionRule::new(
                ToolPermission::ReadWorkspace,
                "docs/private/*",
                PermissionAction::Deny,
            )
            .expect("valid rule"),
        ]);
        let request = PermissionRequest::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            None,
            ToolName::new("filesystem.read_file").expect("valid tool name"),
            ToolPermission::ReadWorkspace,
            vec![
                "docs/repl/architecture.md".to_string(),
                "docs/private/token.md".to_string(),
            ],
            ToolRiskLevel::Low,
            Value::Object(Default::default()),
            1_775_000_000_000,
        )
        .expect("valid request");

        assert_eq!(ruleset.evaluate_request(&request), PermissionAction::Deny);
    }

    #[test]
    fn wildcard_patterns_support_prefix_suffix_and_middle_segments() {
        assert!(wildcard_match("src/*", "src/lib.rs"));
        assert!(wildcard_match("*.rs", "src/lib.rs"));
        assert!(wildcard_match("apps/*/Cargo.toml", "apps/coddy/Cargo.toml"));
        assert!(!wildcard_match("docs/*", "src/lib.rs"));
    }

    #[test]
    fn permission_request_and_response_roundtrip_through_json() {
        let request = PermissionRequest::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Some(Uuid::new_v4()),
            ToolName::new("shell.run").expect("valid tool name"),
            ToolPermission::ExecuteCommand,
            vec!["cargo test -p coddy-core".to_string()],
            ToolRiskLevel::High,
            serde_json::json!({ "description": "Run focused tests" }),
            1_775_000_000_000,
        )
        .expect("valid request");
        let encoded = serde_json::to_string(&request).expect("serialize request");
        let decoded: PermissionRequest =
            serde_json::from_str(&encoded).expect("deserialize request");

        assert_eq!(decoded, request);

        let response = PermissionResponse::new(
            request.id,
            PermissionReply::Once,
            Some("approved for this run".to_string()),
            1_775_000_000_001,
        );
        let encoded = serde_json::to_string(&response).expect("serialize response");
        let decoded: PermissionResponse =
            serde_json::from_str(&encoded).expect("deserialize response");

        assert_eq!(decoded, response);
    }
}
