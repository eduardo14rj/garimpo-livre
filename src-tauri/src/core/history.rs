use chrono::{Days, NaiveDate};

use crate::models::{HistoricoProduto, PontoHistorico};

use super::promo_suspeita;

pub fn calcular_historico(
    product_id: &str,
    points: &[PontoHistorico],
    hoje: NaiveDate,
) -> HistoricoProduto {
    let mut ordered = points
        .iter()
        .filter_map(|point| {
            NaiveDate::parse_from_str(&point.data, "%Y-%m-%d")
                .ok()
                .map(|date| (date, point))
        })
        .collect::<Vec<_>>();
    ordered.sort_by_key(|(date, _)| *date);

    let minimum_date = hoje.checked_sub_days(Days::new(29));
    let window_30 = ordered
        .iter()
        .filter(|(date, _)| minimum_date.is_none_or(|start| *date >= start) && *date <= hoje)
        .map(|(_, point)| point.preco)
        .collect::<Vec<_>>();
    let minima_30d = window_30.into_iter().min_by(f64::total_cmp);

    let moving_start = hoje.checked_sub_days(Days::new(6));
    let last_7 = ordered
        .iter()
        .filter(|(date, _)| moving_start.is_none_or(|start| *date >= start) && *date <= hoje)
        .map(|(_, point)| point.preco)
        .collect::<Vec<_>>();
    let media_movel_7d =
        (!last_7.is_empty()).then(|| last_7.iter().sum::<f64>() / last_7.len() as f64);

    let current = ordered.last().copied();
    let first_date = ordered.first().map(|(date, _)| *date);
    let unique_days = ordered
        .iter()
        .map(|(date, _)| *date)
        .collect::<std::collections::BTreeSet<_>>();
    let enough_history = first_date
        .zip(current.map(|(date, _)| date))
        .is_some_and(|(first, last)| last.signed_duration_since(first).num_days() >= 6)
        && unique_days.len() >= 7;
    let alerta_minima = enough_history
        && current
            .is_some_and(|(_, point)| minima_30d.is_some_and(|minimum| point.preco <= minimum));

    let promo_suspeita = current.is_some_and(|(date, point)| {
        let start = date.checked_sub_days(Days::new(7));
        let previous_originals = ordered
            .iter()
            .filter(|(previous_date, _)| {
                start.is_none_or(|first_day| *previous_date >= first_day) && *previous_date < date
            })
            .filter_map(|(_, previous)| previous.preco_original)
            .collect::<Vec<_>>();
        promo_suspeita(point.preco, point.preco_original, &previous_originals)
    });

    HistoricoProduto {
        produto_id: product_id.to_owned(),
        pontos: points.to_vec(),
        minima_30d,
        media_movel_7d,
        alerta_minima,
        promo_suspeita,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(day: u32, price: f64, original: Option<f64>) -> PontoHistorico {
        PontoHistorico {
            data: format!("2026-10-{day:02}"),
            preco: price,
            preco_original: original,
            nota: Some(4.5),
            qtd_avaliacoes: 80,
        }
    }

    #[test]
    fn calcula_minima_media_movel_e_alerta_com_sete_dias() {
        let points = (1..=7)
            .map(|day| point(day, 160.0 - day as f64 * 10.0, Some(100.0)))
            .collect::<Vec<_>>();
        let history = calcular_historico(
            "MLB1",
            &points,
            NaiveDate::from_ymd_opt(2026, 10, 7).unwrap(),
        );
        assert_eq!(history.minima_30d, Some(90.0));
        assert_eq!(history.media_movel_7d, Some(120.0));
        assert!(history.alerta_minima);
        assert!(!history.promo_suspeita);
    }

    #[test]
    fn nao_alerta_antes_de_sete_dias_e_identifica_promocao_suspeita() {
        let short = vec![point(1, 100.0, Some(100.0)), point(2, 95.0, Some(100.0))];
        let history = calcular_historico(
            "MLB2",
            &short,
            NaiveDate::from_ymd_opt(2026, 10, 2).unwrap(),
        );
        assert!(!history.alerta_minima);

        let mut week = (1..=7)
            .map(|day| point(day, 120.0, Some(100.0)))
            .collect::<Vec<_>>();
        week[6].preco = 90.0;
        week[6].preco_original = Some(120.0);
        let history =
            calcular_historico("MLB2", &week, NaiveDate::from_ymd_opt(2026, 10, 7).unwrap());
        assert!(history.promo_suspeita);
    }
}
