# Code Quality

O projeto deve buscar alta qualidade de código, com validação objetiva.

## Princípios

1. Código simples antes de código esperto.
2. Testes antes de confiança.
3. Interfaces claras.
4. Baixo acoplamento.
5. Alta coesão.
6. Erros explícitos.
7. Logs úteis.
8. Configuração tipada.
9. Segurança por padrão.
10. Documentação suficiente.
11. Mudanças pequenas e revisáveis.
12. Comportamento validável.

## Padrões esperados

### Tipagem

Se a linguagem suportar tipagem, usar tipos fortes.

Exigir tipos para:

- configs;
- tools;
- subagents;
- eventos;
- policies;
- resultados;
- logs;
- erros;
- contexto;
- memória;
- evals.

### Erros

Todo erro deve carregar:

```json
{
  "code": "string",
  "message": "string",
  "details": {},
  "recoverable": true
}
```

### Resultados

Evitar retornos soltos.

Preferir:

```json
{
  "ok": true,
  "data": {},
  "error": null
}
```

ou equivalente idiomático da linguagem.

### Logs

Logs devem incluir:

- timestamp;
- nível;
- componente;
- evento;
- correlation id;
- session id;
- tool call id;
- subagent id, quando houver;
- duração.

### Configuração

Configuração deve ser:

- validada;
- documentada;
- segura por padrão;
- sobrescrevível por perfil;
- testável.

### Testes

Criar testes para:

- agent loop;
- parser de comandos;
- tool registry;
- tool executor;
- sandbox;
- approvals;
- command guard;
- memory;
- context manager;
- subagents;
- MCP adapter;
- eval runner;
- logging;
- error handling.

### Cobertura mínima sugerida

```txt
core: 80%
security: 90%
tools: 80%
subagents: 75%
cli/repl: 60%
```

Adapte às condições reais do projeto.

## Checklist de Pull Request

Antes de considerar uma tarefa concluída:

- [ ] build passa;
- [ ] testes passam;
- [ ] lint passa;
- [ ] type-check passa;
- [ ] não há secrets expostos;
- [ ] comandos perigosos estão protegidos;
- [ ] alterações estão documentadas;
- [ ] diff foi revisado;
- [ ] casos de erro foram tratados;
- [ ] comportamento foi validado manualmente quando necessário.

## Revisão de código

O reviewer deve procurar:

- bugs lógicos;
- regressões;
- race conditions;
- falta de validação;
- vazamento de secrets;
- permissões excessivas;
- tratamento ruim de erros;
- abstrações desnecessárias;
- duplicação;
- APIs confusas;
- testes frágeis;
- comportamento não documentado;
- dependências desnecessárias;
- violações de sandbox;
- outputs excessivamente verbosos.

## Padrão de documentação

Documentar:

- comandos;
- arquitetura;
- configuração;
- segurança;
- tools;
- subagents;
- MCP;
- evals;
- troubleshooting;
- exemplos de uso.
