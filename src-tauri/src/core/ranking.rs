use std::collections::{HashMap, HashSet};

use chrono::NaiveDate;

use crate::core::{tokenizar, tokenizar_com_sinonimos};

pub fn tukey_limite_inferior(values: &[f64]) -> Option<f64> {
    let mut sorted = values
        .iter()
        .copied()
        .filter(|value| value.is_finite())
        .collect::<Vec<_>>();
    if sorted.len() < 4 {
        return None;
    }
    sorted.sort_by(f64::total_cmp);
    let middle = sorted.len() / 2;
    let (lower, upper) = if sorted.len() % 2 == 0 {
        (&sorted[..middle], &sorted[middle..])
    } else {
        (&sorted[..middle], &sorted[middle + 1..])
    };
    let q1 = median(lower);
    let q3 = median(upper);
    Some(q1 - 1.5 * (q3 - q1))
}

pub fn outliers_iqr(values: &[f64]) -> Vec<bool> {
    let Some(limit) = tukey_limite_inferior(values) else {
        return vec![false; values.len()];
    };
    values.iter().map(|value| *value < limit).collect()
}

fn median(values: &[f64]) -> f64 {
    let middle = values.len() / 2;
    if values.len().is_multiple_of(2) {
        (values[middle - 1] + values[middle]) / 2.0
    } else {
        values[middle]
    }
}

pub fn bayesiana(nota: f64, votos: u32, media_busca: f64, peso_minimo: f64) -> f64 {
    let votes = f64::from(votos);
    (votes * nota + peso_minimo * media_busca) / (votes + peso_minimo)
}

/// Percentual e feedback são indicadores do vendedor, não reviews do produto.
pub fn reputacao_percentual(percentual: f64, amostra: u32) -> Option<f64> {
    (percentual.is_finite() && (0.0..=100.0).contains(&percentual))
        .then(|| bayesiana(percentual / 100.0, amostra, 0.5, 50.0))
}

/// Nota observada da loja; selo sozinho não inventa uma nota. Bônus não se somam.
pub fn reputacao_shopee(nota_loja: Option<f64>, oficial: bool, preferida: bool) -> Option<f64> {
    let nota = nota_loja.filter(|nota| nota.is_finite() && (0.0..=5.0).contains(nota))?;
    let bonus = if oficial {
        0.05
    } else if preferida {
        0.025
    } else {
        0.0
    };
    Some((nota / 5.0 + bonus).min(1.0))
}

#[cfg(test)]
mod shopee_badge_bonus_tests {
    use super::reputacao_shopee;

    #[test]
    fn badges_aplicam_bonus_autorizado_sem_soma_e_com_teto() {
        let oficial = reputacao_shopee(Some(4.0), true, false).unwrap();
        let preferida = reputacao_shopee(Some(4.0), false, true).unwrap();
        let ambas = reputacao_shopee(Some(4.0), true, true).unwrap();
        assert!((oficial - 0.85).abs() < 1e-12);
        assert!((preferida - 0.825).abs() < 1e-12);
        assert!((ambas - 0.85).abs() < 1e-12);
        assert_eq!(reputacao_shopee(Some(5.0), true, false), Some(1.0));
        assert_eq!(reputacao_shopee(None, true, false), None);
    }
}

pub fn reputacao_positiva(percentual_loja: Option<f64>) -> Option<f64> {
    percentual_loja
        .filter(|valor| valor.is_finite() && (0.0..=100.0).contains(valor))
        .map(|valor| valor / 100.0)
}

#[cfg(test)]
mod testes_reputacao_percentual {
    use super::*;
    #[test]
    fn vendedor_sem_amostra_neutro_e_amostra_pequena_nao_domina() {
        assert_eq!(reputacao_percentual(100.0, 0), Some(0.5));
        assert!((reputacao_percentual(100.0, 2).unwrap() - 27.0 / 52.0).abs() < 1e-12);
        assert!(
            reputacao_percentual(99.8, 1000).unwrap() > reputacao_percentual(100.0, 2).unwrap()
        );
        assert_eq!(reputacao_percentual(f64::NAN, 10), None);
        assert_eq!(reputacao_percentual(101.0, 10), None);
    }
}

