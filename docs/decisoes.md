# Decisões técnicas

## 2026-10-12 — v4: somente WebView

Esta decisão substitui as estratégias de coleta e precificação descritas nas decisões v1–v3: as lojas são lidas por WebView, sem APIs, credenciais, conversão de moeda ou estimativa de importação. Só preço mostrado em reais participa do ranking; frete desconhecido continua nulo. Login e verificações ficam para a pessoa na janela visível. Os testes automáticos usam fixtures e não acessam lojas reais.

A migração v4 preserva produtos, ofertas e histórico útil, remove campos de câmbio/importação e apaga uma vez somente os identificadores exatos de credenciais antigas gravados pelo aplicativo no Windows. O marcador só é gravado após a limpeza terminar; falha de exclusão pode ser tentada novamente na próxima inicialização.

As fixtures atuais são sintéticas. A confirmação dos seletores das lojas depende do HTML renderizado capturado pelo modo diagnóstico e da aceitação manual, conforme `docs/plano-v4.md`.

## 2026-10-12 — eBay e transporte multi-loja

O contrato foi conferido no OpenAPI oficial antes da implementação. O token eBay é de aplicação e fica em memória; segredos e CEP opcional ficam no cofre do Windows. Uma emissão sob mutex atende chamadas concorrentes. O cache de ofertas é invalidado quando a configuração da loja muda, para não reaproveitar frete de outro CEP. Ausência de CEP ou destino Brasil não confirmado deixa o frete desconhecido.

Feedback do vendedor não é quantidade de reviews. A função pura de reputação usa percentual e `max(feedbackScore,0)`, prior 0,5 e peso 50. GTIN/MPN são obtidos no detalhe; listas de catálogo só são utilizadas quando há um único valor. Condições de caixa aberta, defeitos e peças não entram como novo.

O transporte WebView comum recebe extrator/URLs/parser da loja e cria eventos e rótulos próprios por tentativa. Não compartilha o canal do coletor legado ML. O guard remove listeners e janelas no cancelamento. Fontes com paginação controlam seu rate limit entre pedidos; o orquestrador não adiciona outra espera. O deadline de 60 s é definido antes da consulta PTAX. Fixture usa câmbio sintético somente no modo de demonstração e não chama serviços externos.

## 2026-10-12 — Clique de acesso na verificação da conta

O diagnóstico fornecido pelo Eduardo registra um clique em um link habilitado na página `/gz/account-verification`, com destino `https://www.mercadolivre.com/jms/mlb/lgz/login`. Esse JSON é telemetria do clique e não um erro retornado pelo site. O domínio do destino já estava permitido. O fluxo anterior dependia de uma chamada IPC feita pela página externa; agora o script cancela o handler hidratado e navega diretamente para o destino validado no mesmo WebView, mantendo a sessão. A validação local cobre o clique e o endereço; a conclusão do login no site ainda depende de observação real.

O runner `npm run tauri dev` também procura outra porta quando 1420 está ocupada e usa um identificador de desenvolvimento separado nessa situação, para manter os dados de teste separados do perfil normal.

Foi adicionado um script de inicialização nos coletores principal e popup. Ele captura somente o clique de acesso dessa página, preserva o endereço completo do link e solicita a navegação nativa na mesma janela/perfil, sem depender dos handlers de clique do site. O Rust confere novamente a página de origem e o destino. Não há preenchimento de credenciais, resolução de CAPTCHA ou avanço automático. O listener usa um evento próprio por janela e é removido quando ela é destruída. As permissões IPC existentes não foram ampliadas.

Os testes sintéticos cobrem o clique mesmo quando um handler da página cancela a ação padrão, a ausência de navegação automática, o comportamento sem IPC, links desabilitados, eventos distintos por janela e a rejeição de destinos fora do fluxo. Passaram 66 testes Rust, 18 testes JavaScript, Clippy sem warnings e formatação.

Na primeira tentativa real, o circuito da fonte WebView estava aberto e a coleta usou o cache. Foi liberada uma sondagem da fonte, preservando a linha anterior em um arquivo temporário. Após a recompilação, o reteste pela interface concluiu com fonte WebView, 48 anúncios analisados e 13 produtos no ranking às 14:57. A tela de login não apareceu nessa execução; esse resultado confirma a coleta real, mas não comprova o clique de acesso ao vivo. O login continua dependendo de ação manual do Eduardo.

## 2026-10-12 — Contratos v3 e compatibilidade com v1/v2

O plano completo está em `docs/plano-v3.md`. Nenhum commit será feito, conforme instrução direta do Eduardo, que prevalece sobre a sugestão de commits por fase no plano.

