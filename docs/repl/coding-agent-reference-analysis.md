# Analise comparativa do coding agent

Data: 2026-04-29

## Escopo

Esta analise revisa os documentos `.agent` do Coddy e compara o estado atual do
projeto com duas implementacoes maduras de coding agents:

- opencode: https://github.com/anomalyco/opencode, revisado no commit
  `23b8ed7`.
- Crush: https://github.com/charmbracelet/crush, revisado no commit `86bb805`.

O objetivo nao e copiar arquitetura de forma literal. O objetivo e identificar
padroes comprovados que encaixam no Coddy, preservando a separacao futura entre
Coddy e VisionClip e evitando acoplamentos de repositorio.

Tambem ha uma restricao de produto importante: a UI/UX do Coddy deve preservar o
padrao visual ja existente nos prototipos e telas atuais, incluindo paleta,
transparencias, blur e glassmorphism. opencode e Crush servem como referencia
para a base logica do coding agent, nao como referencia visual.

Diretriz de stack: o runtime, contratos, seguranca, tool registry, agent loop,
filesystem guard, command guard, context manager, memory e evals devem ser
implementados majoritariamente em Rust por performance, robustez e distribuicao
simples. TypeScript deve ficar no necessario para Electron/UI, apresentacao,
estado visual e adaptadores de frontend.

## Leitura dos documentos `.agent`

Os documentos `.agent` do Coddy definem uma direcao consistente para um REPL
agentic moderno:

- `TARGET_ARCHITECTURE.md`: separa REPL, agent loop, planner, executor,
  registry de ferramentas, contexto, memoria, subagents, seguranca, evals,
  observabilidade e configuracao.
- `TOOLING_AND_MCP.md`: exige Tool Registry central com schema de entrada,
  schema de saida, nivel de risco, permissoes, timeout, approval policy,
  executor e logs.
- `SECURITY_POLICY.md`: define sandbox por padrao, approval explicito,
  command guard, filesystem guard, network policy, secret scanner, defesa
  contra prompt injection e audit log.
- `SUBAGENTS.md`: define explorer, planner, coder, reviewer,
  security-reviewer, test-writer, docs-writer e eval-runner como papeis
  declarativos.
- `EVALS_AND_METRICS.md`: define metricas como task success, validation pass,
  unsafe action block, approval accuracy, regression rate, context efficiency e
  latency.
- `IMPLEMENTATION_ROADMAP.md`: prioriza REPL, agent loop, tool registry,
  seguranca, contexto/memoria, subagents, MCP, evals e DX.

Conclusao: os documentos ja formam um bom contrato de produto e arquitetura.
A lacuna principal esta na implementacao incremental desses contratos em codigo.

## Estado atual do Coddy

O Coddy ja esta extraido para `/home/aethyr/Documents/coddy` e tem uma estrutura
adequada para virar repositorio independente:

- `apps/coddy`: CLI atual, com comandos de ask, voice, UI, screen, shortcuts,
  session e doctor.
- `apps/coddy-electron`: interface desktop.
- `crates/coddy-core`: dominio atual de sessoes, eventos, politicas,
  comandos, contexto visual, busca e intencoes de voz.
- `crates/coddy-ipc`: contrato binario versionado para comunicacao entre CLI,
  daemon/runtime e UI.
- `crates/coddy-client`: cliente IPC.
- `crates/coddy-voice-input`: entrada de voz.
- `docs/repl`: documentacao de arquitetura, contratos e plano.

Pontos fortes atuais:

- contratos Rust simples e testaveis em `coddy-core`;
- eventos de sessao e replay em `ReplEventLog` e `ReplEventBroker`;
- IPC com magic/versionamento em `coddy-ipc`;
- limites documentados para desacoplar Coddy de VisionClip;
- comandos CLI ja estabelecidos.

Lacunas atuais para um coding agent:

- nao ha Tool Registry real;
- nao ha executor de ferramentas com schema, risco e approval;
- nao ha agent loop de coding com etapas formais de plan/action/observation;
- nao ha permission service;
- nao ha filesystem guard nem command guard;
- nao ha read-before-edit tracking;
- nao ha snapshot/diff de arquivos;
- nao ha MCP runtime;
- nao ha subagents executaveis;
- nao ha eval runner.

