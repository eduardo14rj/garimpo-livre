import { Fragment, useEffect, useRef, useState } from "react";
import type { Comparada, Grupo, LojaId, Ranking, Status } from "./multi-types";
import { Icon } from "./Icons";
import type { IconName } from "./Icons";
import { dinheiro, gruposVisiveis, lojasInfo, textoStatus } from "./multi-view";

export function BadgeLoja({ id }: { id: LojaId }) {
  const loja = lojasInfo[id];
  return <span className="store-badge" style={{ color: loja.cor }}>{loja.nome}</span>;
}

function iconeStatus(status?: Status): IconName | null {
  if (!status) return "loading" as const;
  if (status.estado === "ok") return "check" as const;
  if (status.estado === "desativada") return "pause" as const;
  if (status.estado === "falhou") return "error" as const;
  return null;
}

export function FaixaStatus({ ranking, pendentes = [], onSelectStore }: {
  ranking: Ranking | null;
  pendentes?: LojaId[];
  onSelectStore?: (id: LojaId) => void;
}) {
  const ids = [...new Set([...pendentes, ...Object.keys(ranking?.status ?? {}) as LojaId[]])];
  return <div className="store-status-strip" aria-label="Status das lojas" aria-live="polite">{ids.map(id => {
    const status = ranking?.status[id];
    const ofertasNoRanking = ranking && !pendentes.includes(id)
      ? ranking.produtos.reduce((total, grupo) => total + grupo.ofertas.filter(o => o.oferta.loja === id).length, 0)
      : undefined;
    const texto = textoStatus(status, ofertasNoRanking);
    return <button
      key={id}
      type="button"
      className="store-status-item"
      data-store-status={id}
      data-status={status?.estado ?? "aguardando"}
      onClick={() => onSelectStore?.(id)}
      aria-label={`Abrir opções do WebView para ${lojasInfo[id].nome}. Status: ${texto}`}
      title={`Clique para abrir as opções do WebView de ${lojasInfo[id].nome}`}
    >
      {iconeStatus(status) && <Icon name={iconeStatus(status)!} className="store-status-icon" />}
      <BadgeLoja id={id} />
      <span>{texto}</span>
      <Icon name="arrowRight" className="store-status-action" />
    </button>;
  })}</div>;
}

export function GraficoLojas({ grupos }: { grupos: Grupo[] }) {
  const ofertas = grupos.flatMap(g => g.ofertas);
  const valores = ofertas.map(o => Number(o.oferta.preco_brl));
  const max = Math.max(...valores, 1);
  const ids = [...new Set(ofertas.map(o => o.oferta.loja))];
  const notas = ofertas.map(o => o.nota_bayesiana).filter(Number.isFinite);
  const menorNota = notas.length ? Math.max(0, Math.min(...notas)) : 0;
  const inicioNota = notas.length ? (menorNota >= 5 ? 4.8 : menorNota) : 0;
  const faixaNota = 5 - inicioNota;
  const yNota = (nota: number) => 225 - (nota - inicioNota) / faixaNota * 190;
  const marcasNota = Array.from({ length: 6 }, (_, i) => inicioNota + faixaNota * i / 5);
  return <section className="chart-card"><h2><Icon name="chart" /> Preço × avaliação</h2><p className="muted">Cada ponto é uma oferta; o eixo começa na menor nota ajustada encontrada.</p>
    <svg viewBox="0 0 740 260" role="img" aria-label="Gráfico de preço final e nota por loja">
      {marcasNota.map(n => <g key={n}><line x1="45" x2="710" y1={yNota(n)} y2={yNota(n)} stroke="#e8ecee" /><text x="25" y={yNota(n) + 4} fontSize="11">{n.toLocaleString("pt-BR", { maximumFractionDigits: 2 })}</text></g>)}
      <text x="45" y="17" fontSize="11">Nota</text><text x="290" y="253" fontSize="11">Preço exibido (R$ 0 → {dinheiro(max.toFixed(2))})</text>
      {ofertas.map(o => <circle key={o.oferta.id} data-store-point={o.oferta.loja} cx={45 + Number(o.oferta.preco_brl) / max * 660} cy={yNota(o.nota_bayesiana)} r="5" fill={lojasInfo[o.oferta.loja].cor}><title>{lojasInfo[o.oferta.loja].nome}: {o.oferta.titulo} · {dinheiro(o.oferta.preco_brl)} · nota {o.oferta.nota ?? "não informada"}</title></circle>)}
    </svg><div className="tag-row">{ids.map(id => <BadgeLoja key={id} id={id} />)}</div>
  </section>;
}

