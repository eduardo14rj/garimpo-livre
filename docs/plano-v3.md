# Garimpo Livre — Multi-loja (PRD + TRD)

Versão: v3 · Base: v1 (Fases 0–8) e v2 (Fases 9–13) já implementadas.

---

# PARTE 1 — PRD (Produto)

## 1. Problema

Hoje o Garimpo Livre só busca no Mercado Livre. O mesmo produto costuma estar mais barato em outra loja (KaBuM!, Pichau, Shopee) ou fora do Brasil (eBay, AliExpress, Amazon), e comparar isso à mão é lento e propenso a erro, porque frete, câmbio e imposto mudam o preço real.

## 2. Objetivo

Permitir que cada busca escolha **uma ou mais lojas**, compare os resultados num **ranking único em R$ (preço final)** e continue respondendo mesmo quando alguma loja falhar.

## 3. Lojas suportadas

| ID | Loja | Mercado | Moeda origem |
|---|---|---|---|
| `ml` | Mercado Livre | BR | BRL (já implementada) |
| `ebay` | eBay | Internacional | USD |
| `amazon` | Amazon (amazon.com.br) | BR | BRL |
| `shopee` | Shopee | BR | BRL |
| `aliexpress` | AliExpress | Internacional | USD (ou BRL via API) |
| `kabum` | KaBuM! | BR | BRL |
| `pichau` | Pichau | BR | BRL |

## 4. Histórias de usuário

1. Como usuário, quero **marcar quais lojas** uma busca deve consultar, para focar onde faz sentido (ex.: hardware → KaBuM!, Pichau, ML).
2. Como usuário, quero definir **lojas padrão** nas configurações, para não marcar toda vez.
3. Como usuário, quero ver **um ranking único** com todas as lojas, com o **preço final em R$** (produto + frete + câmbio + imposto estimado).
4. Como usuário, quero **filtrar o resultado por loja** e ver um **badge da loja** em cada item.
5. Como usuário, quero ver o **mesmo produto agrupado** entre lojas ("SSD Kingston NV2 1TB: KaBuM! R$ 389 · ML R$ 405 · Pichau R$ 399").
6. Como usuário, quero que, **se uma loja falhar, as outras continuem** e o app me diga qual falhou.
7. Como usuário, quero **ativar/desativar lojas** e informar credenciais só das que uso.
8. Como usuário, quero ver **separado** quanto é preço, frete e imposto estimado nos itens importados.

## 5. Requisitos funcionais

| # | Requisito |
|---|---|
| RF01 | A busca aceita `lojas: [...]`; vazio = lojas padrão da configuração. |
| RF02 | As lojas selecionadas são consultadas **em paralelo**. |
| RF03 | Resultado parcial: o que responder dentro do tempo limite aparece, com status por loja (ok / degradado / falhou / sem credencial). |
| RF04 | Todos os preços são convertidos para **BRL** com a cotação do dia. |
| RF05 | Itens importados mostram o **preço final estimado** (produto + frete + imposto de importação + ICMS), com regras configuráveis. |
| RF06 | Agrupamento do mesmo produto entre lojas (GTIN/EAN → modelo → similaridade de título). |
| RF07 | Ranking único (score + Pareto) calculado sobre o **preço final em BRL**. |
| RF08 | Filtro por loja e badge da loja na tabela e no gráfico. |
| RF09 | Tela de lojas: ativar/desativar, credenciais e status de saúde por loja. |
| RF10 | Histórico de preço e alertas funcionam por oferta (loja + item) e por grupo (menor preço entre lojas). |

## 6. Requisitos não funcionais

| # | Requisito |
|---|---|
| RNF01 | Busca com 3 lojas via API: resposta em **< 30s**. Timeout global de 60s. |
| RNF02 | Falha de uma loja **nunca** bloqueia as outras. |
| RNF03 | Rate limit, circuit breaker e cache **por loja**. |
| RNF04 | Credenciais no cofre do SO, nunca no repositório ou em `config.json`. |
| RNF05 | Adicionar uma nova loja = criar 1 módulo + fixtures, **sem alterar o core**. |
| RNF06 | Regras de imposto e câmbio são **dados configuráveis**, não código. |

