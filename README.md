# Coddy

Coddy is an independent agentic REPL and desktop assistant extracted from
VisionClip. The project is being developed as a modern coding-agent runtime with
a Rust-first backend, a TypeScript/Electron frontend, secure model configuration,
event-driven sessions, tool governance, and a terminal-first developer
experience.

The long-term goal is to provide a high-quality coding agent similar in spirit to
professional agentic CLIs: fast local runtime, explicit permission boundaries,
model/provider flexibility, streaming chat, audit-friendly events, and a UI that
keeps the existing Coddy visual language: dark surfaces, transparency, blur and
glassmorphism.

## Status

Coddy is under active development. The current implementation includes the CLI,
runtime IPC, Electron UI, model discovery, secure credential storage, local
Ollama chat completion, OpenAI/OpenRouter compatible chat execution, Gemini API
chat execution, Azure OpenAI chat execution, Vertex Claude execution, session
events, tool metadata, permission primitives, guarded local tools, conversation
history, deterministic quality evals and early runtime streaming. Advanced
capabilities such as isolated executable subagents and MCP are planned or
partially scaffolded and should be evolved incrementally with tests.

Last full validation recorded in this branch: 2026-05-03.

## Main Features

- Rust CLI with commands for REPL, ask, voice, model selection, UI mode,
  session snapshots, event watching, tool catalog and runtime serving.
- Electron desktop UI with floating terminal and desktop modes.
- Event-driven REPL session model with snapshots, incremental events and live
  watch streams.
- Model selector with provider search, responsive dropdown, secure credential
  persistence and provider-specific notices.
- Runtime-backed chat responses for local Ollama, OpenAI-compatible providers
  and Anthropic Claude partner models through Vertex AI.
- Provider model listing for:
  - Local Ollama.
  - OpenAI models.
  - OpenRouter text models.
  - Google Gemini API models with API keys.
  - Google Vertex AI Model Garden publishers with OAuth access tokens, ADC or a
    local `gcloud` login.
  - Azure OpenAI deployments.
- Secure token handling through Electron `safeStorage` when available.
- Agent tool registry, tool metadata, risk levels, permissions and approval
  primitives.
- Deterministic quality gates for multiagent routing, reducer contracts,
  grounded response citations and the 1200-prompt subagent prompt battery.
- Live prompt-battery execution against selected providers for measuring model
  routing quality and recoverable provider failures without exposing API keys.
- Declarative subagent registry with explorer, planner, coder, reviewer,
  security-reviewer, test-writer, eval-runner and docs-writer roles exposed
  through the read-only `subagent.list` tool.
- Command guard and shell planning primitives for controlled command execution.
- Local context, read tracking, preview edit flow and approved edit application
  in the Rust agent crate.
- Voice input and optional overlay support.
- Repository-boundary tests to support the Coddy/VisionClip split.

## Current Coding-Agent Readiness

Coddy is ready for assisted coding workflows where the user keeps review control
and validation is run locally. It should not yet be treated as a fully
autonomous merge/release agent.

| Area | Current level | Notes |
| --- | --- | --- |
| Codebase analysis | Strong | Local filesystem tools, search tools, workspace selection and session context are wired through the Rust runtime and Electron UI. |
| Coding workflow | Strong for assisted work | `/code`, `/plan`, `/review` and `/test` steer the model toward inspect-plan-edit-validate behavior with evidence requirements. |
| Tool governance | Strong foundation | Tool registry metadata, risk levels, permissions, command guard and redaction are implemented for the local tool surface. |
| Subagents | Medium-high | Subagent registry, routing, preparation, team planning and reducer contracts are deterministic and evaluated; isolated executable subagent sessions are still pending. |
| Provider reliability | Medium-high | OpenAI-compatible runtime, OpenRouter, Gemini API, Azure OpenAI, Ollama and Vertex Claude paths exist; routed providers can still fail or return empty assistant messages. |
| Evals and metrics | Strong foundation | Multiagent evals, quality evals and prompt batteries are available from the CLI and Electron IPC, including live provider sampling. |
| MCP and external tools | Planned | MCP readiness is documented and surfaced in UI commands, but the runtime MCP adapter and permission bridge are not complete yet. |
| Autonomous PR/release flow | Planned | Coddy can assist with commits and PR descriptions through local workflows, but it does not yet run isolated branch workers with PR lifecycle automation. |

