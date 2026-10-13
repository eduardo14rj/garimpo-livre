# Progresso da v3

Atualizado em 2026-10-12. Escopo integral: [PRD/TRD e checklist](plano-v3.md).

## Implementado e verificado — Fases 14 e 15

| Requisito | Evidência no código e nos testes |
|---|---|
| IDs conhecidos, contrato de loja, ofertas decimais, registro e padrões | `lojas/mod.rs`; `busca_antiga_e_desconhecida`, `registro_devolve_cada_id_e_rejeita_duplicados` |
| Adaptador ML; compatibilidade de v1/v2 | `lojas/ml.rs`; fixture de 15 anúncios; os 45 testes preexistentes continuam passando |
| Migração v2 → v3 com IDs e snapshots preservados | `storage/multiloja.rs`; `migration_v2_preserva_ids_vinculos_contagens`, rollback de colisão e upsert entre dias |
| Paralelismo, resultado parcial e limite global de 60 s | `lojas/orchestrator.rs`; relógio Tokio pausado com sucesso em 1 s, falha em 2 s e fonte travada |
| Eventos por ordem de conclusão | Mesmo teste verifica os três eventos; Fase 22 integra os parciais na interface e testa quatro conclusões |
| Três APIs em menos de 30 s | Fake com fontes de 5, 8 e 12 s; conclusão total em 12 s |
| Circuito, saúde, rate limit e cache independentes | Testes de isolamento por loja/fonte; cache também separa termos; loja desativada não inicia fonte |
| Cancelamento e cache de contingência | Timeout registra falha da sondagem e retorna cache antigo com data; `SessaoColetor` remove listeners e janelas ao cancelar |
| Câmbio PTAX, cache diário, último valor e aviso de desatualização | `lojas/ptax.rs`; resposta HTTP sintética com cinco casas decimais, segunda chamada sem HTTP e falha com fallback |
| Arquivo de regras criado sem sobrescrever o usuário | `storage/multiloja.rs`; valida JSON inválido, campos obrigatórios e faixas |
| Importação em Decimal | `core/custo.rs`; gabaritos R$ 300,00 e R$ 975,00; frete fora da base; overflow e limites inválidos |
| Oferta nacional e importada já em BRL | `lojas/mod.rs`; preço nacional + frete sem tributos, USD importado e BRL importado equivalentes |

Comandos Tauri incluem `list_stores`, `configure_store`, cofre por loja, regras, `run_multi_search`, `run_all_multi_searches`, resultados/histórico/CSV multi-loja e entrada manual. As sete lojas estão registradas. A interface ativa usa o fluxo multi-loja, com `garimpo:loja-concluida`, `garimpo:ranking-parcial` e `garimpo:ranking-multi`.

### Checks executados

- `cargo test`: 63 testes unitários, zero falhas; binário e doc-tests concluídos.
- `cargo clippy -- -D warnings`: aprovado.
- `cargo fmt --check`: aprovado.
- `npm run build`: TypeScript e bundle Vite aprovados.
- `npm run test:extractor`: 13 testes aprovados.
- App desktop após o rebuild: a busca existente permaneceu visível e o limite de R$ 50,00 continuou formatado corretamente. Isso confirma a leitura da busca na interface; não substitui a validação manual das novas lojas.
- `git diff --check`: aprovado para os arquivos rastreados; o projeto continua majoritariamente não rastreado, como estava no início. Nenhum commit foi feito.

## Implementado e verificado — Fase 16

- `lojas/ebay.rs`: Browse API, token de aplicação compartilhado até expiração, uma renovação após 401, invalidação quando a credencial muda, paginação limitada e detalhe com GTIN/MPN.
- `lojas/credenciais.rs`: cofre por loja, campos públicos sem segredo, preservação do segredo existente e remoção. Troca de configuração invalida somente o cache daquela loja.
- `ebay_extractor.js` e transporte `lojas/webview.rs`: dinheiro em string, verificação assistida, URLs restritas, janelas/eventos/listeners/popups por tentativa e limpeza no cancelamento. IPC remoto concede somente emissão de eventos.
- A reputação bayesiana do vendedor fica no core. Feedback não vira review de produto. Frete sem contexto de entrega continua desconhecido.
- `run_multi_search` registra ML/eBay. O deadline começa antes da PTAX; fixture usa câmbio sintético offline.
- Teste integrado com as duas implementações: 17 ofertas, dois eventos, eBay sem credencial continua pelo fallback; desmarcar importados mantém as 15 ofertas nacionais. Ainda não é o ranking combinado da Fase 21.
- Gates desta entrega: **77 testes Rust**, **23 testes JavaScript**, `cargo clippy -- -D warnings`, adicionalmente Clippy de todos os targets, `cargo fmt --check` e `npm run build` aprovados. Testes automáticos somente com fixtures, cofre fake e HTTP local.
- Contrato e limitações em `lojas/ebay.md`; aprendizado da Fase 16 no README. Conta dev, credencial real e busca ao vivo seguem desmarcadas para Eduardo.

