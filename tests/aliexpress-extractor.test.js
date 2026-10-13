import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { JSDOM } from 'jsdom';

const read = (path) => readFileSync(new URL(path, import.meta.url), 'utf8');
const extractor = read('../src-tauri/src/lojas/aliexpress_extractor.js');
const html = read('../src-tauri/tests/fixtures/aliexpress/listagem.html');
const fixture = JSON.parse(read('../src-tauri/tests/fixtures/aliexpress/query.json'));
const products = (result) => result.data.aliexpress_affiliate_product_query_response.resp_result.result.products.product;
const run = (body) => {
  const dom = new JSDOM(body,{url:'https://www.aliexpress.com/w/wholesale-ssd.html',runScripts:'outside-only'});
  try { return JSON.parse(JSON.stringify(dom.window.eval(extractor))); }
  finally { dom.window.close(); }
};

test('AliExpress extrai somente itens cujo preço exibido está em reais', () => {
  const api = fixture.aliexpress_affiliate_product_query_response.resp_result.result.products.product;
  const canonical = (item) => ({ id:String(item.product_id),title:item.product_title,url:item.product_detail_url,
    price:item.target_sale_price ?? item.sale_price,currency:item.target_sale_price_currency ?? item.sale_price_currency,
    sales:item.lastest_volume,shop:item.shop_name });
  const actual = products(run(html)).map(canonical);
  // Comparação textual decimal sem converter dinheiro para Number.
  assert.equal(actual[0].price,'200.00');
  assert.deepEqual(actual,api.filter(item => (item.target_sale_price_currency ?? item.sale_price_currency)==='BRL').map(canonical));
});

test('JSON-LD ignora preço em USD sem conversão', () => {
  const result = run(`<script type="application/ld+json">{"@type":"Product","name":"SSD 1TB",
    "url":"https://www.aliexpress.com/item/1005001001.html","gtin13":"0740617329919","brand":{"name":"Kingston"},
    "model":"NV2","offers":{"priceCurrency":"USD","price":25.123456789012345678,
    "itemCondition":"https://schema.org/NewCondition"},"aggregateRating":{"ratingValue":4.8,"reviewCount":12}}</script>`);
  assert.equal(result,null);
});

test('frete desconhecido difere de zero e reputação exige percentual da loja', () => {
  const baseline = products(run(html))[0];
  assert.equal(baseline._garimpo.frete,null);
  assert.equal(baseline._garimpo.percentual_positivo_loja,null);
  const altered = html.replace('<p data-sqe="price">','<span data-sqe="shipping">Frete grátis</span>'
    + '<span data-sqe="shop-positive">98,5% positivas</span><span data-sqe="rating">4,7</span>'
    + '<span data-sqe="review-count">12 avaliações</span><p data-sqe="price">');
  const product = products(run(altered))[0];
  assert.equal(product._garimpo.frete,'0');
  assert.equal(product._garimpo.frete_moeda,'BRL');
  assert.equal(product._garimpo.percentual_positivo_loja,98.5);
  assert.equal(product._garimpo.nota_produto,4.7);
  assert.equal(product._garimpo.qtd_avaliacoes,12);
});

test('JSON-LD e DOM complementares mantêm preço preciso e acrescentam frete', () => {
  const ld = `<script type="application/ld+json">{"@type":"Product","name":"SSD 1TB",
    "url":"https://www.aliexpress.com/item/1005001001.html","gtin13":"0740617329919",
    "offers":{"priceCurrency":"BRL","price":200.123456789012345678}}</script>`;
  const result = products(run(ld + html.replace('<p data-sqe="price">','<span data-sqe="shipping">R$ 19,99</span><p data-sqe="price">')));
  assert.equal(result.length,1);
  assert.equal(result[0].target_sale_price,'200.123456789012345678');
  assert.equal(result[0]._garimpo.gtin,'0740617329919');
  assert.equal(result[0]._garimpo.frete,'19.99');
  assert.equal(result[0].lastest_volume,300);
});

test('verificação/login aguardam pessoa, vazio só com mensagem explícita', () => {
  assert.deepEqual(run('<h1>Slide to verify CAPTCHA</h1>'),{verification:true});
  assert.deepEqual(run('<input type="password">'),{login_required:true});
  assert.equal(products(run('<h1>No results found</h1>')).length,0);
  assert.equal(run('<h1>Carregando...</h1>'),null);
});

test('símbolo $ ambíguo, moedas não suportadas e domínios externos são recusados', () => {
  assert.equal(run(html.replaceAll('R$','$').replaceAll('US $','$')),null);
  assert.equal(run(html.replaceAll('https://www.aliexpress.com/','https://aliexpress.com.evil.invalid/')),null);
  assert.equal(run('<script type="application/ld+json">{"@type":"Product","name":"SSD","url":"https://www.aliexpress.com/item/1.html","offers":{"priceCurrency":"EUR","price":"10"}}</script>'),null);
});