Recent secure live evals against OpenRouter using
`deepseek/deepseek-v4-flash`, with the API key loaded locally and not printed:

- Deterministic quality gate: score 100, prompt-battery 1200/1200.
- Core live project matrix over `apex`, `Guardian`, `maker`, `visionclip`
  samples: provider errors 0, pseudo-tool markup 0, secret hits 0, observed
  scores after hardening in the 80-100 range.
- Local SWE-bench-style patch battery: 3/3 resolved across Python, Node and
  Rust fixtures; patches extracted, applied and validated by tests.
- Official SWE-bench execution remains blocked locally until Docker daemon
  access is available for the benchmark harness.

See [docs/coding-agent-capability-review.md](docs/coding-agent-capability-review.md)
for the deeper comparison against current coding-agent systems.

## Important Provider Notes

### Google Gemini API keys

Google API keys list Gemini API models through:

```text
https://generativelanguage.googleapis.com/v1beta/models
```

At runtime, Gemini API-key models use the non-streaming `generateContent`
endpoint:

```text
https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent
```

The API key is sent with `x-goog-api-key`; Coddy does not place the key in the
request URL. These keys do not list or execute Anthropic Claude models in Vertex
AI Model Garden.

### Claude on Vertex AI

Claude models from Anthropic are Vertex AI partner models. Listing them through
`publishers/anthropic/models` requires a Google credential that asserts a
principal, such as:

- OAuth access token (`Bearer ya29...`);
- Application Default Credentials;
- service account based authentication.

Plain Gemini API keys are rejected by the Vertex AI publisher model API. The UI
shows a notice when Gemini models are loaded with an API key so users understand
why Claude models are not present.

### Local gcloud Authentication

For Vertex AI publisher models, Coddy can use local Google credentials without
asking you to paste a token. Leave the Vertex credential field empty and click
`Load`.

The Electron main process resolves credentials in this order when the Vertex
credential field is left empty:

1. `gcloud auth print-access-token`.
2. `GOOGLE_APPLICATION_CREDENTIALS` or the default ADC file.
3. `gcloud config get-value project` for request-scoped Vertex runtime
   metadata.

The `gcloud` token is short-lived, kept only in memory for the Vertex model list
request and never persisted by Coddy. For development, run these before opening
the UI:

```bash
gcloud auth application-default login
gcloud config set project YOUR_PROJECT_ID
```

If you only ran `gcloud auth login`, Coddy can still use the active local
account through `gcloud auth print-access-token`, but ADC is the recommended
development setup. Claude models also need to be enabled in Vertex AI Model
Garden for the selected project and region.

When a non-local model is selected, the Electron main process can pass a
short-lived credential to the Rust CLI through the internal
`CODDY_EPHEMERAL_MODEL_CREDENTIAL` environment variable. The CLI validates this
payload, forwards it to the runtime over Coddy IPC and redacts the token from
debug output. This is a credential bridge for runtime adapters; providers that
do not yet have a Rust chat adapter still remain marked as adapter pending in
the UI. The bridge may include non-secret metadata such as `project_id` and
`region`; metadata values are not shown in Rust debug output.

## Repository Layout

```text
.
├── apps/
│   ├── coddy/                 # Rust CLI application.
│   └── coddy-electron/        # Electron + React + TypeScript frontend.
├── crates/
│   ├── coddy-agent/           # Agent runtime primitives, tools, guards, evals.
│   ├── coddy-client/          # Runtime client for commands, snapshots, events.
│   ├── coddy-core/            # Domain model, events, sessions, policies.
│   ├── coddy-ipc/             # IPC wire contracts and framing.
│   ├── coddy-runtime/         # Local runtime server and request handlers.
│   └── coddy-voice-input/     # Voice capture and overlay support.
├── docs/repl/                 # Architecture, contracts and implementation docs.
├── repl_ui/                   # UI prototypes and visual references.
├── AGENTS.md                  # Agent instructions for this repository.
├── Cargo.toml                 # Rust workspace manifest.
└── README.md                  # Project overview and usage guide.
```

## Architecture

Coddy is organized around clear boundaries:

- `coddy-core` owns pure domain contracts: sessions, events, messages,
  policies, model refs and REPL shell behavior.
- `coddy-ipc` owns transport-safe request and response contracts.
- `coddy-runtime` owns the local runtime server and bridges IPC requests to the
  session/event model.