As fontes antigas trabalham com `AnuncioBruto` e preços `f64`. Para preservar seus parsers e testes, a v3 usa `FonteLoja`/`OfertaBruta` com `Decimal` e um adaptador explícito somente para o Mercado Livre. `Loja` continua independente do core. O registro recebe `Box<dyn Loja>` e guarda `Arc<dyn Loja>`, permitindo compartilhar cada implementação com tarefas de `JoinSet` sem copiar credenciais ou bloquear outras lojas.

O limite de preço das buscas passou a ser decimal. JSONs antigos com valores numéricos continuam aceitos; novos JSONs preservam o valor como string. A interface converte para número somente na formatação visual.

## 2026-10-12 — Migração e isolamento das fontes

Uma transação converte IDs e referências para `ml:<id>`, cria as sete configurações de loja e os campos monetários de texto. As constraints ficam diferidas somente durante a transação, e colisões fazem rollback integral. O upsert de produtos usa `ON CONFLICT DO UPDATE`, evitando o delete implícito de `INSERT OR REPLACE` que apagaria snapshots anteriores por cascade.

Circuitos e saúde identificam o par loja/fonte pela chave canônica `ml:API`, `ebay:API` etc. As linhas antigas recebem prefixo `ml:`; os métodos legados continuam aceitando apenas `API`. O estado público separa `loja` e `fonte`. Limitadores usam tupla `(LojaId, String)`; caches usam diretório da loja e SHA-256 de fonte + termo. Timeout/cancelamento registra a falha e libera a sondagem de circuito; o coletor remove os listeners e fecha suas janelas quando seu future é cancelado.

## 2026-10-12 — Custo estimado e dados ausentes

As regras iniciais reproduzem os exemplos do PRD e continuam sujeitas à conferência manual. O cálculo usa operações decimais verificadas contra overflow e não arredonda bases intermediárias. Valores finais são apresentados em centavos. Quando `base_inclui_frete=false`, o frete é excluído da tributação, mas continua somado ao custo final, conforme o requisito de incluir o que o comprador paga.

Frete desconhecido permanece `null`, sem ser rotulado como grátis. Importados ofertados em BRL são convertidos a USD apenas para escolher a faixa e aplicar a dedução, preservando os preços originais. Sem cotação real ou salva não se calcula imposto fictício.

## 2026-10-11 — Criar o esqueleto Tauri 2 no checkout existente

O repositório tinha apenas o README. Foi usado o template oficial React + TypeScript e ele foi copiado para a raiz, preservando o README original. O app mantém o backend em `src-tauri/`, a interface em `src/`, os dados em `app_data_dir()` e fixtures versionadas em `src-tauri/tests/fixtures/`.

## 2026-10-11 — Ícones emoji

As instruções deste checkout pedem seguir a regra dos ícones emoji. A interface usa emojis nos rótulos e estados em vez de adicionar uma biblioteca de ícones ou desenhar glifos próprios.

## 2026-10-11 — WebView remoto com escopo limitado

O WebView coletor só navega para os domínios do Mercado Livre declarados na configuração. O acesso IPC remoto é concedido apenas ao WebView `collector` e ao plugin de eventos, para que o script envie o resultado ao Rust. O conteúdo remoto não recebe permissões de filesystem nem comandos de negócio.

## 2026-10-11 — Rate limit e relógio injetável

O intervalo de dois segundos é imposto na camada de coleta antes de cada navegação. Um relógio abstrato permite testar retry e rate limit sem esperas reais.

## 2026-10-11 — Estado da validação

Os testes automáticos usam somente fixtures, sem acessar a internet. A coleta real é conferida separadamente pela interface; a notificação no Windows continua pendente de validação manual.

## 2026-10-11 — Separar demonstração da coleta real

Novas instalações usam WebView. O resultado registra a fonte para evitar a exibição de demonstrações antigas como ofertas reais. O exemplo de fones permanece identificado como demonstração, com links de compra desabilitados. Novas buscas têm filtros neutros. A compatibilidade verifica modelo, variante e o tipo de acessório; o modelo do celular não identifica uma capa para deduplicação.

## 2026-10-11 — Extração e verificação de segurança

O coletor lê preço atual e centavos, preserva o endereço do anúncio e distingue quantidade vendida de quantidade de avaliações. O JSON-LD do detalhe tem prioridade sobre cards de recomendação e usa a variante atual do ProductGroup. A ausência de cards aguarda renderização; somente uma mensagem explícita do site confirma ausência de resultados. A verificação mantém a janela visível e aguarda a intervenção manual, sem timeout ou reinício automático. Fechar o coletor cancela a tentativa.