## Implementado e verificado — Fase 17

- Contrato oficial BR lido antes da implementação, incluindo assets públicos da documentação. Gabarito SHA-256 oficial confirmado de forma independente e por teste puro no core.
- `lojas/shopee.rs`: GraphQL com variáveis, assinatura dos mesmos bytes enviados, paginação limitada, rate limit por POST, timestamp após espera e deduplicação. HTTP 200 com erro GraphQL não vira sucesso vazio; corpo remoto não vaza em erros.
- Mapeamento Decimal de mínimo/máximo e `preco_variavel`, nota de produto, vendas e vendedor. API não fornece nota da loja/review count/origem/frete/condição; esses limites estão documentados para a UI da Fase 22.
- Extrator próprio JSON-LD/DOM com precisão decimal, campos complementares, importação explícita, login/verificação assistidos. Bônus de reputação escolhido pelo Eduardo: oficial +0,05 ou preferida +0,025, somente com nota da loja, teto 1.
- `run_multi_search` registra a Shopee; API sem cofre é pulada e o fallback continua. Teste integrado confirma 15 ofertas ML + 2 Shopee e dois eventos, com status `SemCredencial` na Shopee.
- Gates após a integração: **86 testes Rust**, **31 testes JavaScript**, `cargo clippy -- -D warnings`, adicionalmente Clippy de todos os targets, `cargo fmt --check` e `npm run build` aprovados. Tudo com fixtures/fakes e HTTP local; sem automação da área de trabalho.
- Checklist automático da Fase 17 marcado com data; cadastro/credencial/busca real da Shopee permanecem desmarcados para Eduardo. README contém o aprendizado da fase.

## Implementado e verificado — Fase 18

- Contrato oficial e SDK inspecionados antes do cliente: protocolo TOP, HMAC-SHA256 ordenado, `method` dentro do mapa, sem prefixo e timestamp em milissegundos. Teste puro usa o gabarito oficial atual, também confirmado pelo .NET.
- `lojas/aliexpress.rs`: form POST assinado antes do encoding, destino BR/BRL/PT, paginação limitada, timestamp após rate limit, deduplicação, IDs inteiros sem perda e mensagens de erro sem corpo remoto.
- Fixture BRL retorna R$ 200 de produto e R$ 300 finais; USD preserva 25.123456789 e calcula R$ 188,43. Preço exclusivo do app não substitui preço comum; `evaluate_rate` do produto não vira reputação da loja.
- WebView próprio usa moeda explícita, JSON-LD/DOM complementares, frete e percentual positivo da loja quando disponíveis. API sem cofre continua pelo fallback e o filtro de importados remove as ofertas AliExpress.
- Gates: **95 testes Rust**, **37 JavaScript**, Clippy de todos os targets com `-D warnings`, `cargo fmt --check` e `npm run build` aprovados. Nenhum teste automatizado acessou a loja real ou a área de trabalho.
- README e contrato atualizados; validação real permanece desmarcada. A UI deverá comunicar a possível presença de tributos no preço de destino retornado pela API.

## Implementado e verificado — Fase 19

- Creators API atual confirmada em fontes oficiais antes do código: OAuth JSON, endpoint por versão, scope, Bearer, marketplace BR e recursos camelCase. Não usa PA-API antiga.
- Cliente compartilhado mantém token em memória, impede emissões concorrentes duplicadas, invalida por segredo/versão, renova uma vez após 401 e limita páginas/pedidos. Opções públicas e Secret ficam no cofre; a fonte indisponível é pulada sem bloquear o fallback.
- WebView Amazon assistida inicia visível. Extrator JSON-LD/DOM preserva dinheiro, reviews e catálogo, exige evidência de venda e envio Amazon para reputação 1 e não mistura vendedores de um mesmo ASIN.
- API não fornece frete/fulfillment/review count nos recursos utilizados; campos desconhecidos permanecem desconhecidos. Ofertas Prime exclusivas, assinatura recorrente, MAP e indisponíveis não entram como preços gerais.
- Gates finais: **103 testes Rust**, **44 JavaScript**, Clippy de todos os targets sem warnings, formatação e build. Testes usam somente fakes/fixtures/HTTP local. Busca real e acesso Associado continuam manuais e desmarcados.

