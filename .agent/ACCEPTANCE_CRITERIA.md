# Acceptance Criteria

O projeto só deve ser considerado um REPL agentic de ponta quando cumprir os critérios abaixo.

## REPL

- [ ] Possui interface interativa funcional.
- [ ] Possui comandos de ajuda.
- [ ] Possui histórico de sessão.
- [ ] Possui tratamento de erro amigável.
- [ ] Possui modos de execução.
- [ ] Permite sair com segurança.
- [ ] Exibe status da sessão.

## Agent Loop

- [ ] Possui ciclo input -> plan -> act -> observe -> validate -> respond.
- [ ] Planeja antes de agir.
- [ ] Registra ações.
- [ ] Trata falhas.
- [ ] Suporta retry controlado.
- [ ] Suporta interrupção.
- [ ] Suporta validação final.

## Tools

- [ ] Possui Tool Registry.
- [ ] Cada tool possui schema.
- [ ] Cada tool possui riskLevel.
- [ ] Cada tool possui timeout.
- [ ] Cada tool possui logs.
- [ ] Tools validam input.
- [ ] Tools retornam resultado padronizado.
- [ ] Tools sensíveis pedem approval.

## Segurança

- [ ] Possui sandbox read-only.
- [ ] Possui sandbox workspace-write.
- [ ] Possui approval policy.
- [ ] Bloqueia comandos destrutivos.
- [ ] Detecta secrets.
- [ ] Não imprime secrets em logs.
- [ ] Possui audit logs.
- [ ] Protege contra path traversal.
- [ ] Controla acesso à rede.
- [ ] Trata prompt injection.

## Context

- [ ] Possui Context Manager.
- [ ] Carrega contexto do projeto.
- [ ] Resume outputs grandes.
- [ ] Compacta histórico.
- [ ] Remove ruído.
- [ ] Protege secrets.
- [ ] Ranking de relevância existe ou está planejado.

## Memory

- [ ] Possui memória persistente.
- [ ] Memória tem escopo claro.
- [ ] Memória não salva secrets.
- [ ] Memória pode ser consultada.
- [ ] Memória pode ser apagada.

## Subagents

- [ ] Possui runtime de subagents.
- [ ] Possui explorer read-only.
- [ ] Possui planner.
- [ ] Possui reviewer.
- [ ] Possui security reviewer.
- [ ] Possui test writer.
- [ ] Possui docs writer.
- [ ] Possui eval runner.
- [ ] Subagents têm tools limitadas.
- [ ] Subagents têm contexto isolado.
- [ ] Subagents têm timeout.
- [ ] Resultados são consolidados.

## MCP

- [ ] Possui configuração MCP.
- [ ] Possui adapter MCP.
- [ ] MCP tools passam pelo Tool Registry.
- [ ] MCP respeita permissões.
- [ ] MCP respeita approvals.
- [ ] MCP gera logs.
- [ ] MCP pode ser desativado.

## Qualidade

- [ ] Testes passam.
- [ ] Lint passa.
- [ ] Type-check passa quando aplicável.
- [ ] Cobertura é monitorada.
- [ ] Diffs são revisados.
- [ ] Documentação existe.
- [ ] Erros são tipados ou padronizados.
- [ ] Logs são estruturados.

## Evals

- [ ] Possui casos de avaliação.
- [ ] Possui eval runner.
- [ ] Possui score.
- [ ] Possui baseline.
- [ ] Detecta regressões.
- [ ] Gera relatório.

## Documentação

- [ ] README explica uso.
- [ ] README explica instalação.
- [ ] README explica configuração.
- [ ] README explica segurança.
- [ ] README explica tools.
- [ ] README explica subagents.
- [ ] README explica MCP.
- [ ] README explica evals.

## Entrega final

O agente deve entregar um relatório final com:

```md
# Final Report

## Summary

## Architecture implemented

## Files changed

## Commands run

## Tests

## Security checks

## Evals

## Known limitations

## Recommended next steps
```