Na listagem, o JSON-LD complementa o título com a opção selecionada (por exemplo, um título genérico S25/Plus/Ultra pode apontar para S25 FE). O filtro considera o último modelo informado e exige a variante adjacente, evitando combinar o `Plus` de um S23 com o `S25 Ultra` citado em outro trecho.

## 2026-10-11 — Validação da correção pela interface

Executada no app a busca `capa de celular samsung s25 plus`, após Eduardo concluir a verificação do Mercado Livre. Foram coletados 56 anúncios e exibidos 16 produtos no ranking. O botão de abrir a primeira oferta levou a uma página existente, com `Transparente e Película | S25 PLUS` selecionado e preço de R$ 19,01, igual ao coletado. O perfil, avatar e aviso redundante de armazenamento local foram removidos. Os ícones continuam em emoji.

A busca anterior `Capa celular samsung s25 plus`, mantendo preço máximo de R$ 50 e mínimo de 4 avaliações, também foi executada: 3 ofertas passaram pelos filtros, com 2.493, 32 e 1.317 avaliações obtidas nos detalhes dos produtos. Passaram os 28 testes de Rust, 10 testes do extrator, build do frontend, Clippy sem warnings e verificação de formatação. Nenhum commit foi feito.
# Shopee — contrato e dados ausentes (2026-10-12)

- O portal oficial BR confirma SHA-256 de AppId + timestamp + corpo JSON exato + Secret. O cliente serializa uma vez e usa variáveis GraphQL para o termo; assinatura e POST recebem os mesmos bytes. Não usar HMAC ou interpolar o termo dentro da query.
- `ratingStar` mede o produto. `productOfferV2` não fornece rating da loja nem review count: não converter vendas em quantidade de reviews, nem selo oficial em nota inventada. Vendas ficam em coluna própria para a política de confiança do ranking da Fase 21.
- Eduardo escolheu bônus +0,05 para oficial e +0,025 para preferida quando a nota da loja estiver disponível. Aplica somente o maior bônus, com teto 1,0; sem nota, o resultado é desconhecido. Cálculo puro e testado no core.
- Preço mínimo/máximo entra em Decimal, usando mínimo com `preco_variavel`. Frete, condição e origem não documentados ficam desconhecidos no contrato; API BR usa BRL sem presumir imposto de importação. A UI deve avisar que a API não identifica origem/frete/condição. WebView complementa esses campos quando explícitos, inclusive importação, e usa PTAX para calcular a faixa de imposto.
- JSON-LD é lido preservando a representação original dos números monetários; sem suporte do engine ao contexto de JSON.parse, não aceitar um valor numérico já arredondado. DOM usa strings. JSON-LD e card se complementam sem substituir o preço preciso do dado estruturado.
- WebView Shopee é fallback degradado; coleta nova pode gerar snapshot mesmo degradada, enquanto cache com data não é regravado como preço observado hoje. Os testes não acessam sites reais nem o perfil do usuário.

# AliExpress — TOP, BRL e reputação (2026-10-12)

- Guia atual 634, exemplo 633 e SDK oficial concordam: o método Affiliate é TOP; ordena nomes/valores incluindo `method` e assina HMAC-SHA256 sem prefixo de caminho. O cliente usa milissegundos Unix como o SDK Java atual; exemplos antigos de GOP/data textual não são misturados.
- `target_currency=BRL`, `ship_to_country=BR` e `target_language=PT` constam do contrato oficial. BRL não recebe câmbio novamente; para importação, a conversão inversa serve somente à faixa/dedução em USD do JSON fiscal.
- `evaluate_rate` é documentado como avaliação do produto. Sem percentual positivo explícito da loja, reputação permanece desconhecida; vendas dos últimos trinta dias não viram reviews.
- API pode incluir efeitos da política tributária do destino sem discriminar todos os tributos. A UI deve avisar que o cálculo local é estimativa e pode sobrepor valores já incluídos. Frete não informado permanece desconhecido.
- Testes cobrem assinatura oficial, HTTP local, precisão e equivalência API/DOM. A busca real exige cadastro e segue manual.

# Amazon — API condicional e assistência (2026-10-12)

