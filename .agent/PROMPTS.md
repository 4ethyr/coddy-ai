# Operational Prompts

Este arquivo contém prompts prontos para operar o agente fase por fase.

## 1. Diagnóstico inicial

```md
Leia o arquivo `AGENTS.md` e todos os arquivos referenciados dentro da pasta `.agent/`.

Sua missão é transformar este projeto em um REPL agentic de ponta.

Primeiro, não altere nenhum arquivo.

Execute apenas a fase de diagnóstico:

1. Analise a estrutura do projeto.
2. Detecte a stack.
3. Identifique pontos de entrada.
4. Identifique comandos de build, teste, lint e type-check.
5. Identifique arquitetura atual.
6. Identifique lacunas em relação aos arquivos `.agent/`.
7. Proponha um plano incremental de implementação dividido por fases.
8. Liste riscos técnicos e riscos de segurança.
9. Informe quais arquivos provavelmente serão criados ou alterados.

Responda com um relatório estruturado.

Não implemente nada ainda.
```

## 2. Implementar Fase 1: REPL básico

```md
Com base no diagnóstico anterior, implemente apenas a Fase 1 do `.agent/IMPLEMENTATION_ROADMAP.md`.

Objetivo da Fase 1:

Criar ou melhorar o núcleo básico do REPL.

Requisitos:

- interface interativa funcional;
- comandos básicos;
- help;
- status;
- exit;
- config;
- tools;
- tratamento de erro;
- estrutura preparada para agent loop futuro.

Antes de alterar arquivos:

1. Liste o plano.
2. Liste os arquivos que serão alterados.
3. Explique riscos.

Depois implemente.

Após implementar:

1. Rode testes relevantes.
2. Rode lint/type-check se existirem.
3. Revise o diff.
4. Atualize documentação mínima.
5. Entregue relatório final.
```

## 3. Implementar Tool Registry

```md
Implemente a Fase 3 do roadmap: Tool Registry.

Leia:

- `.agent/TOOLING_AND_MCP.md`
- `.agent/SECURITY_POLICY.md`
- `.agent/CODE_QUALITY.md`
- `.agent/ACCEPTANCE_CRITERIA.md`

Objetivo:

Criar um sistema extensível de tools com registro, schema, riskLevel, timeout, permissions, approval policy, execução padronizada e logs.

Tools mínimas:

- read_file;
- list_files;
- search_files;
- write_file;
- shell_safe;
- git_status;
- git_diff.

Requisitos obrigatórios:

1. Toda tool deve ter metadata.
2. Toda tool deve validar input.
3. Toda tool deve retornar resultado padronizado.
4. Toda tool deve ter testes.
5. Tools de escrita devem ser classificadas como medium ou superior.
6. Shell deve passar pelo command guard.
7. Outputs grandes devem ser truncados ou resumidos.
8. Erros devem ser padronizados.
9. Documentação deve ser atualizada.

Implemente incrementalmente e rode validações.
```

## 4. Implementar Segurança

```md
Implemente a Fase 4 do roadmap: Segurança.

Leia:

- `.agent/SECURITY_POLICY.md`
- `.agent/TOOLING_AND_MCP.md`
- `.agent/CODE_QUALITY.md`
- `.agent/ACCEPTANCE_CRITERIA.md`

Objetivo:

Adicionar sandbox, approvals, command guard, secret scanner, filesystem guard, network policy e audit log.

Requisitos:

1. Implementar modos de sandbox:
   - read-only;
   - workspace-write;
   - full-access.

2. Implementar approval policies:
   - never;
   - on-request;
   - always;
   - untrusted.

3. Implementar command guard para detectar comandos perigosos.

4. Implementar proteção contra:
   - path traversal;
   - escrita fora do workspace;
   - comandos destrutivos;
   - secrets em logs;
   - execução de shell insegura;
   - rede não autorizada.

5. Criar testes para todos os casos críticos.

6. Atualizar documentação.

Antes de implementar, gere plano e liste riscos.
Depois de implementar, rode testes e entregue relatório.
```