- `coddy-client` is the runtime client used by the CLI and integrations.
- `coddy-agent` owns agentic building blocks: tool definitions, command guard,
  permission requests, shell planning, context and evals.
- `apps/coddy` is the CLI surface.
- `apps/coddy-electron` is the desktop frontend. It follows a clean layering
  style: domain, application, infrastructure, presentation and main process.

The frontend communicates with the runtime through Electron IPC and Coddy IPC
commands. Session state is synchronized from runtime snapshots and event streams.

## Stack

### Backend and Runtime

- Rust 2021 workspace.
- Tokio for async runtime and Unix socket serving.
- Serde and bincode for contracts and wire format.
- Clap for CLI parsing.
- Tracing for runtime logs.
- UUIDs for sessions, runs and permission requests.

### Frontend

- Electron 33.
- React 19.
- TypeScript 5.7.
- Vite 6.
- TailwindCSS 3.
- Vitest and React Testing Library.
- ESLint 9.

### Package Managers

- Cargo for Rust crates.
- npm for the Electron app.

## Prerequisites

- Rust toolchain with Cargo.
- Node.js and npm compatible with the Electron/Vite toolchain.
- Linux desktop environment for the current Electron and voice workflows.
- Ollama, API keys or OAuth credentials only if you want to use external model
  discovery.

## Setup

Install frontend dependencies:

```bash
cd apps/coddy-electron
npm install
```

Build the Rust workspace:

```bash
cargo build
```

## Installable Linux Build

Coddy can be packaged as a Linux desktop bundle containing:

- the Rust `coddy` backend CLI;
- the Electron/React frontend as an AppImage;
- a `coddy-desktop` launcher;
- a desktop entry for application menus.

Build the local release bundle:

```bash
./scripts/package_linux.sh
```

The script produces:

```text
dist/coddy-linux-x64.tar.gz
dist/coddy-linux-x64.tar.gz.sha256
```

For GitHub releases, upload those two files as release assets. Users can then
install with:

```bash
curl -fsSL https://raw.githubusercontent.com/4ethyr/coddy-ai/main/scripts/install.sh -o /tmp/coddy-install.sh
sh /tmp/coddy-install.sh
```

To install a specific release tag:

```bash
CODDY_VERSION=v0.1.0 sh /tmp/coddy-install.sh
```

The installer writes only to the user's local prefix by default:
`~/.local/bin`, `~/.local/share/coddy`, and
`~/.local/share/applications`.

After installation:

```bash
coddy-desktop
```

starts the Electron app and its bundled Rust runtime. On systems without
`libfuse.so.2`, the launcher automatically uses AppImage extract-and-run mode
and writes desktop logs to `~/.local/state/coddy/coddy-desktop.log` instead of
printing the AppImage extraction list in the terminal.

CLI commands such as `coddy repl` and `coddy ui open` launch/focus Coddy Desktop
when the installed `coddy-desktop` launcher is available. `coddy ask`,
`coddy session snapshot`, and other runtime commands use the same local runtime
socket while the desktop app or `coddy runtime serve` is running. For a
terminal-only REPL, use:

```bash
coddy repl --terminal
```

For local/offline validation, point the installer at an existing release
archive and an isolated prefix:

```bash
CODDY_ARCHIVE=dist/coddy-linux-x64.tar.gz \
CODDY_INSTALL_PREFIX=/tmp/coddy-install-test \
CODDY_DESKTOP_DIR=/tmp/coddy-install-test/share/applications \
sh scripts/install.sh
```

The repository also includes a fast installer contract smoke test that creates a
minimal local archive and verifies the CLI launcher, desktop launcher, checksum
flow, and desktop entry without network access:

```bash
./scripts/test_install_local.sh
```

Release packaging also runs a high-confidence secret guard over tracked and
staged files. It reports only pattern names and file paths, never the matching
values:

```bash
./scripts/guard_no_secrets.sh
./scripts/test_guard_no_secrets.sh
```

## Running the Local Stack

Use separate terminals.

### 1. Start the Coddy runtime manually

```bash
./target/debug/coddy runtime serve --socket /tmp/coddy-repl-dev.sock
```

This manual runtime is useful for backend development. The Electron app also
starts a bundled runtime automatically; when running both together, use
`CODDY_DAEMON_SOCKET` to point Electron and CLI commands at the manual socket.

