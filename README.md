# Coddy

Coddy e o projeto de REPL/CLI agentic extraido do VisionClip para evoluir como repositorio independente.

## Workspace

- `apps/coddy`: CLI do Coddy, incluindo `coddy repl` para abrir o terminal flutuante e `coddy repl --terminal` para o modo stdin/stdout com historico local.
- `apps/coddy-electron`: interface desktop Electron.
- `crates/coddy-agent`: runtime agentic Rust, registry de tools locais, router de tools, contexto local, executor deterministico de planos, eval runner local, run state minimo, tools read-only, read tracker, previews, aplicacao aprovada de edicao, command guard, shell planner e shell executor controlado.
- `crates/coddy-core`: dominio, sessoes, politicas, eventos, contratos e parser/handler desacoplado de comandos do REPL shell.
- `crates/coddy-ipc`: transporte e contratos IPC, incluindo snapshot/eventos e catalogo read-only de tools.
- `crates/coddy-client`: cliente do runtime Coddy, incluindo comandos, eventos, snapshots e listagem de tools.
- `crates/coddy-voice-input`: entrada de voz e overlay opcional.
- `docs/repl`: documentacao de arquitetura, contratos e plano do REPL.
- `repl_ui`: prototipos visuais.

## Validacao

```bash
cargo test -p coddy-ipc --test repository_boundaries
cargo test -p coddy-voice-input
cargo test -p coddy-client
cargo test -p coddy-core repl_shell
cargo test -p coddy

cd apps/coddy-electron
npm test
npm run typecheck
```

## Relacao com VisionClip

Este repositorio ainda contem crates compartilhados que o VisionClip referencia por paths relativos durante a fase de separacao. A direcao esperada e manter o Coddy como produto independente e reduzir gradualmente os acoplamentos restantes por contratos explicitos.

O manifesto de separacao esta em [docs/repl/repository-split-manifest.md](docs/repl/repository-split-manifest.md).
