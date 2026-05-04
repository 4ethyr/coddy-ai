# Code Quality

Coddy should favor clear, small, testable modules over clever or broad changes.

## Standards

- Rust code must pass `cargo fmt --check`, `cargo clippy --workspace
  --all-targets -- -D warnings` and `cargo test --workspace`.
- Electron code must pass `npm run typecheck`, `npm run typecheck:main`,
  `npm run lint`, `npm test`, `npm run test:e2e` and `npm run build`.
- Behavior changes require tests.
- Security changes require regression tests with synthetic secrets or denied
  commands.
- Public contracts must remain backward-compatible unless a migration is
  documented.

## Design Rules

- Keep domain logic out of UI components.
- Keep provider-specific logic behind adapters.
- Avoid god files by extracting stable contracts and cohesive helpers.
- Prefer structured parsers and schemas over string guessing.
- Use explicit error types and recoverability where possible.
- Do not add dependencies without security and maintenance justification.
