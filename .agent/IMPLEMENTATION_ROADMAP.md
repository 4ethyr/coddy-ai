# Implementation Roadmap

Implemente as melhorias em fases.

Não tente implementar tudo de uma vez.

## Fase 0: Diagnóstico

Objetivo:

Entender o projeto atual.

Entregáveis:

- relatório da stack;
- mapa da arquitetura;
- lista de comandos;
- lista de riscos;
- plano de implementação.

Critérios de aceite:

- o agente sabe como rodar o projeto;
- o agente sabe onde estão os principais módulos;
- o agente sabe quais validações existem.

## Fase 1: Núcleo do REPL

Objetivo:

Garantir uma interface REPL funcional.

Entregáveis:

- loop de input/output;
- comandos básicos;
- histórico;
- help;
- tratamento de erro;
- configuração inicial.

Comandos mínimos:

```txt
/help
/status
/exit
/config
/tools
```

## Fase 2: Agent Loop

Objetivo:

Criar ciclo agentic explícito.

Entregáveis:

- planner;
- executor;
- observation;
- validation;
- response builder;
- state manager.

## Fase 3: Tool Registry

Objetivo:

Criar sistema extensível de tools.

Entregáveis:

- registro de tools;
- schemas;
- executor;
- logs;
- tratamento de erro;
- testes.

Tools mínimas:

- read_file;
- list_files;
- search_files;
- write_file;
- shell_safe;
- git_status;
- git_diff.

## Fase 4: Segurança

Objetivo:

Adicionar segurança por padrão.

Entregáveis:

- sandbox modes;
- approval policies;
- command guard;
- secret scanner;
- filesystem guard;
- network policy;
- audit log.

## Fase 5: Context e Memory

Objetivo:

Gerenciar contexto com qualidade.

Entregáveis:

- context manager;
- memory manager;
- resumo de sessão;
- compactação;
- ranking de relevância;
- proteção contra secrets.

## Fase 6: Subagents

Objetivo:

Adicionar agentes especializados.

Entregáveis:

- subagent runtime;
- explorer;
- planner;
- coder;
- reviewer;
- security reviewer;
- test writer;
- docs writer;
- eval runner.

## Fase 7: MCP

Objetivo:

Adicionar integração com MCP.

Entregáveis:

- MCP client;
- server registry;
- tool adapter;
- permission bridge;
- configuração;
- testes.

## Fase 8: Qualidade e Evals

Objetivo:

Medir qualidade objetivamente.

Entregáveis:

- eval cases;
- eval runner;
- metrics;
- reports;
- baseline;
- regression detection.

## Fase 9: Developer Experience

Objetivo:

Tornar o REPL excelente de usar.

Entregáveis:

- CLI refinada;
- modos de execução;
- logs claros;
- exportação de sessão;
- diffs;
- documentação;
- exemplos.

## Ordem de prioridade

1. Diagnóstico
2. REPL básico
3. Agent loop
4. Tools
5. Segurança
6. Context
7. Subagents
8. MCP
9. Evals
10. UX

## Regra de execução

Ao iniciar cada fase:

1. explique o estado atual;
2. liste arquivos que pretende alterar;
3. implemente incrementalmente;
4. rode validações;
5. gere relatório.

## Regra de parada

Pare e solicite confirmação quando:

- a próxima ação for destrutiva;
- envolver full-access;
- envolver rede não configurada;
- envolver secrets;
- envolver deploy;
- envolver alteração irreversível;
- envolver remoção em massa;
- testes críticos falharem de forma inesperada.
