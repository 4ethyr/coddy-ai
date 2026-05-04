# Workflows

## Standard Coding Workflow

1. Understand the request.
2. Inspect relevant files.
3. Identify risks and acceptance criteria.
4. Add or update a failing test when behavior changes.
5. Implement the smallest safe change.
6. Run targeted tests.
7. Run broader gates.
8. Review the diff.
9. Report evidence, residual risks and next steps.

## Security Fix Workflow

1. Reproduce or encode the risk with a synthetic test.
2. Implement redaction, denial, sandboxing or least-privilege behavior.
3. Verify the secret or dangerous operation is not exposed/executed.
4. Run secret scan and relevant integration tests.
5. Update docs or CI gates if the fix creates a new invariant.

## Dependency Update Workflow

1. Run audit.
2. Apply the smallest update that removes vulnerable packages.
3. Run type-check, lint, unit, e2e and build.
4. Record breaking-change risks.

## Release Workflow

Release must run the same gates as CI plus installer/package checks and artifact
hash generation.