- A documentação atual substitui PA-API por Creators API OAuth. Credential Version define o endpoint do token; marketplace e Partner Tag definem o destino BR. Essas informações ficam no cofre junto ao Secret, com lista explícita de opções públicas.
- Sem identificador/segredo/Partner Tag, a fonte API é pulada; fallback continua e o status informa `SemCredencial`. Com aprovação, o cliente já implementa token, cache, renovação única após 401 e SearchItems.
- A coleta assistida Amazon inicia com janela própria visível. Não automatiza login/verificação. Os testes ficam no projeto, com HTML e HTTP locais.
- Reputação 1 exige venda e envio Amazon explícitos; Prime, FBA de terceiros e nome parecido não bastam. OffersV2 informa vendedor, mas não fulfillment/frete, portanto a API não inventa esses dados.
- Ao complementar JSON-LD e DOM do mesmo ASIN, vendedores diferentes não são mesclados. Preços Prime exclusivos, assinatura recorrente, indisponíveis ou MAP não representam ofertas gerais comparáveis.

# KaBuM!/Pichau — reconciliação e pagamento (2026-10-12)

- As três estratégias são complementares. Preço à vista explícito supera preço genérico; total parcelado permanece separado. Parcela isolada, preço antigo e preço Prime exclusivo não entram no ranking geral.
- URL canônica identifica o item em listagem e detalhe. GTIN exige campo explícito; SKU/slug/código interno não o substituem. Fabricante no estado pode corrigir a marca editorial do JSON-LD.
- Venda direta confirmada dá reputação 1; terceiros e vendedor desconhecido permanecem sem dado. Frete desconhecido permanece nulo, e estoque explicitamente indisponível remove o item.
- A inspeção confirmou HTML/estado público da KaBuM!. Pichau permitiu leitura textual pública, mas retornou 403 no HTML bruto. Fixtures comprovam as regras de extração, sem alegar compatibilidade ao vivo; validação manual permanece aberta.

# Ranking combinado e confiança (2026-10-12)

- GTIN tem prioridade; conflitos explícitos de catálogo/capacidade/voltagem ficam separados para evitar comparar variantes. Modelo exige marca conhecida igual. Um candidato precisa ser compatível com todos os membros do grupo, impedindo pontes transitivas.
- Preço máximo, agrupamento, Pareto, outliers e ordenação usam o preço final Decimal. No score, converte-se somente a razão 0–1 do preço. Reputação 0–1 entra diretamente, com 0,5 neutro quando desconhecida; nota ausente usa a média observada para a regularização, continuando ausente na oferta.
- Para Shopee/AliExpress sem review count, `v = min(vendas, maior contagem de avaliações conhecida no corpus filtrado)`. É uma interpretação conservadora do teto pedido: sem contagem conhecida, v=0. Com reviews próprios, usa a contagem real. A UI identifica confiança por vendas; o filtro `min_avaliacoes` sempre exige reviews reais.
- Histórico do grupo usa mínimo por data entre seus vínculos persistidos; mantém 30 dias e não grava cache como observação atual. Alerta exige sete datas observadas nos últimos 30 dias e snapshot hoje. Registro por entidade/tipo/data evita repetição; demonstração não grava snapshots reais.

# Interface, fallback manual e execução isolada (2026-10-12)

- `MultiApp` é a entrada ativa. Eventos carregam a busca e a interface rejeita atualizações de outra busca; subscriptions são removidas ao desmontar. Grupos filtrados escolhem a oferta de menor preço final entre as lojas visíveis.
- Editor mantém expressões em texto bruto até salvar, permitindo espaços. Dinheiro permanece string na ponte; BigInt faz comparação e formatação monetária na UI. Parcelamento conserva a moeda de origem.
- Entrada manual exige HTTPS da loja, valida valores e catálogo, e só aceita loja ativa e selecionada. Preço observado agora pode gerar snapshot; fallback posterior conserva a data. Quando um lote contém ofertas antigas, sua data é a mais conservadora. Headless mantém cache e Manual sem abrir janelas.
- Fixtures usam o mesmo fluxo compartilhado no desktop e headless; não usam nem escrevem cache real de loja ou snapshots reais. `npm run test:multi` valida o backend com banco temporário e a UI com uma API fake. Não inicia Tauri, navegador ou login.
- Novas informações de catálogo podem dividir grupos. A persistência não reutiliza um grupo incompatível nem atribui o mesmo grupo a dois conjuntos; chaves antigas ocupadas recebem uma revisão, e vínculos de cada oferta acompanham o grupo corrigido. Teste cobre repetição e histórico sem mistura.
- Mercado Livre: quando `seller_reputation.level_id` está disponível, a v3 mapeia os cinco níveis para 0/0,25/0,5/0,75/1. O score legado fica intacto para v1/v2 e é o fallback quando não há nível. Fontes oficiais e contrato em `lojas/ml.md`.
- Amazon: mantém a oferta geral mais barata por condição, para que uma usada não esconda a nova antes do filtro. Novas conservam ID ASIN; outras condições têm sufixo estável. Nenhum campo desconhecido vira frete grátis ou reputação positiva.
