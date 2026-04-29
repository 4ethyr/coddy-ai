# Workflows

Este projeto deve possuir workflows claros para o agente.

## Workflow 1: Diagnóstico inicial

Use quando iniciar em um projeto desconhecido.

Passos:

1. Listar estrutura do projeto.
2. Detectar stack.
3. Identificar package manager.
4. Identificar comandos disponíveis.
5. Identificar entrada principal.
6. Identificar testes.
7. Identificar módulos de CLI/REPL.
8. Identificar riscos.
9. Gerar relatório.
10. Propor plano.

Saída esperada:

```json
{
  "stack": [],
  "entrypoints": [],
  "commands": {},
  "testSetup": {},
  "architecture": "string",
  "risks": [],
  "recommendedPlan": []
}
```

## Workflow 2: Implementar funcionalidade

Passos:

1. Entender requisito.
2. Criar plano.
3. Identificar arquivos.
4. Criar ou atualizar testes.
5. Implementar.
6. Rodar testes focados.
7. Rodar lint/type-check.
8. Revisar diff.
9. Atualizar documentação.
10. Resumir entrega.

## Workflow 3: Corrigir bug

Passos:

1. Reproduzir bug.
2. Localizar causa raiz.
3. Criar teste que falha.
4. Corrigir menor área possível.
5. Rodar teste que antes falhava.
6. Rodar suíte relevante.
7. Revisar efeitos colaterais.
8. Documentar causa e correção.

## Workflow 4: Refatorar

Passos:

1. Entender comportamento atual.
2. Garantir testes existentes.
3. Criar testes ausentes se necessário.
4. Refatorar em passos pequenos.
5. Garantir compatibilidade pública.
6. Rodar testes a cada etapa.
7. Revisar diff.
8. Documentar decisão.

## Workflow 5: Adicionar tool

Passos:

1. Definir objetivo da tool.
2. Definir schema de entrada.
3. Definir schema de saída.
4. Definir riskLevel.
5. Definir approval policy.
6. Implementar executor.
7. Implementar validação de input.
8. Implementar logs.
9. Implementar testes.
10. Registrar no Tool Registry.
11. Documentar uso.

## Workflow 6: Adicionar subagent

Passos:

1. Definir responsabilidade.
2. Definir prompt.
3. Definir tools permitidas.
4. Definir limites de contexto.
5. Definir timeout.
6. Definir formato de resposta.
7. Implementar runtime.
8. Implementar testes.
9. Documentar.
10. Criar exemplo de uso.

## Workflow 7: Adicionar MCP server

Passos:

1. Definir finalidade.
2. Definir transporte.
3. Definir configuração.
4. Registrar tools MCP.
5. Aplicar permission bridge.
6. Aplicar riskLevel.
7. Aplicar approvals.
8. Testar conexão.
9. Testar erros.
10. Documentar.

## Workflow 8: Rodar validação final

Passos:

1. Rodar testes.
2. Rodar lint.
3. Rodar type-check.
4. Rodar security checks.
5. Rodar evals.
6. Revisar diff.
7. Validar documentação.
8. Gerar relatório final.

Relatório final:

```md
## Summary

## Files changed

## Tests run

## Validation result

## Risks

## Follow-ups
```

## Workflow 9: Revisão de segurança

Passos:

1. Revisar mudanças em filesystem.
2. Revisar shell execution.
3. Revisar permissões de tools.
4. Revisar MCP.
5. Revisar logs.
6. Revisar secret handling.
7. Revisar prompt injection.
8. Classificar severidade.
9. Exigir correções bloqueantes.

## Workflow 10: Release readiness

Passos:

1. Conferir critérios de aceite.
2. Rodar validações disponíveis.
3. Conferir documentação.
4. Conferir segurança.
5. Conferir evals.
6. Conferir known limitations.
7. Gerar recomendação final.