pub fn score_wilson(positivos: u32, total: u32, z: f64) -> f64 {
    if total == 0 {
        return 0.0;
    }
    let n = f64::from(total);
    let p = f64::from(positivos.min(total)) / n;
    let z2 = z * z;
    let denominator = 1.0 + z2 / n;
    let center = p + z2 / (2.0 * n);
    let margin = z * ((p * (1.0 - p) / n + z2 / (4.0 * n * n)).sqrt());
    ((center - margin) / denominator).clamp(0.0, 1.0)
}

pub fn min_max(values: &[f64]) -> Vec<f64> {
    if values.is_empty() {
        return vec![];
    }
    let min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if (max - min).abs() <= f64::EPSILON {
        return vec![1.0; values.len()];
    }
    values
        .iter()
        .map(|value| (value - min) / (max - min))
        .collect()
}

pub fn indice_invertido<'a>(
    items: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> HashMap<String, Vec<String>> {
    let mut index: HashMap<String, Vec<String>> = HashMap::new();
    for (id, title) in items {
        for token in tokenizar(title) {
            let ids = index.entry(token).or_default();
            if ids.last().is_none_or(|last| last != id) {
                ids.push(id.to_owned());
            }
        }
    }
    index
}

pub fn bm25(termo: &str, titulo: &str, corpus: &[String], k1: f64, b: f64) -> f64 {
    bm25_com_sinonimos(termo, titulo, corpus, k1, b, &Default::default())
}

pub fn bm25_com_sinonimos(
    termo: &str,
    titulo: &str,
    corpus: &[String],
    k1: f64,
    b: f64,
    sinonimos: &std::collections::BTreeMap<String, Vec<String>>,
) -> f64 {
    let query_tokens = tokenizar_com_sinonimos(termo, sinonimos);
    if query_tokens.is_empty() || corpus.is_empty() {
        return 0.0;
    }
    let documents = corpus
        .iter()
        .map(|document| tokenizar_com_sinonimos(document, sinonimos))
        .collect::<Vec<_>>();
    let average_length =
        documents.iter().map(Vec::len).sum::<usize>() as f64 / documents.len() as f64;
    let target = tokenizar_com_sinonimos(titulo, sinonimos);
    let document_frequency = |term: &str| {
        documents
            .iter()
            .filter(|document| document.iter().any(|token| token == term))
            .count() as f64
    };
    let mut score = 0.0;
    for term in query_tokens {
        let frequency = target.iter().filter(|token| *token == &term).count() as f64;
        if frequency == 0.0 {
            continue;
        }
        let df = document_frequency(&term);
        let idf = ((documents.len() as f64 - df + 0.5) / (df + 0.5) + 1.0).ln();
        let normalization =
            frequency + k1 * (1.0 - b + b * target.len() as f64 / average_length.max(1.0));
        score += idf * frequency * (k1 + 1.0) / normalization;
    }
    score
}

pub fn jaccard(left: &str, right: &str) -> f64 {
    let a = tokenizar(left).into_iter().collect::<HashSet<_>>();
    let b = tokenizar(right).into_iter().collect::<HashSet<_>>();
    let union = a.union(&b).count();
    if union == 0 {
        1.0
    } else {
        a.intersection(&b).count() as f64 / union as f64
    }
}

pub fn levenshtein_similaridade(left: &str, right: &str) -> f64 {
    let a = crate::core::normalizar(left).replace(' ', "");
    let b = crate::core::normalizar(right).replace(' ', "");
    let longest = a.chars().count().max(b.chars().count());
    if longest == 0 {
        1.0
    } else {
        1.0 - strsim::levenshtein(&a, &b) as f64 / longest as f64
    }
}

pub fn fronteira_pareto(points: &[(f64, f64)]) -> Vec<bool> {
    points
        .iter()
        .enumerate()
        .map(|(index, &(price, rating))| {
            !points
                .iter()
                .enumerate()
                .any(|(other_index, &(other_price, other_rating))| {
                    index != other_index
                        && other_price <= price
                        && other_rating >= rating
                        && (other_price < price || other_rating > rating)
                })
        })
        .collect()
}

