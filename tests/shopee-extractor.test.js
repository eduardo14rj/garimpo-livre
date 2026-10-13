import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { JSDOM } from 'jsdom';

const read = (path) => readFileSync(new URL(path, import.meta.url), 'utf8');
const extractor = read('../src-tauri/src/lojas/shopee_extractor.js');
const html = read('../src-tauri/tests/fixtures/shopee/listagem.html');
const fixture = JSON.parse(read('../src-tauri/tests/fixtures/shopee/product_offer.json'));
const run = (body) => {
  const dom = new JSDOM(body, { url:'https://shopee.com.br/search?keyword=ssd', runScripts:'outside-only' });
  try { return JSON.parse(JSON.stringify(dom.window.eval(extractor))); }
  finally { dom.window.close(); }
};

test('Shopee DOM preserva faixa decimal e coincide com a fixture local', () => {
  const result = run(html);
  assert.equal(result.kind, 'list');
  const canonical = (item) => Object.fromEntries(['itemId','shopId','productName','productLink',
    'priceMin','priceMax','ratingStar','sales','shopName'].map((key) => [key, String(item[key])]));
  assert.deepEqual(result.data.data.productOfferV2.nodes.map(canonical),fixture.data.productOfferV2.nodes.map(canonical));
  assert.equal(result.data.data.productOfferV2.nodes[0].priceMin,'289.123456789');
});

test('JSON-LD numérico mantém a precisão original e extrai GTIN e reviews sem usar vendas', () => {
  const result = run(`<script type="application/ld+json">{"@type":"Product","name":"SSD 1TB",
    "url":"https://shopee.com.br/product/2001/1001","gtin13":"0740617329919","brand":{"name":"Kingston"},
    "offers":{"@type":"AggregateOffer","priceCurrency":"BRL","lowPrice":289.123456789012345678,
    "highPrice":329.99,"itemCondition":"https://schema.org/NewCondition"},
    "aggregateRating":{"ratingValue":4.8,"reviewCount":12}}</script>`);
  const product = result.data.data.productOfferV2.nodes[0];
  assert.equal(product.priceMin,'289.123456789012345678');
  assert.equal(product.priceMax,'329.99');
  assert.equal(product.sales,null);
  assert.equal(product._garimpo.qtd_avaliacoes,12);
  assert.equal(product._garimpo.gtin,'0740617329919');
  assert.equal(product._garimpo.condicao,'new');
});

test('frete desconhecido é distinto de zero e selo usa dados explícitos da loja', () => {
  const baseline = run(html).data.data.productOfferV2.nodes[0];
  assert.equal(baseline._garimpo.frete,null);
  assert.equal(baseline._garimpo.nota_loja,null);
  const changed = run(html.replace('<article class=', '<article data-shop-rating="4.5" class=')
    .replace('<span>Loja oficial</span>','<span>Loja oficial</span><span data-sqe="condition">Novo</span><span data-sqe="shipping">Frete grátis</span>'));
  const product = changed.data.data.productOfferV2.nodes[0];
  assert.equal(product._garimpo.frete,'0');
  assert.equal(product._garimpo.nota_loja,4.5);
  assert.equal(product._garimpo.oficial,true);
  assert.equal(product._garimpo.condicao,'new');
});

test('JSON-LD e DOM complementares preservam preço preciso e frete explícito', () => {
  const jsonld = `<script type="application/ld+json">{"@type":"Product","name":"SSD Kingston NV2 1TB NVMe",
    "url":"https://shopee.com.br/product/2001/1001","gtin13":"0740617329919",
    "offers":{"priceCurrency":"BRL","price":289.123456789012345678}}</script>`;
  const result = run(jsonld + html.replace('<span>Loja oficial</span>',
    '<span>Loja oficial</span><span data-sqe="shipping">R$ 19,99</span>'));
  const product = result.data.data.productOfferV2.nodes[0];
  assert.equal(result.data.data.productOfferV2.nodes.length,2);
  assert.equal(product.priceMin,'289.123456789012345678');
  assert.equal(product._garimpo.gtin,'0740617329919');
  assert.equal(product._garimpo.frete,'19.99');
  assert.equal(product.sales,320);
});

test('challenge/login aguardam pessoa; só mensagem explícita confirma vazio', () => {
  assert.deepEqual(run('<h1>Verificação de segurança: CAPTCHA</h1>'),{verification:true});
  assert.deepEqual(run('<input type="password">'),{login_required:true});
  assert.equal(run('<h1>Nenhum resultado encontrado</h1>').data.data.productOfferV2.nodes.length,0);
  assert.equal(run('<h1>Carregando...</h1>'),null);
});

test('links externos e moedas diferentes não viram oferta Shopee BR', () => {
  assert.equal(run(html.replaceAll('https://shopee.com.br/','https://shopee.com.br.evil.invalid/')),null);
  assert.equal(run(html.replaceAll('R$','US $')),null);
});
