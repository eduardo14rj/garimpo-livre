import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { JSDOM } from "jsdom";

const read = (path) => readFileSync(new URL(path, import.meta.url), "utf8");
const extractor = read("../src-tauri/src/coleta/extractor.js");
const listFixture = read("../src-tauri/tests/fixtures/listagem.html");
const productFixture = read("../src-tauri/tests/fixtures/produto.html");
const verificationFixture = read("../src-tauri/tests/fixtures/verificacao.html");
const jsonLdListFixture = read("../src-tauri/tests/fixtures/listagem_so_jsonld.html");
const stateListFixture = read("../src-tauri/tests/fixtures/listagem_so_estado.html");
const domListFixture = read("../src-tauri/tests/fixtures/listagem_so_dom.html");

function extract(html, url) {
  let emitted;
  const dom = new JSDOM(html, { url, runScripts: "dangerously" });
  dom.window.__TAURI__ = { event: { emit: (_name, payload) => { emitted = payload; return Promise.resolve(); } } };
  dom.window.eval(extractor);
  dom.window.__garimpoStopExtraction?.();
  dom.window.close();
  return emitted;
}

test("extrai os mesmos campos canônicos da fixture de listagem", () => {
  const payload = extract(listFixture, "https://lista.mercadolivre.com.br/fone");
  assert.equal(payload.kind, "list");
  assert.equal(payload.anuncios.length, 15);
  assert.deepEqual(JSON.parse(JSON.stringify(payload.anuncios[0])), {
    id: "MLB1001",
    titulo: "Fone JBL Tune 520BT ANC preto",
    preco: 249,
    preco_original: null,
    url: "https://produto.mercadolivre.com.br/MLB-1001",
    condicao: "new",
    vendedor: "Loja oficial JBL",
    vendedor_id: null,
    vendedor_score: 0.93,
    nota: 4.8,
    qtd_avaliacoes: 1250,
    modelo: "jbl tune 520bt",
  });
});

test("lê Product JSON-LD sem visitar o site", () => {
  const payload = extract(productFixture, "https://produto.mercadolivre.com.br/MLB-9001");
  assert.equal(payload.kind, "product");
  assert.equal(payload.produto.id, "MLB9001");
  assert.equal(payload.produto.preco, 249.9);
  assert.equal(payload.produto.nota, 4.8);
  assert.equal(payload.produto.qtd_avaliacoes, 1250);
});

test("ML preserva GTIN, marca, modelo e frete textual para matching multi-loja",()=>{
  const html='<script type="application/ld+json">{"@type":"Product","sku":"MLB123","name":"SSD Kingston NV2 1TB","url":"https://produto.mercadolivre.com.br/MLB-123","gtin13":"0740617329919","brand":{"name":"Kingston"},"model":"SNV2S/1000G","offers":{"price":"389.90","shippingDetails":{"shippingRate":{"currency":"BRL","value":10.123456789012345678}}}}</script>';
  const payload=extract(html,"https://lista.mercadolivre.com.br/ssd");const o=payload.anuncios[0];assert.equal(o.gtin,"0740617329919");assert.equal(o.marca,"Kingston");assert.equal(o.modelo,"SNV2S/1000G");assert.equal(o.frete,"10.123456789012345678");
});

test("sinaliza a tela sintética de verificação", () => {
  const payload = extract(verificationFixture, "https://www.mercadolivre.com.br/");
  assert.equal(payload.kind, "list");
  assert.equal(payload.verification, true);
});

test("lê preço atual com centavos e mantém o link real sem confundir vendas com avaliações", () => {
  const link = "https://www.mercadolivre.com.br/capa-s25-plus/p/MLB12345678?pdp_filters=item_id%3AMLB9876543210";
  const payload = extract(`<div class="poly-card"><h3><a class="poly-component__title" href="${link}">Capinha Galaxy S25 Plus</a></h3>
    <span class="poly-component__review-compacted"><span class="polylabel-label">4.7</span><span>+25 vendidos</span></span>
    <span class="poly-component__seller">Loja Capas</span>
    <s class="andes-money-amount--previous"><span class="andes-money-amount__fraction">69</span><span class="andes-money-amount__cents">40</span></s>
    <div class="poly-price__current"><span class="andes-money-amount"><span class="andes-money-amount__fraction">48</span><span class="andes-money-amount__cents">58</span></span><span>30% OFF</span></div>
    <div>2x R$ 24,29</div></div>`, "https://lista.mercadolivre.com.br/capa-s25-plus");
  assert.equal(payload.anuncios.length, 1);
  assert.equal(payload.anuncios[0].preco, 48.58);
  assert.equal(payload.anuncios[0].preco_original, 69.4);
  assert.equal(payload.anuncios[0].url, link);
  assert.equal(payload.anuncios[0].id, "MLB9876543210");
  assert.equal(payload.anuncios[0].nota, 4.7);
  assert.equal(payload.anuncios[0].qtd_avaliacoes, 0);
});

test("página inexistente gera erro em vez de coleta vazia bem-sucedida", () => {
  const payload = extract("<h1>Parece que esta página não existe</h1>", "https://lista.mercadolivre.com.br/erro");
  assert.equal(payload.kind, "error");
  assert.match(payload.error, /não carregou/);
});