## O que opencode faz bem

opencode usa uma arquitetura orientada a sessao. Os pontos mais relevantes para
o Coddy estao em `packages/opencode/src`:

- `tool/tool.ts`: define contrato de ferramenta com `id`, `description`,
  `parameters`, `execute`, contexto de sessao e callback `ask` para permissoes.
- `tool/registry.ts`: agrega ferramentas built-in, plugins, skills, filtros por
  agente/modelo e descricoes dinamicas.
- `permission/index.ts`: mantem rulesets, pedidos pendentes, respostas
  `once/always/reject`, eventos de permissao e aprovacao persistente por
  projeto.
- `session/processor.ts`: processa stream do LLM, cria partes de mensagem,
  registra chamadas de ferramenta, completa/fracassa tool calls, detecta loops e
  dispara compaction quando necessario.
- `session/run-state.ts`: controla concorrencia por sessao, cancelamento e
  estado busy/idle.
- `session/compaction.ts`: resume historico, preserva cauda recente e poda
  outputs antigos de ferramentas.
- `tool/bash.ts`: usa parser de shell para coletar padroes de comando e paths
  externos antes de pedir permissao.
- `tool/edit.ts`: gera diff, solicita permissao com metadata, respeita BOM e
  line endings, aplica formatacao e reporta diagnosticos LSP.
- `mcp/index.ts`: conecta MCP local/remoto, lida com status, OAuth, timeout,
  tools, prompts e resources.

Padroes aproveitaveis:

- ferramenta como unidade completa de contrato, permissao, execucao,
  metadata e truncamento;
- permissao baseada em ruleset e padroes, nao apenas prompt manual;
- session processor como camada separada do CLI/UI;
- compaction e pruning como parte do runtime, nao como tarefa manual;
- Tool Registry filtrado por agente/modelo/permissao;
- eventos para UI observar tool calls e approvals em tempo real.

Cuidados antes de adaptar:

- opencode e TypeScript/Effect e usa AI SDK; Coddy e Rust. O desenho deve ser
  portado como contratos e invariantes, nao como dependencia ou estilo runtime.
- opencode tem plugin system amplo; Coddy deve comecar com API interna pequena.
- MCP deve vir depois do guardrail local, conforme a propria `.agent` do Coddy.

## O que Crush faz bem

Crush e uma referencia forte para terminal-first local agent. Os pontos mais
relevantes estao em `internal`:

- `internal/agent/coordinator.go`: monta modelos, prompts, ferramentas,
  subagents, hooks, skills, LSP e MCP por agente.
- `internal/agent/agent.go`: implementa runtime de sessao, fila por sessao,
  cancelamento, streaming, sumarizacao e historico.
- `internal/permission/permission.go`: permission service com pedidos,
  grant once, grant persistent, deny, auto-approve por sessao, allowlist e
  integracao com hooks.
- `internal/agent/tools/edit.go`: exige que o arquivo tenha sido lido antes de
  editar e bloqueia edicao se o arquivo mudou desde a leitura.
- `internal/filetracker/service.go`: registra leituras por sessao e permite
  validar read-before-edit.
- `internal/agent/tools/bash.go`: separa comandos read-only seguros de comandos
  que exigem permissao, aplica bloqueadores e suporta jobs em background.
- `internal/agent/hooked_tool.go` e `internal/hooks/runner.go`: executam hooks
  PreToolUse, permitem reescrita de input, allow/deny/halt e metadata.
- `internal/agent/loop_detection.go`: detecta repeticao de chamadas de
  ferramenta por assinatura estavel.
- `internal/agent/agent_tool.go`: expõe subagent como ferramenta paralela.

Padroes aproveitaveis:

- read-before-edit deve ser fundacao inicial do Coddy;
- permission service deve ser separado de cada tool;
- hooks devem interceptar tool calls antes de permissao/execucao;
- shell deve ter command guard antes de execucao;
- outputs de tool devem carregar metadata estruturada;
- subagent pode nascer como tool simples depois que o runtime local estiver
  seguro.

Cuidados antes de adaptar:

- Crush mistura partes do dominio com stack Go/Charm/Fantasy. Coddy deve
  preservar contratos Rust e IPC proprio.
- A permissao atual de Crush e mais operacional que declarativa; Coddy deve
  manter rulesets auditaveis desde o inicio.
- Jobs em background sao uteis, mas nao devem ser prioridade antes de command
  guard, timeouts e cancelamento.

## Comparacao direta

| Area | Coddy hoje | opencode | Crush | Recomendacao para Coddy |
|---|---|---|---|---|
| Session model | Sessao e eventos basicos | Sessao persistente com partes, status e snapshots | Sessao com mensagens, fila e sumarizacao | Evoluir `ReplSession` para runtime de runs/tool parts sem quebrar IPC |
| Agent loop | Documentado, nao implementado | Processor stream-driven | SessionAgent/Coordinator | Criar crate de runtime separado de UI/CLI |
| Tool Registry | Ausente | Registry built-in + plugin + skills | BuildTools por agente | Comecar com registry interno tipado em Rust |
| Tool contract | Eventos `ToolStarted/Completed` genericos | Schema, execute, ctx, metadata | Fantasy AgentTool com params e metadata | Definir `ToolDefinition`, `ToolCall`, `ToolResult` em `coddy-core` |
| Permission | Politica de assessment, nao de tools | Ruleset allow/deny/ask | Service de request/grant/deny | Criar permission service separado e auditavel |
| File edit safety | Ausente | Diff + approval + LSP | Read-before-edit + modtime + history | Implementar read tracker antes de edit/write |
| Shell safety | Ausente | Parser + permission patterns | Banned commands + read-only allowlist | Implementar command guard antes de shell tool |
| Context | Contexto visual/workspace simples | Compaction/prune/token budget | Auto-summarizacao | Separar context manager de session state |
| MCP | Documentado | Client completo local/remoto/OAuth | MCP tools/resources | Planejar depois do registry e permission bridge |
| Subagents | Documentado | Task tool/agentes filtrados | Agent tool paralelo | Implementar depois do agent loop minimo |
| Evals | Documentado | Testes e specs amplos | Golden tests e testdata | Criar eval runner simples e casos locais |

## Decisoes arquiteturais recomendadas

1. Manter Coddy independente de VisionClip.

   O runtime do Coddy deve viver dentro de `/home/aethyr/Documents/coddy` e nao
   deve importar crates do VisionClip. A integracao futura com VisionClip deve
   passar por contratos IPC/versionados ou adaptadores opcionais.

2. Separar contratos de runtime.

   `coddy-core` deve conter tipos puros: tool definitions, risk levels,
   permission requests, run state, events e schemas. A execucao concreta deve
   viver em um crate novo, recomendado como `crates/coddy-agent`.

3. Manter Rust como eixo do agent runtime.

   A base logica observada em opencode e Crush deve ser reescrita no modelo do
   Coddy: Rust para dominio, execucao, policies, guardrails, IO local e evals;
   TypeScript para UI e adaptadores de apresentacao. Isso evita acoplamento da
   logica agentic ao Electron e facilita publicar Coddy como repositorio
   independente.

4. Preservar a linguagem visual atual.

   A UI deve continuar seguindo os prototipos do Coddy: cores atuais,
   superficies translucidas, blur, glassmorphism e densidade visual do REPL. A
   comparacao com opencode/Crush nao autoriza trocar o design system.

5. Construir seguranca antes de poder destrutivo.

   A primeira familia de tools deve ser read-only: list/read/search/status. Edit,
   write, shell e MCP devem esperar por permission service, audit log,
   read-before-edit e command guard.

6. Usar eventos como fronteira com UI.

   O Electron/CLI nao deve conhecer implementacao interna de ferramentas. Ele
   deve observar eventos: tool requested, permission requested, permission
   answered, tool started, tool completed, run completed.

7. Priorizar testabilidade.

   Cada bloco deve ter testes unitarios no core e testes de contrato IPC quando
   o wire format mudar.

## Proximos passos de implementacao

### Bloco 1: contratos de ferramenta em `coddy-core`

Status: implementacao inicial concluida em 2026-04-29.

Objetivo:

- criar base tipada para ferramentas sem executar nada ainda.