pub fn media_movel(points: &[(NaiveDate, f64)], days: usize) -> Option<f64> {
    let values = points
        .iter()
        .rev()
        .take(days)
        .map(|(_, price)| *price)
        .collect::<Vec<_>>();
    if values.is_empty() {
        None
    } else {
        Some(values.iter().sum::<f64>() / values.len() as f64)
    }
}

pub fn promo_suspeita(
    current_price: f64,
    current_original: Option<f64>,
    previous_originals: &[f64],
) -> bool {
    let Some(current) = current_original else {
        return false;
    };
    current_price < current
        && previous_originals
            .iter()
            .copied()
            .filter(|value| *value > 0.0)
            .min_by(f64::total_cmp)
            .is_some_and(|previous| current > previous * 1.1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iqr_usa_mediana_das_metades_excluindo_mediana() {
        let values = [89.0, 249.0, 259.0, 270.0, 279.0, 299.0, 310.0];
        assert_eq!(tukey_limite_inferior(&values), Some(174.0));
        assert_eq!(
            outliers_iqr(&values),
            [true, false, false, false, false, false, false]
        );
    }

    #[test]
    fn media_bayesiana_puxa_amostras_pequenas_para_a_media() {
        assert!((bayesiana(5.0, 2, 4.3, 50.0) - 4.33).abs() < 0.01);
        assert!((bayesiana(4.7, 3000, 4.3, 50.0) - 4.69).abs() < 0.01);
    }

    #[test]
    fn wilson_e_minmax_cobrem_gabaritos() {
        assert!((score_wilson(8, 10, 1.96) - 0.490).abs() < 0.001);
        assert_eq!(min_max(&[100.0, 200.0, 300.0]), [0.0, 0.5, 1.0]);
        assert_eq!(min_max(&[50.0, 50.0]), [1.0, 1.0]);
    }

    #[test]
    fn index_bm25_jaccard_e_levenshtein_sao_deterministic() {
        let index = indice_invertido([("a", "Fone JBL ANC"), ("b", "capa para JBL")]);
        assert_eq!(index["anc"], ["a"]);
        let corpus = vec![
            "fone jbl tune 760nc anc".to_owned(),
            "capa para fone jbl".to_owned(),
        ];
        assert!(
            bm25("fone anc", &corpus[0], &corpus, 1.2, 0.75)
                > bm25("fone anc", &corpus[1], &corpus, 1.2, 0.75)
        );
        assert!(
            (jaccard("jbl tune 520bt fone", "fone jbl 520bt preto") - 0.6).abs() < f64::EPSILON
        );
        assert_eq!(levenshtein_similaridade("520bt", "520 bt"), 1.0);
    }

    #[test]
    fn bm25_aplica_sinonimos_fornecidos_sem_estado_global() {
        let synonyms = std::collections::BTreeMap::from([(
            "capa".to_owned(),
            vec!["case".to_owned(), "cover".to_owned()],
        )]);
        let corpus = vec!["Case transparente para Galaxy".to_owned()];
        assert!(bm25_com_sinonimos("capa galaxy", &corpus[0], &corpus, 1.2, 0.75, &synonyms) > 0.0);
    }

    #[test]
    fn fronteira_pareto_mantem_tradeoffs() {
        assert_eq!(
            fronteira_pareto(&[(250.0, 4.7), (280.0, 4.6), (180.0, 4.3), (350.0, 4.9)]),
            [true, false, true, true]
        );
    }

    #[test]
    fn medias_e_promocao_suspeita_usam_series_fornecidas() {
        let day = NaiveDate::from_ymd_opt(2026, 10, 1).unwrap();
        assert_eq!(
            media_movel(
                &[
                    (day, 100.0),
                    (day.succ_opt().unwrap(), 110.0),
                    (day.succ_opt().unwrap().succ_opt().unwrap(), 120.0)
                ],
                2
            ),
            Some(115.0)
        );
        assert!(promo_suspeita(90.0, Some(120.0), &[100.0, 105.0]));
        assert!(!promo_suspeita(120.0, Some(120.0), &[100.0]));
    }
}