test("seleciona a variante da página e herda avaliações do ProductGroup", () => {
  const schema = {'@type':'ProductGroup', aggregateRating:{ratingValue:4.7,reviewCount:148},hasVariant:[
    {'@type':'Product',name:'Capa S25 Ultra',url:'https://www.mercadolivre.com.br/capa-ultra/p/MLB22222',offers:{price:90}},
    {'@type':'Product',name:'Capa S25 Plus',url:'https://www.mercadolivre.com.br/capa-plus/p/MLB11111',offers:{price:20.99}}
  ]};
  const payload = extract(`<h1>Capa S25 Plus</h1><script type="application/ld+json">${JSON.stringify(schema)}</script>`, 'https://www.mercadolivre.com.br/capa-plus/p/MLB11111');
  assert.equal(payload.produto.titulo,'Capa S25 Plus');
  assert.equal(payload.produto.preco,20.99);
  assert.equal(payload.produto.qtd_avaliacoes,148);
});

test("aguarda os cards renderizados depois do carregamento", async () => {
  const dom = new JSDOM("<main>Carregando...</main>", {url:"https://lista.mercadolivre.com.br/capa",runScripts:"outside-only"});
  try {
    const emitted = new Promise(resolve => {
      dom.window.__TAURI__ = {event:{emit:(_name,payload)=>{resolve(payload);return Promise.resolve();}}};
    });
    dom.window.eval(extractor);
    dom.window.document.querySelector('main').innerHTML = `<div class="poly-card" data-price="29.9"><a href="https://produto.mercadolivre.com.br/MLB-9876543210">Capa Samsung S25 Plus</a></div>`;
    const payload = await emitted;
    assert.equal(payload.anuncios[0].titulo, "Capa Samsung S25 Plus");
  } finally { dom.window.close(); }
});

test("o detalhe mantém o produto atual mesmo quando há cards de recomendação", () => {
  const html = `${productFixture}<div class="poly-card" data-price="100"><a href="https://www.mercadolivre.com.br/outro/p/MLB11111">Outro produto recomendado</a></div>`;
  const payload = extract(html, "https://produto.mercadolivre.com.br/MLB-9001");
  assert.equal(payload.kind, "product");
  assert.equal(payload.produto.id, "MLB9001");
  assert.equal(payload.produto.preco, 249.9);
});

test("JSON-LD com vários produtos não transforma a listagem em um detalhe", () => {
  const schema = {'@graph':[{'@type':'Product',name:'Capinha S25 Plus',offers:{price:29.9}}]};
  const html = `<script type="application/ld+json">${JSON.stringify(schema)}</script><div class="poly-card" data-price="29.9"><a href="https://www.mercadolivre.com.br/capa/p/MLB98765432">Capinha S25 Plus</a></div>`;
  const payload = extract(html, "https://lista.mercadolivre.com.br/capa-s25-plus");
  assert.equal(payload.kind, "list");
  assert.equal(payload.anuncios.length, 1);
});

test("a listagem preserva a variante selecionada do JSON-LD no título e o link do card", () => {
  const url = 'https://www.mercadolivre.com.br/capa/up/MLBU3546487622';
  const schema = {'@graph':[{'@type':'Product',name:'Capa S25/S25 Plus/S25 Ultra Transparente S25 Fe',offers:{price:19,url}}]};
  const link = `${url}?wid=MLB6471402578#search`;
  const payload = extract(`<script type="application/ld+json">${JSON.stringify(schema)}</script><div class="poly-card" data-price="19"><a href="${link}">Capa S25/S25 Plus/S25 Ultra</a></div>`, 'https://lista.mercadolivre.com.br/capa-s25-plus');
  assert.equal(payload.anuncios[0].titulo, schema['@graph'][0].name);
  assert.equal(payload.anuncios[0].url, link);
  assert.equal(payload.anuncios[0].id, 'MLBU3546487622');
});

test("estratégia JSON-LD sozinha preenche uma listagem sintética", () => {
  const payload = extract(jsonLdListFixture, "https://lista.mercadolivre.com.br/teclado");
  assert.equal(payload.kind, "list");
  assert.equal(payload.anuncios.length, 2);
  assert.equal(payload.anuncios[0].id, "MLB7001");
  assert.equal(payload.anuncios[0].preco, 199.9);
  assert.equal(payload.estrategias[0].campos.titulo, "jsonld");
});

test("estratégia de estado embutido sozinha preenche uma listagem sintética", () => {
  const payload = extract(stateListFixture, "https://lista.mercadolivre.com.br/teclado");
  assert.equal(payload.kind, "list");
  assert.equal(payload.anuncios.length, 2);
  assert.equal(payload.anuncios[0].id, "MLB7101");
  assert.equal(payload.anuncios[0].vendedor, "Teclas BR");
  assert.equal(payload.estrategias[0].campos.titulo, "estado");
});

test("estratégia DOM sozinha preenche uma listagem sintética", () => {
  const payload = extract(domListFixture, "https://lista.mercadolivre.com.br/teclado");
  assert.equal(payload.kind, "list");
  assert.equal(payload.anuncios[0].id, "MLB7201");
  assert.equal(payload.anuncios[0].preco, 79.9);
  assert.equal(payload.estrategias[0].campos.preco, "dom");
});

test("nível oficial do vendedor atravessa o extrator sem alterar o score legado",()=>{
  const html='<div class="poly-card" data-id="MLB7001" data-price="399" data-seller-score="0.91" data-seller-level="4_light_green"><a href="https://www.mercadolivre.com.br/MLB7001">SSD 1TB</a></div>';
  const a=extract(html,'https://lista.mercadolivre.com.br/ssd').anuncios[0];
  assert.equal(a.vendedor_nivel,'4_light_green');
  assert.equal(a.vendedor_score,.91);
});