Entregaveis:

- `ToolName`;
- `ToolRiskLevel`;
- `ToolPermission`;
- `ApprovalPolicy`;
- `ToolDefinition`;
- `ToolInputSchemaRef` ou schema JSON serializavel;
- `ToolCall`;
- `ToolResult`;
- `ToolError`;
- testes de serializacao e invariantes basicos.

Arquivos provaveis:

- `crates/coddy-core/src/tool.rs`;
- `crates/coddy-core/src/lib.rs`;
- testes unitarios no proprio modulo.

Validacao:

- `cargo test -p coddy-core`;
- `cargo test -p coddy-ipc --test repository_boundaries`, se o IPC expuser
  algum tipo novo.

### Bloco 2: permission model e approval requests

Status: implementacao inicial concluida em 2026-04-29.

Objetivo:

- representar allow/deny/ask antes de qualquer tool perigosa.

Entregaveis:

- `PermissionRule`;
- `PermissionRuleset`;
- `PermissionRequest`;
- `PermissionDecision`;
- matching simples por tool/pattern;
- eventos de permission requested/replied.

Arquivos provaveis:

- `crates/coddy-core/src/permission.rs`;
- `crates/coddy-core/src/event.rs`;
- `crates/coddy-core/src/session.rs`.

### Bloco 3: runtime read-only de tools

Status: implementacao inicial concluida em 2026-04-29.

Objetivo:

- introduzir `crates/coddy-agent` com registry e executor read-only.

Entregaveis:

- registry interno;
- tools `list_files`, `read_file`, `search_files`;
- path normalization;
- workspace root guard;
- metadata estruturada;
- testes com fixture temporaria.

### Bloco 4: file tracker e edit safety

Status: fundacao inicial concluida em 2026-04-29.

Objetivo:

- preparar escrita segura antes de `edit_file` e `write_file`.

Entregaveis:

- read tracker por sessao;
- validacao de modtime/hash desde a ultima leitura;
- diff preview;
- approval obrigatorio para escrita;
- eventos de patch/diff.

### Bloco 5: command guard e shell tool controlada

Status: fundacao inicial concluida em 2026-04-29.

Objetivo:

- executar comandos somente com guardrails.

Entregaveis:

- parser/segmentacao inicial de comandos;
- blocklist explicita;
- allowlist read-only;
- timeout;
- cwd guard;
- output truncation;
- approval para comandos nao read-only.

### Bloco 6: agent loop minimo

Objetivo:

- conectar entrada, contexto, plano simples, tools, observacao e resposta.

Entregaveis:

- `RunState`;
- `AgentStep`;
- `PlanItem`;
- `Observation`;
- loop sem provider real inicialmente, com executor mockado para teste.

### Bloco 7: subagents, MCP e evals

Objetivo:

- adicionar extensibilidade depois do nucleo seguro.

Entregaveis:

- subagent como tool;
- MCP adapter atras de permission bridge;
- eval runner com baseline local;
- golden tests para fluxos de coding.

## Primeiro bloco recomendado agora

Implementado: contratos de ferramenta em `coddy-core`.

Motivo:

- e pequeno e reversivel;
- nao executa comandos nem escreve arquivos;
- cria o vocabulario comum que REPL, UI, permission service, runtime e IPC vao
  usar;
- reduz risco de acoplamento com VisionClip;
- prepara o caminho para registry e permission model sem pular etapas.

Critero de sucesso:

- `cargo test -p coddy-core` passa;
- os tipos sao serializaveis com `serde`;
- nenhuma dependencia externa nova e adicionada sem necessidade;
- nenhum codigo do VisionClip e referenciado.

## Proximo bloco recomendado agora

Implementado: preview de edicao com approval obrigatorio, ainda sem aplicar
escrita em disco por padrao.

Motivo:

- `coddy-agent` ja possui registry read-only, workspace guard, lifecycle events
  e read tracker por sessao;
- `validate_recent_read` ja bloqueia arquivos nao lidos ou modificados desde a
  leitura;
- o proximo passo seguro e gerar diff/preview e permission request antes de
  qualquer escrita real;
- isso preserva o principio Rust-first e deixa a UI apenas apresentar approval
  no padrao visual atual.

