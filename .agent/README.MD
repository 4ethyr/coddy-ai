# REPL Agentic Prompt Pack

Este pacote contém arquivos Markdown prontos para orientar um agente de desenvolvimento a transformar um projeto em um **REPL agentic de ponta**.

## Estrutura

```txt
AGENTS.md
.agent/
  AGENT_MASTER_PROMPT.md
  PROJECT_CONTEXT.md
  TARGET_ARCHITECTURE.md
  SUBAGENTS.md
  TOOLING_AND_MCP.md
  SECURITY_POLICY.md
  CODE_QUALITY.md
  WORKFLOWS.md
  EVALS_AND_METRICS.md
  IMPLEMENTATION_ROADMAP.md
  ACCEPTANCE_CRITERIA.md
  PROMPTS.md
```

## Como usar

1. Extraia este pacote na raiz do seu projeto.
2. Abra seu agent no projeto.
3. Peça para ele ler `AGENTS.md`.
4. Execute primeiro o prompt de diagnóstico em `.agent/PROMPTS.md`.
5. Depois implemente fase por fase.

## Recomendação

Não peça para o agent implementar tudo de uma vez.

A ordem recomendada é:

1. Diagnóstico
2. REPL básico
3. Agent Loop
4. Tool Registry
5. Segurança
6. Context e Memory
7. Subagents
8. MCP
9. Evals
10. Developer Experience