## 5. Implementar Subagents

```md
Implemente a Fase 6 do roadmap: Subagents.

Leia:

- `.agent/SUBAGENTS.md`
- `.agent/SECURITY_POLICY.md`
- `.agent/TOOLING_AND_MCP.md`
- `.agent/CODE_QUALITY.md`
- `.agent/ACCEPTANCE_CRITERIA.md`

Objetivo:

Criar um runtime de subagents especializados com contexto isolado, tools limitadas, timeout, logs e consolidação de respostas.

Subagents mínimos:

- explorer;
- planner;
- coder;
- reviewer;
- security-reviewer;
- test-writer;
- docs-writer;
- eval-runner.

Requisitos:

1. Cada subagent deve ter definição declarativa.
2. Cada subagent deve ter tools permitidas.
3. Cada subagent deve ter timeout.
4. Cada subagent deve ter formato de resposta.
5. Subagents read-only não podem escrever arquivos.
6. O agente principal deve consolidar respostas.
7. Deve existir teste de delegação.
8. Deve existir teste de bloqueio de tool não permitida.
9. Deve existir documentação de uso.

Implemente de forma incremental.
```

## 6. Implementar MCP

```md
Implemente a Fase 7 do roadmap: MCP.

Leia:

- `.agent/TOOLING_AND_MCP.md`
- `.agent/SECURITY_POLICY.md`
- `.agent/CODE_QUALITY.md`
- `.agent/ACCEPTANCE_CRITERIA.md`

Objetivo:

Adicionar suporte a MCP de forma segura e integrada ao Tool Registry.

Requisitos:

1. Criar configuração para MCP servers.
2. Criar MCP client ou adapter.
3. Registrar MCP tools no Tool Registry.
4. Aplicar riskLevel em MCP tools.
5. Aplicar approval policy em MCP tools.
6. Gerar logs de chamadas MCP.
7. Permitir desativar MCP.
8. Tratar erros de conexão.
9. Tratar outputs grandes.
10. Criar testes com MCP mockado.
11. Atualizar documentação.

Não conecte serviços reais sem configuração explícita.
Não exponha secrets.
```

## 7. Revisão geral de qualidade

```md
Faça uma revisão profunda do projeto usando os arquivos da pasta `.agent/`.

Não implemente nada inicialmente.

Avalie:

1. Arquitetura.
2. Segurança.
3. Tool Registry.
4. Agent Loop.
5. Context Manager.
6. Memory Manager.
7. Subagents.
8. MCP.
9. Testes.
10. Documentação.
11. Observabilidade.
12. Developer Experience.
13. Evals.
14. Critérios de aceite.

Para cada área, classifique:

- status atual;
- lacunas;
- riscos;
- prioridade;
- esforço estimado;
- arquivos envolvidos;
- recomendação.

Formato:

# Deep Review Report

## Executive Summary

## Scores

| Area | Score | Priority |
|---|---:|---|

## Findings

## Critical Risks

## Recommended Roadmap

## Immediate Next Actions
```

## 8. Validação final

```md
Execute uma validação final do projeto como se fosse uma revisão de release.

Leia:

- `AGENTS.md`
- `.agent/ACCEPTANCE_CRITERIA.md`
- `.agent/EVALS_AND_METRICS.md`
- `.agent/SECURITY_POLICY.md`
- `.agent/CODE_QUALITY.md`

Valide:

1. Testes.
2. Lint.
3. Type-check.
4. Segurança.
5. Sandbox.
6. Approval policies.
7. Tools.
8. Subagents.
9. MCP.
10. Documentação.
11. Evals.
12. Experiência do REPL.

Gere um relatório final com:

# Release Readiness Report

## Summary

## Pass/Fail

## Validation commands

## Results

## Security review

## Known issues

## Blockers

## Non-blocking improvements

## Final recommendation

Não esconda problemas.
Se algo não puder ser validado, marque como não validado.
```
