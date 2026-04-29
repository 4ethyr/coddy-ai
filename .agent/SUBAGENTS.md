# Subagents

Este projeto deve implementar um sistema de subagents especializados.

Subagents são agentes menores, com função específica, contexto isolado e tools limitadas.

Eles reduzem poluição de contexto, aumentam segurança, melhoram revisão e permitem trabalho paralelo ou especializado.

## Regras gerais

1. Subagents devem ter responsabilidades claras.
2. Subagents não devem ter acesso irrestrito a tools.
3. Subagents devem retornar respostas estruturadas.
4. Subagents read-only não devem modificar arquivos.
5. Subagents devem respeitar timeout.
6. Subagents devem gerar logs.
7. O agente principal deve consolidar resultados.
8. Nenhum subagent deve executar comandos perigosos sem aprovação.
9. Subagents devem herdar políticas globais de segurança.
10. Subagents não devem ignorar sandbox.

## Subagents obrigatórios

### 1. Explorer Agent

Responsabilidade:

- explorar o repositório;
- localizar arquivos;
- identificar arquitetura;
- encontrar pontos de entrada;
- encontrar testes;
- mapear dependências;
- identificar scripts úteis.

Tools permitidas:

- read_file;
- list_files;
- search_files;
- grep;
- git_status.

Tools proibidas:

- write_file;
- edit_file;
- delete_file;
- shell_write;
- network_request.

Modo:

```txt
read-only
```

Resposta:

```json
{
  "summary": "string",
  "importantFiles": [],
  "entrypoints": [],
  "testFiles": [],
  "commands": {},
  "risks": [],
  "recommendations": []
}
```

### 2. Planner Agent

Responsabilidade:

- criar plano técnico;
- dividir tarefas;
- identificar riscos;
- propor validações;
- definir critérios de sucesso.

Tools permitidas:

- read_file;
- search_files;
- grep;
- git_status.

Resposta:

```json
{
  "goal": "string",
  "plan": [],
  "risks": [],
  "requiredTools": [],
  "validation": [],
  "approvalNeeded": false
}
```

### 3. Coder Agent

Responsabilidade:

- implementar alterações;
- criar novos módulos;
- corrigir bugs;
- aplicar refactors pequenos;
- preservar compatibilidade.

Tools permitidas:

- read_file;
- write_file;
- edit_file;
- search_files;
- shell_safe;
- git_diff.

Regras:

- modificar poucos arquivos por vez;
- manter compatibilidade;
- criar testes;
- não executar comandos destrutivos;
- não adicionar dependências sem justificar.

Resposta:

```json
{
  "changedFiles": [],
  "summary": "string",
  "testsAdded": [],
  "risks": [],
  "nextSteps": []
}
```

### 4. Reviewer Agent

Responsabilidade:

- revisar diff;
- encontrar bugs;
- avaliar legibilidade;
- avaliar manutenção;
- avaliar aderência à arquitetura;
- identificar regressões prováveis.

Tools permitidas:

- read_file;
- git_diff;
- search_files;
- shell_safe.

Resposta:

```json
{
  "approved": true,
  "issues": [],
  "suggestions": [],
  "blockingProblems": [],
  "nonBlockingProblems": []
}
```

### 5. Security Reviewer Agent

Responsabilidade:

- analisar segurança;
- detectar comandos perigosos;
- detectar vazamento de secrets;
- avaliar filesystem/network policies;
- avaliar prompt injection;
- avaliar permissões de tools;
- revisar uso de MCP.

Tools permitidas:

- read_file;
- search_files;
- grep;
- git_diff;
- shell_safe.

Resposta:

```json
{
  "riskLevel": "low|medium|high|critical",
  "findings": [],
  "requiredFixes": [],
  "recommendations": []
}
```

### 6. Test Writer Agent

Responsabilidade:

- criar testes unitários;
- criar testes de integração;
- criar fixtures;
- melhorar cobertura;
- validar edge cases.

Tools permitidas:

- read_file;
- write_file;
- edit_file;
- shell_safe;
- search_files.

Resposta:

```json
{
  "testsCreated": [],
  "coverageFocus": [],
  "edgeCases": [],
  "commandsToRun": []
}
```

### 7. Eval Runner Agent

Responsabilidade:

- rodar avaliações;
- medir qualidade;
- comparar resultados;
- gerar score;
- reportar regressões.

Tools permitidas:

- shell_safe;
- read_file;
- write_file;
- search_files.

Resposta:

```json
{
  "score": 0,
  "passed": true,
  "failedChecks": [],
  "metrics": {},
  "recommendations": []
}
```

### 8. Docs Writer Agent

Responsabilidade:

- atualizar README;
- documentar arquitetura;
- documentar comandos;
- documentar configuração;
- documentar policies;
- criar exemplos de uso.

Tools permitidas:

- read_file;
- write_file;
- edit_file;
- search_files.

Resposta:

```json
{
  "docsUpdated": [],
  "sectionsAdded": [],
  "missingDocs": []
}
```

## Definição declarativa de subagent

Cada subagent deve poder ser definido em arquivo ou configuração:

```json
{
  "name": "explorer",
  "description": "Explora o projeto em modo read-only.",
  "mode": "read-only",
  "tools": ["read_file", "list_files", "search_files", "grep"],
  "timeoutMs": 60000,
  "maxContextTokens": 8000,
  "systemPrompt": "Você é um agente de exploração read-only..."
}
```

## Delegação

O agente principal deve delegar quando:

- a tarefa exige análise ampla;
- a tarefa exige revisão independente;
- a tarefa exige segurança;
- a tarefa exige testes;
- a tarefa exige documentação;
- há risco de poluição de contexto;
- há benefício claro em especialização.

O agente principal não deve delegar quando:

- a tarefa é trivial;
- há risco alto sem approval;
- o custo é maior que o benefício;
- o usuário pediu execução direta;
- a tarefa exige uma decisão única e simples.

## Consolidação

O agente principal deve consolidar resultados de subagents em:

```json
{
  "subagentsUsed": [],
  "summary": "string",
  "agreements": [],
  "conflicts": [],
  "finalDecision": "string",
  "risks": [],
  "nextActions": []
}
```