### 2. Start the Vite renderer

```bash
cd apps/coddy-electron
npm run dev:renderer
```

### 3. Start Electron

```bash
cd apps/coddy-electron
ELECTRON_DISABLE_SANDBOX=1 \
CODDY_BIN=/home/aethyr/Documents/coddy/target/debug/coddy \
CODDY_DAEMON_SOCKET=/tmp/coddy-repl-dev.sock \
VITE_DEV_SERVER_URL=http://localhost:5173 \
npm run electron:dev
```

Adjust `CODDY_BIN` and socket paths if your repository lives elsewhere.

## CLI Usage

Show top-level commands:

```bash
./target/debug/coddy --help
```

Open the terminal REPL:

```bash
./target/debug/coddy repl --terminal
```

Send a text command to the runtime:

```bash
./target/debug/coddy ask "explain this module"
```

Select a chat model:

```bash
./target/debug/coddy model select --provider ollama --name qwen2.5:0.5b --role chat
```

Inspect runtime session state:

```bash
./target/debug/coddy session snapshot
./target/debug/coddy session tools
./target/debug/coddy session watch --after 0
```

Work with conversation history:

```bash
./target/debug/coddy session history
./target/debug/coddy session new
```

Open UI modes through the runtime:

```bash
./target/debug/coddy ui open --mode floating-terminal
./target/debug/coddy ui open --mode desktop-app
```

## Desktop, Workspace and Slash Commands

Coddy Desktop exposes the same runtime concepts through FloatingTerminal and
Desktop mode. The workspace flow lets the user select a folder and keep Coddy
bound to that project, which mirrors terminal-based agents that start inside a
repository directory.

The Workspace tab also exposes local eval harnesses. Multiagent and
prompt-battery panels can run deterministic checks, compare against a baseline
path, and optionally write a fresh baseline for later regression tracking.

Supported local slash commands in the Electron UI:

| Command | Purpose |
| --- | --- |
| `/help` or `/?` | Show local command help without contacting the model. |
| `/code <goal>` or `/implement <goal>` | Start an implementation workflow with exploration, plan, incremental edits and validation guidance. |
| `/plan <goal>` | Produce a read-only implementation plan with assumptions, risks and validation steps. |
| `/review <scope>` | Review a diff, file or area for bugs, regressions, security issues and missing tests. |
| `/test <goal>` or `/tests <goal>` | Choose and run or recommend the smallest useful validation. |
| `/workspace`, `/workspaces`, `/files` | Open workspace/file context and related tools. |
| `/tools` or `/tool` | Inspect tool catalog, risk levels and eval readiness. |
| `/subagents`, `/subagent`, `/agents` | Inspect subagent contracts and orchestration state. |
| `/mcp` | Inspect MCP readiness and external-tool integration status. |
| `/quality`, `/eval`, `/evals`, `/metrics` | Open or run quality gates and evals. Use `/quality run` to execute the local quality flow. |
| `/models` or `/model` | Open provider/model selection and runtime readiness. |
| `/settings`, `/setting`, `/settins`, `/config` | Open settings, including appearance, provider preference and speech response options. |
| `/history` | Show persisted, redacted conversation history. |
| `/new` | Start a clean session while preserving safe model/workspace settings. |
| `/status` | Show current session, model, workspace and active run state. |
| `/capabilities`, `/agent`, `/readiness` | Show coding-agent readiness and known gaps. |
| `/speak on` or `/speak off` | Enable or disable spoken replies after voice input. |

Conversation history is stored without provider secrets. API keys pasted in the
UI are request-scoped unless the user explicitly enables secure remembering,
which uses Electron `safeStorage` when OS-backed encryption is available.

## Model Configuration

Model credentials are request-scoped by default. If the user enables secure
remembering in the UI, credentials are stored through Electron `safeStorage`.
When OS-backed encryption is unavailable, credentials are not persisted.

Supported model discovery paths:

- Ollama: local `http://127.0.0.1:11434/api/tags`.
- OpenAI: `/v1/models` with bearer API key.
- OpenRouter: `/api/v1/models` with bearer API key.
- Gemini API: Google API key through `x-goog-api-key`.
- Vertex AI Model Garden: OAuth bearer token for `publishers/google` and
  `publishers/anthropic`, Application Default Credentials, or an active local
  `gcloud` login.
- Azure OpenAI: HTTPS resource endpoint and API key.

