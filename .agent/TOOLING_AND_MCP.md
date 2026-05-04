# Tooling And MCP

All tools must be registered through a central catalog with schemas, risk levels,
permissions, timeout and output bounds.

## Tool Requirements

- Strong input schema.
- Structured output schema.
- Risk level: low, medium, high or critical.
- Explicit permissions.
- Timeout.
- Bounded stdout/stderr or text output.
- Structured errors.
- Redaction before model/UI exposure.
- Audit event for start, completion, denial and failure.

## Risk Levels

- Low: local read-only inspection.
- Medium: bounded computation or low-risk workspace operations.
- High: writes, shell, network, dependency managers or external paths.
- Critical: production, secrets, deploy, destructive operations or infrastructure.

## MCP Target

MCP tools must use the same policy bridge as local tools:

- discover server capabilities;
- classify each capability by risk;
- require approval for high-risk tools;
- sandbox outputs against prompt injection;
- redact secrets;
- log every call;
- disable production access by default.

MCP runtime integration is not considered production-ready until it has tests for
permissions, redaction, timeout, failure and prompt-injection handling.