Escopo recomendado:

- criar modelo de `EditPreview` no `coddy-agent` ou `coddy-core`;
- validar leitura recente antes de preview;
- gerar diff textual para replace simples;
- criar `PermissionRequest` com metadata de diff;
- nao aplicar escrita ate existir fluxo de approval;
- cobrir com testes de arquivo nao lido, arquivo stale e preview aprovado como
  contrato, sem mutar o filesystem.

## Proximo bloco recomendado

Implementado: aplicacao controlada de edicao somente apos approval explicito.

Motivo:

- `EditPreview` ja valida read-before-edit, stale read e gera diff;
- `PermissionRequest` ja carrega metadata de diff para a UI apresentar;
- ainda falta uma etapa separada que receba `PermissionReply::Once` ou
  `PermissionReply::Always` e aplique a escrita;
- essa separacao preserva a regra de seguranca: preview e approval antes de
  mutacao.

Escopo recomendado:

- criar `ApprovedEdit` ou `EditApplication`;
- exigir `PermissionReply` aprovado;
- revalidar fingerprint imediatamente antes da escrita;
- escrever arquivo de forma atomica quando possivel;
- registrar metadata de patch e `ToolResult`;
- manter testes cobrindo rejeicao, stale after approval e sucesso controlado.

## Proximo bloco recomendado

Implementado: command guard para preparar uma futura shell tool controlada.

Motivo:

- leitura, preview, approval e edicao controlada ja existem no runtime Rust;
- shell e o proximo maior risco de seguranca;
- antes de executar qualquer comando real, o Coddy precisa classificar comandos,
  bloquear padroes destrutivos e separar read-only de comandos que exigem
  approval;
- esse bloco pode ser implementado sem executar comandos, apenas como parser e
  avaliador testavel.

Escopo recomendado:

- criar `CommandGuard` no `coddy-agent`;
- definir `CommandRisk`, `CommandDecision` e `BlockedCommandReason`;
- bloquear `rm -rf`, `git reset --hard`, `git clean -fd`, `sudo`, `curl | sh`,
  `wget | sh`, `chmod -R`, `chown -R` e equivalentes;
- reconhecer comandos read-only seguros como `ls`, `pwd`, `git status`,
  `cargo test`, `cargo fmt --check`;
- gerar `PermissionRequest` para comandos nao read-only;
- adicionar testes sem executar shell.

## Proximo bloco recomendado

Implementado: `ShellPlan` sem execucao real e registry local de contratos.

Motivo:

- `CommandGuard` ja bloqueia padroes destrutivos, permite comandos read-only e
  gera `PermissionRequest` para comandos que exigem approval;
- antes de executar shell, o Coddy precisa transformar a decisao do guard em um
  plano auditavel com cwd, timeout, descricao, risk e approval state;
- isso permite conectar UI/approval sem abrir processo ainda.

Escopo entregue:

- `ShellPlan` e `ShellPlanner`;
- cwd obrigatoriamente resolvido dentro do workspace;
- timeout padrao e maximo;
- decisao do `CommandGuard` anexada ao plano;
- endurecimento da allowlist read-only para exigir approval quando houver
  redirecionamento, pipe, controle de shell, `sed -i`, `find -delete` ou
  mutacao de branch via `git branch`;
- `Blocked` mapeado para `ToolResultStatus::Denied`;
- `RequiresApproval` mapeado para evento `PermissionRequested`;
- `AgentToolRegistry` com contratos locais para read-only, edit preview/apply e
  `shell.run`;
- testes sem `std::process::Command`.

## Proximo bloco recomendado

Implementado: executor de shell controlado atras de approval, ainda com escopo
minimo.

Motivo:

- o comando ja passa por `CommandGuard`;
- o `ShellPlan` ja define cwd, timeout, risk e approval state;
- a UI/CLI podem receber `PermissionRequested` antes de qualquer mutacao;
- a nova camada recebe plano aprovado, executa com timeout, captura stdout/stderr
  truncados e emite eventos padronizados.

Escopo entregue:

- `ShellExecutor` separado de `ShellPlanner`;
- aceita apenas `ShellApprovalState::NotRequired` ou approval aprovado;
- executa sempre dentro do cwd resolvido no workspace;
- impoe timeout e limite de output;
- retorna `ToolResult` com metadata de exit code, duration, stdout/stderr
  truncados;
- nao adiciona rede especial nem escalacao de privilegio;
- cobre comandos read-only, comandos aprovados, rejeicao, bloqueio, timeout e
  truncamento.

## Proximo bloco recomendado

Implementado: integracao do runtime local de tools ao agent loop minimo, ainda
sem provider real.

Motivo:

- contratos, permissoes, registry, filesystem guard, edit safety, command guard,
  shell planning e shell execution ja existem no crate Rust;
- falta uma camada de orquestracao que transforme uma intencao em passos
  observaveis e roteie tools sem acoplar UI/CLI;
- isso prepara a futura ligacao com modelos, subagents, MCP e a UI glassmorphism
  atual apenas por eventos/estado.

Escopo entregue:

- `LocalToolRouter` pequeno para read-only, edit e shell;
- armazenamento interno de previews de edicao e shell plans pendentes por
  `PermissionRequest`;
- `reply_permission` para aplicar edit/shell apenas apos resposta humana;
- `RunState`, `AgentStep`, `PlanItem`, `Observation` e `LocalAgentRuntime`;
- eventos de run/tool/permission agregados no estado local;
- testes cobrindo read-only, edit approval/reject, shell approval, shell block,
  observations e conclusao de run;
- nenhuma alteracao visual ou comando de usuario ainda.

## Proximo bloco recomendado

Implementado: contexto local e executor de plano deterministico antes de plugar
LLM.

Motivo:

- o runtime ja consegue rotear tools e registrar observations;
- falta um contexto estruturado que alimente o futuro planner/model provider sem
  depender da UI ou de VisionClip;
- um executor deterministico permite validar fluxos agentic completos antes de
  introduzir chamadas reais de modelo, subagents ou MCP.

Escopo entregue:

- `ContextSnapshot` com workspace root, goal, run status, plan, observations,
  tools disponiveis e contagem de eventos;
- truncamento conservador de texto de observations grandes;
- `DeterministicPlanExecutor` para executar uma lista de `DeterministicPlanItem`
  ate completar, falhar ou aguardar approval;
- retomada deterministica via `resume_after_permission`;
- testes cobrindo plano read-only completo, pausa/retomada de shell com approval
  e falha limpa em tool inexistente/arquivo ausente;
- manter TypeScript fora desse bloco, preservando a UI atual para integração
  posterior por eventos.

## Proximo bloco recomendado

Implementado: golden tests/evals locais em cima do executor deterministico.

Motivo:

- agora existe um caminho completo e sem LLM para executar fluxos agentic;
- os `.agent` pedem evals, baseline e regressao antes de expandir MCP/subagents;
- golden tests vao proteger as garantias atuais de seguranca: read-before-edit,
  approval antes de shell/edit e parada em falha.

Escopo entregue:

- `EvalCase`, `EvalExpectations`, `EvalRunner`, `EvalReport` e
  `EvalSuiteReport`;
- evals locais para read-only, edit approval/reject, shell approval e shell
  block;
- reports com status final, approvals solicitados, failures e plan report;
- suite com contagem de casos aprovados/reprovados;
- manter tudo local/offline, sem chamadas de rede e sem provider real.

## Proximo bloco recomendado

Implementado: fundacao do REPL shell desacoplada da UI.

Motivo:

- a Fase 1 dos `.agent` pede comandos basicos antes de expandir o agent loop;
- o parser precisa viver no core Rust para ser reaproveitado por CLI, Electron e
  uma futura UI terminal sem misturar regras de dominio com renderizacao;
- o alias `coddy repl` deve abrir o terminal flutuante atual sem criar
  dependencia com VisionClip.

Escopo entregue:

- `ReplShellInput`, `ReplShellAction`, `ReplShellContext` e
  `ReplShellResponse`;
- parser puro para texto livre, entrada vazia e comandos `/help`, `/?`,
  `/status`, `/config`, `/tools`, `/exit` e `/quit`;
- handler que converte texto livre em `ReplCommand::Ask` com
  `ContextPolicy::WorkspaceOnly`;