Do not commit `.env` files or API keys. The repository ignores common `.env`
variants.

For Vertex AI partner models such as Claude, paste a Google OAuth bearer token,
configure Application Default Credentials with `GOOGLE_APPLICATION_CREDENTIALS`,
or leave the credential field empty after authenticating with `gcloud`. Plain
Google API keys intentionally use the Gemini API model list and do not return
Anthropic publisher models.

The Vertex provider accepts an optional region or endpoint override such as
`global`, `us-east5`, `europe-west1`, or
`https://us-east5-aiplatform.googleapis.com`.

Runtime chat completion currently supports local Ollama models, Gemini API-key
models, OpenAI-compatible chat execution for OpenAI/OpenRouter, Azure OpenAI
deployments, and Anthropic Claude partner models through Vertex AI `rawPredict`.
By default Coddy connects to `http://127.0.0.1:11434/api/chat` for Ollama; set
`OLLAMA_HOST` to override the host, for example `OLLAMA_HOST=127.0.0.1:11434` or
`OLLAMA_HOST=http://localhost:11434`.

OpenAI and OpenRouter chat execution use non-streaming `/chat/completions`
requests from the Rust runtime. The Electron main process sends the selected
provider credential to the CLI as a request-scoped environment payload; the
token is not stored in renderer state and is redacted from Rust debug output.
Custom OpenAI-compatible runtime endpoints must use HTTPS.

OpenRouter routes can occasionally return an empty assistant message even when
the request itself is valid. Coddy treats the known empty-response shape as
recoverable, retries boundedly, and adds a short internal retry instruction on
subsequent attempts asking the provider to return non-empty content. Timeouts
are still treated separately and are not retried past the client budget. In the
latest 50-prompt live battery with `openrouter/deepseek/deepseek-v4-flash`, this
reduced the observed model error rate from 14% to 6% and raised the guarded
score from 86 to 94.

Vertex Claude execution uses Google OAuth/ADC/gcloud credentials, the active
gcloud project, and the selected Vertex region. Claude model IDs must use the
Vertex Anthropic form, for example `claude-sonnet-4-5@20250929`. Gemini API-key
execution is intentionally separate from Vertex Claude: OAuth/ADC credentials
are rejected by the Gemini API adapter so credential type mistakes surface
clearly. Azure model discovery is wired in the UI, while the Azure runtime
adapter executes selected deployments through:

```text
{endpoint}/openai/deployments/{deployment-id}/chat/completions?api-version=2024-10-21
```

The Azure API key is sent with the `api-key` header. The selected model name is
treated as the Azure deployment ID. The UI defaults to API version
`2024-10-21`, and an Azure API version override is stored only with the
encrypted provider credential record when secure persistence is enabled.

## Development Workflow

Prefer small, reversible changes with tests first.

Recommended loop:

```bash
cargo test -p coddy-runtime
cargo test

cd apps/coddy-electron
npm test -- modelProviders ModelSelector
npm test
npm run test:e2e
npm run typecheck
npm run typecheck:main
npm run lint
npm run build
```

Use focused tests while developing and run the broader suite before commits.

## Validation Commands

Rust:

```bash
cargo test
cargo clippy --all-targets --all-features
cargo fmt --all --check
```

Electron frontend:

```bash
cd apps/coddy-electron
npm test
npm run test:e2e
npm run typecheck
npm run typecheck:main
npm run lint
npm run build
```

Repository split boundary:

```bash
cargo test -p coddy-ipc --test repository_boundaries
```

Agent quality gates:

```bash
./target/debug/coddy eval multiagent --json
./target/debug/coddy eval prompt-battery --json
./target/debug/coddy eval quality --json
```

Prompt-battery baseline comparison:

```bash
./target/debug/coddy eval prompt-battery --json \
  --write-baseline evals/baselines/prompt-battery.json

./target/debug/coddy eval prompt-battery --json \
  --baseline evals/baselines/prompt-battery.json
```

Live model routing sample, using the provider credential configured for the
selected provider:

```bash
./target/debug/coddy eval prompt-battery --json \
  --model-provider openrouter \
  --model-name deepseek/deepseek-v4-flash \
  --limit 50 \
  --concurrency 4
```

Live project/codebase battery across local repositories:

```bash
MODEL_PROVIDER=openrouter \
MODEL_NAME=deepseek/deepseek-v4-flash \
./scripts/run_live_project_battery.sh
```