export function ResultadosMulti({ ranking, busy = false, pendentes = [], acoes = [], historico, abrir, exportar, abrirLoja, continuarColeta, tentarNovamente }: {
  ranking: Ranking | null;
  busy?: boolean;
  pendentes?: LojaId[];
  acoes?: LojaId[];
  historico: (id: string, grupo: boolean) => void;
  abrir: (url: string) => void;
  exportar: () => void;
  abrirLoja?: (id: LojaId) => void;
  continuarColeta?: (id: LojaId) => void;
  tentarNovamente?: (id: LojaId) => void;
}) {
  const [loja, setLoja] = useState<LojaId | "todas">("todas");
  const [ordem, setOrdem] = useState("score");
  const [abertos, setAbertos] = useState<Set<string>>(new Set());
  const [lojaPopover, setLojaPopover] = useState<LojaId | null>(null);
  const statusAreaRef = useRef<HTMLDivElement | null>(null);
  useEffect(() => {
    if (!lojaPopover) return;
    const fecharFora = (event: PointerEvent) => {
      if (event.target instanceof Element && !statusAreaRef.current?.contains(event.target)) setLojaPopover(null);
    };
    const fecharEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") setLojaPopover(null);
    };
    document.addEventListener("pointerdown", fecharFora);
    document.addEventListener("keydown", fecharEscape);
    return () => {
      document.removeEventListener("pointerdown", fecharFora);
      document.removeEventListener("keydown", fecharEscape);
    };
  }, [lojaPopover]);
  const grupos = gruposVisiveis(ranking, loja, ordem);
  const ofertas = grupos.flatMap(g => g.ofertas);
  function expandir(chave: string) {
    setAbertos(v => {
      const copia = new Set(v);
      if (copia.has(chave)) copia.delete(chave);
      else copia.add(chave);
      return copia;
    });
  }
  function linha(o: Comparada, principal: boolean, g: Grupo) {
    const p = o.oferta;
    return <tr key={p.id} className={principal ? "group-primary" : "group-child"} data-offer-id={p.id}>
      <td className="multi-title">
        <div className="offer-store-line"><BadgeLoja id={p.loja} />{principal && g.ofertas.length > 1 && <button className="text-button" aria-expanded={abertos.has(g.chave)} onClick={() => expandir(g.chave)}><Icon name={abertos.has(g.chave) ? "arrowDown" : "arrowRightSmall"} /> {g.ofertas.length} ofertas</button>}</div>
        <strong>{p.titulo}</strong><span className="muted">{p.vendedor || "Vendedor não informado"}</span>
        <div className="tag-row">{p.preco_variavel && <span className="filter-tag"><Icon name="price" /> Preço a partir de</span>}{o.suspeito && <span className="filter-tag"><Icon name="alert" /> Preço fora da faixa</span>}{o.avisos?.map(aviso => <span className="filter-tag" key={aviso}><Icon name="alert" /> {aviso}</span>)}{principal && g.pareto && <span className="filter-tag"><Icon name="sparkle" /> Pareto</span>}</div>
        <span className="muted">{p.nota == null ? "Sem nota informada" : <><Icon name="star" /> {p.nota.toFixed(1)}</>} · {p.qtd_avaliacoes == null ? "Reviews não informados" : `${p.qtd_avaliacoes} avaliações`}{o.confianca_por_vendas ? " · confiança por vendas" : ""}</span>
      </td>
      <td><strong>{dinheiro(p.preco_brl)}</strong>{p.preco_variavel && <small className="muted">Preço anunciado como faixa</small>}</td><td>{p.frete_brl == null ? "Não informado" : dinheiro(p.frete_brl)}</td>
      <td>{(o.score * 100).toFixed(1)}<div className="row-actions"><button className="text-button" disabled={ranking?.demonstracao} onClick={() => abrir(p.url)}><Icon name="externalLink" /> Abrir</button><button className="text-button" disabled={ranking?.demonstracao} onClick={() => historico(p.id, false)}><Icon name="history" /> Oferta</button>{principal && <button className="text-button" disabled={ranking?.demonstracao} onClick={() => historico(g.chave, true)}><Icon name="file" /> Grupo</button>}</div></td>
    </tr>;
  }
  return <div className="page-section">
    <div className="store-status-area" ref={statusAreaRef}>
      <FaixaStatus ranking={ranking} pendentes={pendentes} onSelectStore={setLojaPopover} />
      {lojaPopover && <PopoverWebview
        id={lojaPopover}
        status={ranking?.status[lojaPopover]}
        texto={textoStatus(ranking?.status[lojaPopover])}
        requerAcao={acoes.includes(lojaPopover)}
        busy={busy}
        fechar={() => setLojaPopover(null)}
        abrirLoja={() => abrirLoja?.(lojaPopover)}
        continuar={() => continuarColeta?.(lojaPopover)}
        tentarNovamente={tentarNovamente ? () => tentarNovamente(lojaPopover) : undefined}
      />}
    </div>
    {ranking?.demonstracao && <div className="tip-row"><Icon name="testTube" /> <span>Demonstração com ofertas fictícias. Links e histórico real desabilitados.</span></div>}
    {ranking && <p className="result-meta"><Icon name={busy ? "loading" : "check"} /> {busy ? "Resultado parcial" : "Resultado da coleta"} · {ranking.anuncios_coletados} anúncios · {ranking.anuncios_filtrados} passaram pelos filtros · {ranking.produtos.length} grupos</p>}
    {ranking && Object.keys(ranking.excluidos).length > 0 && <details className="tip-row"><summary><Icon name="filter" /> Motivos dos filtros</summary><ul>{Object.entries(ranking.excluidos).map(([motivo, n]) => <li key={motivo}>{motivo.replace(/_/g, " ")}: {n}</li>)}</ul><p>Condição ou avaliações ausentes aparecem com aviso. Condição incompatível e avaliações abaixo do mínimo continuam excluídas.</p></details>}
    {ofertas.some(o => o.oferta.frete_brl == null) && <p className="tip-row"><Icon name="lightbulb" /><span>O frete só aparece quando a página da loja informa esse valor.</span></p>}
    <div className="multi-toolbar"><label>Loja <select aria-label="Filtrar por loja" value={loja} onChange={e => setLoja(e.target.value as LojaId | "todas")}><option value="todas">Todas as lojas</option>{Object.entries(lojasInfo).map(([id, l]) => <option key={id} value={id}>{l.nome}</option>)}</select></label><label>Ordenar <select aria-label="Ordenar ofertas" value={ordem} onChange={e => setOrdem(e.target.value)}><option value="score">Melhor score</option><option value="preco">Menor preço</option><option value="nota">Melhor nota ajustada</option></select></label><button className="button button-secondary" disabled={!ranking || busy} onClick={exportar}><Icon name="download" /> Exportar CSV</button></div>
    <GraficoLojas grupos={grupos} />
    <div className="multi-table-wrap"><table className="multi-table"><thead><tr><th>Oferta</th><th>Preço</th><th>Frete</th><th>Score / ações</th></tr></thead><tbody>{grupos.map(g => <Fragment key={g.chave}>{linha(g.principal, true, g)}{abertos.has(g.chave) && g.ofertas.filter(o => o.oferta.id !== g.principal.oferta.id).map(o => linha(o, false, g))}</Fragment>)}{grupos.length === 0 && <tr><td colSpan={4}>{busy ? <><Icon name="loading" /> Aguardando ofertas das lojas…</> : ranking ? "Nenhuma oferta passou pelos filtros. Confira os motivos e o status das lojas; você pode ajustar a busca ou informar uma oferta manualmente." : "Escolha uma busca e execute a primeira coleta."}</td></tr>}</tbody></table></div>
  </div>;
}

