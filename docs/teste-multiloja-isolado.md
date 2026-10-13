# Testar sem acessar o computador remotamente

Na pasta do projeto, execute:

```powershell
npm run test:multi
```

O runner usa o diretório `src-tauri/target-codex-offline` por padrão, separado do build de desenvolvimento do Tauri. Ele roda o pipeline com sete implementações e fixtures, SQLite em memória e a interface com Tauri falso em `jsdom`. Não abre janelas, não usa conta, não lê o cofre do Windows e não envia tráfego a lojas.

Para repetir especificamente a navegação de “Já tenho conta” com a telemetria enviada e o handler da página que cancela o clique:

```powershell
npm run test:login
```

Para validar todos os extratores sobre HTML local, incluindo conteúdo carregado depois e rolagem simulada:

```powershell
npm run test:extractor
```

Esses testes comprovam o comportamento local sob fixtures. Não comprovam que a página ao vivo ainda usa o mesmo HTML ou seletores. A Fase C5 prevê um relatório de diagnóstico feito no próprio app para compartilhar apenas o resultado relevante, sem abrir acesso remoto ao computador.
