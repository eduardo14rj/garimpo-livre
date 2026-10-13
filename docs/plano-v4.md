# PRD de Correção — Garimpo Livre: Somente WebView (sem API, sem credenciais)

Versão: v4 (correção) · Base: v1, v2 e v3.

Checklist copiado integralmente da especificação fornecida. Itens só serão marcados após os respectivos testes/evidências. Validações contra sites reais permanecem obrigatórias.

## 1. Problema

A v3 adicionou APIs oficiais/de afiliado (OAuth, App ID, Secret, Tracking ID, cofre de credenciais) e uma estimativa de importação com câmbio e regras fiscais em JSON. Isso deixou o app complexo, exige cadastros em programas de desenvolvedor/afiliado e não é o que o projeto pretende.

Além disso, a coleta via WebView do AliExpress está com **0% de sucesso** (tela Lojas).

## 2. Objetivo

O app busca em **todas as lojas apenas navegando nas páginas por WebView**, como uma pessoa faria, e extrai os dados com regras determinísticas (sem LLM). Nada de API, chave, secret, token, cadastro de desenvolvedor/afiliado ou cálculo de imposto.

## 3. Princípios

1. **Uma única forma de coleta por loja: WebView.** Fallback: WebView assistida → Cache → Manual.
2. **Sem credenciais.** Se uma loja pedir login (ex.: Shopee), o usuário entra com a própria conta dentro da janela do app; os cookies ficam no perfil do WebView. O app nunca vê nem salva senha.
3. **Preço = o que a página mostra em R$.** Sem câmbio, sem imposto estimado.
4. **Sem LLM.** Extração por JSON-LD → dados embutidos em `<script>` → seletores CSS.
5. **Sem burlar captcha.** Verificação é resolvida pelo usuário na janela visível.

## 4. Regras para o programador

1. Só marcar `[x]` após o teste passar, com `[completion:: YYYY-MM-DD]`.
2. `cargo test`, `clippy -D warnings`, `fmt --check` e testes do front verdes. Commit por fase.
3. **O teste real com o site é obrigatório** (Seção 9). Se o ambiente do programador for bloqueado pelas lojas, entregar o **modo diagnóstico** (Fase C5) para o Eduardo rodar com um clique e anexar o relatório gerado.
4. Seletores e padrões de cada loja ficam em **um arquivo por loja** (`coleta/lojas/<loja>.rs` + `extratores/<loja>.js`), documentados em `docs/lojas/<loja>.md` com a data em que foram verificados.
5. Antes de escrever um extrator, **capturar o HTML renderizado real** (Fase C2) e basear seletores nele; nunca em suposição.
6. README: "O que aprendi — Fase CX".

---

## 5. O que REMOVER

### Código (Rust)
- [ ] `FonteApi` do Mercado Livre (OAuth, login, refresh token, PKCE).
- [ ] Clientes de API: eBay (Browse/client credentials), Amazon (Creators API), Shopee (Affiliate GraphQL + assinatura SHA-256), AliExpress (Open Platform + assinatura).
- [ ] Módulo de credenciais/cofre (crate `keyring` e qualquer leitura/gravação de secret).
- [ ] Câmbio: cliente PTAX/BCB, tabela `cotacoes`, conversão de moeda.
- [ ] Custo de importação: `core/custo.rs`, `regras_importacao.json`, flags `importado`/`cotacao_desatualizada`.
- [ ] Status `SemCredencial` e feature flags por credencial.
- [ ] Dependências que ficarem sem uso no `Cargo.toml` (ex.: `keyring`, clientes OAuth, crates de assinatura/HMAC se não usadas em outro lugar).

### Interface
- [ ] Botões "Configurar API" / "Fechar credencial" e os campos Identificador da aplicação, Secret, Tracking ID, "Salvar no cofre", "Remover".
- [ ] Texto "API opcional com cofre" nos cards de loja.
- [ ] Card "Estimativa de importação" (editor JSON) em Configurações.
- [ ] Colunas frete/imposto/final de importação na tabela (manter só **Preço** e, quando a página mostrar, **Frete**).
- [ ] Ícone/badge "estimativa".