## Implementado e verificado — Fase 20

- KaBuM!/Pichau possuem módulos, URLs restritas, WebView assistida e fixtures próprias. JSON-LD, estado JSON e DOM se complementam; dinheiro permanece textual até o Decimal.
- Listagem e detalhe das três estratégias retornam os mesmos campos; PIX/boleto tem prioridade, parcelamento é informação separada e parcela isolada não vira preço total. GTIN, estoque e venda direta são tratados explicitamente.
- Inspeção pública confirmou JSON-LD/Next.js da KaBuM!. Na Pichau, conteúdo público confirmou os campos, mas o HTML bruto retornou 403; não se afirma validação do layout real.
- Integração com os sete módulos reais em modo fixture: **27 ofertas, sete eventos de conclusão, sete status Ok**. Gates: **106 testes Rust**, **64 JavaScript**, Clippy de todos os targets com `-D warnings`, formatação e build aprovados.
- Contratos, decisões e README atualizados; busca real continua desmarcada. Fallback manual genérico e interface foram integrados na Fase 22 abaixo.

## Implementado e verificado — Fase 21

- `core/multiloja.rs`: GTIN/modelo+marca/Jaccard, proteção de variantes e conflitos, bloqueio por tokens, corpus BM25 combinado, filtros e ranking com final Decimal, reputação normalizada e Pareto.
- `storage/ranking_multi.rs`: persistência dos grupos, resultado/CSV próprios, histórico Decimal por oferta e mínimo diário por grupo, janela de 30 dias, média de 7 dias e deduplicação de alertas. Exige sete datas observadas; cache não é observação nova e fixture não contamina o histórico real.
- Comando multi-loja retorna ranking e publica parciais a cada loja. Testes: três lojas incluindo importada, dinheiro de alta precisão, variantes, filtro de reviews reais, confiança por vendas, persistência, 30 dias e amostra curta.
- Gates: **114 testes Rust**, **64 JavaScript**, Clippy de todos os targets com `-D warnings`, formatação e build aprovados.

## Implementado e verificado — Fase 22 e integração final

- `src/main.tsx` usa `MultiApp`: tela Lojas com ativação/padrão/cofre/saúde, seleção múltipla e padrões na busca, importados, status por loja, resultados progressivos, grupos expansíveis, filtro, tabela de preço/frete/imposto/final e gráfico com cores/legenda por loja. Ícones ativos são emoji.
- Histórico e CSV usam o ranking combinado. Configurações permitem modo demonstração/real, intervalo por fonte e regras fiscais JSON, preservando dinheiro textual. Secret nunca é reexibido; apenas campos públicos e indicador de segredo salvo.
- Editor preserva espaços enquanto se digita; vírgulas separam expressões somente ao salvar. A UI mostra exclusões por filtro, dados desconhecidos, estimativas e avisos de API Shopee/AliExpress, evitando apresentar falta de informação como valor zero.
- Fallback manual valida loja/domínio/Decimal/catálogo e persiste a captura datada. Execução headless compartilha o fluxo multi-loja e pode reutilizar cache/manual sem janela. Demonstração não acessa PTAX/cofre/lojas nem grava snapshots reais, e não reutiliza cache real.
- Auditoria adicionou proteção ao dividir grupos persistidos após correção de GTIN, níveis de reputação ML sem alterar o score legado, e oferta Amazon por condição para preservar a nova quando há usada mais barata.
- `npm run test:multi` executado e aprovado: sete módulos, **27 ofertas, sete eventos**, repetidos sem cache/snapshots reais, e **cinco testes de interface**. A UI fake usa quatro lojas com uma falha, verifica parciais, expansão, filtro e pontos do gráfico; também cobre termos com espaço, cofre, data do cache, demonstração e parcelamento USD.
- Gates finais aprovados em 2026-10-12: **122 testes Rust**, **66 testes de extratores/login**, **5 testes de UI**, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`, `npm run build` e `npm run test:multi`. Todos os testes automáticos usam fixtures/fakes/banco temporário/HTTP local; nenhum executa login real ou controla a área de trabalho.
- README inclui aprendizados das Fases 14–22. Contratos e decisões atualizados. Nenhum commit feito.

## Validações manuais pendentes

Todas as tarefas marcadas **validação manual — Eduardo** continuam desmarcadas: regras fiscais vigentes, credenciais/aprovações e consultas reais nas lojas, incluindo a busca conjunta indicada no checklist. A validação do clique real “Já tenho conta”, do layout atual dos sites e das notificações do Windows também exige execução no aplicativo. Os testes sintéticos do clique reproduzem o diagnóstico recebido e passaram, mas não comprovam o login real.
