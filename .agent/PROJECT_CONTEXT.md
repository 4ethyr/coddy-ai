# Project Context

Este projeto deve evoluir para um **REPL agentic avançado**.

## O que é um REPL agentic

Um REPL agentic é uma interface interativa onde o usuário conversa com um agente capaz de:

- entender comandos naturais;
- raciocinar sobre tarefas;
- consultar contexto;
- usar ferramentas;
- modificar arquivos;
- executar comandos;
- validar resultados;
- lembrar preferências e convenções;
- delegar trabalho;
- operar com segurança.

Ele não é apenas:

- um chat;
- um terminal;
- um autocomplete;
- um executor de shell;
- um wrapper simples de API.

Ele é um sistema de orquestração.

## Estado atual

O agente deve descobrir o estado atual do projeto antes de implementar qualquer mudança.

Analise:

- linguagem principal;
- framework;
- package manager;
- estrutura de pastas;
- comandos de build;
- comandos de teste;
- comandos de lint;
- comandos de formatação;
- arquivos de configuração;
- arquitetura atual;
- pontos de entrada;
- módulos existentes;
- dívida técnica;
- ausência de testes;
- riscos de segurança;
- dependências críticas;
- lacunas de documentação.

## Arquitetura conceitual alvo

Adapte esta estrutura ao projeto real:

```txt
interface/
  repl/
  cli/
  commands/

core/
  agent-loop/
  planner/
  executor/
  context/
  memory/
  events/
  state/

tools/
  registry/
  filesystem/
  shell/
  git/
  http/
  mcp/
  code-search/

subagents/
  definitions/
  runtime/
  delegation/
  coordination/

security/
  sandbox/
  approvals/
  policies/
  command-guard/
  secret-scanner/

quality/
  tests/
  lint/
  typecheck/
  evals/
  metrics/

observability/
  logs/
  traces/
  audit/
  sessions/

config/
  loader/
  schema/
  profiles/
```

## Regras de adaptação

1. Não force essa estrutura literalmente se o projeto já tiver arquitetura coerente.
2. Preserve padrões existentes quando fizer sentido.
3. Introduza abstrações apenas quando agregarem clareza.
4. Evite overengineering.
5. Priorize um núcleo funcional antes de recursos avançados.
6. Toda melhoria deve ser validável.
7. Toda decisão arquitetural relevante deve ser documentada.
8. Toda mudança de comportamento deve ter teste ou validação manual descrita.

## Objetivo de evolução

O resultado final deve permitir:

- executar uma sessão REPL;
- carregar contexto do projeto;
- entender tarefas;
- planejar;
- usar tools;
- executar comandos seguros;
- editar arquivos;
- pedir aprovação para ações sensíveis;
- delegar para subagents;
- registrar logs;
- medir qualidade;
- integrar MCP;
- exportar relatórios;
- operar de forma confiável em projetos reais.
