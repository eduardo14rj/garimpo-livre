import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { JSDOM } from 'jsdom';

const read = (path) => readFileSync(new URL(path,import.meta.url),'utf8');
const extractor = read('../src-tauri/src/lojas/varejo_extractor.js');
const run = (html,url) => {
  const dom = new JSDOM(html,{url,runScripts:'outside-only'});
  try { return JSON.parse(JSON.stringify(dom.window.eval(extractor))); }
  finally { dom.window.close(); }
};
for (const store of ['kabum','pichau']) {
  const fixture = (path) => read(`../src-tauri/tests/fixtures/${store}/${path}`);
  const expected = JSON.parse(fixture('ofertas.json'));
  const searchUrl = `https://www.${store}.com.br/` + (store === 'kabum' ? 'busca/ssd-1tb' : 'search?q=ssd');
  for (const kind of ['listagem','produto']) {
    for (const strategy of ['jsonld','estado','dom']) {
      test(`${store}: ${kind} somente ${strategy} produz os campos canônicos equivalentes`, () => {
        const result = run(fixture(`${kind}/so_${strategy}.html`),kind === 'produto' ? expected[0].url : searchUrl);
        assert.equal(result.kind,kind === 'produto' ? 'product' : 'list');
        assert.deepEqual(kind === 'produto' ? [result.data] : result.data,kind === 'produto' ? expected.slice(0,1) : expected);
      });
    }
  }
  test(`${store}: preço à vista vence o genérico e mantém total parcelado separado`, () => {
    const html = fixture('listagem/so_jsonld.html').replaceAll('PIX à vista','Preço genérico') + fixture('listagem/so_estado.html');
    const result = run(html,searchUrl).data;
    assert.equal(result[0].preco,expected[0].preco);
    assert.equal(result[0].preco_parcelado,'429.90');
    assert.equal(result[0].marca,'Kingston');
  });
  test(`${store}: produto indisponível não usa preço antigo e detalhe ignora recomendações`, () => {
    const ld = fixture('produto/so_jsonld.html').replaceAll('https://schema.org/InStock','https://schema.org/OutOfStock');
    assert.deepEqual(run(ld,expected[0].url),{error:'Produto indisponível nesta loja.'});
    const detail = run(fixture('listagem/so_jsonld.html'),expected[0].url);
    assert.equal(detail.kind,'product');
    assert.equal(detail.data.id_externo,expected[0].id_externo);
    const list = run(ld,searchUrl);
    assert.deepEqual(list,{kind:'list',data:[]});
  });
  test(`${store}: parcela isolada, host externo e carregamento não viram oferta`, () => {
    const html = `<article class="retail-product-card"><a href="${expected[0].url}"><h2>SSD 1TB</h2></a><span class="product-price">10x de R$ 49,99</span></article>`;
    const result = run(html,searchUrl);
    assert.ok(result === null || result.data?.length === 0);
    assert.equal(run(fixture('listagem/so_dom.html').replaceAll(`https://www.${store}.com.br/`,'https://evil.invalid/'),searchUrl),null);
    assert.equal(run('<h1>Carregando</h1>',searchUrl),null);
    assert.deepEqual(run('<h1>Just a moment CAPTCHA</h1>',searchUrl),{verification:true});
    assert.deepEqual(run('<h1>Nenhum produto encontrado</h1>',searchUrl),{kind:'list',data:[]});
  });
}
test('KaBuM!: ignora preço Prime exclusivo e preserva Decimal numérico do estado', () => {
  const html = `<script id="__NEXT_DATA__" type="application/json">{"props":{"pageProps":{"data":{"catalogServer":{"data":[{
    "code":1001,"name":"SSD 1TB","friendlyName":"ssd-1tb","price":499.99,"priceWithDiscount":389.123456789012345678,
    "prime":{"priceWithDiscount":99.99,"isLoggedUserExclusive":true},"sellerName":"KaBuM!","flags":{"isOpenbox":false},"available":true
  }]}}}}}</script>`;
  const item = run(html,'https://www.kabum.com.br/busca/ssd').data[0];
  assert.equal(item.preco,'389.123456789012345678');
  assert.equal(item.preco_parcelado,'499.99');
});
test('Pichau: rótulos de pagamento e link permitem DOM sem classes fixas', () => {
  const html = `<a href="https://www.pichau.com.br/ssd-1tb"><h2>SSD 1TB</h2><div>de R$ 742.34 por:</div>
    <div>à vista</div><div>R$ 499.99</div><div>no PIX com 15% desconto</div>
    <div>R$ 588.22</div><div>em até 12x de R$ 49.02</div></a>`;
  const item = run(html,'https://www.pichau.com.br/search?q=ssd').data[0];
  assert.equal(item.preco,'499.99');
  assert.equal(item.preco_parcelado,'588.22');
  assert.equal(item.vendedor,'Pichau');
});
