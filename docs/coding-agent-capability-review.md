# Coding Agent Capability Review

Data da revisao: 2026-05-02.

## Resumo

O Coddy ja possui uma base forte para cenarios de codificacao: loop agentic
com tools, runtime Rust, frontend Electron, workspace, historico, subagents,
prompt battery, guardrails de shell e fluxo de approval. A principal diferenca
em relacao a agentes top-tier esta menos no modelo isolado e mais no harness:
contexto, plano, ferramentas, permissoes, validacao e registro de evidencias.

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

## Comparacao com agentes e modelos top-tier

### OpenAI Codex / GPT-5.x Codex

Pontos fortes observados:

- Agent loop explicito com prompt, tools, observacoes e novas inferencias.
- Uso de instrucoes por projeto e contexto de ambiente.
- Tool schema para shell, plano, web e MCP.
- Modos de approval/sandbox para controlar leitura, escrita e execucao.
- Modelos Codex atuais sao otimizados para tarefas agentic coding longas.

Implicacao para Coddy:

- Manter o loop Coddy como harness independente do modelo.
- Fortalecer instrucoes do sistema e workflows para reduzir variancia entre
  providers.
- Evoluir MCP e permissao por tool, sem depender de prompt apenas.

### Claude Code / Claude Opus e Sonnet

Pontos fortes observados:

- Workflow recomendado: explorar, planejar, implementar, validar e commitar.
- Forte enfase em dar ao agente uma forma de verificar o proprio trabalho.
- Gestao agressiva de contexto, memorias, subagents, hooks e permissoes.
- Claude Code usa checkpoints, subagents e automacao de sessoes como parte do
  fluxo de engenharia.

Implicacao para Coddy:

- Tornar validacao e criterios de sucesso parte do prompt default e dos slash
  workflows.
- Usar subagents principalmente para investigacao e revisao, reduzindo ruido no
  contexto principal.
- Adicionar checklists/status visiveis para plano, execucao e validacao.

### Gemini CLI / Gemini 3

Pontos fortes observados:

- CLI open source com loop ReAct.
- Built-in tools para filesystem, shell, grep, web e MCP.
- Grande janela de contexto em modelos Gemini 3 Pro.
- Integracao com Code Assist e comandos como `/memory`, `/stats`, `/tools` e
  `/mcp`.

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

Implicacao para Coddy:

- Fortalecer trilha de auditoria local e relatorios de sessao.
- Associar tarefas de codificacao a validacoes e resultados rastreaveis.
- Continuar tratando secrets, rede e shell como superficies sensiveis.

## Lacunas atuais prioritarias

1. Status/checklist visual do workflow de codificacao.
2. Export de relatorio de sessao com arquivos alterados, tools, checks e erros.
3. Context budget por run, incluindo resumo de tool outputs grandes.
4. MCP adapter com permission bridge e catalogo unificado de tools.
5. Evals especificos para tarefas reais de patch: bug fix, refactor, test
   writing, security fix e frontend visual validation.
6. Memoria persistente segura para convencoes do projeto e comandos de teste.
7. Melhor separacao entre plan-only, code, review e validation no runtime.

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

## Fontes pesquisadas

- SWE-bench Verified: https://www.swebench.com/verified.html
- Terminal-Bench: https://www.tbench.ai/leaderboard
- OpenAI, Codex agent loop: https://openai.com/index/unrolling-the-codex-agent-loop/
- OpenAI, GPT-5.3-Codex model: https://developers.openai.com/api/docs/models/gpt-5.3-codex
- Anthropic, Claude Code best practices: https://code.claude.com/docs/en/best-practices
- Anthropic, Claude Opus 4.5: https://www.anthropic.com/news/claude-opus-4-5
- Google, Gemini CLI: https://cloud.google.com/gemini/docs/codeassist/gemini-cli
- Google, Gemini models: https://ai.google.dev/models/gemini
- GitHub, Copilot coding agent: https://docs.github.com/en/copilot/concepts/agents/cloud-agent/about-cloud-agent
