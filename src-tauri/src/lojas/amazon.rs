use async_trait::async_trait;
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::{Value, json};
use url::Url;

use super::{FonteDaLoja, FonteLoja, Loja, LojaId, OfertaBruta};

pub const EXTRATOR: &str = include_str!("amazon_extractor.js");

pub struct LojaAmazon {
    fontes: Vec<FonteDaLoja>,
}
impl LojaAmazon {
    pub fn nova(fontes: Vec<FonteDaLoja>) -> Self {
        Self { fontes }
    }
}
impl Loja for LojaAmazon {
    fn id(&self) -> LojaId {
        LojaId::Amazon
    }
    fn fontes(&self) -> Vec<FonteDaLoja> {
        self.fontes.clone()
    }
}
fn texto<'a>(valor: &'a Value, caminho: &str) -> Option<&'a str> {
    valor
        .pointer(caminho)
        .and_then(Value::as_str)
        .filter(|texto| !texto.trim().is_empty())
}
fn decimal(valor: &Value) -> Result<Decimal, String> {
    let texto = match valor {
        Value::String(texto) => texto.clone(),
        Value::Number(numero) => numero.to_string(),
        _ => String::new(),
    };
    texto.parse().map_err(|_| "Preço Amazon inválido.".into())
}
fn identidade(item: &Value) -> Result<(&str, &str, &str), String> {
    let asin = texto(item, "/asin")
        .filter(|id| id.len() == 10 && id.bytes().all(|byte| byte.is_ascii_alphanumeric()))
        .ok_or("ASIN Amazon inválido.")?;
    let titulo = texto(item, "/itemInfo/title/displayValue")
        .filter(|titulo| !titulo.trim().is_empty())
        .ok_or("Título Amazon ausente.")?;
    let url = texto(item, "/detailPageURL")
        .filter(|valor| Url::parse(valor).is_ok_and(|url| url_amazon(&url)))
        .ok_or("URL de produto Amazon BR inválida.")?;
    Ok((asin, titulo, url))
}
pub fn parse_busca(valor: Value) -> Result<Vec<OfertaBruta>, String> {
    let items = valor
        .pointer("/searchResult/items")
        .and_then(Value::as_array)
        .ok_or("Nenhum card de produto reconhecido na página da Amazon.")?;
    let mut ofertas = vec![];
    for item in items {
        let (asin, titulo, url) = identidade(item)?;
        let Some(listings) = item.pointer("/offersV2/listings").and_then(Value::as_array) else {
            continue;
        };
        let mut candidatas = std::collections::BTreeMap::<&str, (Decimal, &Value)>::new();
        for listing in listings {
            if listing.get("violatesMAP").and_then(Value::as_bool) == Some(true)
                || texto(listing, "/dealDetails/accessType").is_some_and(|tipo| tipo != "ALL")
                || texto(listing, "/type").is_some_and(|tipo| {
                    tipo.replace('_', "")
                        .eq_ignore_ascii_case("SUBSCRIBEANDSAVE")
                })
                || texto(listing, "/availability/type").is_some_and(|tipo| {
                    ["OUTOFSTOCK", "UNAVAILABLE", "PREORDER", "AVAILABLEDATE"]
                        .contains(&tipo.replace('_', "").as_str())
                })
            {
                continue;
            }
            let Some(money) = listing.pointer("/price/money") else {
                continue;
            };
            if money.get("currency").and_then(Value::as_str) != Some("BRL") {
                return Err("Amazon BR retornou moeda não suportada; esperado BRL.".into());
            }
            let preco = decimal(money.get("amount").ok_or("Amazon sem valor de preço.")?)?;
            if preco <= Decimal::ZERO {
                return Err("Preço Amazon precisa ser positivo.".into());
            }
            let condicao = match texto(listing, "/condition/value") {
                Some("New") => "new",
                Some("Used") => "used",
                Some("Refurbished") => "refurbished",
                _ => "unknown",
            };
            if candidatas
                .get(condicao)
                .is_none_or(|(anterior, _)| preco < *anterior)
            {
                candidatas.insert(condicao, (preco, listing));
            }
        }
        let gtin = item
            .pointer("/itemInfo/externalIds/eans/displayValues")
            .or_else(|| item.pointer("/itemInfo/externalIds/upcs/displayValues"))
            .and_then(Value::as_array)
            .and_then(|valores| {
                valores.iter().filter_map(Value::as_str).find(|gtin| {
                    [8, 12, 13, 14].contains(&gtin.len())
                        && gtin.bytes().all(|byte| byte.is_ascii_digit())
                })
            })
            .map(str::to_owned);
        // Mantém o menor preço de cada condição; um usado barato não apaga a oferta nova.
        for (condicao, (preco, listing)) in candidatas {
            ofertas.push(OfertaBruta {
                id_externo: if condicao == "new" {
                    asin.into()
                } else {
                    format!("{asin}:{condicao}")
                },
                titulo: titulo.into(),
                url: url.into(),
                gtin: gtin.clone(),
                modelo: texto(item, "/itemInfo/manufactureInfo/model/displayValue")
                    .map(str::to_owned),
                marca: texto(item, "/itemInfo/byLineInfo/brand/displayValue").map(str::to_owned),
                preco,
                frete: None,
                condicao: condicao.into(),
                vendedor: texto(listing, "/merchantInfo/name")
                    .unwrap_or_default()
                    .into(),
                nota: None,
                qtd_avaliacoes: None,
                vendas: None,
                reputacao: None,
                preco_variavel: false,
                preco_parcelado: None,
            });
        }
    }
    Ok(ofertas)
}
#[derive(Default, Deserialize)]
struct DadosPagina {
    frete: Option<Decimal>,
    nota: Option<f64>,
    qtd_avaliacoes: Option<u32>,
    #[serde(default)]
    vendido_amazon: bool,
    #[serde(default)]
    enviado_amazon: bool,
}
pub fn parse_webview(valor: Value) -> Result<Vec<OfertaBruta>, String> {
    let mut ofertas = parse_busca(valor.clone())?;
    let items = valor
        .pointer("/searchResult/items")
        .and_then(Value::as_array)
        .ok_or("Cards Amazon inválidos.")?;
    for oferta in &mut ofertas {
        let node = items
            .iter()
            .find(|item| {
                item.get("asin").and_then(Value::as_str) == oferta.id_externo.split(':').next()
            })
            .unwrap();
        let pagina: DadosPagina =
            serde_json::from_value(node.get("_garimpo").cloned().unwrap_or_else(|| json!({})))
                .map_err(|_| "Dados complementares Amazon inválidos.")?;
        if pagina.frete.is_some_and(|frete| frete < Decimal::ZERO) {
            return Err("Frete Amazon não pode ser negativo.".into());
        }
        oferta.frete = pagina.frete;
        oferta.nota = pagina
            .nota
            .filter(|nota| nota.is_finite() && (0.0..=5.0).contains(nota));
        oferta.qtd_avaliacoes = pagina.qtd_avaliacoes;
        oferta.reputacao = (pagina.vendido_amazon && pagina.enviado_amazon).then_some(1.0);
    }
    Ok(ofertas)
}
pub fn url_amazon(url: &Url) -> bool {
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url
            .host_str()
            .is_some_and(|host| host == "amazon.com.br" || host.ends_with(".amazon.com.br"))
}
pub fn url_busca(termo: &str) -> Result<Url, String> {
    let mut url = Url::parse("https://www.amazon.com.br/s").unwrap();
    url.query_pairs_mut().append_pair("k", termo);
    Ok(url)
}
pub struct FonteAmazonFixture;
#[async_trait]
impl FonteLoja for FonteAmazonFixture {
    async fn listar(&self, _: &str) -> Result<Vec<OfertaBruta>, String> {
        parse_webview(
            serde_json::from_str(include_str!("../../tests/fixtures/amazon/search.json")).unwrap(),
        )
    }
}
