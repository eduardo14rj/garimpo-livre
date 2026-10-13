import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { JSDOM } from 'jsdom';

const read = (path) => readFileSync(new URL(path,import.meta.url),'utf8');
const extractor = read('../src-tauri/src/lojas/amazon_extractor.js');
const html = read('../src-tauri/tests/fixtures/amazon/listagem.html');
const run = (body) => {
  const dom = new JSDOM(body,{url:'https://www.amazon.com.br/s?k=ssd',runScripts:'outside-only'});
  try { return JSON.parse(JSON.stringify(dom.window.eval(extractor))); }
  finally { dom.window.close(); }
};
test('Amazon DOM lê preços precisos, reviews e venda/envio sem confundir FBA de terceiro', () => {
  const items = run(html).data.searchResult.items;
  assert.equal(items.length,2);
  assert.equal(items[0].offersV2.listings[0].price.money.amount,'389.123456789');
  assert.equal(items[0]._garimpo.nota,4.8);
  assert.equal(items[0]._garimpo.qtd_avaliacoes,120);
  assert.equal(items[0]._garimpo.vendido_amazon,true);
  assert.equal(items[0]._garimpo.enviado_amazon,true);
  assert.equal(items[1]._garimpo.vendido_amazon,false);
  assert.equal(items[1]._garimpo.enviado_amazon,true);
  assert.equal(items[0]._garimpo.frete,'0');
  assert.equal(items[1]._garimpo.frete,null);
});
test('JSON-LD mantém precisão, catálogo e nota sem inferir envio', () => {
  const result = run(`<script type="application/ld+json">{"@type":"Product","name":"SSD 1TB",
    "url":"https://www.amazon.com.br/dp/B0B25LZGGW","gtin13":"0740617329919","brand":"Kingston","model":"NV2",
    "offers":{"priceCurrency":"BRL","price":389.123456789012345678,"seller":{"name":"Amazon.com.br"}},
    "aggregateRating":{"ratingValue":4.8,"reviewCount":12}}</script>`);
  const item = result.data.searchResult.items[0];
  assert.equal(item.offersV2.listings[0].price.money.amount,'389.123456789012345678');
  assert.equal(item.itemInfo.externalIds.eans.displayValues[0],'0740617329919');
  assert.equal(item._garimpo.vendido_amazon,true);
  assert.equal(item._garimpo.enviado_amazon,false);
  assert.equal(item._garimpo.qtd_avaliacoes,12);
});
test('JSON-LD e card complementares preservam dinheiro e campos de catálogo', () => {
  const ld = `<script type="application/ld+json">{"@type":"Product","name":"SSD 1TB",
    "url":"https://www.amazon.com.br/dp/B0B25LZGGW","gtin13":"0740617329919",
    "offers":{"priceCurrency":"BRL","price":389.123456789012345678}}</script>`;
  const items = run(ld + html).data.searchResult.items;
  assert.equal(items.length,2);
  assert.equal(items[0].offersV2.listings[0].price.money.amount,'389.123456789012345678');
  assert.equal(items[0]._garimpo.frete,'0');
  assert.equal(items[0]._garimpo.enviado_amazon,true);
});
test('não confunde nome parecido com Amazon nem combina vendedores diferentes do mesmo ASIN', () => {
  const similar = run(html.replaceAll('Amazon.com.br','Amazon Marketplace Fixture')).data.searchResult.items;
  assert.equal(similar[0]._garimpo.vendido_amazon,false);
  const ld = `<script type="application/ld+json">{"@type":"Product","name":"SSD 1TB",
    "url":"https://www.amazon.com.br/dp/B0B25LZGGW",
    "offers":{"priceCurrency":"BRL","price":"399.90","seller":{"name":"Terceiro Fixture"}}}</script>`;
  const combined = run(ld + html).data.searchResult.items[0];
  assert.equal(combined.offersV2.listings[0].merchantInfo.name,'Terceiro Fixture');
  assert.equal(combined.offersV2.listings[0].price.money.amount,'399.90');
  assert.equal(combined._garimpo.vendido_amazon,false);
  assert.equal(combined._garimpo.enviado_amazon,false);
});
test('preço Prime exclusivo e assinatura recorrente não entram como preço geral', () => {
  const result = run(html.replace('<span data-sqe="price">','<span>Exclusivo Prime</span><span data-sqe="price">'));
  assert.equal(result.data.searchResult.items.length,1);
  const subscription = run(html.replace('<span data-sqe="price">','<span>Programe e poupe</span><span data-sqe="price">'));
  assert.equal(subscription.data.searchResult.items.length,1);
});
test('moeda estrangeira, host enganoso e loading não viram oferta BRL', () => {
  assert.equal(run(html.replaceAll('R$','US $')),null);
  assert.equal(run(html.replaceAll('https://www.amazon.com.br/','https://amazon.com.br.evil.invalid/')),null);
  assert.equal(run('<h1>Carregando</h1>'),null);
});
test('verificação e login pedem pessoa; mensagem explícita confirma vazio', () => {
  assert.deepEqual(run('<h1>Digite os caracteres CAPTCHA</h1>'),{verification:true});
  assert.deepEqual(run('<input type="password">'),{login_required:true});
  assert.equal(run('<h1>Nenhum resultado para a busca</h1>').data.searchResult.items.length,0);
});