## 7. Fora de escopo

- Checkout, carrinho ou compra automática
- Cupons, cashback e preços exclusivos de app
- Burlar captcha/anti-bot (proxies, solvers, disfarce de fingerprint)
- Garantia legal do valor de imposto (o app mostra **estimativa**)

## 8. Métricas de sucesso

- 100% das buscas multi-loja retornam resultado ou cache com aviso (nenhuma tela vazia por falha).
- Pelo menos 1 grupo cross-loja corretamente formado em buscas de produtos com GTIN (ex.: SSD, memória RAM).
- Preço final estimado de um item importado com diferença < 5% de uma simulação manual.

## 9. Riscos

| Risco | Mitigação |
|---|---|
| Amazon exige aprovação no Programa de Associados (Creators API) | Loja funciona via WebView assistida até a credencial existir |
| Anti-bot forte (Amazon, KaBuM!, Pichau) | Cadeia de fallback v2 + coleta assistida |
| Regras de imposto de importação mudam | Arquivo `regras_importacao.json` editável + aviso "estimativa" |
| Layout dos sites muda | Múltiplas estratégias de extração + teste de contrato (v2) |
| APIs de afiliado mudam assinatura/campos | Isolar cada cliente em módulo próprio + fixtures |

---

# PARTE 2 — TRD (Técnico)

## 1. Regras para o programador (mantidas + novas)

1. Só marcar `[x]` após o teste passar, com `[completion:: YYYY-MM-DD]`.
2. **(validação manual — Eduardo)**: implementar, deixar desmarcado e seguir.
3. Testes automáticos só com fixtures/fakes. Uma pasta de fixtures por loja: `tests/fixtures/<loja>/`.
4. `cargo test`, `clippy -D warnings`, `fmt --check` verdes. Commit por fase.
5. README: "O que aprendi — Fase X".
6. **Endpoints, assinaturas e campos das APIs devem ser confirmados na documentação oficial** antes de implementar, e registrados em `docs/lojas/<loja>.md`. Os valores citados aqui são ponto de partida.
7. Sem segredos no repositório.

## 2. Arquitetura

```
Busca { termo, lojas: [ml, ebay, kabum] }
        │
  OrquestradorMultiLoja ── JoinSet (paralelo, timeout 60s)
        │
   ┌────┼──────────────┐
  Loja ML     Loja eBay      Loja KaBuM!
   │           │              │
 Cadeia v2   Cadeia v2      Cadeia v2        ← cada loja com suas Fontes
 (Api→Web→   (Api→Web→      (Web→Assistida→
  Assist→     Cache)         Cache→Manual)
  Cache→Manual)
        │
  Normalização → OfertaNormalizada (BRL, frete, imposto, reputação 0–1)
        │
  Matching cross-loja (GTIN → modelo → Jaccard)
        │
  Filtros → Score → Pareto → Ranking único
```

## 3. Contratos