### Banco (migration)
- [ ] `DROP TABLE cotacoes`.
- [ ] Remover de `snapshots`: `moeda_origem`, `preco_origem`, `imposto_brl`, `cotacao`. Manter `preco_brl` e `frete_brl`; `preco_final_brl` = `preco_brl + frete_brl` (quando houver frete), senão `preco_brl`.
- [ ] Remover `produtos.importado`.
- [ ] Apagar do cofre do SO as credenciais salvas pelo app (rotina de limpeza executada uma vez na migration).

### Documentação
- [ ] Remover dos `docs/lojas/*.md` e do README tudo sobre API, credenciais, câmbio e imposto.
- [ ] Registrar em `docs/decisoes.md`: "v4 — coleta somente por WebView; APIs e estimativa de importação removidas".

**Teste da remoção:** `grep -ri "secret\|oauth\|keyring\|ptax\|cotacao\|imposto" src-tauri/src src` → nenhum resultado (exceto este histórico em `docs/decisoes.md`). Banco v3 migrado abre sem erro e o histórico de preços é preservado.

---

## 6. Arquitetura após a correção

```
Busca { termo, lojas: [ml, aliexpress, amazon] }
        │
  OrquestradorMultiLoja (paralelo, timeout por loja 45s, global 90s)
        │
  Por loja:  WebView oculta ──► WebView assistida ──► Cache ──► Manual
                  │
            MotorWebview (genérico)
              1. navegar(url_busca(termo))
              2. aguardar cards (polling 500ms, até 15s)
              3. detectar verificação/login → escalar para assistida
              4. rolar a página até N itens ou 3 rolagens sem item novo
              5. executar extrator JS da loja → JSON
              6. (modo diagnóstico) salvar HTML renderizado
        │
  Normalização → filtros → score → matching → Pareto → ranking único
```

### Contrato do extrator (JS, um por loja)

```js
// retorna sempre este formato
{
  status: "ok" | "verificacao" | "login" | "vazio",
  itens: [{
    id_externo, titulo, url,
    preco_texto,          // "R$ 49,90" (cru, o parse é feito no Rust)
    frete_texto,          // opcional
    nota_texto,           // opcional: "4,5 de 5 estrelas"
    avaliacoes_texto,     // opcional: "(1.234)"
    vendidos_texto,       // opcional: "1.000+ vendidos"
    patrocinado,          // bool
    estrategia            // "jsonld" | "script" | "css"
  }]
}
```

Todo o parse de texto (preço, nota, contagem) é feito em **Rust, com funções puras testadas**, nunca no JS.

---

## 7. Especificação por loja (WebView)

| Loja | URL de busca | Estratégia de extração (ordem) | Atenção |
|---|---|---|---|
| **Mercado Livre** | `https://lista.mercadolivre.com.br/<termo-com-hifens>` | (já existe) JSON-LD → estado embutido → CSS | Remover a dependência da API; o WebView vira a fonte principal |
| **Amazon** | `https://www.amazon.com.br/s?k=<termo+com+mais>` | CSS nos resultados (cards com `data-asin`) → JSON-LD | Ignorar cards sem ASIN; marcar patrocinados; verificação comum (página de "digite os caracteres") → assistida |
| **AliExpress** | `https://pt.aliexpress.com/w/wholesale-<termo-com-hifens>.html` | **Dados embutidos em `<script>`** (JSON com id/título/preço) → CSS por **estrutura** (links `a[href*="/item/"]` + texto com `R$`) | Classes CSS são ofuscadas e mudam, então **não depender de nome de classe**; lista carrega com rolagem (lazy load); pode haver pop-up de idioma/região (ignorar, extrair mesmo assim); ID vem de `/item/(\d+)\.html` |
| **eBay** | `https://www.ebay.com/sch/i.html?_nkw=<termo>` | CSS (`li` de resultado) | Usar só o preço em **R$** se a página exibir; sem R$ → item sem preço (fora do ranking de preço, com aviso) |
| **Shopee** | `https://shopee.com.br/search?keyword=<termo>` | Dados embutidos → CSS | Costuma exigir login → janela assistida para o usuário entrar com a própria conta |
| **KaBuM!** | `https://www.kabum.com.br/busca/<termo-com-hifens>` | JSON-LD/estado embutido → CSS | Preço **à vista** como `preco_brl` |
| **Pichau** | `https://www.pichau.com.br/search?q=<termo>` | Estado embutido → CSS | Preço **à vista** como `preco_brl` |

