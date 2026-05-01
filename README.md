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
Ollama chat completion, session events, tool metadata, permission primitives,
declarative subagent metadata and early runtime streaming. Some advanced
capabilities, such as complete multi-provider LLM execution, executable
subagents and MCP, are planned or partially scaffolded and should be evolved
incrementally with tests.

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
- Declarative subagent registry with explorer, planner, coder, reviewer,
  security-reviewer, test-writer, eval-runner and docs-writer roles exposed
  through the read-only `subagent.list` tool.
- Command guard and shell planning primitives for controlled command execution.
- Local context, read tracking, preview edit flow and approved edit application
  in the Rust agent crate.
- Voice input and optional overlay support.
- Repository-boundary tests to support the Coddy/VisionClip split.

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

Open UI modes through the runtime:

```bash
./target/debug/coddy ui open --mode floating-terminal
./target/debug/coddy ui open --mode desktop-app
```

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

## Documentation

Additional documentation lives in:

- [docs/repl/architecture.md](docs/repl/architecture.md)
- [docs/repl/backend-contracts.md](docs/repl/backend-contracts.md)
- [docs/repl/coddy-decoupling-plan.md](docs/repl/coddy-decoupling-plan.md)
- [docs/repl/repository-split-manifest.md](docs/repl/repository-split-manifest.md)
- [docs/repl/ui-ux-spec.md](docs/repl/ui-ux-spec.md)
- [docs/repl/coding-agent-reference-analysis.md](docs/repl/coding-agent-reference-analysis.md)

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

- connect the runtime to production cloud model clients for real chat
  completion;
- expand Vertex region and project configuration for partner model execution;
- add durable agent memory with sensitive-data policy;
- evolve the tool loop from primitives to full action/observation/validation;
- add MCP client/server registry support;
- implement subagent orchestration;
- expand evals for coding-agent behavior and regression detection;
- add signed release artifacts and automatic update metadata.

## License

AGPL-3.0-only. See [LICENSE](LICENSE).