```rust
#[derive(Clone, Copy, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub enum LojaId { Ml, Ebay, Amazon, Shopee, Aliexpress, Kabum, Pichau }

#[async_trait]
pub trait Loja: Send + Sync {
    fn id(&self) -> LojaId;
    fn moeda_origem(&self) -> Moeda;
    fn requer_credencial(&self) -> bool;
    fn fontes(&self) -> Vec<Box<dyn Fonte>>;              // cadeia de fallback v2
    fn normalizar(&self, bruto: AnuncioBruto) -> Result<OfertaNormalizada, ErroNormalizacao>;
    fn reputacao_0_1(&self, bruto: &AnuncioBruto) -> Option<f64>;
}

pub struct OfertaNormalizada {
    pub id: String,                 // "<loja>:<id_externo>"  ex.: "kabum:123456"
    pub loja: LojaId,
    pub titulo: String,
    pub url: String,
    pub gtin: Option<String>,
    pub modelo: Option<String>,
    pub preco_origem: Decimal, pub moeda_origem: Moeda,
    pub preco_brl: Decimal,
    pub frete_brl: Option<Decimal>,
    pub imposto_estimado_brl: Option<Decimal>,
    pub preco_final_brl: Decimal,   // usado no ranking
    pub nota: Option<f64>, pub qtd_avaliacoes: Option<u32>,
    pub vendas: Option<u32>,        // proxy de confiança quando não há nº de avaliações
    pub reputacao: Option<f64>,     // 0–1
    pub importado: bool,
}

pub struct ResultadoMultiLoja {
    pub ofertas: Vec<OfertaNormalizada>,
    pub status: HashMap<LojaId, StatusLoja>, // Ok | Degradado{fonte} | Falhou{erro} | SemCredencial | Desativada
}
```

Usar `rust_decimal` para dinheiro (nunca `f64` para valores monetários).

## 4. Especificação por loja

| Loja | Fonte principal | Autenticação | Fallback | Reputação 0–1 |
|---|---|---|---|---|
| **ML** | API v2 (já existe) | OAuth (já existe) | WebView → Assistida → Cache → Manual | nível do vendedor (5 níveis → 0, .25, .5, .75, 1) |
| **eBay** | Browse API `GET /buy/browse/v1/item_summary/search?q=` | OAuth **client credentials** (scope `https://api.ebay.com/oauth/api_scope`) | WebView `ebay.com/sch/` → Cache | `feedbackPercentage/100`, ponderado pelo `feedbackScore` (média bayesiana) |
| **Amazon** | Creators API (substituiu a PA-API 5.0, desligada em 30/04/2026) | Credenciais do Programa de Associados | **WebView assistida** (anti-bot forte) → Cache → Manual | vendido/enviado pela Amazon = 1.0; demais = sem dado |
| **Shopee** | Affiliate Open API (GraphQL, `productOfferV2`) | App ID + Secret, header assinado com SHA-256 | WebView → Cache | avaliação da loja (`/5`); loja oficial/preferida = bônus |
| **AliExpress** | Open Platform Affiliate (`aliexpress.affiliate.product.query`) | App Key + Secret, assinatura dos parâmetros | WebView → Cache | avaliação positiva da loja (%) |
| **KaBuM!** | **WebView** (sem API pública) | — | Assistida → Cache → Manual | 1.0 (loja vende direto); marketplace = sem dado |
| **Pichau** | **WebView** (sem API pública) | — | Assistida → Cache → Manual | 1.0 (loja vende direto) |

Notas:
- **eBay**: definir marketplace (`X-EBAY-C-MARKETPLACE-ID`) e o contexto de entrega para o Brasil, para obter frete; confirmar o header correto na documentação.
- **AliExpress**: testar se a API aceita moeda/país de destino BRL/BR; se aceitar, usar e pular a conversão.
- **Shopee/AliExpress**: podem não trazer o nº de avaliações. Usar `vendas` como proxy de confiança na média bayesiana (`v = min(vendas, avaliações conhecidas)`), registrando a decisão em `docs/decisoes.md`.
- **KaBuM!/Pichau**: na Fase 0 de cada loja, investigar as estratégias de extração na ordem: JSON-LD → estado embutido (ex.: `__NEXT_DATA__`, se for Next.js) → seletores CSS.

## 5. Câmbio e custo de importação

**Câmbio:** cotação PTAX de venda do dia via API pública do Banco Central (Olinda/BCB, sem autenticação). Cache de 1 dia na tabela `cotacoes`. Se a API falhar, usar a última cotação salva e marcar `cotacao_desatualizada`.

**Imposto:** regras em `regras_importacao.json` (na pasta de dados), editáveis pelo usuário:

```json
{
  "vigente_desde": "2024-08-01",
  "aviso": "Estimativa. Confirme as regras vigentes na Receita Federal.",
  "faixas_imposto_importacao": [
    { "ate_usd": 50,   "aliquota": 0.20, "deducao_usd": 0 },
    { "ate_usd": 3000, "aliquota": 0.60, "deducao_usd": 20 }
  ],
  "icms": { "aliquota": 0.20, "por_dentro": true },
  "base_inclui_frete": true
}
```

Cálculo (função pura em `core/custo.rs`):
```
base_usd   = preco_usd + (base_inclui_frete ? frete_usd : 0)
ii_usd     = base_usd * aliquota_faixa − deducao_faixa   (mínimo 0)
subtotal   = (base_usd + ii_usd) * cotacao
icms       = por_dentro ? subtotal * a / (1 − a) : subtotal * a
final_brl  = subtotal + icms
```
Os valores padrão são **ponto de partida**: o Eduardo confere e ajusta no JSON.

## 6. Matching cross-loja

Ordem de decisão (para na primeira que casar):
1. **GTIN/EAN** igual → mesmo produto.
2. **Modelo** extraído igual + marca igual.
3. **Jaccard ≥ 0.8** nos tokens do título normalizado **e** atributos-chave compatíveis (capacidade, voltagem).

Tabela `grupos` + `grupo_ofertas`. O grupo exibe a oferta de menor `preco_final_brl` como principal.

## 7. Modelo de dados (migration)

```sql
CREATE TABLE lojas (
  id TEXT PRIMARY KEY, nome TEXT, ativa INTEGER, padrao INTEGER,
  moeda TEXT, requer_credencial INTEGER
);
ALTER TABLE produtos ADD COLUMN loja TEXT NOT NULL DEFAULT 'ml';
ALTER TABLE produtos ADD COLUMN gtin TEXT;
ALTER TABLE produtos ADD COLUMN importado INTEGER DEFAULT 0;
ALTER TABLE snapshots ADD COLUMN moeda_origem TEXT DEFAULT 'BRL';
ALTER TABLE snapshots ADD COLUMN preco_origem TEXT;   -- decimal como texto
ALTER TABLE snapshots ADD COLUMN frete_brl TEXT;
ALTER TABLE snapshots ADD COLUMN imposto_brl TEXT;
ALTER TABLE snapshots ADD COLUMN preco_final_brl TEXT;
ALTER TABLE snapshots ADD COLUMN cotacao TEXT;
CREATE TABLE cotacoes (data TEXT, moeda TEXT, valor TEXT, PRIMARY KEY (data, moeda));
CREATE TABLE grupos (id INTEGER PRIMARY KEY, chave TEXT UNIQUE, titulo TEXT);
CREATE TABLE grupo_ofertas (grupo_id INTEGER, produto_id TEXT, PRIMARY KEY (grupo_id, produto_id));
```
- Migrar os IDs existentes do ML para o formato `ml:<id>` (atualizar `produtos` e `snapshots` na mesma transação).
- Rate limit, circuit breaker, cache e `saude_fontes` passam a usar a chave `(loja, fonte)`.

## 8. Formato da busca (atualizado)

```json
{
  "termo": "ssd nvme 1tb",
  "lojas": ["ml", "kabum", "pichau", "aliexpress"],
  "incluir_importados": true,
  "preco_max": 500,
  "proibido": ["usado", "recondicionado"],
  "min_avaliacoes": 10,
  "pesos": { "preco": 0.4, "nota": 0.3, "vendedor": 0.2, "relevancia": 0.1 }
}
```
- `lojas` ausente/vazio → lojas com `padrao = 1`.
- Loja desconhecida → erro de validação claro.
- `preco_max` é comparado com o `preco_final_brl`.

---

# PARTE 3 — Fases de implementação

## Fase 14 — Core multi-loja