The live project battery writes redacted artifacts under `/tmp`, separates
model stdout from CLI stderr, tracks provider errors, tool failures,
pseudo-tool markup, incomplete answers, secret hits and a per-prompt quality
score. Use `PROJECT_FILTER`, `CATEGORY_FILTER` or `PROJECTS_CSV` to run a
smaller matrix.

Local SWE-bench-style patch battery:

```bash
MODEL_PROVIDER=openrouter \
MODEL_NAME=deepseek/deepseek-v4-flash \
./scripts/run_coddy_patch_task_battery.sh
```

This local patch battery creates temporary fixtures, asks Coddy for unified
diffs, extracts patches, applies them with `git apply` and runs fixture tests.
It is not an official SWE-bench score; official SWE-bench requires the upstream
Docker harness and benchmark dataset.

Latest validated local suite on this branch:

- `cargo test -p coddy-core -p coddy-agent -p coddy-client -p coddy-runtime -p coddy -- --test-threads=1`.
- `npm run typecheck`.
- `./scripts/guard_no_secrets.sh`.
- `./target/debug/coddy eval quality --json`: score 100.
- `./target/debug/coddy eval prompt-battery --json`: 1200/1200 passed.
- `./scripts/run_coddy_patch_task_battery.sh`: 3/3 local patch tasks resolved.

## Security Model

Coddy is designed around explicit boundaries:

- model credentials are not stored in browser `localStorage`;
- optional token persistence uses Electron `safeStorage`;
- filesystem and shell tools are represented with permissions and risk levels;
- command guard blocks destructive or privilege-escalating command shapes;
- shell execution paths are planned and approval-aware;
- repository-boundary tests help keep Coddy independent from VisionClip.

Security-sensitive changes should include tests and should not print secrets in
logs or terminal output.

## Known Limitations

- Isolated executable subagent sessions are not complete yet; current subagent
  behavior is strongest for deterministic routing, planning, readiness and
  reducer evaluation.
- MCP is documented and planned, but runtime MCP discovery, execution and
  permission bridging still need implementation.
- Live provider reliability depends on the selected provider and routed model.
  OpenRouter empty assistant responses are retried when recognized, but repeated
  provider-side failures can still surface to the user.
- Adaptive context compaction and per-run tool budgeting are improving, but
  broad prompts over large repositories still need tighter summarization and
  retrieval ranking.
- Coddy does not yet automate the full cloud-agent flow of creating isolated
  branches, pushing commits and opening PRs from a background worker.
- Voice capture and speech response flows require Linux desktop/audio
  validation on target machines because hardware, permissions and audio
  backends vary.

## Documentation

Additional documentation lives in:

- [docs/repl/architecture.md](docs/repl/architecture.md)
- [docs/repl/backend-contracts.md](docs/repl/backend-contracts.md)
- [docs/repl/coddy-decoupling-plan.md](docs/repl/coddy-decoupling-plan.md)
- [docs/repl/repository-split-manifest.md](docs/repl/repository-split-manifest.md)
- [docs/repl/ui-ux-spec.md](docs/repl/ui-ux-spec.md)
- [docs/repl/coding-agent-reference-analysis.md](docs/repl/coding-agent-reference-analysis.md)
- [docs/coding-agent-capability-review.md](docs/coding-agent-capability-review.md)
- [docs/repl/multiagent-hardness-eval.md](docs/repl/multiagent-hardness-eval.md)

## Relationship With VisionClip

Coddy was separated from VisionClip and is being prepared as an independent
repository. During the transition, some paths and compatibility contracts still
reflect the original integration. The target direction is to keep Coddy
self-contained and expose stable contracts that VisionClip can consume without
sharing source ownership.

See the split manifest:

[docs/repl/repository-split-manifest.md](docs/repl/repository-split-manifest.md)

## Roadmap

Near-term priorities:

- connect isolated subagent execution sessions to the existing reducer and
  approval gates;
- improve adaptive tool budgeting and observation compaction for broad coding
  prompts;
- expand live eval sampling and baseline comparison across providers;
- add durable agent memory with sensitive-data policy;
- add MCP client/server registry support;
- keep improving provider error recovery and user-facing diagnostics;
- add signed release artifacts and automatic update metadata.

## License

AGPL-3.0-only. See [LICENSE](LICENSE).
