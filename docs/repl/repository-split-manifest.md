# Manifesto de Separacao de Repositorios

Este manifesto define o pacote que pertence ao repositorio Coddy depois da separacao fisica do monorepo do VisionClip.

## Repositorio Coddy

Mover:

- `apps/coddy`
- `apps/coddy-electron`
- `crates/coddy-core`
- `crates/coddy-ipc`
- `crates/coddy-client`
- `crates/coddy-voice-input`
- `docs/IMPLEMENTATION_PLAN.md`
- `docs/repl`
- `docs/coddy-architecture-diagram.html`
- `repl_ui`
- `.agent`
- `AGENTS.md`

Esses pacotes devem depender apenas entre si, de dependencias externas publicas e do daemon via `coddy-ipc`/socket. Eles nao devem importar crates runtime do VisionClip nem incluir arquivos de `apps/visionclip`.

## Repositorio VisionClip

Manter:

- `apps/visionclip`
- `apps/visionclip-daemon`
- `apps/visionclip-config`
- `crates/common`
- `crates/infer`
- `crates/output`
- `crates/tts`
- `deploy`
- `scripts` de operacao do daemon VisionClip
- `tools` de runtime local do VisionClip

O VisionClip nao deve depender diretamente de crates Coddy por path, workspace, git ou registry. Enquanto o daemon do VisionClip ainda implementar o servidor do protocolo Coddy, essa compatibilidade deve ficar atras da feature explicita `coddy-protocol`, desativada por padrao, usando apenas uma camada local de wire compatibility em `apps/visionclip-daemon/src/coddy_contract.rs`. O `coddy_bridge.rs` concentra estado runtime do REPL, stream de eventos, dispatch de comandos, orquestracao Ask/VoiceTurn, construcao de eventos/intents, adaptacao de intents de voz, comandos locais e ciclo de policy de screen-assist. O `main.rs` do daemon deve apenas adaptar servicos nativos do VisionClip por `ReplNativeServices`, sem possuir o pipeline Coddy nem importar `process_repl_command` do bridge. `apps/visionclip-daemon/src/main.rs` nao deve importar tipos core/protocolo do Coddy, construir `ReplEvent`/`ReplIntent` diretamente nem chamar internals de resolucao de voz do Coddy. `visionclip-common` e o cliente `visionclip` nao devem depender de crates Coddy. O VisionClip tambem nao deve conhecer `coddy-voice-input`, UI Electron, docs do REPL, prompt pack agentic ou detalhes internos do app Coddy.

## Fronteira Operacional

O Coddy deve falar com VisionClip por:

- `CODDY_BIN`, para a UI/bridge localizar o binario Coddy.
- `CODDY_DAEMON_SOCKET`, para apontar ao socket Unix do daemon.
- `CODDY_CONFIG`, para configuracao propria do Coddy.
- `CoddyWireRequest` e `CoddyWireResult`, definidos em `crates/coddy-ipc`.

`VISIONCLIP_CONFIG` continua aceito pelo CLI Coddy apenas como fallback de compatibilidade local. Novos scripts do Coddy devem preferir `CODDY_CONFIG`.

`apps/coddy-electron` deve usar somente variaveis publicas do Coddy (`CODDY_BIN`, `CODDY_CONFIG`, `CODDY_DAEMON_SOCKET`) em scripts e bridge. O teste `repository_boundaries` cobre essa regra para evitar que a UI volte a depender de variaveis operacionais do VisionClip.

## Validacao

Antes de mover ou publicar repositorios separados:

```bash
cargo test -p coddy-ipc --test repository_boundaries
cargo test -p coddy-voice-input
cargo test -p coddy-client
cargo test -p coddy
```

O teste `repository_boundaries` deve falhar se Coddy voltar a depender diretamente de crates runtime ou fontes do VisionClip.

No lado VisionClip, o teste `visionclip-common --test repository_boundaries` deve falhar se diretorios Coddy voltarem ao repositorio VisionClip, se os pacotes Coddy forem adicionados de volta como membros do workspace, se qualquer manifest do VisionClip voltar a declarar dependencias de crates Coddy, se a feature `coddy-protocol` voltar a acionar path dependencies para o repositorio irmao, se `visionclip-common`/`visionclip` voltarem a depender de qualquer crate Coddy, se `coddy_core` ou `coddy_ipc` aparecerem nas fontes do VisionClip, se o bridge deixar de possuir o pipeline REPL Coddy, se `main.rs` deixar de ser apenas o adaptador de servicos nativos, se `main.rs` voltar a importar tipos core/protocolo do Coddy, se `main.rs` voltar a construir eventos/intents Coddy diretamente ou se `main.rs` voltar a chamar internals de resolucao de voz do Coddy.
