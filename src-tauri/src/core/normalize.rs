use std::sync::OnceLock;

use regex::Regex;
use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};

use crate::models::AtributosProduto;

pub fn normalizar(texto: &str) -> String {
    let texto = normalizar_variantes_modelo(texto);
    let decomposed = texto
        .nfd()
        .filter(|character| !is_combining_mark(*character))
        .collect::<String>();
    let lower = decomposed.to_lowercase();
    let cleaned = lower
        .chars()
        .map(|character| {
            if character.is_alphanumeric() {
                character
            } else {
                ' '
            }
        })
        .collect::<String>();
    cleaned.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn normalizar_variantes_modelo(texto: &str) -> String {
    static SUFIXO_MAIS: OnceLock<Regex> = OnceLock::new();
    static PALAVRA_PLUS: OnceLock<Regex> = OnceLock::new();
    let sufixo_mais = SUFIXO_MAIS.get_or_init(|| {
        Regex::new(r"(?i)([a-z]*\d+)\s*\+(?:\s|$)").expect("regex de modelo com + válida")
    });
    let palavra_plus = PALAVRA_PLUS
        .get_or_init(|| Regex::new(r"(?i)([a-z]*\d+)plus\b").expect("regex de modelo plus válida"));
    let texto = sufixo_mais.replace_all(texto, "$1 plus ");
    palavra_plus.replace_all(&texto, "$1 plus").into_owned()
}

pub fn tokenizar(texto: &str) -> Vec<String> {
    tokenizar_com_sinonimos(texto, &std::collections::BTreeMap::new())
}

pub fn tokenizar_com_sinonimos(
    texto: &str,
    sinonimos: &std::collections::BTreeMap<String, Vec<String>>,
) -> Vec<String> {
    const STOPWORDS: &[&str] = &["de", "da", "do", "com", "para", "e", "o", "a"];
    normalizar(texto)
        .split_whitespace()
        .filter(|token| !STOPWORDS.contains(token))
        .map(|token| {
            sinonimos
                .iter()
                .find(|(canonical, alternatives)| {
                    normalizar(canonical) == token
                        || alternatives
                            .iter()
                            .any(|alternative| normalizar(alternative) == token)
                })
                .map(|(canonical, _)| normalizar(canonical))
                .unwrap_or_else(|| match token {
                    "capinha" | "case" | "cover" | "capas" => "capa".to_owned(),
                    value => value.to_owned(),
                })
        })
        .collect()
}

pub fn normalizar_com_sinonimos(
    texto: &str,
    sinonimos: &std::collections::BTreeMap<String, Vec<String>>,
) -> String {
    tokenizar_com_sinonimos(texto, sinonimos).join(" ")
}

pub fn extrair_atributos(texto: &str) -> AtributosProduto {
    static CAPACITY: OnceLock<Regex> = OnceLock::new();
    static VOLTAGE: OnceLock<Regex> = OnceLock::new();
    static BLUETOOTH: OnceLock<Regex> = OnceLock::new();
    let capacity =
        CAPACITY.get_or_init(|| Regex::new(r"(?i)\b(\d+(?:[.,]\d+)?)\s*(gb|tb)\b").unwrap());
    let voltage = VOLTAGE.get_or_init(|| Regex::new(r"(?i)\b(110|127|220)\s*v\b").unwrap());
    let bluetooth = BLUETOOTH
        .get_or_init(|| Regex::new(r"(?i)\b(?:bluetooth\s*|bt\s*)(\d+(?:\.\d+)?)\b").unwrap());
    let capacity_match = capacity.captures(texto);
    let capacity_value = capacity_match
        .as_ref()
        .and_then(|captures| captures.get(1))
        .and_then(|value| value.as_str().replace(',', ".").parse().ok());
    let unit = capacity_match
        .as_ref()
        .and_then(|captures| captures.get(2))
        .map(|value| value.as_str().to_lowercase());
    AtributosProduto {
        capacidade: capacity_value,
        unidade: unit,
        voltagem: voltage
            .captures(texto)
            .and_then(|captures| captures.get(1))
            .and_then(|value| value.as_str().parse().ok()),
        bluetooth: bluetooth
            .captures(texto)
            .and_then(|captures| captures.get(1))
            .map(|value| value.as_str().to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normaliza_acentos_pontuacao_e_espacos() {
        assert_eq!(
            normalizar("Fone Cancelamento Ruído!!"),
            "fone cancelamento ruido"
        );
        assert_eq!(normalizar("  ANC—fone   sem fio "), "anc fone sem fio");
    }

    #[test]
    fn remove_stopwords_sem_apagar_termos_utiles() {
        assert_eq!(
            tokenizar("fone de ouvido com anc"),
            ["fone", "ouvido", "anc"]
        );
    }

    #[test]
    fn normaliza_mais_e_sinonimos_de_modelo_ou_acessorio() {
        for texto in ["Capa Galaxy S25+ Silicone", "Capa Galaxy S25 + Silicone"] {
            assert_eq!(
                tokenizar(texto),
                ["capa", "galaxy", "s25", "plus", "silicone"]
            );
        }
        assert_eq!(tokenizar("Capinha S25plus"), ["capa", "s25", "plus"]);
        for termo in ["capa", "capinha", "case", "cover", "capas"] {
            assert_eq!(tokenizar(termo), ["capa"]);
        }
    }

    #[test]
    fn sinonimos_configurados_mapeiam_para_o_canonico() {
        let synonyms = std::collections::BTreeMap::from([(
            "fone".to_owned(),
            vec!["headphone".to_owned(), "headset".to_owned()],
        )]);
        assert_eq!(
            tokenizar_com_sinonimos("Headphone bluetooth", &synonyms),
            ["fone", "bluetooth"]
        );
        assert_eq!(
            normalizar_com_sinonimos("Headset sem fio", &synonyms),
            "fone sem fio"
        );
    }

    #[test]
    fn extrai_capacidade_voltagem_e_bluetooth() {
        let attributes = extrair_atributos("SSD 1 TB Kingston 220V Bluetooth 5.3");
        assert_eq!(attributes.capacidade, Some(1.0));
        assert_eq!(attributes.unidade.as_deref(), Some("tb"));
        assert_eq!(attributes.voltagem, Some(220));
        assert_eq!(attributes.bluetooth.as_deref(), Some("5.3"));
        assert_eq!(
            extrair_atributos("SSD 128GB").unidade.as_deref(),
            Some("gb")
        );
    }
}