URLs e seletores devem ser **confirmados no HTML capturado** (Fase C2) e registrados em `docs/lojas/<loja>.md`.

---

## 8. Fases

### Fase C1 — Remoção

**Conceitos:** refatoração segura (remover com testes como rede de proteção), migrations destrutivas com preservação de dados, poda de dependências.

- [ ] Executar tudo da Seção 5.
  **Teste:** o `grep` da Seção 5 vazio; `cargo test` verde; banco v3 com dados semeados → migração preserva produtos e snapshots (contagem igual) e remove as colunas/tabela.
- [ ] Tela Lojas simplificada: por loja, apenas **Ativa**, **Padrão**, status de saúde e dois botões: **"Abrir loja"** (abre a WebView visível na home da loja, para login/verificação manual) e **"Limpar sessão"** (apaga os cookies daquela loja).
  **Teste:** nenhum campo de credencial na UI; "Limpar sessão" remove os cookies do domínio da loja.

### Fase C2 — Motor WebView genérico

**Conceitos:** ciclo de vida de página (load vs conteúdo dinâmico), espera por condição (polling) vs `sleep` fixo, lazy loading/rolagem infinita, isolamento de sessão por domínio, captura de DOM renderizado.

- [ ] `MotorWebview` reutilizável por todas as lojas: navegar, aguardar seletor/condição (polling 500ms, timeout configurável), rolar até N itens ou 3 rolagens sem item novo, executar o extrator, retornar JSON.
  **Teste:** página local de teste (servida pelo próprio app) que insere cards via JS após 2s e mais cards ao rolar → o motor espera, rola e retorna o total correto.
- [ ] Detecção de verificação/login por loja (padrões de texto/URL/seletor configurados no módulo da loja) → `status: "verificacao" | "login"`.
  **Teste:** fixtures `verificacao.html` e `login.html` de cada loja → status correto.
- [ ] Escalonamento para assistida: janela visível, mensagem "Resolva a verificação/entre na sua conta e clique em Continuar", e a coleta segue após o clique.
- [ ] Comando de **captura**: salva o HTML renderizado (após scripts e rolagem) em `pasta_de_dados/capturas/<loja>_<data>.html`.
  **Teste:** captura de uma página local gera um arquivo contendo o conteúdo inserido via JS.

### Fase C3 — Extratores: Mercado Livre, Amazon, AliExpress

**Conceitos:** seletores robustos (estrutura e atributos estáveis vs classes ofuscadas), extração de dados embutidos em `<script>`, IDs extraídos de URL com regex.

- [ ] Capturar o HTML real das 3 lojas para o termo **"capa samsung s25 plus"** e salvar como fixtures em `tests/fixtures/<loja>/busca_capa_s25_plus.html`.
- [ ] Extrator Mercado Livre usando só WebView (sem API).
- [ ] Extrator Amazon (cards com ASIN, título, preço, nota, nº de avaliações, flag patrocinado).
- [ ] Extrator AliExpress (dados embutidos primeiro; fallback estrutural por links `/item/`; ID pela URL; preço em R$; "vendidos").
  **Teste (cada loja):** extrator rodando sobre a fixture real (Vitest + jsdom) → ≥ 10 itens, 100% com `id_externo`, `titulo`, `url` e `preco_texto`.

### Fase C4 — Normalização para buscas reais

**Conceitos:** normalização de símbolos com significado (`+`), sinônimos, desambiguação de variantes de modelo, parsing de números no formato brasileiro.

- [x] **Símbolo `+` em modelos:** antes de remover pontuação, converter `S25+`, `S25 +` e `S25plus` em `s25 plus`. [completion:: 2026-10-12]
  **Teste:** `"Capa Galaxy S25+ Silicone"` → tokens contêm `s25` e `plus`; `"Capinha S25plus"` → `s25`, `plus`.
- [x] **Sinônimos** (arquivo `sinonimos.json` na pasta de dados): `capa = capinha = case = cover = capas`. [completion:: 2026-10-12]
  **Teste:** a busca "capa" pontua no BM25 um título com "Case".
