import assert from "node:assert/strict";
import { readFileSync, mkdirSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import test from "node:test";
import ts from "typescript";
import { JSDOM } from "jsdom";

// Renderiza a interface com jsdom e uma API fake. Nenhum WebView, rede ou computador externo.
const cache=resolve("node_modules/.cache/garimpo-ui-tests");mkdirSync(cache,{recursive:true});
for(const arquivo of ["multi-view.ts","Icons.tsx","MultiRanking.tsx","MultiApp.tsx","tauri-api.ts"]){
  const texto=readFileSync(resolve("src",arquivo),"utf8");
  let js=ts.transpileModule(texto,{compilerOptions:{module:ts.ModuleKind.ESNext,target:ts.ScriptTarget.ES2022,jsx:ts.JsxEmit.ReactJSX}}).outputText;
  js=js.replace(/^import ["'][^"']+\.css["'];?\s*$/gm,"").replace(/from "\.\/(multi-view|MultiRanking|tauri-api|Icons)"/g,'from "./$1.js"');
  writeFileSync(resolve(cache,arquivo.replace(/\.tsx?$/,".js")),js);
}
const dom=new JSDOM("<!doctype html><html><body></body></html>",{url:"http://fixture.local/"});
for(const chave of ["window","document","HTMLElement","HTMLInputElement","HTMLSelectElement","Event","MouseEvent","FormData","Node"]) globalThis[chave]=dom.window[chave];
globalThis.IS_REACT_ACT_ENVIRONMENT=true;
const React=await import("react");const {createRoot}=await import("react-dom/client");
const {default:MultiApp}=await import(pathToFileURL(resolve(cache,"MultiApp.js")));
const {ResultadosMulti}=await import(pathToFileURL(resolve(cache,"MultiRanking.js")));
const {compararDinheiro,dinheiro,gruposVisiveis,textoStatus}=await import(pathToFileURL(resolve(cache,"multi-view.js")));
const {act}=React;
const lojas=["ml","kabum","pichau","aliexpress"].map(id=>({id,nome:id,ativa:true,padrao:true}));
const pesos={preco:.4,nota:.3,vendedor:.2,relevancia:.1};
const config={termo:"ssd nvme 1tb",lojas:lojas.map(l=>l.id),preco_max:null,condicao:"any",obrigatorio:[],proibido:[],min_avaliacoes:0,pesos};
const ad=(loja,preco)=>({oferta:{id:loja+":1",loja,titulo:"SSD Kingston NV2 1TB NVMe",url:"https://"+loja+".invalid/fixture",gtin:"0740617329919",modelo:"SNV2S/1000G",marca:"Kingston",preco_brl:preco,frete_brl:null,preco_final_brl:preco,nota:4.8,qtd_avaliacoes:120,vendas:null,reputacao:1,condicao:"new",vendedor:"Loja fixture",preco_variavel:false,preco_parcelado:null},nota_bayesiana:4.8,votos_confianca:120,confianca_por_vendas:false,relevancia:1,suspeito:false,avisos:[],score:.9});
const ofertas=[ad("kabum","389.90"),ad("ml","405.00"),ad("aliexpress","350.00")];
function ranking(completas=["kabum","ml","aliexpress"],status={kabum:{estado:"ok"},ml:{estado:"ok"},pichau:{estado:"falhou",erro:"Falha fixture"},aliexpress:{estado:"ok"}}){const os=ofertas.filter(o=>completas.includes(o.oferta.loja));return {busca:"ssd",executada_em:"2026-10-12T15:00:00Z",demonstracao:false,anuncios_coletados:os.length,anuncios_filtrados:os.length,excluidos:{},produtos:os.length?[{chave:"gtin:00740617329919",principal:os[0],ofertas:os,score:.9,pareto:true}]:[],status};}
async function montar(elemento){const el=document.createElement("div");document.body.append(el);const root=createRoot(el);await act(async()=>root.render(elemento));return {el,root,async fechar(){await act(async()=>root.unmount());el.remove();}};}
async function clicar(el){assert.ok(el,"Controle existe");await act(async()=>el.dispatchEvent(new MouseEvent("click",{bubbles:true})));}
async function mudar(el,valor){const proto=el instanceof HTMLSelectElement?HTMLSelectElement.prototype:HTMLInputElement.prototype;await act(async()=>{Object.getOwnPropertyDescriptor(proto,"value").set.call(el,valor);el.dispatchEvent(new Event(el instanceof HTMLSelectElement?"change":"input",{bubbles:true}));});}
const botao=(el,texto)=>[...el.querySelectorAll("button")].find(b=>b.textContent.trim()===texto);
function fakeApi(){const listeners=new Map();const chamadas=[];let resolver;
  const api={isDesktopApp:()=>true,desktopRequiredError:()=>new Error("Desktop requerido"),openUrl:async()=>{},listen:async(nome,fn)=>{const fs=listeners.get(nome)??new Set();fs.add(fn);listeners.set(nome,fs);return ()=>fs.delete(fn);},invoke:async(cmd,args)=>{chamadas.push({cmd,args});if(cmd==="list_searches")return [{slug:"ssd",config}];if(cmd==="list_stores")return lojas;if(cmd==="get_source_health")return [];if(cmd==="get_settings")return {fonte:"webview",intervalo_coleta_segundos:2,coleta_periodica_minutos:0,pesos_padrao:pesos};if(cmd==="get_multi_results")return null;if(cmd==="run_multi_search")return new Promise(r=>resolver=r);if(cmd==="save_search")return "ssd";return null;}};
  return {api,chamadas,emit(nome,payload){for(const fn of listeners.get(nome)??[])fn({payload});},concluir(r){resolver(r);},listeners};
}

test("ranking parcial tolera falha, atualiza por loja, filtra e não exibe campos de importação",async()=>{
  const fake=fakeApi();const tela=await montar(React.createElement(MultiApp,{api:fake.api}));
  try{await clicar(botao(tela.el,"Executar"));assert.match(tela.el.textContent,/Coletando/);
    const estados={};const completas=[];
    for(const id of ["kabum","pichau","ml","aliexpress"]){estados[id]=id==="pichau"?{estado:"falhou",erro:"Falha fixture"}:{estado:"ok"};if(id!=="pichau")completas.push(id);await act(async()=>fake.emit("garimpo:ranking-parcial",ranking(completas,{...estados})));assert.ok(tela.el.querySelector('[data-store-status="'+id+'"]'));if(id==="kabum")assert.ok(tela.el.querySelector('[data-offer-id="kabum:1"]'));}
    assert.match(tela.el.textContent,/Falha fixture/);assert.doesNotMatch(tela.el.textContent,/Imposto|estimativa|PTAX|importados/);
    await act(async()=>fake.concluir(ranking()));assert.match(tela.el.textContent,/Coleta concluída/);assert.match(tela.el.querySelector('[data-store-status="aliexpress"]').textContent,/1 oferta no ranking/);
    await clicar(botao(tela.el,"3 ofertas"));assert.equal(tela.el.querySelectorAll("[data-offer-id]").length,3);
    assert.equal(tela.el.querySelectorAll("[data-store-point]").length,3);
    await mudar(tela.el.querySelector('[aria-label="Filtrar por loja"]'),"ml");assert.equal(tela.el.querySelectorAll("[data-offer-id]").length,1);assert.ok(tela.el.querySelector('[data-offer-id="ml:1"]'));assert.equal(tela.el.querySelectorAll("[data-store-point]").length,1);
    assert.deepEqual([...tela.el.querySelectorAll(".multi-table thead th")].map(x=>x.textContent),["Oferta","Preço","Frete","Avaliação","Score / ações"]);
  }finally{await tela.fechar();}assert.equal([...fake.listeners.values()].reduce((n,s)=>n+s.size,0),0);
});

test("editor preserva espaços em termos obrigatórios e salva somente filtros atuais",async()=>{
  const fake=fakeApi();const tela=await montar(React.createElement(MultiApp,{api:fake.api}));try{await clicar(tela.el.querySelector('[aria-label="Editar ssd nvme 1tb"]'));const input=tela.el.querySelector('[aria-label="Termos obrigatórios"]');await mudar(input,"s25");await mudar(input,"s25 ");assert.equal(input.value,"s25 ");await mudar(input,"s25 plus, nv2 1tb");
    await act(async()=>tela.el.querySelector("form").dispatchEvent(new Event("submit",{bubbles:true,cancelable:true})));const salvo=fake.chamadas.find(c=>c.cmd==="save_search");assert.deepEqual(salvo.args.config.obrigatorio,["s25 plus","nv2 1tb"]);assert.equal("incluir_importados" in salvo.args.config,false);assert.deepEqual(salvo.args.config.lojas,config.lojas);
  }finally{await tela.fechar();}
});

test("tela Lojas oferece só sessão WebView e alternâncias da loja",async()=>{
  const fake=fakeApi();const tela=await montar(React.createElement(MultiApp,{api:fake.api}));try{await clicar(botao(tela.el,"Lojas"));assert.match(tela.el.textContent,/Coleta por WebView/);assert.doesNotMatch(tela.el.textContent,/Configurar API|Identificador da aplicação|Secret|cofre|OAuth/);await clicar(botao(tela.el,"Abrir loja"));await clicar(botao(tela.el,"Limpar sessão"));assert.ok(fake.chamadas.some(c=>c.cmd==="open_store"));assert.ok(fake.chamadas.some(c=>c.cmd==="clear_store_session"));}finally{await tela.fechar();}
});

test("status de cache, demonstração, preço e frete são exibidos sem efeitos externos",async()=>{
  const r=ranking();r.demonstracao=true;r.status.pichau={estado:"degradado",fonte:"Cache",capturado_em:"2026-10-12T12:00:00Z"};r.status.amazon={estado:"falhou",erro:"Bloqueio fixture"};r.produtos[0].ofertas.find(o=>o.oferta.loja==="aliexpress").avisos=["Avaliações não informadas (mínimo: 8)"];let abertos=0;
  const tela=await montar(React.createElement(ResultadosMulti,{ranking:r,historico:()=>{},abrir:()=>abertos++,exportar:()=>{}}));try{assert.match(tela.el.textContent,/Cache de/);assert.match(tela.el.textContent,/Bloqueio fixture/);assert.match(tela.el.textContent,/Demonstração/);assert.match(tela.el.textContent,/Avaliações não informadas \(mínimo: 8\)/);assert.ok(botao(tela.el,"Abrir").disabled);assert.equal(abertos,0);assert.match(tela.el.textContent,/Não informado/);assert.doesNotMatch(tela.el.textContent,/USD|Imposto|PTAX|estimativa/);}finally{await tela.fechar();}
  assert.equal(compararDinheiro("12345678901234567890.11","12345678901234567890.12"),-1);assert.equal(dinheiro("12345678901234567890.12"),"R$ 12.345.678.901.234.567.890,12");assert.equal(gruposVisiveis(r,"aliexpress","preco")[0].principal.oferta.loja,"aliexpress");
});

test("repetir busca pelo popover envia somente a loja selecionada para sondagem manual",async()=>{
  const fake=fakeApi();const tela=await montar(React.createElement(MultiApp,{api:fake.api}));
  try{
    await clicar(botao(tela.el,"Executar"));
    const r=ranking([], {shopee:{estado:"falhou",erro:"WebView: circuito aberto"}});
    await act(async()=>fake.concluir(r));
    await clicar(tela.el.querySelector('[data-store-status="shopee"]'));
    await clicar(botao(tela.el,"Tentar busca novamente"));
    const chamadas=fake.chamadas.filter(c=>c.cmd==="run_multi_search");
    assert.equal(chamadas.length,2);
    assert.equal(chamadas[1].args.retryStore,"shopee");
    await act(async()=>fake.concluir(r));
  }finally{await tela.fechar();}
});

test("status concluído informa quantas ofertas de cada loja passaram pelos filtros",()=>{
  assert.equal(textoStatus({estado:"ok"},0),"Concluída · 0 ofertas no ranking");
  assert.equal(textoStatus({estado:"ok"},1),"Concluída · 1 oferta no ranking");
  assert.equal(textoStatus({estado:"ok"}),"Concluída");
});
