//! Matching e ranking puros. Dinheiro só vira f64 depois de formar uma razão 0–1.
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use regex::Regex;
use rust_decimal::{Decimal, prelude::ToPrimitive};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{
    bayesiana, bm25_com_sinonimos, jaccard, min_max, normalizar, normalizar_com_sinonimos,
    tokenizar,
};
use crate::{
    lojas::{LojaId, OfertaNormalizada, ResultadoMultiLoja, StatusLoja},
    models::SearchConfig,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OfertaComparada {
    pub oferta: OfertaNormalizada,
    pub nota_bayesiana: f64,
    pub votos_confianca: u32,
    pub confianca_por_vendas: bool,
    pub relevancia: f64,
    pub suspeito: bool,
    #[serde(default)]
    pub avisos: Vec<String>,
    pub score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GrupoComparado {
    pub chave: String,
    pub principal: OfertaComparada,
    pub ofertas: Vec<OfertaComparada>,
    pub score: f64,
    pub pareto: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct RankingMultiLoja {
    pub busca: String,
    pub executada_em: String,
    pub demonstracao: bool,
    pub anuncios_coletados: usize,
    pub anuncios_filtrados: usize,
    pub excluidos: BTreeMap<String, usize>,
    pub produtos: Vec<GrupoComparado>,
    pub status: BTreeMap<LojaId, StatusLoja>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Variantes {
    capacidades_gb: BTreeSet<Decimal>,
    voltagens: BTreeSet<u16>,
    tipo: Option<&'static str>,
}

fn variantes(titulo: &str) -> Variantes {
    static CAPACIDADE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    static VOLTAGEM: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let capacidade =
        CAPACIDADE.get_or_init(|| Regex::new(r"(?i)\b(\d+(?:[.,]\d+)?)\s*(gb|tb)\b").unwrap());
    let voltagem = VOLTAGEM.get_or_init(|| Regex::new(r"(?i)\b(110|127|220)\s*v\b").unwrap());
    let tokens = tokenizar(titulo);
    Variantes {
        capacidades_gb: capacidade
            .captures_iter(titulo)
            .filter_map(|c| {
                let valor: Decimal = c[1].replace(',', ".").parse().ok()?;
                if c[2].eq_ignore_ascii_case("tb") {
                    valor.checked_mul(1000.into())
                } else {
                    Some(valor)
                }
            })
            .collect(),
        voltagens: voltagem
            .captures_iter(titulo)
            .filter_map(|c| c[1].parse().ok())
            .collect(),
        tipo: ["capa", "capinha", "case"]
            .iter()
            .any(|t| tokens.iter().any(|x| x == t))
            .then_some("acessorio"),
    }
}

fn atributos_compativeis(a: &Variantes, b: &Variantes) -> bool {
    (a.capacidades_gb.is_empty()
        || b.capacidades_gb.is_empty()
        || a.capacidades_gb == b.capacidades_gb)
        && (a.voltagens.is_empty() || b.voltagens.is_empty() || a.voltagens == b.voltagens)
        && a.tipo == b.tipo
}

fn gtin(oferta: &OfertaNormalizada) -> Option<String> {
    oferta
        .gtin
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| [8, 12, 13, 14].contains(&s.len()) && s.bytes().all(|c| c.is_ascii_digit()))
        .map(|s| format!("{s:0>14}"))
}
fn catalogo(valor: &Option<String>) -> Option<String> {
    valor
        .as_ref()
        .map(|s| normalizar(s).replace(' ', ""))
        .filter(|s| !s.is_empty())
}

pub(crate) fn sem_conflito(a: &OfertaNormalizada, b: &OfertaNormalizada) -> bool {
    let mesmo_gtin = gtin(a).zip(gtin(b)).is_some_and(|(x, y)| x == y);
    atributos_compativeis(&variantes(&a.titulo), &variantes(&b.titulo))
        && gtin(a).zip(gtin(b)).is_none_or(|(x, y)| x == y)
        && (mesmo_gtin
            || (catalogo(&a.marca)
                .zip(catalogo(&b.marca))
                .is_none_or(|(x, y)| x == y)
                && catalogo(&a.modelo)
                    .zip(catalogo(&b.modelo))
                    .is_none_or(|(x, y)| x == y)))
}

pub fn mesmo_produto(a: &OfertaNormalizada, b: &OfertaNormalizada) -> bool {
    if !sem_conflito(a, b) {
        return false;
    }
    if let Some((x, y)) = gtin(a).zip(gtin(b)) {
        return x == y;
    }
    if let Some(((ma, ba), (mb, bb))) = catalogo(&a.modelo)
        .zip(catalogo(&a.marca))
        .zip(catalogo(&b.modelo).zip(catalogo(&b.marca)))
        && ma == mb
        && ba == bb
    {
        return true;
    }
    jaccard(&a.titulo, &b.titulo) >= 0.8
}

fn chave(ofertas: &[OfertaComparada]) -> String {
    if let Some(gtin) = ofertas.iter().filter_map(|o| gtin(&o.oferta)).min() {
        return format!("gtin:{gtin}:{:?}", variantes(&ofertas[0].oferta.titulo));
    }
    if let Some((marca, modelo)) = ofertas
        .iter()
        .filter_map(|o| catalogo(&o.oferta.marca).zip(catalogo(&o.oferta.modelo)))
        .min()
    {
        return format!(
            "modelo:{marca}:{modelo}:{:?}",
            variantes(&ofertas[0].oferta.titulo)
        );
    }
    // Associação persistida por oferta preserva a chave quando um título melhora.
    let mut tokens = tokenizar(&ofertas[0].oferta.titulo);
    tokens.sort();
    tokens.dedup();
    format!("titulo:{:x}", Sha256::digest(tokens.join(" ").as_bytes()))
}

pub fn agrupar_multi(mut ofertas: Vec<OfertaComparada>) -> Vec<GrupoComparado> {
    ofertas.sort_by(|a, b| {
        gtin(&b.oferta)
            .is_some()
            .cmp(&gtin(&a.oferta).is_some())
            .then_with(|| a.oferta.id.cmp(&b.oferta.id))
    });
    let mut grupos: Vec<Vec<OfertaComparada>> = Vec::new();
    // Bloqueio por identificadores e tokens: similaridade >= .8 exige token comum.
    let mut blocos: HashMap<String, BTreeSet<usize>> = HashMap::new();
    for oferta in ofertas {
        let mut chaves = tokenizar(&oferta.oferta.titulo);
        if let Some(g) = gtin(&oferta.oferta) {
            chaves.push(format!("gtin:{g}"));
        }
        if let Some((m, b)) = catalogo(&oferta.oferta.modelo).zip(catalogo(&oferta.oferta.marca)) {
            chaves.push(format!("modelo:{b}:{m}"));
        }
        let candidatos = chaves
            .iter()
            .filter_map(|k| blocos.get(k))
            .flatten()
            .copied()
            .collect::<BTreeSet<_>>();
        let candidatos = candidatos
            .into_iter()
            .filter(|&g| {
                grupos[g]
                    .iter()
                    .any(|o| mesmo_produto(&o.oferta, &oferta.oferta))
                    && grupos[g]
                        .iter()
                        .all(|o| sem_conflito(&o.oferta, &oferta.oferta))
            })
            .collect::<Vec<_>>();
        // Sem identificação suficiente, não escolher arbitrariamente entre GTINs diferentes.
        let existente = (candidatos.len() == 1).then(|| candidatos[0]);
        let indice = existente.unwrap_or_else(|| {
            grupos.push(vec![]);
            grupos.len() - 1
        });
        grupos[indice].push(oferta);
        for k in chaves {
            blocos.entry(k).or_default().insert(indice);
        }
    }
    grupos
        .into_iter()
        .map(|mut ofertas| {
            ofertas.sort_by(|a, b| {
                a.oferta
                    .preco_final_brl
                    .cmp(&b.oferta.preco_final_brl)
                    .then_with(|| a.oferta.id.cmp(&b.oferta.id))
            });
            let principal = ofertas[0].clone();
            GrupoComparado {
                chave: chave(&ofertas),
                score: principal.score,
                principal,
                ofertas,
                pareto: false,
            }
        })
        .collect()
}

fn texto_busca(texto: &str) -> String {
    static UNIDADE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = UNIDADE.get_or_init(|| Regex::new(r"(?i)\b(\d+)\s+(tb|gb|v)\b").unwrap());
    let texto = re.replace_all(texto, "${1}${2}");
    super::pipeline::expand_model_variants(&texto)
}

fn motivo_exclusao(o: &OfertaNormalizada, c: &SearchConfig) -> Option<&'static str> {
    let titulo = normalizar_com_sinonimos(&texto_busca(&o.titulo), &c.sinonimos);
    let cercado = format!(" {titulo} ");
    if c.preco_max.is_some_and(|max| o.preco_final_brl > max) {
        return Some("preco_final");
    }
    let condicao = normalizar(&o.condicao);
    let condicao_ausente = condicao.is_empty()
        || matches!(
            condicao.as_str(),
            "unknown" | "desconhecido" | "nao informado"
        );
    if c.condicao != "any" && !condicao_ausente && condicao != normalizar(&c.condicao) {
        return Some("condicao_diferente");
    }
    if c.min_avaliacoes > 0 && o.qtd_avaliacoes.is_some_and(|n| n < c.min_avaliacoes) {
        return Some("avaliacoes_insuficientes");
    }
    if c.obrigatorio
        .iter()
        .filter(|t| !t.trim().is_empty())
        .any(|t| {
            !cercado.contains(&format!(
                " {} ",
                normalizar_com_sinonimos(&texto_busca(t), &c.sinonimos)
            ))
        })
    {
        return Some("termos_obrigatorios");
    }
    if c.proibido.iter().filter(|t| !t.trim().is_empty()).any(|t| {
        cercado.contains(&format!(
            " {} ",
            normalizar_com_sinonimos(&texto_busca(t), &c.sinonimos)
        ))
    }) {
        return Some("termos_proibidos");
    }
    if !atributos_compativeis(&variantes(&c.termo), &variantes(&o.titulo))
        || !super::pipeline::consulta_compativel(&texto_busca(&c.termo), &titulo)
    {
        return Some("produto_ou_variante");
    }
    None
}

fn avisos_dados_ausentes(o: &OfertaNormalizada, c: &SearchConfig) -> Vec<String> {
    let condicao = normalizar(&o.condicao);
    let condicao_ausente = condicao.is_empty()
        || matches!(
            condicao.as_str(),
            "unknown" | "desconhecido" | "nao informado"
        );
    let mut avisos = Vec::new();
    if c.condicao != "any" && condicao_ausente {
        avisos.push("Condição não informada".into());
    }
    if c.min_avaliacoes > 0 && o.qtd_avaliacoes.is_none() {
        avisos.push(format!(
            "Avaliações não informadas (mínimo: {})",
            c.min_avaliacoes
        ));
    }
    avisos
}

fn mediana(valores: &[Decimal]) -> Option<Decimal> {
    let meio = valores.len() / 2;
    if valores.len().is_multiple_of(2) {
        valores[meio - 1]
            .checked_div(2.into())?
            .checked_add(valores[meio].checked_div(2.into())?)
    } else {
        Some(valores[meio])
    }
}
fn limite_iqr(precos: &[Decimal]) -> Option<Decimal> {
    if precos.len() < 4 {
        return None;
    }
    let mut v = precos.to_vec();
    v.sort();
    let meio = v.len() / 2;
    let q1 = mediana(&v[..meio])?;
    let q3 = mediana(&v[meio + usize::from(!v.len().is_multiple_of(2))..])?;
    q1.checked_sub(q3.checked_sub(q1)?.checked_mul(Decimal::new(15, 1))?)
}

pub fn ranquear_multi(
    busca: &str,
    config: &SearchConfig,
    coletado: ResultadoMultiLoja,
) -> Result<RankingMultiLoja, String> {
    config.validate()?;
    let total = coletado.ofertas.len();
    let mut excluidos = BTreeMap::new();
    let mut ids = HashSet::new();
    let ofertas = coletado
        .ofertas
        .into_iter()
        .filter(|o| {
            if !ids.insert(o.id.clone()) {
                return false;
            }
            if let Some(motivo) = motivo_exclusao(o, config) {
                *excluidos.entry(motivo.into()).or_default() += 1;
                false
            } else {
                true
            }
        })
        .collect::<Vec<_>>();
    let precos = ofertas
        .iter()
        .map(|o| o.preco_final_brl)
        .collect::<Vec<_>>();
    let min = precos.iter().min().copied().unwrap_or_default();
    let max = precos.iter().max().copied().unwrap_or_default();
    let limite = limite_iqr(&precos);
    let corpus = ofertas.iter().map(|o| o.titulo.clone()).collect::<Vec<_>>();
    let notas = ofertas.iter().filter_map(|o| o.nota).collect::<Vec<_>>();
    let media = if notas.is_empty() {
        0.0
    } else {
        notas.iter().sum::<f64>() / notas.len() as f64
    };
    let teto_vendas = ofertas
        .iter()
        .filter_map(|o| o.qtd_avaliacoes)
        .max()
        .unwrap_or(0);
    let votos = ofertas
        .iter()
        .map(|o| {
            o.qtd_avaliacoes
                .unwrap_or_else(|| o.vendas.unwrap_or(0).min(teto_vendas))
        })
        .collect::<Vec<_>>();
    let bayesianas = ofertas
        .iter()
        .zip(&votos)
        .map(|(o, &v)| bayesiana(o.nota.unwrap_or(media), v, media, 50.0))
        .collect::<Vec<_>>();
    let relevancias = ofertas
        .iter()
        .map(|o| {
            bm25_com_sinonimos(
                &config.termo,
                &o.titulo,
                &corpus,
                1.2,
                0.75,
                &config.sinonimos,
            )
        })
        .collect::<Vec<_>>();
    let notas_norm = min_max(&bayesianas);
    let rel_norm = min_max(&relevancias);
    let comparadas = ofertas
        .into_iter()
        .enumerate()
        .map(|(i, oferta)| {
            let preco_score = if max == min {
                1.0
            } else {
                // Nunca converter os valores monetários em float para ordenar ou filtrar.
                (max - oferta.preco_final_brl)
                    .checked_div(max - min)
                    .and_then(|v| v.to_f64())
                    .unwrap_or(0.0)
            };
            OfertaComparada {
                score: preco_score * config.pesos.preco
                    + notas_norm[i] * config.pesos.nota
                    + oferta.reputacao.unwrap_or(0.5) * config.pesos.vendedor
                    + rel_norm[i] * config.pesos.relevancia,
                nota_bayesiana: bayesianas[i],
                votos_confianca: votos[i],
                confianca_por_vendas: oferta.qtd_avaliacoes.is_none() && votos[i] > 0,
                relevancia: relevancias[i],
                suspeito: limite.is_some_and(|l| oferta.preco_final_brl < l),
                avisos: avisos_dados_ausentes(&oferta, config),
                oferta,
            }
        })
        .collect();
    let mut grupos = agrupar_multi(comparadas);
    let pareto = grupos
        .iter()
        .map(|a| {
            !grupos.iter().any(|b| {
                b.principal.oferta.preco_final_brl <= a.principal.oferta.preco_final_brl
                    && b.principal.nota_bayesiana >= a.principal.nota_bayesiana
                    && (b.principal.oferta.preco_final_brl < a.principal.oferta.preco_final_brl
                        || b.principal.nota_bayesiana > a.principal.nota_bayesiana)
            })
        })
        .collect::<Vec<_>>();
    for (g, p) in grupos.iter_mut().zip(pareto) {
        g.pareto = p;
    }
    grupos.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| {
                a.principal
                    .oferta
                    .preco_final_brl
                    .cmp(&b.principal.oferta.preco_final_brl)
            })
            .then_with(|| a.chave.cmp(&b.chave))
    });
    Ok(RankingMultiLoja {
        busca: busca.into(),
        executada_em: String::new(),
        demonstracao: false,
        anuncios_coletados: total,
        anuncios_filtrados: precos.len(),
        excluidos,
        produtos: grupos,
        status: coletado.status,
    })
}

#[cfg(test)]
#[path = "multiloja_tests.rs"]
mod tests;
