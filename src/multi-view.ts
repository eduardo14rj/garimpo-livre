import type { Grupo, LojaId, Ranking, Status } from "./multi-types";
export const lojasInfo: Record<LojaId, { nome: string; cor: string }> = {
  ml: { nome: "Mercado Livre", cor: "#b78a00" }, ebay: { nome: "eBay", cor: "#276bcc" }, amazon: { nome: "Amazon", cor: "#914e12" }, shopee: { nome: "Shopee", cor: "#db5030" }, aliexpress: { nome: "AliExpress", cor: "#bb3374" }, kabum: { nome: "KaBuM!", cor: "#6c51bb" }, pichau: { nome: "Pichau", cor: "#267b62" },
};
const partes = (valor: string | number) => { const [inteiro, frac = ""] = String(valor).split("."); return [BigInt(inteiro || "0"), frac] as const; };
export function compararDinheiro(a: string, b: string) {
  const [ia,fa]=partes(a), [ib,fb]=partes(b); const n=Math.max(fa.length,fb.length);
  if (ia!==ib) return ia<ib?-1:1;
  return fa.padEnd(n,"0").localeCompare(fb.padEnd(n,"0"));
}
export function dinheiro(valor: string | number | null | undefined) {
  if (valor == null) return "Não informado";
  const [inteiro,frac]=partes(valor);
  return `R$ ${new Intl.NumberFormat("pt-BR").format(inteiro)},${frac.padEnd(2,"0").slice(0,2)}`;
}
export function textoStatus(status?: Status, ofertasNoRanking?: number) {
  if (!status) return "Aguardando";
  let texto: string;
  if (status.estado==="ok") texto="Concluída";
  else if (status.estado==="falhou") texto=`${status.erro}`;
  else if (status.estado==="desativada") texto="Desativada";
  else if (status.estado==="degradado") texto=status.capturado_em ? `${status.fonte} de ${new Date(status.capturado_em).toLocaleDateString("pt-BR")}` : status.fonte;
  else texto="Status da fonte desconhecido";
  if (ofertasNoRanking===undefined) return texto;
  const quantidade=ofertasNoRanking===1?"oferta":"ofertas";
  return `${texto} · ${ofertasNoRanking} ${quantidade} no ranking`;
}
export function gruposVisiveis(ranking: Ranking | null, loja: LojaId | "todas", ordem: string): Grupo[] {
  const grupos=(ranking?.produtos ?? []).flatMap(g=> {
    const ofertas=g.ofertas.filter(o=>loja==="todas" || o.oferta.loja===loja).sort((a,b)=>compararDinheiro(a.oferta.preco_final_brl,b.oferta.preco_final_brl)||a.oferta.id.localeCompare(b.oferta.id));
    return ofertas.length ? [{...g,ofertas,principal:ofertas[0],score:ofertas[0].score}] : [];
  });
  for (const g of grupos) g.pareto=!grupos.some(b=>compararDinheiro(b.principal.oferta.preco_final_brl,g.principal.oferta.preco_final_brl)<=0 && b.principal.nota_bayesiana>=g.principal.nota_bayesiana && (compararDinheiro(b.principal.oferta.preco_final_brl,g.principal.oferta.preco_final_brl)<0 || b.principal.nota_bayesiana>g.principal.nota_bayesiana));
  const compararAvaliacao=(a:Grupo,b:Grupo,direcao:"asc"|"desc")=> {
    const notaA=a.principal.oferta.nota, notaB=b.principal.oferta.nota;
    if(notaA===null) return notaB===null ? 0 : 1;
    if(notaB===null) return -1;
    const porNota=direcao==="asc" ? notaA-notaB : notaB-notaA;
    return porNota || (b.principal.oferta.qtd_avaliacoes ?? -1)-(a.principal.oferta.qtd_avaliacoes ?? -1);
  };
  return grupos.sort((a,b)=> {
    if(ordem==="preco") return compararDinheiro(a.principal.oferta.preco_final_brl,b.principal.oferta.preco_final_brl) || a.chave.localeCompare(b.chave);
    if(ordem==="preco_desc") return compararDinheiro(b.principal.oferta.preco_final_brl,a.principal.oferta.preco_final_brl) || a.chave.localeCompare(b.chave);
    if(ordem==="avaliacao_asc" || ordem==="avaliacao_desc") return compararAvaliacao(a,b,ordem==="avaliacao_asc" ? "asc" : "desc") || a.chave.localeCompare(b.chave);
    return (ordem==="nota" ? b.principal.nota_bayesiana-a.principal.nota_bayesiana : b.score-a.score) || a.chave.localeCompare(b.chave);
  });
}