- respostas deterministicas para status, config, tools, help e comando
  desconhecido;
- `coddy repl` como alias para `ReplCommand::OpenUi` em
  `ReplMode::FloatingTerminal`;
- testes focados cobrindo parser, handler, comandos, ordenacao de tools,
  entrada vazia e parsing CLI.

## Proximo bloco recomendado

Implementado: adaptador terminal stdin/stdout para o REPL shell.

Motivo:

- os comandos slash ja estao no dominio, mas ainda nao existe loop de
  input/output com historico;
- o proximo passo da Fase 1 e exercitar `/help`, `/status`, `/tools`, `/config`
  e `/exit` em uma sessao real;
- o loop deve continuar em Rust e enviar apenas comandos/eventos estruturados
  para preservar o desacoplamento da UI glassmorphism em TypeScript.

Escopo entregue:

- `apps/coddy/src/repl_terminal.rs` como adaptador fino, sem regra de dominio;
- `coddy repl --terminal` para executar um loop local stdin/stdout;
- renderizacao testavel de `ReplShellResponse`;
- despacho de texto livre como `ReplCommand` estruturado para o daemon;
- `/exit` com encerramento limpo;
- refatoracao de formatacao de `CoddyResult` para reaproveitamento no loop.

## Proximo bloco recomendado

Implementado: historico local persistente no terminal REPL.

Motivo:

- o terminal ja renderiza slash commands e despacha comandos, mas ainda nao
  persiste historico;
- `/tools` ainda usa a lista recebida no contexto do shell e precisa ser
  alimentado por um registry real exposto sem acoplar `apps/coddy` ao runtime;
- o proximo passo deve melhorar ergonomia sem alterar a UI Electron.

Escopo entregue:

- `TerminalHistory` com limite conservador, normalizacao e deduplicacao
  consecutiva;
- persistencia em arquivo no diretorio de dados proprio do Coddy;
- `/exit`, `/quit` e entradas vazias nao sao gravados;
- `Ctrl+C` encerra o modo terminal com mensagem limpa;
- falhas de leitura/escrita do historico geram `warn` e nao quebram a sessao;
- testes cobrindo normalizacao, limite, roundtrip e criacao de diretorio pai.

## Proximo bloco recomendado

Implementado: contrato read-only de tools para alimentar `/tools`.

Motivo:

- `/status` ja pode consultar snapshot do daemon, mas `/tools` ainda recebe
  lista vazia no contexto local;
- o caminho correto e expor um contrato read-only de tools pelo runtime/daemon,
  sem acoplar `apps/coddy` diretamente ao `coddy-agent`;
- essa etapa prepara UI Electron e terminal para exibirem as mesmas tools.

Escopo entregue:

- `ReplToolsJob`, `CoddyRequest::Tools` e `CoddyResult::ReplTools` no IPC;
- `ReplToolCatalogItem` e `CoddyResult::ReplToolCatalog` para transportar
  metadata publica do registry sem quebrar o resultado legado;
- `CoddyClient::tools()` com validacao de resposta e request id;
- `CoddyClient::tool_catalog()` aceita tanto catalogo rico quanto resposta
  legada de nomes;
- `load_repl_shell_context` passa a consultar tools pelo cliente Coddy;
- `/tools` continua com fallback vazio quando o daemon ainda nao suporta o
  contrato;
- formatacao explicita de `ReplTools` e `ReplToolCatalog` no CLI;
- testes focados no IPC, client e CLI.

## Proximo bloco recomendado

Implementar o handler `Tools` no daemon/runtime.

Motivo:

- o contrato ja existe no client e no IPC, mas o lado servidor ainda precisa
  responder com o catalogo real;
- o catalogo deve vir do runtime/registry, mantendo `apps/coddy` desacoplado de
  `coddy-agent`;
- depois disso, terminal e UI podem compartilhar a mesma fonte de verdade.

Escopo recomendado:

- adicionar handler de `CoddyRequest::Tools` no daemon que fala o protocolo
  Coddy;
- retornar catalogo ordenado vindo de `AgentToolRegistry`, preferindo
  `ReplToolCatalog` e mantendo `ReplTools` apenas como compatibilidade;