function PopoverWebview({ id, status, texto, requerAcao, busy, fechar, abrirLoja, continuar, tentarNovamente }: {
  id: LojaId;
  status?: Status;
  texto: string;
  requerAcao: boolean;
  busy: boolean;
  fechar: () => void;
  abrirLoja: () => void;
  continuar: () => void;
  tentarNovamente?: () => void;
}) {
  const icone = iconeStatus(status);
  return <div className="store-webview-popover" role="dialog" aria-labelledby="store-webview-title">
    <div className="store-webview-popover-head">
      <div><h2 id="store-webview-title">WebView · <BadgeLoja id={id} /></h2><p className={status?.estado === "falhou" ? "store-webview-error" : "store-webview-status"}>{icone && <Icon name={icone} />} {texto}</p></div>
      <button type="button" className="icon-button" aria-label="Fechar opções da loja" onClick={fechar}><Icon name="close" /></button>
    </div>
    {requerAcao
      ? <p>A loja solicitou autenticação ou verificação. Conclua na janela de coleta que está aberta e retome por aqui.</p>
      : <p>Se a loja exigir autenticação, abra o WebView, entre na sua conta e depois tente a busca novamente. A sessão fica salva neste computador.</p>}
    <div className="store-webview-actions">
      {requerAcao
        ? <button type="button" className="button button-primary" onClick={continuar}><Icon name="play" /> Continuar coleta</button>
        : <>
          <button type="button" className="button button-secondary" onClick={abrirLoja}><Icon name="globe" /> Abrir WebView da loja</button>
          <button type="button" className="button button-primary" disabled={busy || !tentarNovamente} onClick={tentarNovamente}><Icon name={busy ? "loading" : "refresh"} /> Tentar busca novamente</button>
        </>}
    </div>
  </div>;
}