**Conceitos:** trait objects (`Box<dyn Loja>`), registry pattern, concorrência estruturada (`JoinSet`), timeouts, resultado parcial, aritmética decimal para dinheiro, migrations com transformação de dados.

- [x] `LojaId`, trait `Loja`, `OfertaNormalizada`, `ResultadoMultiLoja` e `RegistroLojas` (mapa `LojaId → Box<dyn Loja>`). [completion:: 2026-10-12]
  **Teste:** o registro devolve a loja certa para cada ID; ID desconhecido → erro.
- [x] Adaptar o ML existente como a primeira implementação de `Loja` (sem mudar o comportamento). [completion:: 2026-10-12]
  **Teste:** todos os testes v1/v2 continuam verdes.
- [x] Migration da Seção 7, incluindo a conversão dos IDs para `ml:<id>`. [completion:: 2026-10-12]
  **Teste:** banco v2 com dados semeados → após a migration, os IDs começam com `ml:`, os snapshots continuam ligados e a contagem é igual.
- [x] `OrquestradorMultiLoja`: consulta as lojas em paralelo com timeout global de 60s e devolve resultado parcial + status por loja. [completion:: 2026-10-12]
  **Teste:** 3 lojas fake [ok em 1s, falha, trava] → resultado com as ofertas da 1ª; status = Ok / Falhou / Falhou(timeout); tempo total ≤ 60s (relógio fake).
- [x] Rate limit, circuit breaker, cache e saúde com chave `(loja, fonte)`. [completion:: 2026-10-12]
  **Teste:** circuito aberto no eBay não afeta o ML.
- [x] Validação da busca (`lojas`, padrão, loja desconhecida). [completion:: 2026-10-12]
  **Teste:** `["ml","xpto"]` → erro; `[]` → usa as lojas padrão.

## Fase 15 — Câmbio + custo de importação

**Conceitos:** consumo de API pública (BCB/PTAX), cache diário, `rust_decimal`, configuração como dado, funções puras para regra fiscal.

- [x] Cliente PTAX (BCB) + tabela `cotacoes` + fallback para a última cotação. [completion:: 2026-10-12]
  **Teste:** com resposta fixture → cotação salva; com a API falhando → usa a última e marca `cotacao_desatualizada`.
- [x] `regras_importacao.json` criado com os valores padrão na primeira execução; carregado e validado. [completion:: 2026-10-12]
  **Teste:** JSON inválido → erro claro; faixa sem `ate_usd` → erro.
- [x] `calcular_preco_final` (Seção 5), puro e com `Decimal`. [completion:: 2026-10-12]
  **Teste:** US$ 40 + US$ 0 de frete, cotação 5.00, regras padrão → II = 8.00 USD, subtotal = 240.00, ICMS por dentro 20% = 60.00, final = R$ 300.00.
  **Teste:** US$ 100 + US$ 10 de frete, cotação 5.00 → II = 110×0.60 − 20 = 46 USD, subtotal = 780.00, ICMS = 195.00, final = R$ 975.00.
- [x] Itens nacionais: `preco_final_brl = preco_brl + frete_brl` (sem imposto). [completion:: 2026-10-12]
- [ ] **(validação manual — Eduardo)** Conferir os valores padrão do JSON com as regras vigentes.

## Fase 16 — eBay

**Conceitos:** OAuth client credentials (token de aplicação, sem usuário), headers de contexto (marketplace/localização), paginação por `offset`/`limit`.

- [x] `docs/lojas/ebay.md`: endpoint, scope, headers e campos confirmados. [completion:: 2026-10-12]
- [x] Token client credentials com cache até expirar (credenciais no cofre). [completion:: 2026-10-12]
  **Teste:** mock → 2 buscas seguidas = 1 pedido de token.
- [x] `FonteEbayApi::listar` + mapeamento para `OfertaNormalizada` (preço USD, frete, `feedbackPercentage`, `feedbackScore`, condição, GTIN se disponível). [completion:: 2026-10-12]
  **Teste:** `fixtures/ebay/search.json` → ofertas com `importado = true`, preço final calculado e reputação correta.
