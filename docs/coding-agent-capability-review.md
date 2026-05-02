# Coding Agent Capability Review

Data da revisao: 2026-05-02.

## Resumo

O Coddy ja possui uma base forte para cenarios de codificacao assistida: loop
agentic com tools, runtime Rust, frontend Electron, workspace, historico,
subagents deterministas, prompt battery, guardrails de shell, fluxo de approval
e evals locais. A principal diferenca em relacao a agentes top-tier esta menos
no modelo isolado e mais no harness: contexto, plano, ferramentas, permissoes,
validacao, isolamento de execucao, memoria, MCP e registro de evidencias.

Conclusao atual: Coddy esta apto para ajudar na analise de codebases, revisoes,
planejamento e tarefas de implementacao com revisao humana. Ainda nao deve ser
tratado como agente autonomo de merge/release em producao, porque execucao
isolada de subagents, MCP runtime, compaction adaptativa e PR automation ainda
estao incompletos.

## Scorecard atual

| Dimensao | Score | Avaliacao |
| --- | ---: | --- |
| Analise local de codebase | 8.0/10 | Workspace, filesystem/search tools, historico e comandos `/workspace`, `/tools`, `/status` e `/capabilities` estao integrados. |
| Fluxo de codificacao | 8.0/10 | `/code`, `/plan`, `/review` e `/test` reforcam explorar, planejar, editar incrementalmente, validar e reportar evidencias. |
| Tools e seguranca | 8.0/10 | Registry, risk level, permission primitives, shell guard, redacao de secrets e guardrails de comandos estao presentes. |
| Subagents | 7.0/10 | Registro, roteamento, preparo, team plan e reducer contracts sao testados; falta runtime isolado executavel por subagent. |
| Pesquisa e contexto externo | 6.5/10 | O produto ainda depende mais de contexto local/modelo do que de conectores MCP e web/documentacao live integrados ao runtime. |
| Evals e MLOps | 8.0/10 | Multiagent eval, quality eval e prompt battery existem com JSON e comparacao baseline; falta matriz continua multi-provider. |
| Autonomia de producao | 6.5/10 | Bom para assistencia supervisionada; ainda falta worker isolado, branch/PR automation e politicas de rede/MCP maduras. |
| Qualidade geral do coding agent | 7.6/10 | Forte base local, com lacunas claras e mensuraveis para chegar ao nivel de agentes cloud/CLI top-tier. |

Nesta rodada foram aplicadas melhorias incrementais para aproximar o Coddy de
um harness de coding agent mais previsivel:

- Prompt default do coding agent reforcado para exigir exploracao antes de
  edicao, TDD quando pratico, validacao real e resumo final com arquivos,
  checks e riscos.
- Novo workflow `/code` e alias `/implement` no REPL e no Electron para
  conduzir tarefas de implementacao com TDD, clean code, clean architecture e
  validacao incremental.
- Regras de evidencia no prompt default para impedir alegacoes sobre testes,
  cobertura ou arquivos ausentes sem leitura ou busca feita no proprio turno.
- Retry model-backed mais robusto para respostas vazias de providers
  OpenAI-compatible: Coddy tenta novamente de forma limitada e adiciona uma
  instrucao interna curta pedindo resposta nao vazia quando a falha foi
  `response did not include assistant content or tool calls`.
- Bateria live com OpenRouter/DeepSeek V4 Flash validada sem expor a chave:
  o score subiu de 86 para 94 e a taxa de erro de provider caiu de 14% para 6%
  em amostra de 50 prompts.
- README atualizado para refletir o estado real de readiness, comandos slash,
  validacoes, limitacoes conhecidas e fluxos de workspace/historico.

## Metodo de analise

A avaliacao combinou:

- leitura da arquitetura local e dos docs `.agent/*`;
- revisao dos comandos CLI e slash commands reais;
- resultados das suites Rust/TypeScript/Electron ja executadas nesta branch;
- prompt battery live segura contra OpenRouter/DeepSeek V4 Flash;
- comparacao com referencias oficiais e benchmarks publicos de agentes de
  codificacao.

As comparacoes abaixo sao inferencias de engenharia a partir das fontes
oficiais listadas no fim deste documento. Elas nao afirmam que Coddy possui a
mesma performance dos modelos citados; medem o harness do produto e seus gaps.

