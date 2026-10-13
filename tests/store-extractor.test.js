import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { JSDOM } from 'jsdom';

const read = (path) => readFileSync(new URL(path, import.meta.url), 'utf8');
const ebay = read('../src-tauri/src/lojas/ebay_extractor.js');
const transport = read('../src-tauri/src/lojas/webview_transport.js');
const html = read('../src-tauri/tests/fixtures/ebay/listagem.html');
const api = JSON.parse(read('../src-tauri/tests/fixtures/ebay/search.json'));
const url = 'https://www.ebay.com/sch/i.html?_nkw=ssd';
const script = (event, extractor = ebay) => transport
  .replace('__GARIMPO_STORE_EXTRACTOR__', extractor.trim().replace(/;$/, ''))
  .replaceAll('__GARIMPO_STORE_EVENT__', JSON.stringify(event));

function environment(body, event = 'garimpo:store-extraction-fixture', extractor = ebay, prepare = () => {}) {
  const dom = new JSDOM(body, { url, runScripts: 'outside-only' });
  const emitted = [];
  dom.window.scrollTo = () => {};
  dom.window.__TAURI__ = { event: { emit: (name, payload) => {
    emitted.push({ name, payload: JSON.parse(JSON.stringify(payload)) }); return Promise.resolve();
  } } };
  prepare(dom.window);
  dom.window.eval(script(event, extractor));
  return { dom, emitted };
}
const settled = () => new Promise((resolve) => setTimeout(resolve, 1700));
const canonical = (item) => ({
  itemId: item.itemId, title: item.title, itemWebUrl: item.itemWebUrl,
  price: item.price, seller: item.seller, conditionId: item.conditionId,
  shippingCost: item.shippingOptions?.[0]?.shippingCost ?? null,
});

test('extrator eBay entrega os mesmos campos da fixture local sem converter dinheiro para Number', async () => {
  const { dom, emitted } = environment(html);
  try {
    await settled();
    assert.equal(emitted.length, 1);
    assert.equal(emitted[0].payload.kind, 'list');
    assert.equal(emitted[0].payload.url, url);
    assert.deepEqual(emitted[0].payload.data.itemSummaries.map(canonical), api.itemSummaries.map(canonical));
    assert.equal(emitted[0].payload.data.itemSummaries[1].price.value, '25.123456789');
  } finally { dom.window.close(); }
});

test('frete em outra localização fica desconhecido e variação do link é preservada', async () => {
  const altered = html.replace('Ship to: Brazil', 'Ship to: United States')
    .replace(' data-item-id="v1|1001|0"', '').replace('/itm/1001"', '/itm/1001?var=123"');
  const { dom, emitted } = environment(altered);
  try {
    await settled();
    const item = emitted[0].payload.data.itemSummaries[0];
    assert.equal(item.itemId, 'v1|1001|123');
    assert.equal(item.shippingOptions.length, 0);
    assert.match(item.itemWebUrl, /var=123/);
  } finally { dom.window.close(); }
});

test('transporte aguarda renderização da SPA e separa os eventos das lojas', async () => {
  const first = environment('<main>Loading...</main>', 'garimpo:store-extraction-a');
  const second = environment(html, 'garimpo:store-extraction-b');
  try {
    assert.equal(first.emitted.length, 0);
    first.dom.window.document.body.innerHTML = new JSDOM(html).window.document.body.innerHTML;
    await settled();
    assert.equal(first.emitted.length, 1);
    assert.equal(first.emitted[0].name, 'garimpo:store-extraction-a');
    assert.equal(second.emitted[0].name, 'garimpo:store-extraction-b');
    first.dom.window.document.body.appendChild(first.dom.window.document.createElement('aside'));
    await new Promise((resolve) => setTimeout(resolve, 0));
    assert.equal(first.emitted.length, 1);
  } finally { first.dom.window.close(); second.dom.window.close(); }
});

test('verificação pede intervenção e recomeça após a pessoa clicar em Continuar', async () => {
  const { dom, emitted } = environment('<h1>Pardon our interruption. Verify yourself.</h1>');
  try {
    assert.equal(emitted[0].payload.verification, true);
    dom.window.document.body.innerHTML = new JSDOM(html).window.document.body.innerHTML;
    dom.window.__garimpoRunStoreExtraction();
    await settled();
    assert.equal(emitted.length, 2);
    assert.equal(emitted[1].payload.kind, 'list');
  } finally { dom.window.close(); }
});

test('vazio só é confirmado por mensagem explícita, e moeda diferente não vira USD', () => {
  const empty = environment('<h1>No results found</h1>');
  const invalid = environment(html.replaceAll('R$', 'US $'));
  try {
    assert.equal(empty.emitted[0].payload.data.itemSummaries.length, 0);
    assert.equal(invalid.emitted.length, 0);
    invalid.dom.window.__garimpoStopStoreExtraction();
  } finally { empty.dom.window.close(); invalid.dom.window.close(); }
});

test('motor local espera cards dinâmicos e rola até três tentativas sem novos itens', async () => {
  const localExtractor = `(() => {const items=[...document.querySelectorAll('.card')].map((item)=>item.textContent);return items.length?{kind:'list',data:{items}}:null})()`;
  let scrolls = 0;
  const { dom, emitted } = environment('<main id="results"></main>', 'garimpo:store-extraction-local', localExtractor, (window) => {
    const addCards = (start, count) => {
      const root = window.document.getElementById('results');
      for (let index = start; index < start + count; index += 1) {
        const card = window.document.createElement('article');
        card.className = 'card';
        card.textContent = `Oferta ${index + 1}`;
        root.append(card);
      }
    };
    window.scrollTo = () => {
      scrolls += 1;
      if (scrolls === 1) addCards(2, 3);
    };
    window.setTimeout(() => addCards(0, 2), 200);
  });
  try {
    await new Promise((resolve) => setTimeout(resolve, 2600));
    assert.equal(emitted.length, 1);
    assert.equal(emitted[0].payload.data.items.length, 5, JSON.stringify({ scrolls, items: emitted[0].payload.data.items }));
    assert.ok(scrolls >= 4);
  } finally { dom.window.close(); }
});