- [x] Fallback `FonteWebview` para `ebay.com/sch/` com extrator próprio. [completion:: 2026-10-12]
  **Teste:** `fixtures/ebay/listagem.html` → mesmos campos da API.
- [ ] **(validação manual — Eduardo)** Criar conta dev no eBay, cadastrar as credenciais e fazer uma busca real.

## Fase 17 — Shopee

**Conceitos:** GraphQL (queries, variáveis, paginação), assinatura de requisição com SHA-256 (timestamp + payload + segredo), proxy de confiança (vendas).

- [x] `docs/lojas/shopee.md`: endpoint GraphQL BR, formato da assinatura, campos de `productOfferV2`. [completion:: 2026-10-12]
- [x] Gerador de assinatura (função pura). [completion:: 2026-10-12]
  **Teste:** entrada fixa (appId, timestamp, payload, secret) → assinatura igual ao valor calculado no teste (documentar o cálculo).
- [x] `FonteShopeeApi::listar` + mapeamento (preço mín/máx, nota, vendas, loja, link). [completion:: 2026-10-12]
  **Teste:** `fixtures/shopee/product_offer.json` → ofertas corretas; item com faixa de preço usa o preço mínimo e marca `preco_variavel = true`.
- [x] Fallback WebView. [completion:: 2026-10-12]
- [ ] **(validação manual — Eduardo)** Cadastro de afiliado Shopee + busca real.

## Fase 18 — AliExpress

**Conceitos:** assinatura de parâmetros ordenados (HMAC), APIs estilo "method + params", moeda/país de destino.

- [x] `docs/lojas/aliexpress.md`: método, parâmetros, algoritmo de assinatura, suporte a `target_currency`/`ship_to_country`. [completion:: 2026-10-12]
- [x] Gerador de assinatura (função pura). [completion:: 2026-10-12]
  **Teste:** parâmetros fixos → assinatura esperada.
- [x] `FonteAliexpressApi::listar` + mapeamento (preço, frete se houver, avaliação da loja, vendas). [completion:: 2026-10-12]
  **Teste:** `fixtures/aliexpress/query.json` → ofertas importadas com preço final calculado (ou BRL direto, se a API suportar).
- [x] Fallback WebView. [completion:: 2026-10-12]
- [ ] **(validação manual — Eduardo)** Cadastro no Open Platform + busca real.

## Fase 19 — Amazon

**Conceitos:** feature flag por credencial, degradação planejada (loja funcional só com fallback).

- [x] `docs/lojas/amazon.md`: requisitos de acesso da Creators API, endpoint e campos. [completion:: 2026-10-12]
- [x] `FonteAmazonApi` atrás de flag: só entra na cadeia se houver credencial. [completion:: 2026-10-12]
  **Teste:** sem credencial → a fonte é pulada e o status é `SemCredencial` (as outras fontes da Amazon continuam).
- [x] `FonteWebview` para `amazon.com.br/s?k=` com coleta **assistida** como padrão. [completion:: 2026-10-12]
  **Teste:** `fixtures/amazon/listagem.html` → ofertas com preço, nota, nº de avaliações e flag "vendido pela Amazon".
- [ ] **(validação manual — Eduardo)** Busca real via WebView assistida; API quando o acesso de Associado for aprovado.

## Fase 20 — KaBuM! e Pichau

**Conceitos:** reaproveitar o extrator multi-estratégia da v2, estado embutido de apps Next.js/SPA, preço à vista vs parcelado.

- [x] `docs/lojas/kabum.md` e `docs/lojas/pichau.md`: URL de busca, estratégia de extração escolhida, campos disponíveis. [completion:: 2026-10-12]
- [x] Extratores (JSON-LD → estado embutido → CSS) para listagem e produto de cada loja. [completion:: 2026-10-12]
  **Teste parametrizado:** fixtures `so_jsonld`, `so_estado` e `so_dom` de cada loja produzem as mesmas ofertas.