## Comparacao com agentes e modelos top-tier

### OpenAI Codex / GPT-5.x Codex

Pontos fortes observados:

- Agent loop explicito: prompt, chamada de tool, observacao e nova inferencia.
- Instrucoes por projeto e contexto de ambiente influenciam execucao.
- Ferramentas estruturadas, inclusive shell, plano, web e MCP.
- Controles de sandbox/approval reduzem risco em escrita e execucao.
- Modelos Codex atuais sao ajustados para tarefas longas de agentic coding.

Implicacao para Coddy:

- Manter o loop Coddy como harness independente do modelo.
- Fortalecer instrucoes do sistema e workflows para reduzir variancia entre
  providers.
- Evoluir MCP, permissoes por tool e compaction, sem depender apenas de prompt.

### Claude Code / Claude Opus e Sonnet

Pontos fortes observados:

- Workflow recomendado: explorar, planejar, implementar, validar e commitar.
- Forte enfase em dar ao agente uma forma de verificar o proprio trabalho.
- Gestao explicita de contexto, memoria, subagents, hooks e permissoes.
- Subagents especializados podem ter contexto, prompts, tools e permissoes
  proprias.
- Checkpoints, hooks e automacao de sessoes apoiam fluxos reais de engenharia.

Implicacao para Coddy:

- Tornar validacao e criterios de sucesso parte do prompt default e dos slash
  workflows.
- Usar subagents principalmente para investigacao e revisao, reduzindo ruido no
  contexto principal.
- Adicionar checklists/status visiveis para plano, execucao e validacao.

### Gemini CLI / Gemini 3

Pontos fortes observados:

- CLI open source com loop agentic/ReAct.
- Built-in tools para filesystem, shell, grep, web e MCP.
- Grande janela de contexto em modelos Gemini 3 Pro.
- Integracao com Code Assist e comandos como `/memory`, `/stats`, `/tools` e
  `/mcp`.
- Sistema de extensoes empacota comandos customizados, servidores MCP e contexto
  reutilizavel.

Implicacao para Coddy:

- Priorizar comandos de introspeccao e status operacional.
- Tornar `/tools`, `/workspace`, `/models`, `/history`, `/new` e agora `/code`
  consistentes entre Desktop, FloatingTerminal e CLI.
- Medir custo de contexto para evitar degradacao em sessoes longas.

### GitHub Copilot Coding Agent

Pontos fortes observados:

- Execucao em ambiente efemero controlado por GitHub Actions.
- Gera PRs, commits e logs revisaveis.
- Possui protecoes integradas: CodeQL, secret scanning, dependency scanning,
  branches restritas e revisao humana.
- Pode ser customizado com instrucoes, MCP, custom agents, hooks e skills.
- O fluxo cloud-agent combina investigacao do repo, alteracao em branch propria,
  execucao de testes/linters e abertura de PR para revisao humana.

Implicacao para Coddy:

- Fortalecer trilha de auditoria local e relatorios de sessao.
- Associar tarefas de codificacao a validacoes e resultados rastreaveis.
- Continuar tratando secrets, rede e shell como superficies sensiveis.

### Benchmarks publicos: SWE-bench Verified e Terminal-Bench

Pontos fortes observados:

- SWE-bench Verified mede resolucao de issues reais de repositorios Python com
  500 problemas revisados por humanos, sendo uma referencia util para patches
  reais.
- Terminal-Bench mede agentes em ambientes de terminal, incluindo uso de shell,
  arquivos, instalacao e validacao sob restricoes.
- Ambos reforcam que bons resultados dependem do sistema completo: modelo,
  contexto, ferramenta, isolamento, tempo, feedback de testes e politica de
  execucao.

Implicacao para Coddy:

- Criar uma matriz interna parecida, com tarefas pequenas de bug fix, refactor,
  frontend visual, seguranca, docs e testes.
- Medir sucesso por evidencia objetiva: patch correto, checks passados, secrets
  preservados, uso correto de tools e qualidade da resposta final.
- Separar falhas de modelo, falhas de provider, falhas de tool e falhas de
  planejamento para melhorar o harness sem mascarar problemas.

## Lacunas atuais prioritarias

