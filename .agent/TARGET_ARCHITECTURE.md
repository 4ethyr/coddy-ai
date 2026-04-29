# Target Architecture

A arquitetura alvo deve permitir que o REPL opere como um sistema agentic extensível, seguro, observável e testável.

## Visão geral

```txt
User
  -> REPL/CLI Interface
  -> Agent Loop
  -> Context Manager
  -> Planner
  -> Tool/Subagent Router
  -> Tool Executor or Subagent Runtime
  -> Security Layer
  -> Observation
  -> Validation
  -> Response Builder
  -> Memory/Audit Logs
```

## 1. REPL Interface

Responsável por:

- receber input do usuário;
- exibir respostas;
- mostrar status;
- mostrar diffs;
- pedir confirmações;
- listar comandos;
- manter histórico;
- aceitar slash commands;
- lidar com interrupções;
- exportar sessão.

Comandos esperados:

```txt
/help
/init
/status
/plan
/run
/review
/test
/tools
/subagents
/mcp
/config
/memory
/clear
/export
/exit
```

## 2. Agent Loop

Responsável por controlar o ciclo principal:

```txt
Input -> Context -> Plan -> Act -> Observe -> Validate -> Respond
```

Deve suportar:

- execução passo a passo;
- modo automático controlado;
- modo seguro;
- modo apenas planejamento;
- interrupção;
- retry;
- timeout;
- erro recuperável;
- erro fatal;
- resumo de execução.

## 3. Planner

Responsável por transformar intenção em plano.

O plano deve conter:

- objetivo;
- hipóteses;
- arquivos relevantes;
- ferramentas necessárias;
- riscos;
- passos;
- critérios de sucesso;
- validações;
- necessidade de aprovação.

Formato sugerido:

```json
{
  "goal": "string",
  "assumptions": [],
  "steps": [],
  "tools": [],
  "risks": [],
  "validation": [],
  "requiresApproval": false
}
```

## 4. Tool Registry

Responsável por registrar e descrever ferramentas.

Cada tool deve possuir:

```json
{
  "name": "string",
  "description": "string",
  "category": "filesystem|shell|git|network|mcp|memory|eval|project",
  "inputSchema": {},
  "outputSchema": {},
  "riskLevel": "low|medium|high|critical",
  "requiresApproval": true,
  "timeoutMs": 30000,
  "permissions": []
}
```

## 5. Tool Executor

Responsável por:

- validar input;
- verificar permissões;
- aplicar sandbox;
- executar tool;
- capturar output;
- truncar output grande;
- registrar logs;
- padronizar erro;
- retornar resultado estruturado.

Formato de resposta:

```json
{
  "ok": true,
  "tool": "string",
  "output": {},
  "error": null,
  "durationMs": 0,
  "metadata": {}
}
```

## 6. Context Manager

Responsável por carregar, priorizar e compactar contexto.

Deve gerenciar:

- contexto de sistema;
- contexto de projeto;
- contexto de usuário;
- contexto da sessão;
- contexto dos arquivos;
- contexto das tools;
- contexto dos subagents;
- contexto recuperado de memória.

Deve suportar:

- ranking de relevância;
- truncamento;
- resumo;
- compactação;
- exclusão de ruído;
- proteção de secrets.

## 7. Memory Manager

Responsável por persistir informações úteis.

Tipos de memória:

- preferências do usuário;
- convenções do projeto;
- comandos úteis;
- decisões arquiteturais;
- problemas conhecidos;
- padrões de teste;
- integrações configuradas.

Não armazenar:

- secrets;
- tokens;
- senhas;
- dados pessoais sensíveis;
- logs confidenciais;
- outputs enormes.

## 8. Subagent Runtime

Responsável por criar, executar e consolidar subagents.

Deve suportar:

- subagent read-only;
- subagent executor;
- subagent reviewer;
- subagent security;
- subagent testing;
- subagent docs;
- execução paralela, se possível;
- timeout por subagent;
- limite de profundidade;
- logs por subagent.

## 9. Security Layer

Responsável por:

- sandbox;
- approvals;
- command guard;
- secret scanner;
- network policy;
- filesystem policy;
- audit log;
- prompt-injection defense.

## 10. Evaluation Layer

Responsável por medir qualidade.

Deve suportar:

- testes automatizados;
- lint;
- type-check;
- coverage;
- benchmarks;
- avaliações por tarefa;
- regressões;
- scores;
- relatórios.

## 11. Observability Layer

Responsável por rastrear o comportamento do sistema.

Deve registrar:

- sessões;
- planos;
- tool calls;
- subagent runs;
- erros;
- approvals;
- comandos executados;
- arquivos alterados;
- métricas de latência;
- métricas de sucesso/falha.

## 12. Configuration Layer

Responsável por carregar e validar configuração.

Deve suportar:

- configuração global;
- configuração por projeto;
- perfis;
- sandbox mode;
- approval policy;
- MCP servers;
- tools habilitadas;
- limites de execução;
- limites de contexto.
