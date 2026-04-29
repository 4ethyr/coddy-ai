# Tooling and MCP

Este projeto deve possuir um sistema robusto de tools e integração com MCP.

## Tool Registry

Implemente um registry central de tools.

Cada tool deve ser registrada com:

- nome;
- descrição;
- categoria;
- schema de entrada;
- schema de saída;
- risco;
- permissões;
- timeout;
- retry policy;
- approval policy;
- executor;
- logger.

Exemplo:

```json
{
  "name": "read_file",
  "description": "Lê o conteúdo de um arquivo dentro do workspace.",
  "category": "filesystem",
  "riskLevel": "low",
  "requiresApproval": false,
  "permissions": ["filesystem:read"],
  "timeoutMs": 10000
}
```

## Tools mínimas

### Filesystem

- list_files
- read_file
- write_file
- edit_file
- delete_file
- create_directory
- file_exists
- get_file_metadata

### Search

- grep
- semantic_search
- search_files
- search_symbols

### Shell

- shell_safe
- shell_with_approval
- shell_readonly
- shell_test_runner

### Git

- git_status
- git_diff
- git_log
- git_branch
- git_commit
- git_checkout
- git_create_branch

### Project

- detect_stack
- detect_package_manager
- get_scripts
- run_tests
- run_lint
- run_typecheck
- run_formatter

### Network

- http_get
- http_post
- mcp_call

### Memory

- memory_read
- memory_write
- memory_delete
- memory_search

### Evaluation

- eval_run
- eval_compare
- eval_report

## Classificação de risco

### Low

- ler arquivos;
- listar diretórios;
- buscar texto;
- ver git diff;
- rodar comandos read-only.

### Medium

- editar arquivos;
- criar arquivos;
- rodar testes;
- rodar lint;
- rodar type-check;
- formatar código.

### High

- deletar arquivos;
- mover diretórios;
- alterar configurações sensíveis;
- rodar migrations;
- executar comandos com rede;
- modificar git history.

### Critical

- remover diretórios recursivamente;
- acessar secrets;
- fazer deploy;
- executar comandos com sudo;
- alterar permissões do sistema;
- enviar dados externos;
- rodar scripts desconhecidos.

## Tool execution contract

Toda tool deve retornar algo equivalente a:

```json
{
  "ok": true,
  "tool": "string",
  "output": {},
  "error": null,
  "durationMs": 0,
  "metadata": {
    "riskLevel": "low",
    "approvalRequired": false,
    "approvalGranted": false
  }
}
```

Em caso de erro:

```json
{
  "ok": false,
  "tool": "string",
  "output": null,
  "error": {
    "code": "TOOL_ERROR",
    "message": "Human readable error",
    "details": {},
    "recoverable": true
  },
  "durationMs": 0,
  "metadata": {}
}
```

## MCP

O sistema deve suportar MCP para integração com ferramentas externas.

### Objetivos do MCP

- conectar ferramentas externas;
- padronizar acesso a recursos;
- expor tools remotas;
- acessar documentação;
- acessar issues;
- acessar PRs;
- acessar bancos;
- acessar sistemas internos.

### Estrutura sugerida

```txt
mcp/
  client/
  server-registry/
  transport/
  tool-adapter/
  permission-bridge/
  config-loader/
```

### Transportes desejados

- stdio;
- HTTP;
- SSE;
- WebSocket, se fizer sentido.

### Configuração MCP

Exemplo:

```json
{
  "mcpServers": {
    "github": {
      "transport": "stdio",
      "command": "mcp-github",
      "enabled": true,
      "permissions": ["issues:read", "pull_requests:read"]
    },
    "filesystem": {
      "transport": "stdio",
      "command": "mcp-filesystem",
      "enabled": true,
      "permissions": ["filesystem:read"]
    }
  }
}
```

## Regras para MCP

1. MCP nunca deve bypassar o sistema de permissões.
2. MCP tools devem ser registradas no Tool Registry.
3. MCP tools devem ter riskLevel.
4. MCP tools devem gerar audit logs.
5. MCP tools de escrita exigem approval.
6. MCP tools não devem receber secrets sem necessidade.
7. MCP outputs grandes devem ser resumidos.
8. MCP deve poder ser desativado por configuração.
9. MCP deve respeitar sandbox.
10. Falhas de MCP devem ser tratadas como erros recuperáveis sempre que possível.