1. Runtime isolado executavel para subagents com timeout, tools limitadas,
   contexto proprio e reducer final.
2. Status/checklist visual do workflow de codificacao no Desktop e
   FloatingTerminal.
3. Export de relatorio de sessao com arquivos alterados, tools, checks e erros.
4. Context budget por run, incluindo resumo de tool outputs grandes e ranking
   de relevancia.
5. MCP adapter com permission bridge e catalogo unificado de tools.
6. Evals especificos para tarefas reais de patch: bug fix, refactor, test
   writing, security fix e frontend visual validation.
7. Memoria persistente segura para convencoes do projeto e comandos de teste.
8. Melhor separacao entre plan-only, code, review e validation no runtime.
9. Matriz live multi-provider com OpenRouter, OpenAI-compatible, Ollama, Gemini
   API, Azure e Vertex, sempre sem expor credenciais.
10. Worker local para branch/commit/PR assistido com auditoria e rollback claro.

## Recomendacoes objetivas

### P0: Confiabilidade do harness de coding

- Transformar `/code` em estado de runtime observavel: `Inspecting`, `Planning`,
  `Editing`, `Validating`, `Summarizing`, `Blocked`, `Cancelled`.
- Persistir evidencias de cada run: arquivos lidos, tools chamadas, checks
  executados, status e erros normalizados.
- Adicionar teste de regressao para prompts que pedem analise de codebase e
  garantem que tools registradas usam nomes locais validos.
- Manter observacoes de tools dentro de budget previsivel para evitar que
  arquivos grandes dominem a proxima inferencia do modelo.

### P1: Subagents reais

- Criar sessao isolada por subagent com prompt, contexto, permissao e toolset
  proprios.
- Exigir output estruturado por role e consolidar por reducer deterministico.
- Adicionar eval que falha se subagent read-only tentar escrever ou executar
  shell mutavel.

### P1: Contexto e pesquisa

- Expandir resumo automatico de tool outputs longos para ranking por relevancia,
  citacoes por trecho e re-leitura focada quando o conteudo omitido for
  necessario.
- Criar ranking simples por workspace: manifestos, docs, arquivos recentemente
  tocados, testes relacionados e resultados de busca.
- Integrar MCP como fonte externa permissionada, preservando redacao de secrets
  e politica de rede.

### P2: Benchmark continuo

- Manter baseline local de prompt battery e multiagent eval.
- Rodar bateria pequena em PR e bateria maior sob demanda.
- Registrar `modelErrorRate`, `toolErrorRate`, `validationPassRate`,
  `secretLeakCount`, `cancelRecoveryRate` e `averageRunLatencyMs`.

## Rodada atual: compactacao de observacoes

Problema validado na codebase: o runtime limitava numero de rounds de tools,
mas a observacao textual enviada ao follow-up do modelo podia carregar um
arquivo grande quase inteiro. Isso desperdicava contexto e ia contra o padrao
observado em agentes top-tier: gerenciamento explicito de janela de contexto,
compaction e recuperacao focada.

Melhoria implementada:

- limite de 12 Ki chars por observacao de tool enviada ao follow-up do modelo
  no fluxo `/ask`;
- compactacao estrutural tambem no loop agentic direto do `coddy-agent`,
  aplicada antes da serializacao da mensagem `tool`;
- preservacao de JSON valido apos compactacao, evitando que o follow-up do
  modelo receba observacoes truncadas fora do formato esperado;
- preservacao do inicio e do fim do output, para manter imports/cabecalhos e
  conclusoes/erros finais;
- marcadores explicitos `Coddy compacted tool output` e
  `Coddy compacted tool observation` informando que o meio foi omitido por
  budget de contexto;
- instrucao para reexecutar leitura/busca mais estreita quando o trecho omitido
  for necessario;
- teste de regressao cobrindo arquivo grande, preservacao de `BEGIN_MARKER` e
  `END_MARKER`, presenca do marcador de compactacao, JSON valido e limite de
  tamanho.
- retry controlado tambem no loop agentic direto do `coddy-agent` para erros
  recuperaveis de provider e respostas vazias, com guidance adicional somente
  quando o provider retorna sem assistant content/tool calls;
- timeouts de transporte continuam sem retry automatico para respeitar o budget
  de latencia da chamada.