- [x] **Variantes conflitantes:** se a busca contém `plus`, títulos com `ultra`, `edge` ou `fe` **sem** `plus` são removidos. Títulos que listam vários modelos ("S25 / S25+ / S25 Ultra") são aceitos. [completion:: 2026-10-12]
  **Teste:** "Capa S25 Ultra" → removido; "Capa S25" (sem plus) → removido; "Capa S25/S25+/S25 Ultra" → mantido.
- [x] **Parsers BR** (Rust, puros): [completion:: 2026-10-12]
  **Teste:** `"R$ 1.234,56"` → 1234.56; `"R$49,90"` → 49.90; `"R$ 12"` → 12.00; faixa `"R$ 10,50 - R$ 20,00"` → 10.50 com `preco_variavel = true`; `"4,5 de 5 estrelas"` → 4.5; `"(1.234)"` → 1234; `"1.000+ vendidos"` → 1000; `"5 mil vendidos"` → 5000.

### Fase C5 — Modo diagnóstico + teste real

**Conceitos:** testes end-to-end contra sistema real, relatórios reproduzíveis, separação entre teste determinístico (fixtures) e teste de aceitação (site ao vivo).

- [ ] Comando `garimpo-livre.exe --diagnostico "capa samsung s25 plus" --lojas ml,aliexpress,amazon` e botão equivalente em Configurações. Ele roda a busca real, salva as capturas HTML e gera `pasta_de_dados/diagnostico/<data>.md` com, por loja: status, fonte usada, tempo, nº de itens, % com preço, top 10 (título, preço, url) e erros.
  **Teste:** com lojas fake, o relatório é gerado com todas as seções.
- [ ] Executar o teste de aceitação da Seção 9 e anexar o relatório ao commit (`docs/diagnosticos/`).

### Fase C6 — Demais lojas por WebView

Só iniciar após a Fase C5 aprovada.

- [ ] eBay, Shopee, KaBuM!, Pichau: capturar o HTML real com o termo de teste da loja, criar fixtures e extratores conforme a Seção 7.
  **Teste:** ≥ 10 itens por loja na fixture real; Shopee aceita `status: "login"` → fluxo assistido.

---

## 9. Teste de aceitação (obrigatório)

**Busca:** `capa samsung s25 plus` · **Lojas:** Mercado Livre, AliExpress, Amazon · **Modo:** app instalado no Windows, WebView.

| # | Critério | Esperado |
|---|---|---|
| A1 | Itens extraídos por loja | ≥ 10 em cada uma das 3 lojas |
| A2 | Completude | 100% dos itens com título, URL e preço > 0 em R$ |
| A3 | Relevância | ≥ 8 dos 10 primeiros do ranking são **capas** de **S25 Plus/S25+** (conferência manual pelo título) |
| A4 | Variantes erradas | 0 itens de S25 Ultra/Edge/FE (sem Plus) ou S25 base no top 10 |
| A5 | Preço | 5 itens por loja conferidos manualmente na página batem com o app (diferença 0) |
| A6 | Multi-loja | Ranking único com badges das 3 lojas; filtro por loja funciona |
| A7 | Tempo | Busca nas 3 lojas termina em ≤ 90s (sem verificação pendente) |
| A8 | Verificação | Se aparecer captcha/login, a janela assistida abre, e após resolver a coleta conclui sem reiniciar a busca |
| A9 | Falha parcial | Desativando a internet no meio (ou bloqueando uma loja), as outras lojas retornam e o status mostra a falha |
| A10 | Sem credenciais | Nenhuma tela, arquivo ou cofre contém chave, secret ou token |

O relatório do `--diagnostico` + capturas de tela do ranking ficam em `docs/diagnosticos/` como evidência.

## 10. Fora de escopo

- Qualquer API oficial ou de afiliado, OAuth, chaves, secrets ou tokens
- Câmbio e estimativa de imposto de importação
- LLM/IA
- Burlar captcha/anti-bot (proxies, solvers, disfarce de fingerprint)

## 11. Definição de pronto

- Seção 5 removida por completo e verificada.
- Mercado Livre, AliExpress e Amazon coletando só por WebView.
- Teste de aceitação (Seção 9) aprovado com relatório anexado.
- Fase C6 iniciada só após a aprovação.
