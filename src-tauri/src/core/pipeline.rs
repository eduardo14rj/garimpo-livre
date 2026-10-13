use std::collections::HashMap;

use regex::Regex;

use crate::{
    core::{
        bayesiana, bm25_com_sinonimos, extrair_atributos, fronteira_pareto, jaccard, min_max,
        normalizar, normalizar_com_sinonimos, outliers_iqr, tokenizar,
    },
    models::{AnuncioBruto, GrupoRanking, OfertaRanking, ResultadoBusca, SearchConfig},
};

pub fn filtrar_anuncios(anuncios: &[AnuncioBruto], config: &SearchConfig) -> Vec<AnuncioBruto> {
    let required = config
        .obrigatorio
        .iter()
        .map(|term| normalizar_com_sinonimos(&expand_model_variants(term), &config.sinonimos))
        .filter(|term| !term.is_empty())
        .collect::<Vec<_>>();
    let forbidden = config
        .proibido
        .iter()
        .map(|term| normalizar_com_sinonimos(&expand_model_variants(term), &config.sinonimos))
        .filter(|term| !term.is_empty())
        .collect::<Vec<_>>();
    let condition = normalizar(&config.condicao);
    anuncios
        .iter()
        .filter(|ad| {
            let title =
                normalizar_com_sinonimos(&expand_model_variants(&ad.titulo), &config.sinonimos);
            let padded = format!(" {title} ");
            let has_forbidden = forbidden
                .iter()
                .any(|term| filtro_corresponde(&padded, term));
            let has_required = required
                .iter()
                .all(|term| filtro_corresponde(&padded, term));
            consulta_compativel(&config.termo, &ad.titulo)
                && !has_forbidden
                && has_required
                && config.preco_max.is_none_or(|maximum| {
                    ad.preco
                        .to_string()
                        .parse::<rust_decimal::Decimal>()
                        .is_ok_and(|preco| preco <= maximum)
                })
                && (condition == "any" || normalizar(&ad.condicao) == condition)
                && ad.qtd_avaliacoes >= config.min_avaliacoes
        })
        .cloned()
        .collect()
}

fn filtro_corresponde(padded_title: &str, term: &str) -> bool {
    padded_title.contains(&format!(" {term} "))
}

