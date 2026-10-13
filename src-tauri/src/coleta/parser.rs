use scraper::{Html, Selector};
use serde_json::Value;

use crate::models::{AnuncioBruto, ProdutoBruto};

#[derive(Debug, thiserror::Error)]
pub enum ParserErro {
    #[error("nenhum anúncio reconhecido na listagem")]
    SemAnuncios,
    #[error("produto JSON-LD ausente ou inválido")]
    ProdutoAusente,
    #[error("preço ausente ou inválido")]
    PrecoInvalido,
}

pub fn parse_listagem_html(html: &str) -> Result<Vec<AnuncioBruto>, ParserErro> {
    let document = Html::parse_document(html);
    let poly_cards = selector(".poly-card");
    let fallback_cards = selector("li.ui-search-layout__item");
    let title = selector(".poly-component__title, .ui-search-item__title, h2, [class*='title']");
    let price = selector(".poly-price, [itemprop='price'], .andes-money-amount__fraction");
    let seller = selector(".poly-seller, .ui-search-official-store-label, [class*='seller']");
    let link = selector("a[href]");
    let mut ads = Vec::new();

    let preferred = document.select(&poly_cards).collect::<Vec<_>>();
    let cards = if preferred.is_empty() {
        document.select(&fallback_cards).collect::<Vec<_>>()
    } else {
        preferred
    };
    for (index, card) in cards.into_iter().enumerate() {
        let Some(title_node) = card.select(&title).next() else {
            continue;
        };
        let title_text = title_node.text().collect::<String>().trim().to_owned();
        let url = title_node
            .value()
            .attr("href")
            .or_else(|| {
                card.select(&link)
                    .next()
                    .and_then(|node| node.value().attr("href"))
            })
            .unwrap_or_default()
            .to_owned();
        let id = card
            .value()
            .attr("data-id")
            .map(str::to_owned)
            .or_else(|| {
                url.rsplit('/')
                    .next()
                    .map(|part| part.trim_start_matches("MLB-").to_owned())
            })
            .unwrap_or_else(|| format!("item-{}", index + 1));
        let price_node = card.select(&price).next();
        let price_text = card
            .value()
            .attr("data-price")
            .map(str::to_owned)
            .or_else(|| price_node.and_then(|node| node.value().attr("content").map(str::to_owned)))
            .or_else(|| price_node.map(|node| node.text().collect::<String>()))
            .unwrap_or_default();
        let Some(preco) = parse_preco(&price_text) else {
            continue;
        };
        let vendedor = card
            .select(&seller)
            .next()
            .map(|node| node.text().collect::<String>().trim().to_owned())
            .unwrap_or_default();
        let parse_float = |name: &str| {
            card.value()
                .attr(name)
                .and_then(|value| value.parse::<f64>().ok())
        };
        let parse_count = |name: &str| {
            card.value()
                .attr(name)
                .and_then(|value| value.parse::<u32>().ok())
                .unwrap_or(0)
        };
        let condicao = card
            .value()
            .attr("data-condition")
            .unwrap_or("new")
            .to_owned();
        let modelo = card.value().attr("data-model").map(str::to_owned);
        let nota = parse_float("data-rating").filter(|value| (0.0..=5.0).contains(value));

        ads.push(AnuncioBruto {
            id,
            titulo: title_text,
            preco,
            preco_original: parse_float("data-original-price"),
            url,
            condicao,
            vendedor,
            vendedor_id: card.value().attr("data-seller-id").map(str::to_owned),
            vendedor_nivel: card.value().attr("data-seller-level").map(str::to_owned),
            vendedor_score: parse_float("data-seller-score")
                .unwrap_or(0.5)
                .clamp(0.0, 1.0),
            nota,
            qtd_avaliacoes: parse_count("data-review-count"),
            modelo,
            gtin: card.value().attr("data-gtin").map(str::to_owned),
            marca: card.value().attr("data-brand").map(str::to_owned),
            frete: card
                .value()
                .attr("data-shipping-price")
                .and_then(|s| s.parse().ok()),
        });
    }

    if ads.is_empty() {
        Err(ParserErro::SemAnuncios)
    } else {
        Ok(ads)
    }
}

pub fn parse_detalhe_html(url: &str, html: &str) -> Result<ProdutoBruto, ParserErro> {
    let document = Html::parse_document(html);
    let scripts = selector("script[type='application/ld+json']");
    for script in document.select(&scripts) {
        let json_text = script.text().collect::<String>();
        let Ok(value) = serde_json::from_str::<Value>(&json_text) else {
            continue;
        };
        if let Some(product) = find_product(&value) {
            let titulo = product
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let id = product
                .get("sku")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .or_else(|| {
                    product
                        .get("productID")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                })
                .unwrap_or_else(|| url.rsplit('/').next().unwrap_or_default().to_owned());
            let preco = product
                .get("offers")
                .and_then(|offers| {
                    if let Some(list) = offers.as_array() {
                        list.first()
                    } else {
                        Some(offers)
                    }
                })
                .and_then(|offer| offer.get("price"))
                .and_then(parse_json_number)
                .ok_or(ParserErro::PrecoInvalido)?;
            let rating = product.get("aggregateRating");
            let nota = rating
                .and_then(|value| value.get("ratingValue"))
                .and_then(parse_json_number);
            let qtd_avaliacoes = rating
                .and_then(|value| {
                    value
                        .get("reviewCount")
                        .or_else(|| value.get("ratingCount"))
                })
                .and_then(|value| {
                    value
                        .as_u64()
                        .or_else(|| value.as_str().and_then(|raw| raw.parse().ok()))
                })
                .unwrap_or(0) as u32;
            return Ok(ProdutoBruto {
                id,
                titulo,
                preco,
                nota,
                qtd_avaliacoes,
                url: url.to_owned(),
                gtin: product
                    .get("gtin13")
                    .or_else(|| product.get("gtin"))
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                marca: product
                    .get("brand")
                    .and_then(|v| v.as_str().or_else(|| v.get("name").and_then(Value::as_str)))
                    .map(str::to_owned),
                modelo: product
                    .get("model")
                    .or_else(|| product.get("mpn"))
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                frete: None,
            });
        }
    }
    Err(ParserErro::ProdutoAusente)
}