- politica de retry centralizada no modulo de modelo, compartilhada por runtime
  `/ask`, eval live e loop agentic direto para evitar divergencia de criterios.
- respostas vazias persistentes do provider agora continuam sendo reportadas
  como falha, mas o run e os eventos marcam a falha como recuperavel para que UI
  e usuario possam oferecer retry/troca de provider sem tratar como erro fatal.

Metrica local apos a mudanca:

- `cargo test -p coddy-agent -- --test-threads=1`: 184 passed.
- `cargo test -p coddy-runtime -- --test-threads=1`: 61 passed.
- `./target/debug/coddy eval quality --json`: score 100.
- Multiagent eval: 3/3 passed, score 100.
- Prompt battery deterministica: 1200/1200 passed, score 100.

## Estado validado em 2026-05-02

Validacao local executada:

- `cargo fmt --check`.
- `cargo test --workspace -- --test-threads=1`.
- `cargo clippy --workspace --all-targets -- -D warnings`.
- `npm test -- --run` em `apps/coddy-electron`: 42 files, 328 tests.
- `npm run typecheck`.
- `npm run lint`.
- `npm run build`.
- `git diff --check`.
- `./scripts/guard_no_secrets.sh`.

Validacao live segura:

```sh
target/debug/coddy eval prompt-battery --json \
  --model-provider openrouter \
  --model-name deepseek/deepseek-v4-flash \
  --limit 50 \
  --concurrency 4
```

Resultado final observado:

- Prompts: 50.
- Passed: 47.
- Failed: 3.
- Score guardado: 94.
- Raw score: 88.
- Member recall: 94.
- Model error rate: 6%.

Interpretacao: o Coddy ja esta apto para uso assistido em analises de
codebase e tarefas de codificacao com ferramentas locais, desde que o usuario
mantenha revisao humana para edicoes e aceite que providers roteados pelo
OpenRouter ainda podem retornar respostas vazias persistentes em uma minoria de
casos. A proxima melhoria de maior impacto e reduzir dependencia de tentativas
reativas com roteamento alternativo de provider/modelo e compaction adaptativa
de observacoes.

Smoke live adicional apos a compactacao estrutural do loop agentic direto:

- Comando: `target/debug/coddy eval prompt-battery --json --model-provider openrouter --model-name deepseek/deepseek-v4-flash --limit 20 --concurrency 4`.
- Resultado mais recente: 19/20 passed, score 95, raw score 90, member recall
  92.
- Model error rate: 5%, causado por resposta vazia persistente do provider
  OpenRouter em um caso.
- Interpretacao: a mudanca local nao degradou o harness; a principal aresta
  live continua sendo fallback/roteamento quando o provider retorna resposta
  vazia sem content/tool calls.
- Integracao frontend: falhas recuperaveis publicadas no `AgentRunSummary`
  agora geram um aviso acionavel e redigido no painel de atividade do Desktop,
  incluindo codigo tecnico, detalhe sem secrets e proxima acao para retry,
  reducao de contexto/tool output ou troca de provider/modelo.

## Fontes pesquisadas

- SWE-bench Verified: https://www.swebench.com/verified.html
- Terminal-Bench: https://www.tbench.ai/leaderboard
- OpenAI, Codex agent loop: https://openai.com/index/unrolling-the-codex-agent-loop/
- OpenAI, GPT-5.3-Codex model: https://developers.openai.com/api/docs/models/gpt-5.3-codex
- Anthropic, Claude Code best practices: https://code.claude.com/docs/en/best-practices
- Anthropic, Claude Code subagents: https://docs.anthropic.com/en/docs/claude-code/sub-agents
- Anthropic, Claude Opus 4.5: https://www.anthropic.com/news/claude-opus-4-5
- Google, Gemini CLI: https://cloud.google.com/gemini/docs/codeassist/gemini-cli
- Google, Gemini CLI extensions: https://google-gemini.github.io/gemini-cli/docs/extensions/
- Google, Gemini models: https://ai.google.dev/models/gemini
- GitHub, Copilot coding agent: https://docs.github.com/en/copilot/concepts/agents/cloud-agent/about-cloud-agent
- GitHub, assigning tasks to Copilot: https://docs.github.com/en/copilot/using-github-copilot/coding-agent/about-assigning-tasks-to-copilot
