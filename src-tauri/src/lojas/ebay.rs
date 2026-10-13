use async_trait::async_trait;
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::Value;
use url::Url;

use super::{FonteDaLoja, FonteLoja, Loja, LojaId, OfertaBruta};
use crate::core::reputacao_percentual;

pub const EXTRATOR: &str = include_str!("ebay_extractor.js");

pub struct LojaEbay {
    fontes: Vec<FonteDaLoja>,
}
impl LojaEbay {
    pub fn nova(fontes: Vec<FonteDaLoja>) -> Self {
        Self { fontes }
    }
}
impl Loja for LojaEbay {
    fn id(&self) -> LojaId {
        LojaId::Ebay
    }
    fn fontes(&self) -> Vec<FonteDaLoja> {
        self.fontes.clone()
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Dinheiro {
    value: String,
    currency: String,
}
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct Vendedor {
    #[serde(default)]
    username: String,
    feedback_percentage: Option<String>,
    feedback_score: Option<i64>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Frete {
    shipping_cost: Option<Dinheiro>,
}
#[derive(Deserialize, Default)]
struct Catalogo {
    brand: Option<String>,
    #[serde(default)]
    gtins: Vec<String>,
    #[serde(default)]
    mpns: Vec<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Item {
    item_id: String,
    title: String,
    item_web_url: String,
    price: Dinheiro,
    #[serde(default)]
    seller: Vendedor,
    #[serde(default)]
    shipping_options: Vec<Frete>,
    condition: Option<String>,
    condition_id: Option<String>,
    gtin: Option<String>,
    brand: Option<String>,
    mpn: Option<String>,
    product: Option<Catalogo>,
}

fn valor_dinheiro(valor: Dinheiro) -> Result<Decimal, String> {
    if valor.currency != "BRL" {
        return Err("A página do eBay não mostrou preço em reais.".into());
    }
    valor
        .value
        .parse::<Decimal>()
        .map_err(|_| "eBay retornou um valor monetário inválido.".into())
}
fn mapear(item: Item) -> Result<OfertaBruta, String> {
    let url = Url::parse(&item.item_web_url).map_err(|_| "URL de oferta eBay inválida.")?;
    if !url_ebay(&url) || item.item_id.trim().is_empty() || item.title.trim().is_empty() {
        return Err("Oferta eBay sem ID, título ou URL válida.".into());
    }
    let preco = valor_dinheiro(item.price)?;
    if preco <= Decimal::ZERO {
        return Err("Preço eBay precisa ser positivo.".into());
    }
    let mut frete = None;
    for opcao in item.shipping_options {
        if let Some(custo) = opcao.shipping_cost {
            let custo = valor_dinheiro(custo)?;
            if custo < Decimal::ZERO {
                return Err("Frete eBay inválido.".into());
            }
            frete = Some(frete.map_or(custo, |anterior: Decimal| anterior.min(custo)));
        }
    }
    let reputacao = item
        .seller
        .feedback_percentage
        .as_deref()
        .and_then(|valor| valor.parse().ok())
        .zip(item.seller.feedback_score)
        .and_then(|(percentual, score)| {
            reputacao_percentual(percentual, score.clamp(0, i64::from(u32::MAX)) as u32)
        });
    let catalogo = item.product.unwrap_or_default();
    let gtin = item
        .gtin
        .or_else(|| (catalogo.gtins.len() == 1).then(|| catalogo.gtins[0].clone()))
        .filter(|valor| {
            [8, 12, 13, 14].contains(&valor.len())
                && valor.bytes().all(|byte| byte.is_ascii_digit())
        });
    let modelo = item
        .mpn
        .or_else(|| (catalogo.mpns.len() == 1).then(|| catalogo.mpns[0].clone()));
    let condicao = match item.condition_id.as_deref() {
        Some("1000") => "new".into(),
        Some("1500") => "open_box".into(),
        Some("1750") => "new_with_defects".into(),
        Some("2000" | "2010" | "2020" | "2030" | "2500") => "refurbished".into(),
        Some("2750" | "2990" | "3000" | "3010" | "4000" | "5000" | "6000") => "used".into(),
        Some("7000") => "for_parts".into(),
        _ => match item
            .condition
            .as_deref()
            .unwrap_or("")
            .to_lowercase()
            .as_str()
        {
            "new" | "novo" => "new".into(),
            "used" | "pre-owned" | "usado" => "used".into(),
            _ => "unknown".into(),
        },
    };
    Ok(OfertaBruta {
        id_externo: item.item_id,
        titulo: item.title,
        url: item.item_web_url,
        gtin,
        modelo,
        marca: item.brand.or(catalogo.brand),
        preco,
        frete,
        condicao,
        vendedor: item.seller.username,
        nota: None,
        qtd_avaliacoes: None,
        vendas: None,
        reputacao,
        preco_variavel: false,
        preco_parcelado: None,
    })
}
pub fn parse_busca(valor: Value) -> Result<Vec<OfertaBruta>, String> {
    if valor.get("errors").is_some() {
        return Err("eBay retornou erros na consulta.".into());
    }
    let Some(items) = valor.get("itemSummaries") else {
        return if valor.get("total").and_then(Value::as_u64) == Some(0) {
            Ok(vec![])
        } else {
            Err("Resposta eBay sem listagem reconhecida.".into())
        };
    };
    let items: Vec<Item> = serde_json::from_value(items.clone())
        .map_err(|_| "Campos da listagem eBay inválidos.".to_owned())?;
    items.into_iter().map(mapear).collect()
}
pub fn parse_detalhe(valor: Value) -> Result<OfertaBruta, String> {
    let item = serde_json::from_value(valor)
        .map_err(|_| "Campos do detalhe eBay inválidos.".to_owned())?;
    mapear(item)
}
pub fn url_ebay(url: &Url) -> bool {
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url
            .host_str()
            .is_some_and(|host| host == "ebay.com" || host.ends_with(".ebay.com"))
}
pub fn url_busca(termo: &str) -> Result<Url, String> {
    let mut url = Url::parse("https://www.ebay.com/sch/i.html").unwrap();
    url.query_pairs_mut().append_pair("_nkw", termo);
    Ok(url)
}

pub struct FonteEbayFixture;
#[async_trait]
impl FonteLoja for FonteEbayFixture {
    async fn listar(&self, _: &str) -> Result<Vec<OfertaBruta>, String> {
        parse_busca(
            serde_json::from_str(include_str!("../../tests/fixtures/ebay/search.json")).unwrap(),
        )
    }
    async fn detalhar(&self, _: &str) -> Result<OfertaBruta, String> {
        parse_detalhe(
            serde_json::from_str(include_str!("../../tests/fixtures/ebay/item.json")).unwrap(),
        )
    }
}