pub fn detectar_verificacao(html: &str) -> bool {
    let document = Html::parse_document(html);
    let cards = selector("article.poly-card, li.ui-search-layout__item, .poly-card");
    if document.select(&cards).next().is_some() {
        return false;
    }
    let scripts = selector("script[type='application/ld+json']");
    let has_product = document.select(&scripts).any(|node| {
        serde_json::from_str::<Value>(&node.text().collect::<String>())
            .ok()
            .is_some_and(|json| find_product(&json).is_some())
    });
    if has_product {
        return false;
    }
    let text = document
        .root_element()
        .text()
        .collect::<String>()
        .to_lowercase();
    [
        "não é um robô",
        "nao e um robo",
        "verificação de segurança",
        "verificacao de seguranca",
        "security verification",
        "captcha",
        "robot check",
    ]
    .iter()
    .any(|marker| text.contains(marker))
}

fn find_product(value: &Value) -> Option<&Value> {
    match value {
        Value::Array(values) => values.iter().find_map(find_product),
        Value::Object(object) => {
            let product_type = object.get("@type").is_some_and(|kind| {
                kind.as_str() == Some("Product")
                    || kind.as_array().is_some_and(|types| {
                        types.iter().any(|item| item.as_str() == Some("Product"))
                    })
            });
            if product_type {
                return Some(value);
            }
            object.values().find_map(find_product)
        }
        _ => None,
    }
}

fn parse_json_number(value: &Value) -> Option<f64> {
    value.as_f64().or_else(|| {
        value
            .as_str()
            .and_then(|raw| raw.replace(',', ".").parse().ok())
    })
}

fn selector(value: &str) -> Selector {
    Selector::parse(value).expect("seletor CSS estático válido")
}

fn parse_preco(raw: &str) -> Option<f64> {
    let compact = raw.replace("R$", "").split_whitespace().collect::<String>();
    if compact.is_empty() {
        return None;
    }
    let normalized = if compact.contains(',') {
        compact.replace('.', "").replace(',', ".")
    } else {
        compact
    };
    normalized
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite() && *value >= 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LISTAGEM: &str = include_str!("../../tests/fixtures/listagem.html");
    const PRODUTO: &str = include_str!("../../tests/fixtures/produto.html");
    const VERIFICACAO: &str = include_str!("../../tests/fixtures/verificacao.html");

    #[test]
    fn listagem_tem_15_anuncios_com_dados_corretos() {
        let anuncios = parse_listagem_html(LISTAGEM).unwrap();
        assert_eq!(anuncios.len(), 15);
        assert_eq!(anuncios[0].id, "MLB1001");
        assert_eq!(anuncios[0].titulo, "Fone JBL Tune 520BT ANC preto");
        assert_eq!(anuncios[0].preco, 249.0);
        assert_eq!(
            anuncios[0].url,
            "https://produto.mercadolivre.com.br/MLB-1001"
        );
        assert_eq!(anuncios[0].qtd_avaliacoes, 1250);
    }

    #[test]
    fn produto_json_ld_extrai_oferta_e_avaliacao() {
        let produto =
            parse_detalhe_html("https://produto.mercadolivre.com.br/MLB-9001", PRODUTO).unwrap();
        assert_eq!(produto.id, "MLB9001");
        assert_eq!(produto.preco, 249.9);
        assert_eq!(produto.nota, Some(4.8));
        assert_eq!(produto.qtd_avaliacoes, 1250);
    }

    #[test]
    fn deteccao_separa_verificacao_de_listagem_e_produto() {
        assert!(detectar_verificacao(VERIFICACAO));
        assert!(!detectar_verificacao(LISTAGEM));
        assert!(!detectar_verificacao(PRODUTO));
    }

    #[test]
    fn fixtures_sao_html_com_json_ld_valido() {
        let product = Html::parse_document(PRODUTO);
        let scripts = selector("script[type='application/ld+json']");
        let json = serde_json::from_str::<Value>(
            &product
                .select(&scripts)
                .next()
                .unwrap()
                .text()
                .collect::<String>(),
        )
        .unwrap();
        assert_eq!(json.get("@type").and_then(Value::as_str), Some("Product"));
        assert!(
            Html::parse_document(LISTAGEM)
                .select(&selector("article.poly-card"))
                .count()
                >= 12
        );
        assert!(
            Html::parse_document(VERIFICACAO)
                .select(&selector(".poly-card"))
                .next()
                .is_none()
        );
    }
}
