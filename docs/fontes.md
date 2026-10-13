# Fontes e coleta

## Observação inicial

A especificação inicial registra que a API do Mercado Livre respondeu `403` e que requisições HTTP ao HTML receberam uma tela de verificação anti-bot no ambiente do programador. Esse comportamento é uma observação fornecida pelo responsável pelo projeto; os testes desta aplicação não repetem chamadas ao site.

## Decisão

A coleta ao vivo usa o WebView2 do próprio Tauri, que se comporta como um navegador e conserva cookies no perfil local. O app limita carregamentos a pelo menos dois segundos. Se a página mostrar uma verificação ou não contiver anúncios, o WebView é exibido para que Eduardo a resolva manualmente.

Os testes e a demonstração offline usam HTMLs sintéticos em `src-tauri/tests/fixtures/`. A implementação de parser Rust e o extrator JavaScript usam os mesmos campos canônicos: identificador, título, preço, URL, condição, vendedor, nota e quantidade de avaliações.

## Limites

- A aplicação não tenta resolver nem contornar verificações anti-bot.
- Nenhum teste automatizado visita o site ou depende de uma API externa.
- Mudanças no HTML do Mercado Livre podem exigir a atualização dos seletores do extrator.
- A validação ao vivo da coleta e da persistência de cookies fica pendente para Eduardo.