- adicionar teste de roundtrip servidor/cliente;
- manter fixtures versionadas de evals como bloco posterior.

## Revisao `texts/` de 2026-04-30

Arquivos analisados:

- `texts/Detalhes Tecnicos de IA Avancada.pdf`
- `texts/Detalhes de Modelos e Ferramentas IA.pdf`
- `texts/Pesquisa Detalhada Claude Mythos e Ferramentas.pdf`
- `texts/Detalhes Tecnicos Modelos IA Avancados.pdf`

Os PDFs reforcam uma direcao correta para o Coddy: modelos diferentes devem
entrar em uma plataforma mais forte que o proprio modelo. A parte aproveitavel
e o padrao "fat platform, thin agents": harness deterministico, permissao,
contexto, tools, memoria, subagents, evals e telemetria devem compensar
variacao entre modelos locais e modelos remotos.

Pontos que devem virar codigo:

- agent loop explicito com fases observaveis de context loading, planning,
  action, observation, validation e response;
- subagents com contexto isolado, allowed tools, timeout, budget de contexto,
  checklist, safety notes e retorno resumido;
- roteamento deterministico antes da inferencia para reduzir dependencia de
  instrucao solta no prompt;
- readiness gate antes de executar um subagent real;
- compaction e pruning como responsabilidade do runtime;
- memoria persistente com escopo, redacao de secrets e politica de delecao;
- MCP somente depois de tool registry, permission bridge e audit log locais.

Pontos tratados como especulativos ate haver fonte primaria:

- detalhes internos de modelos como "Claude Mythos", "Opus 4.7" e "GPT-5.5";
- numeros de parametros, custos, benchmarks fechados e codinomes internos;
- alegacoes de capacidades ciberneticas nao acompanhadas de system card ou
  avaliacao publica verificavel.

Fontes publicas verificadas nesta revisao apontam para os mesmos invariantes
praticos sem depender dessas alegacoes: o harness orquestra modelo e tools,
subagents precisam de contexto separado e permissoes proprias, hooks devem
interceptar ferramentas, e MCP e um protocolo de contexto/tooling que nao
substitui governanca local.

## Proximo bloco recomendado apos a revisao dos textos

Implementado como fundacao incremental: readiness scoring para contratos de
handoff de subagents.

Motivo:

- e pequeno, testavel e reversivel;
- nao executa subagents nem altera arquivos do usuario;
- cria uma metrica local de 0 a 100 para impedir execucao futura de handoffs
  incompletos;
- conecta pesquisa, documentos `.agent` e runtime real sem aumentar superficie
  de risco.

Escopo entregue:

- `subagent.prepare` agora retorna `readinessScore` e `readinessIssues`;
- o runtime publica `SubagentHandoffPrepared` com esses campos para UI e
  auditoria;
- o runtime publica `SubagentLifecycleUpdated` como gate observavel:
  `Prepared` para handoffs completos e `Blocked` para readiness incompleto;
- o snapshot Rust reduz esse lifecycle em `subagent_activity`, mantendo
  reconnect e bootstrap alinhados ao stream de eventos;
- o reducer Rust e o reducer TypeScript validam transicoes:
  `Prepared -> Approved -> Running -> Completed/Failed`; saltos como
  `None -> Running` ou readiness abaixo de 100 viram `Blocked`;
- `SubagentExecutionGate` cria o plano de inicio do executor real sem side
  effects, bloqueando readiness incompleto e aguardando aprovacao quando
  necessario;
- o system prompt recebe um resumo do score, mas o evento preserva valores
  completos para observabilidade;
- o frontend exibe a atividade de subagents no painel agentic mantendo o padrao
  glassmorphism existente;
- testes cobrem contratos prontos, incompletos e reducer de lifecycle.

Proxima melhoria recomendada:

- persistir historico de lifecycle por run em storage local, nao apenas no
  snapshot em memoria;
- conectar `Prepared -> Approved -> Running` a um executor isolado, mantendo
  bloqueio quando `readinessScore < 100`;
- adicionar evals deterministicas que falhem quando um subagent sem allowed
  tools, output schema ou preview de edit passa pelo gate.
