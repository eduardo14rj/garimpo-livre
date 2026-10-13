//! Parsers puros para os textos monetários e contagens exibidos pelas lojas.
use regex::Regex;
use rust_decimal::Decimal;
use std::sync::OnceLock;

pub fn preco_brl(texto: &str) -> Option<(Decimal, bool)> {
    static VALORES: OnceLock<Regex> = OnceLock::new();
    let valores = VALORES.get_or_init(|| {
        Regex::new(r"(?i)R\$\s*([0-9][0-9.]*(?:,[0-9]{1,2})?)")
            .expect("regex de preço brasileiro válida")
    });
    let capturas = valores
        .captures_iter(texto)
        .filter_map(|captura| decimal_brl(captura.get(1)?.as_str()))
        .collect::<Vec<_>>();
    let primeiro = *capturas.first()?;
    Some((primeiro, capturas.len() > 1))
}

fn decimal_brl(texto: &str) -> Option<Decimal> {
    let normalizado = texto.replace('.', "").replace(',', ".");
    normalizado
        .parse()
        .ok()
        .filter(|valor| *valor > Decimal::ZERO)
}

pub fn nota(texto: &str) -> Option<f64> {
    static VALOR: OnceLock<Regex> = OnceLock::new();
    let valor = VALOR.get_or_init(|| {
        Regex::new(r"(?i)([0-9]+(?:[,.][0-9]+)?)\s*(?:de\s*5|/\s*5)?(?:\s*estrelas?)?")
            .expect("regex de nota válida")
    });
    valor
        .captures(texto)?
        .get(1)?
        .as_str()
        .replace(',', ".")
        .parse::<f64>()
        .ok()
        .filter(|valor| valor.is_finite() && (0.0..=5.0).contains(valor))
}

pub fn contagem(texto: &str) -> Option<u32> {
    static VALOR: OnceLock<Regex> = OnceLock::new();
    let valor = VALOR.get_or_init(|| {
        Regex::new(r"(?i)([0-9][0-9.,]*)\s*(mil|k)?").expect("regex de contagem válida")
    });
    let captura = valor.captures(texto.trim())?;
    let mut numero = captura
        .get(1)?
        .as_str()
        .replace('.', "")
        .replace(',', ".")
        .parse::<f64>()
        .ok()?;
    if captura.get(2).is_some_and(|unidade| {
        unidade.as_str().eq_ignore_ascii_case("mil") || unidade.as_str().eq_ignore_ascii_case("k")
    }) {
        numero *= 1000.0;
    }
    if !numero.is_finite() || numero < 0.0 || numero > f64::from(u32::MAX) {
        return None;
    }
    Some(numero.floor() as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interpreta_precos_brl_e_faixas_sem_perder_centavos() {
        assert_eq!(
            preco_brl("R$ 1.234,56"),
            Some(("1234.56".parse().unwrap(), false))
        );
        assert_eq!(
            preco_brl("R$49,90"),
            Some(("49.90".parse().unwrap(), false))
        );
        assert_eq!(preco_brl("R$ 12"), Some((12.into(), false)));
        assert_eq!(
            preco_brl("R$ 10,50 - R$ 20,00"),
            Some(("10.50".parse().unwrap(), true))
        );
        assert_eq!(preco_brl("US$ 10,50"), None);
    }

    #[test]
    fn interpreta_notas_reviews_e_vendas_em_portugues() {
        assert_eq!(nota("4,5 de 5 estrelas"), Some(4.5));
        assert_eq!(contagem("(1.234)"), Some(1234));
        assert_eq!(contagem("1.000+ vendidos"), Some(1000));
        assert_eq!(contagem("5 mil vendidos"), Some(5000));
        assert_eq!(contagem("1,5 mil vendidos"), Some(1500));
        assert_eq!(nota("6 de 5 estrelas"), None);
    }
}