pub(super) fn expand_model_variants(value: &str) -> String {
    const VARIANTS: &[&str] = &["ultra", "plus", "max", "pro", "fe"];
    normalizar(value)
        .split_whitespace()
        .map(|token| {
            for variant in VARIANTS {
                if let Some(model) = token.strip_suffix(variant)
                    && model.chars().any(|character| character.is_ascii_digit())
                {
                    return format!("{model} {variant}");
                }
            }
            token.to_owned()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn executar_pipeline(
    busca: &str,
    config: &SearchConfig,
    anuncios: Vec<AnuncioBruto>,
) -> Result<ResultadoBusca, String> {
    config.validate()?;
    let collected = anuncios.len();
    let filtered = filtrar_anuncios(&anuncios, config);
    if filtered.is_empty() {
        return Ok(ResultadoBusca {
            busca: busca.to_owned(),
            fonte: String::new(),
            executada_em: String::new(),
            anuncios_coletados: collected,
            anuncios_filtrados: 0,
            produtos: Vec::new(),
            degradado: false,
            tentativas: Vec::new(),
            cache_origem: None,
        });
    }

    let prices = filtered.iter().map(|ad| ad.preco).collect::<Vec<_>>();
    let suspicious = outliers_iqr(&prices);
    let ratings = filtered.iter().filter_map(|ad| ad.nota).collect::<Vec<_>>();
    let search_average = if ratings.is_empty() {
        0.0
    } else {
        ratings.iter().sum::<f64>() / ratings.len() as f64
    };
    let corpus = filtered
        .iter()
        .map(|ad| ad.titulo.clone())
        .collect::<Vec<_>>();
    let relevance = filtered
        .iter()
        .map(|ad| {
            bm25_com_sinonimos(
                &config.termo,
                &ad.titulo,
                &corpus,
                1.2,
                0.75,
                &config.sinonimos,
            )
        })
        .collect::<Vec<_>>();
    let bayesian = filtered
        .iter()
        .map(|ad| {
            bayesiana(
                ad.nota.unwrap_or(search_average),
                ad.qtd_avaliacoes,
                search_average,
                50.0,
            )
        })
        .collect::<Vec<_>>();
    let normalized_prices = min_max(&prices);
    let same_price = prices
        .iter()
        .all(|price| (*price - prices[0]).abs() <= f64::EPSILON);
    let normalized_ratings = min_max(&bayesian);
    let sellers = filtered
        .iter()
        .map(|ad| ad.vendedor_score)
        .collect::<Vec<_>>();
    let normalized_sellers = min_max(&sellers);
    let normalized_relevance = min_max(&relevance);

    let offers = filtered
        .into_iter()
        .enumerate()
        .map(|(index, ad)| {
            let title = normalizar(&ad.titulo);
            let price_score = if same_price {
                1.0
            } else {
                1.0 - normalized_prices[index]
            };
            let score = price_score * config.pesos.preco
                + normalized_ratings[index] * config.pesos.nota
                + normalized_sellers[index] * config.pesos.vendedor
                + normalized_relevance[index] * config.pesos.relevancia;
            OfertaRanking {
                atributos: extrair_atributos(&ad.titulo),
                anuncio: ad,
                titulo_normalizado: title,
                nota_bayesiana: bayesian[index],
                relevancia: relevance[index],
                suspeito: suspicious[index],
                score,
            }
        })
        .collect::<Vec<_>>();

    let mut groups = agrupar_duplicados(offers);
    let points = groups
        .iter()
        .map(|group| {
            (
                group.principal.anuncio.preco,
                group.principal.nota_bayesiana,
            )
        })
        .collect::<Vec<_>>();
    let pareto = fronteira_pareto(&points);
    for (group, on_frontier) in groups.iter_mut().zip(pareto) {
        group.pareto = on_frontier;
    }
    groups.sort_by(|left, right| {
        right.score.total_cmp(&left.score).then_with(|| {
            left.principal
                .anuncio
                .preco
                .total_cmp(&right.principal.anuncio.preco)
        })
    });

    Ok(ResultadoBusca {
        busca: busca.to_owned(),
        fonte: String::new(),
        executada_em: String::new(),
        anuncios_coletados: collected,
        anuncios_filtrados: groups.iter().map(|group| group.ofertas.len()).sum(),
        produtos: groups,
        degradado: false,
        tentativas: Vec::new(),
        cache_origem: None,
    })
}

pub(super) fn consulta_compativel(termo: &str, titulo: &str) -> bool {
    let query = tokenizar(&expand_model_variants(termo));
    let title = tokenizar(&expand_model_variants(titulo));
    let mentions = |token: &str| title.iter().any(|word| word == token);
    let models_match = query
        .iter()
        .filter(|token| {
            token.chars().any(|c| c.is_ascii_digit()) && token.chars().any(|c| c.is_alphabetic())
        })
        .all(|model| {
            let family = model
                .chars()
                .take_while(|character| character.is_alphabetic())
                .collect::<String>();
            if family.is_empty() {
                return mentions(model);
            }
            // O último modelo inclui a variante selecionada que o JSON-LD acrescenta ao título genérico.
            let selected = title.iter().rposition(|token| {
                token.starts_with(&family)
                    && token.chars().any(|character| character.is_ascii_digit())
            });
            selected.is_some_and(|index| {
                title[index] == *model
                    && query
                        .windows(2)
                        .filter(|pair| {
                            pair[0] == *model
                                && ["plus", "ultra", "pro", "max", "fe"].contains(&pair[1].as_str())
                        })
                        .all(|pair| title.get(index + 1) == Some(&pair[1]))
            })
        });
    let variants_match = ["plus", "ultra", "pro", "max"]
        .iter()
        .filter(|variant| query.iter().any(|token| token == **variant))
        .all(|variant| mentions(variant));
    let busca_plus = query.iter().any(|token| token == "plus");
    let variante_conflitante = ["ultra", "edge", "fe"]
        .iter()
        .any(|variant| title.iter().any(|token| token == variant));
    let lista_modelos = titulo.contains('/') && mentions("plus");
    let case_match = !query
        .iter()
        .any(|token| token == "capa" || token == "capinha")
        || ["capa", "capinha", "case"]
            .iter()
            .any(|word| mentions(word));
    models_match
        && variants_match
        && case_match
        && (!busca_plus || !variante_conflitante || lista_modelos)
        && query.iter().any(|token| mentions(token))
}

fn agrupar_duplicados(offers: Vec<OfertaRanking>) -> Vec<GrupoRanking> {
    let mut parents = (0..offers.len()).collect::<Vec<_>>();
    for left in 0..offers.len() {
        for right in left + 1..offers.len() {
            let left_model = model_key(&offers[left].anuncio);
            let right_model = model_key(&offers[right].anuncio);
            let same_model = left_model
                .as_ref()
                .zip(right_model.as_ref())
                .is_some_and(|(a, b)| a == b);
            if same_model
                || jaccard(
                    &offers[left].titulo_normalizado,
                    &offers[right].titulo_normalizado,
                ) >= 0.8
            {
                union(&mut parents, left, right);
            }
        }
    }

    let mut grouped: HashMap<usize, Vec<OfertaRanking>> = HashMap::new();
    for (index, offer) in offers.into_iter().enumerate() {
        let root = find(&mut parents, index);
        grouped.entry(root).or_default().push(offer);
    }
    let mut groups = grouped
        .into_values()
        .map(|mut offers| {
            offers.sort_by(|left, right| left.anuncio.preco.total_cmp(&right.anuncio.preco));
            let principal = offers[0].clone();
            GrupoRanking {
                score: principal.score,
                principal,
                ofertas: offers,
                pareto: false,
            }
        })
        .collect::<Vec<_>>();
    groups.sort_by(|left, right| left.principal.anuncio.id.cmp(&right.principal.anuncio.id));
    groups
}

fn model_key(ad: &AnuncioBruto) -> Option<String> {
    if let Some(model) = &ad.modelo {
        let normalized = normalizar(model);
        if !normalized.is_empty() {
            return Some(normalized);
        }
    }
    // Em acessórios, o modelo no título é do aparelho compatível, não da capa.
    if tokenizar(&ad.titulo)
        .iter()
        .any(|token| ["capa", "capinha", "case"].contains(&token.as_str()))
    {
        return None;
    }
    static MODEL: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let expression =
        MODEL.get_or_init(|| Regex::new(r"\b[a-z]{0,8}\s?\d{2,6}[a-z]{0,4}\b").unwrap());
    expression
        .find(&normalizar(&ad.titulo))
        .map(|found| found.as_str().replace(' ', ""))
}

fn find(parents: &mut [usize], index: usize) -> usize {
    if parents[index] != index {
        parents[index] = find(parents, parents[index]);
    }
    parents[index]
}

fn union(parents: &mut [usize], left: usize, right: usize) {
    let left_root = find(parents, left);
    let right_root = find(parents, right);
    parents[right_root] = left_root;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{coleta::parse_listagem_html, models::SearchWeights};

    #[test]
    fn consulta_de_capa_preserva_produto_modelo_e_variante() {
        let query = "capa de celular samsung s25 plus";
        assert!(!consulta_compativel(query, "Fone Bluetooth ANC"));
        assert!(!consulta_compativel(query, "Samsung Galaxy S25 Plus 256GB"));
        assert!(!consulta_compativel(query, "Capa Samsung S25 Ultra"));
        assert!(!consulta_compativel(query, "Capa Samsung S25"));
        assert!(!consulta_compativel(query, "Capa S23 Plus S24 S25 Ultra"));
        assert!(!consulta_compativel(
            query,
            "Capa S25/s25 Plus/s25ultra Transparente S24 Plus"
        ));
        assert!(consulta_compativel(
            query,
            "Capa Samsung S25 Ultra / S25+ / S25 Plus"
        ));
        assert!(consulta_compativel(
            query,
            "Capa S25/s25 Plus/s25ultra Transparente S25 Plus"
        ));
        assert!(consulta_compativel(
            query,
            "Capinha para Galaxy S25 Plus anti impacto"
        ));
        assert!(consulta_compativel(
            query,
            "Capa Samsung Galaxy S25+ transparente"
        ));
    }

    #[test]
    fn filtro_obrigatorio_s25plus_aceita_titulo_com_s25_plus() {
        let search = SearchConfig {
            termo: "capa samsung s25 plus".into(),
            preco_max: Some(50.into()),
            condicao: "new".into(),
            obrigatorio: vec!["s25plus".into()],
            proibido: vec!["usado".into()],
            min_avaliacoes: 8,
            pesos: SearchWeights::default(),
            ..Default::default()
        };
        let mut offer = parse_listagem_html(include_str!("../../tests/fixtures/listagem.html"))
            .unwrap()
            .remove(0);
        offer.id = "MLB-S25-PLUS".into();
        offer.titulo = "Capa Samsung Galaxy S25 Plus transparente".into();
        offer.preco = 39.9;
        offer.condicao = "new".into();
        offer.nota = Some(4.7);
        offer.qtd_avaliacoes = 30;
        let mut compact_variant = offer.clone();
        compact_variant.id = "MLB-S25PLUS".into();
        compact_variant.titulo = "Capa Samsung Galaxy S25Plus transparente".into();
        let mut plus_symbol = offer.clone();
        plus_symbol.id = "MLB-S25-SYMBOL".into();
        plus_symbol.titulo = "Capa Samsung Galaxy S25+ transparente".into();

        assert_eq!(
            filtrar_anuncios(&[offer, compact_variant, plus_symbol], &search).len(),
            3
        );
    }

    #[test]
    fn capas_diferentes_nao_sao_duplicadas_pelo_modelo_do_celular() {
        let mut ads =
            parse_listagem_html(include_str!("../../tests/fixtures/listagem.html")).unwrap();
        let mut left = ads.remove(0);
        left.titulo = "Capa Samsung S25 Plus silicone transparente".into();
        left.modelo = None;
        let mut right = left.clone();
        right.id = "segunda-capa".into();
        right.titulo = "Case Galaxy S25 Plus armadura magnetica".into();
        assert!(model_key(&left).is_none());
        let search = SearchConfig {
            termo: "capa samsung s25 plus".into(),
            min_avaliacoes: 0,
            ..SearchConfig::default()
        };
        let result = executar_pipeline("capas", &search, vec![left, right]).unwrap();
        assert_eq!(result.produtos.len(), 2);
    }

    fn config() -> SearchConfig {
        SearchConfig {
            termo: "fone bluetooth anc".into(),
            preco_max: Some(300.into()),
            condicao: "new".into(),
            obrigatorio: vec!["anc".into()],
            proibido: vec![
                "replica".into(),
                "similar".into(),
                "1a linha".into(),
                "capa".into(),
            ],
            min_avaliacoes: 10,
            pesos: SearchWeights::default(),
            ..Default::default()
        }
    }

    #[test]
    fn filtros_removem_replicas_acessorios_acima_do_limite_sem_reviews_e_sem_termo() {
        let ads = parse_listagem_html(include_str!("../../tests/fixtures/listagem.html")).unwrap();
        let filtered = filtrar_anuncios(&ads, &config());
        assert!(!filtered.iter().any(|ad| ad.id == "MLB1006"
            || ad.id == "MLB1007"
            || ad.id == "MLB1009"
            || ad.id == "MLB1012"
            || ad.id == "MLB1013"
            || ad.id == "MLB1015"));
        assert!(
            filtered
                .iter()
                .all(|ad| ad.preco <= 300.0 && ad.titulo.to_lowercase().contains("anc"))
        );
        assert_eq!(filtered.len(), 9);
    }

    #[test]
    fn pesos_invalidos_falham_com_mensagem_clara() {
        let mut search = config();
        search.pesos = SearchWeights {
            preco: 0.4,
            nota: 0.3,
            vendedor: 0.1,
            relevancia: 0.1,
        };
        assert!(search.validate().unwrap_err().contains("soma dos pesos"));
    }

    #[test]
    fn pipeline_mantem_outlier_marca_duplicados_e_define_topo_reproduzivel() {
        let ads = parse_listagem_html(include_str!("../../tests/fixtures/listagem.html")).unwrap();
        let result = executar_pipeline("fone-anc", &config(), ads).unwrap();
        assert_eq!(result.anuncios_coletados, 15);
        assert_eq!(result.anuncios_filtrados, 9);
        let outlier = result
            .produtos
            .iter()
            .flat_map(|group| &group.ofertas)
            .find(|offer| offer.anuncio.id == "MLB1008")
            .unwrap();
        assert!(outlier.suspeito);
        let duplicate = result
            .produtos
            .iter()
            .find(|group| {
                group
                    .ofertas
                    .iter()
                    .any(|offer| offer.anuncio.id == "MLB1002")
            })
            .unwrap();
        assert_eq!(duplicate.ofertas.len(), 3);
        assert_eq!(duplicate.principal.anuncio.id, "MLB1001");
        assert_eq!(result.produtos[0].principal.anuncio.id, "MLB1008");
    }
}
