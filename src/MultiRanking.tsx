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

export function GraficoLojas({ grupos, onSelecionarOferta }: { grupos: Grupo[]; onSelecionarOferta: (ofertaId: string) => void }) {
  const [tooltip, setTooltip] = useState<{ oferta: Comparada; left: number; top: number; posicao: "acima" | "abaixo" } | null>(null);
  const ofertaSobMouseRef = useRef<string | null>(null);
  const ofertaEmFocoRef = useRef<string | null>(null);
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
  function mostrarTooltip(oferta: Comparada, ponto: SVGCircleElement) {
    const bounds = ponto.getBoundingClientRect();
    const centroX = bounds.left + bounds.width / 2;
    const centroY = bounds.top + bounds.height / 2;
    const larguraViewport = document.documentElement.clientWidth || window.innerWidth;
    const alturaViewport = document.documentElement.clientHeight || window.innerHeight;
    const metadeTooltip = Math.min(160, larguraViewport / 2 - 12);
    const left = Math.max(metadeTooltip + 12, Math.min(centroX, larguraViewport - metadeTooltip - 12));
    setTooltip({ oferta, left, top: centroY, posicao: centroY < alturaViewport * .55 ? "abaixo" : "acima" });
  }
  function ocultarTooltip(ofertaId: string) {
    if (ofertaSobMouseRef.current !== ofertaId && ofertaEmFocoRef.current !== ofertaId) {
      setTooltip(atual => atual?.oferta.oferta.id === ofertaId ? null : atual);
    }
  }
  return <section className="chart-card multi-ranking-chart"><h2><span className="chart-title-emoji" aria-hidden="true">📊</span> Preço × avaliação</h2><p className="muted">A altura representa a nota ajustada. Passe o mouse para ver os detalhes e clique para localizar a oferta na lista.</p>
    <svg viewBox="0 0 740 260" role="group" aria-label="Gráfico de preço final e nota por loja">
      {marcasNota.map(n => <g key={n}><line x1="45" x2="710" y1={yNota(n)} y2={yNota(n)} stroke="#e8ecee" /><text x="25" y={yNota(n) + 4} fontSize="11">{n.toLocaleString("pt-BR", { maximumFractionDigits: 2 })}</text></g>)}
      <text x="45" y="17" fontSize="11">Nota</text><text x="290" y="253" fontSize="11">Preço exibido (R$ 0 → {dinheiro(max.toFixed(2))})</text>
      {ofertas.map(o => {
        const lojaNome = lojasInfo[o.oferta.loja].nome;
        const descricao = `${lojaNome}: ${o.oferta.titulo}. Preço exibido ${dinheiro(o.oferta.preco_brl)}. Nota ajustada ${o.nota_bayesiana.toFixed(2)} de 5. Clique para localizar na lista.`;
        return <g key={o.oferta.id}>
          <circle
            className="chart-point-hit-area"
            data-store-point={o.oferta.loja}
            cx={45 + Number(o.oferta.preco_brl) / max * 660}
            cy={yNota(o.nota_bayesiana)}
            r="12"
            fill="transparent"
            role="button"
            tabIndex={0}
            aria-label={descricao}
            aria-describedby={tooltip?.oferta.oferta.id === o.oferta.id ? "chart-offer-tooltip" : undefined}
            onPointerEnter={event => {
              ofertaSobMouseRef.current = o.oferta.id;
              mostrarTooltip(o, event.currentTarget);
            }}
            onPointerLeave={() => {
              if (ofertaSobMouseRef.current === o.oferta.id) ofertaSobMouseRef.current = null;
              ocultarTooltip(o.oferta.id);
            }}
            onFocus={event => {
              ofertaEmFocoRef.current = o.oferta.id;
              mostrarTooltip(o, event.currentTarget);
            }}
            onBlur={() => {
              if (ofertaEmFocoRef.current === o.oferta.id) ofertaEmFocoRef.current = null;
              ocultarTooltip(o.oferta.id);
            }}
            onClick={() => onSelecionarOferta(o.oferta.id)}
            onKeyDown={event => {
              if (event.key === "Enter" || event.key === " ") {
                event.preventDefault();
                onSelecionarOferta(o.oferta.id);
              }
            }}
          />
          <circle className="chart-point" cx={45 + Number(o.oferta.preco_brl) / max * 660} cy={yNota(o.nota_bayesiana)} r="5" style={{ fill: lojasInfo[o.oferta.loja].cor }} pointerEvents="none" />
        </g>;
      })}
    </svg><div className="tag-row">{ids.map(id => <BadgeLoja key={id} id={id} />)}</div>
    {tooltip && <div className={`chart-offer-tooltip chart-offer-tooltip-${tooltip.posicao}`} id="chart-offer-tooltip" role="tooltip" style={{ left: tooltip.left, top: tooltip.top }}>
      <div className="chart-tooltip-store"><span style={{ background: lojasInfo[tooltip.oferta.oferta.loja].cor }} />{lojasInfo[tooltip.oferta.oferta.loja].nome}</div>
      <strong className="chart-tooltip-title" title={tooltip.oferta.oferta.titulo}>{tooltip.oferta.oferta.titulo}</strong>
      <div className="chart-tooltip-prices"><div><span>Preço exibido</span><strong>{dinheiro(tooltip.oferta.oferta.preco_brl)}</strong></div><div><span>Preço final</span><strong>{dinheiro(tooltip.oferta.oferta.preco_final_brl)}</strong></div></div>
      <span className="chart-tooltip-freight">Frete: {dinheiro(tooltip.oferta.oferta.frete_brl)}</span>
      <div className="chart-tooltip-rating"><span aria-hidden="true">⭐</span><strong>{tooltip.oferta.nota_bayesiana.toFixed(2)} / 5</strong><span>Nota ajustada</span></div>
      <div className="chart-tooltip-meta"><span>{tooltip.oferta.oferta.nota == null ? "Nota do produto não informada" : `Nota do produto ${tooltip.oferta.oferta.nota.toFixed(1)}`}</span><span>{tooltip.oferta.oferta.qtd_avaliacoes == null ? "Avaliações não informadas" : `${tooltip.oferta.oferta.qtd_avaliacoes} avaliações`}</span></div>
      <span className="chart-tooltip-seller">{tooltip.oferta.oferta.vendedor || "Vendedor não informado"}</span>
    </div>}
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
  const [ofertaParaRolar, setOfertaParaRolar] = useState<{ id: string; tentativa: number } | null>(null);
  const [ofertaDestacada, setOfertaDestacada] = useState<string | null>(null);
  const tentativaRoloRef = useRef(0);
  const statusAreaRef = useRef<HTMLDivElement | null>(null);
  const tabelaRef = useRef<HTMLDivElement | null>(null);
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
  useEffect(() => {
    if (!ofertaParaRolar) return;
    const frame = requestAnimationFrame(() => {
      const linha = [...(tabelaRef.current?.querySelectorAll<HTMLTableRowElement>("tr[data-offer-id]") ?? [])]
        .find(item => item.dataset.offerId === ofertaParaRolar.id);
      if (!linha) return;
      linha.scrollIntoView({ behavior: "smooth", block: "center" });
      linha.focus({ preventScroll: true });
      setOfertaDestacada(ofertaParaRolar.id);
    });
    return () => cancelAnimationFrame(frame);
  }, [ofertaParaRolar, abertos]);
  useEffect(() => {
    if (!ofertaDestacada) return;
    const timeout = window.setTimeout(() => setOfertaDestacada(null), 2200);
    return () => window.clearTimeout(timeout);
  }, [ofertaDestacada]);
  function localizarOferta(ofertaId: string) {
    const grupo = grupos.find(item => item.ofertas.some(oferta => oferta.oferta.id === ofertaId));
    if (!grupo) return;
    if (grupo.principal.oferta.id !== ofertaId) {
      setAbertos(atual => atual.has(grupo.chave) ? atual : new Set(atual).add(grupo.chave));
    }
    setOfertaParaRolar({ id: ofertaId, tentativa: ++tentativaRoloRef.current });
  }
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
    return <tr key={p.id} tabIndex={-1} className={`${principal ? "group-primary" : "group-child"}${ofertaDestacada === p.id ? " offer-target-highlight" : ""}`} data-offer-id={p.id}>
      <td className="multi-title">
        <div className="offer-store-line"><BadgeLoja id={p.loja} />{principal && g.ofertas.length > 1 && <button className="text-button" aria-expanded={abertos.has(g.chave)} onClick={() => expandir(g.chave)}><Icon name={abertos.has(g.chave) ? "arrowDown" : "arrowRightSmall"} /> {g.ofertas.length} ofertas</button>}</div>
        <strong title={p.titulo}>{p.titulo}</strong><span className="muted">{p.vendedor || "Vendedor não informado"}</span>
        <div className="tag-row">{p.preco_variavel && <span className="filter-tag"><Icon name="price" /> Preço a partir de</span>}{o.suspeito && <span className="filter-tag"><Icon name="alert" /> Preço fora da faixa</span>}{o.avisos?.map(aviso => <span className="filter-tag" key={aviso}><Icon name="alert" /> {aviso}</span>)}{principal && g.pareto && <span className="filter-tag"><Icon name="sparkle" /> Pareto</span>}</div>
      </td>
      <td><strong>{dinheiro(p.preco_brl)}</strong>{p.preco_variavel && <small className="muted">Preço anunciado como faixa</small>}</td><td>{p.frete_brl == null ? "Não informado" : dinheiro(p.frete_brl)}</td>
      <td className="rating-cell">{p.nota == null ? <span className="rating-empty">Sem nota informada</span> : <strong className="rating-value"><span aria-hidden="true">⭐</span> {p.nota.toFixed(1)}</strong>}<small>{p.qtd_avaliacoes == null ? "Avaliações não informadas" : `${p.qtd_avaliacoes.toLocaleString("pt-BR")} avaliações`}</small>{o.confianca_por_vendas && <small>Confiança por vendas</small>}</td>
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
    <div className="multi-toolbar"><label>Loja <select aria-label="Filtrar por loja" value={loja} onChange={e => setLoja(e.target.value as LojaId | "todas")}><option value="todas">Todas as lojas</option>{Object.entries(lojasInfo).map(([id, l]) => <option key={id} value={id}>{l.nome}</option>)}</select></label><label>Ordenar <select aria-label="Ordenar ofertas" value={ordem} onChange={e => setOrdem(e.target.value)}><option value="score">Melhor score</option><option value="preco">Preço final: menor para maior</option><option value="preco_desc">Preço final: maior para menor</option><option value="avaliacao_desc">Avaliação: maior para menor</option><option value="avaliacao_asc">Avaliação: menor para maior</option><option value="nota">Nota ajustada: maior para menor</option></select></label><button className="button button-secondary" disabled={!ranking || busy} onClick={exportar}><Icon name="download" /> Exportar CSV</button></div>
    <GraficoLojas grupos={grupos} onSelecionarOferta={localizarOferta} />
    <div className="multi-table-wrap" ref={tabelaRef}><table className="multi-table"><thead><tr><th>Oferta</th><th>Preço</th><th>Frete</th><th>Avaliação</th><th>Score / ações</th></tr></thead><tbody>{grupos.map(g => <Fragment key={g.chave}>{linha(g.principal, true, g)}{abertos.has(g.chave) && g.ofertas.filter(o => o.oferta.id !== g.principal.oferta.id).map(o => linha(o, false, g))}</Fragment>)}{grupos.length === 0 && <tr><td colSpan={5}>{busy ? <><Icon name="loading" /> Aguardando ofertas das lojas…</> : ranking ? "Nenhuma oferta passou pelos filtros. Confira os motivos e o status das lojas; você pode ajustar a busca ou informar uma oferta manualmente." : "Escolha uma busca e execute a primeira coleta."}</td></tr>}</tbody></table></div>
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