- [x] Tratar preço à vista (PIX/boleto) vs parcelado: usar o **à vista** como `preco_brl` e guardar o parcelado como informação extra. [completion:: 2026-10-12]
  **Teste:** fixture com os dois preços → `preco_brl` = à vista.
- [x] Extrair o GTIN/EAN quando presente na página de produto. [completion:: 2026-10-12]
- [ ] **(validação manual — Eduardo)** Busca real em cada loja.

## Fase 21 — Matching cross-loja + ranking único

**Conceitos:** entity resolution multi-fonte, chaves de bloqueio (agrupar candidatos antes de comparar), BM25 sobre corpus combinado.

- [x] Matching (Seção 6) + tabelas `grupos`/`grupo_ofertas`. [completion:: 2026-10-12]
  **Teste:** ofertas fake → mesmo GTIN em ML/KaBuM!/Pichau = 1 grupo; mesmo modelo "NV2 1TB" sem GTIN = 1 grupo; "NV2 500GB" fica em outro grupo (capacidade diferente).
- [x] Ranking, filtros, BM25, score e Pareto usando `preco_final_brl` e a reputação 0–1 normalizada. [completion:: 2026-10-12]
  **Teste de integração:** busca fake em 3 lojas (1 importada) → o item importado barato mas com imposto alto cai no ranking; os grupos aparecem com o menor preço como principal.
- [x] Alertas de mínima por grupo (menor preço entre as lojas). [completion:: 2026-10-12]
  **Teste:** dados semeados → alerta dispara quando qualquer loja do grupo atinge a mínima de 30 dias do grupo.

## Fase 22 — Interface multi-loja

**Conceitos:** estado de formulário com seleção múltipla, exibição de resultado parcial/progressivo (eventos por loja), comunicação de incerteza (badge "estimativa").

- [x] Tela **Lojas**: ativar/desativar, marcar padrão, credenciais (cofre do SO), status de saúde por loja. [completion:: 2026-10-12]
- [x] Na busca: seleção múltipla de lojas (checkboxes) + atalho "lojas padrão" + opção "incluir importados". [completion:: 2026-10-12]
- [x] Resultado progressivo: cada loja emite um evento ao terminar e a tabela atualiza sem esperar as outras. [completion:: 2026-10-12]
  **Teste:** com lojas fake de tempos diferentes → a UI recebe 3 eventos em ordem de conclusão.
- [x] Tabela: badge da loja, filtro por loja, colunas preço / frete / imposto / **final**, ícone "estimativa" nos importados. [completion:: 2026-10-12]
- [x] Linha de grupo expansível: mostra todas as lojas do mesmo produto. [completion:: 2026-10-12]
- [x] Faixa de status no topo: "ML ✓ · KaBuM! ✓ · Pichau ⚠ cache de 12/10 · Amazon ✗ sem credencial". [completion:: 2026-10-12]
- [x] Gráfico preço × nota colorido por loja. [completion:: 2026-10-12]
  **Teste (fase):** com fontes fixture: busca em 4 lojas, 1 falhando → resultado parcial, faixa de status correta, filtro por loja funcionando.
- [ ] **(validação manual — Eduardo)** Busca real "ssd nvme 1tb" em ML + KaBuM! + Pichau + AliExpress.

---

## Definição de pronto (v3)

- As 7 lojas implementam o trait `Loja`, cada uma com fixtures e testes.
- Busca multi-loja paralela com resultado parcial e status por loja.
- Preço final em BRL com câmbio do dia e imposto configurável (testado com os valores das Fases 15).
- Matching cross-loja e ranking único funcionando.
- UI com seleção de lojas, badges, filtro, grupos e faixa de status.
- `docs/lojas/*.md` preenchidos e README com "O que aprendi" das Fases 14–22.
- Tarefas **(validação manual — Eduardo)** pendentes para conferência no PC.
