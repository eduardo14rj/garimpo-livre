use async_trait::async_trait;
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::{Value, json};
use url::Url;

use super::{FonteDaLoja, FonteLoja, Loja, LojaId, OfertaBruta};

pub const EXTRATOR: &str = include_str!("aliexpress_extractor.js");

pub struct LojaAliexpress {
    fontes: Vec<FonteDaLoja>,
}
impl LojaAliexpress {
    pub fn nova(fontes: Vec<FonteDaLoja>) -> Self {
        Self { fontes }
    }
}
impl Loja for LojaAliexpress {
    fn id(&self) -> LojaId {
        LojaId::Aliexpress
    }
    fn fontes(&self) -> Vec<FonteDaLoja> {
        self.fontes.clone()
    }
}

fn inteiro(valor: &Value) -> Option<u64> {
    valor.as_u64().or_else(|| valor.as_str()?.parse().ok())
}
fn identificador(valor: Value) -> Result<String, String> {
    let id = match valor {
        Value::String(texto) => texto,
        Value::Number(numero) => numero.to_string(),
        _ => String::new(),
    };
    if id.is_empty()
        || !id.bytes().all(|byte| byte.is_ascii_digit())
        || id.bytes().all(|byte| byte == b'0')
    {
        return Err("ID AliExpress inválido.".into());
    }
    Ok(id)
}
#[derive(Deserialize)]
struct Item {
    product_id: Value,
    product_title: String,
    product_detail_url: String,
    target_sale_price: Option<String>,
    target_sale_price_currency: Option<String>,
    sale_price: Option<String>,
    sale_price_currency: Option<String>,
    shop_name: Option<String>,
    shop_id: Option<Value>,
    lastest_volume: Option<u32>,
}
fn mapear(item: Item) -> Result<Option<OfertaBruta>, String> {
    let url =
        Url::parse(&item.product_detail_url).map_err(|_| "URL de oferta AliExpress inválida.")?;
    if !url_aliexpress(&url) || item.product_title.trim().is_empty() {
        return Err("Oferta AliExpress sem título ou URL válida.".into());
    }
    let (preco, divisa) = match (item.target_sale_price, item.target_sale_price_currency) {
        (Some(preco), Some(divisa)) => (preco, divisa),
        (None, None) => item
            .sale_price
            .zip(item.sale_price_currency)
            .ok_or("AliExpress sem preço/moeda reconhecidos.")?,
        _ => return Err("AliExpress retornou preço alvo sem moeda correspondente.".into()),
    };
    if divisa != "BRL" {
        return Ok(None);
    }
    let preco = preco
        .parse::<Decimal>()
        .map_err(|_| "Preço AliExpress inválido.".to_owned())?;
    if preco <= Decimal::ZERO {
        return Err("Preço AliExpress precisa ser positivo.".into());
    }
    let vendedor = if item
        .shop_name
        .as_deref()
        .is_none_or(|nome| nome.trim().is_empty())
    {
        item.shop_id
            .and_then(|id| identificador(id).ok())
            .map(|id| format!("Loja {id}"))
            .unwrap_or_default()
    } else {
        item.shop_name.unwrap()
    };
    Ok(Some(OfertaBruta {
        id_externo: identificador(item.product_id)?,
        titulo: item.product_title,
        url: item.product_detail_url,
        gtin: None,
        modelo: None,
        marca: None,
        preco,
        frete: None,
        condicao: "unknown".into(),
        vendedor,
        nota: None,
        qtd_avaliacoes: None,
        vendas: item.lastest_volume,
        reputacao: None,
        preco_variavel: false,
        preco_parcelado: None,
    }))
}
fn resultado(valor: &Value) -> Result<&Value, String> {
    let envelope = valor
        .get("aliexpress_affiliate_product_query_response")
        .unwrap_or(valor);
    let resposta = envelope
        .get("resp_result")
        .ok_or("Resposta AliExpress sem resp_result.")?;
    if resposta.get("resp_code").and_then(inteiro) != Some(200) {
        return Err("A página do AliExpress não carregou resultados reconhecidos.".into());
    }
    resposta
        .get("result")
        .filter(|value| value.is_object())
        .ok_or_else(|| "Resposta AliExpress sem resultado reconhecido.".into())
}
pub fn parse_busca(valor: Value) -> Result<Vec<OfertaBruta>, String> {
    let result = resultado(&valor)?;
    let Some(products) = result
        .get("products")
        .and_then(|products| products.get("product"))
    else {
        return if result.get("total_record_count").and_then(inteiro) == Some(0) {
            Ok(vec![])
        } else {
            Err("Resposta AliExpress sem products.product.".into())
        };
    };
    let items: Vec<Item> = serde_json::from_value(products.clone())
        .map_err(|_| "Campos de produto AliExpress inválidos.".to_owned())?;
    items
        .into_iter()
        .map(mapear)
        .collect::<Result<Vec<_>, _>>()
        .map(|items| items.into_iter().flatten().collect())
}
#[derive(Default, Deserialize)]
struct DadosPagina {
    frete: Option<Decimal>,
    frete_moeda: Option<String>,
    percentual_positivo_loja: Option<f64>,
    nota_produto: Option<f64>,
    qtd_avaliacoes: Option<u32>,
    condicao: Option<String>,
    gtin: Option<String>,
    modelo: Option<String>,
    marca: Option<String>,
}
pub fn parse_webview(valor: Value) -> Result<Vec<OfertaBruta>, String> {
    let mut ofertas = parse_busca(valor.clone())?;
    if ofertas.is_empty() {
        return Ok(ofertas);
    }
    let nodes = resultado(&valor)?
        .get("products")
        .and_then(|products| products.get("product"))
        .and_then(Value::as_array)
        .ok_or("Cards AliExpress inválidos.")?;
    for oferta in &mut ofertas {
        let node = nodes
            .iter()
            .find(|node| {
                node.get("product_id")
                    .cloned()
                    .and_then(|id| identificador(id).ok())
                    .as_deref()
                    == Some(oferta.id_externo.as_str())
            })
            .ok_or("Card AliExpress sem produto correspondente.")?;
        let pagina: DadosPagina =
            serde_json::from_value(node.get("_garimpo").cloned().unwrap_or_else(|| json!({})))
                .map_err(|_| "Dados complementares da página AliExpress inválidos.")?;
        if let Some(frete) = pagina.frete {
            if frete < Decimal::ZERO || pagina.frete_moeda.as_deref() != Some("BRL") {
                return Err("Frete AliExpress precisa estar em reais e ser não negativo.".into());
            }
            oferta.frete = Some(frete);
        }
        oferta.reputacao = crate::core::reputacao_positiva(pagina.percentual_positivo_loja);
        oferta.nota = pagina
            .nota_produto
            .filter(|nota| nota.is_finite() && (0.0..=5.0).contains(nota));
        oferta.qtd_avaliacoes = pagina.qtd_avaliacoes;
        oferta.condicao = pagina.condicao.unwrap_or_else(|| "unknown".into());
        oferta.gtin = pagina.gtin.filter(|gtin| {
            [8, 12, 13, 14].contains(&gtin.len()) && gtin.bytes().all(|byte| byte.is_ascii_digit())
        });
        oferta.modelo = pagina.modelo;
        oferta.marca = pagina.marca;
    }
    Ok(ofertas)
}
pub fn url_aliexpress(url: &Url) -> bool {
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.host_str().is_some_and(|host| {
            ["aliexpress.com", "aliexpress.us"]
                .iter()
                .any(|domain| host == *domain || host.ends_with(&format!(".{domain}")))
        })
}
pub fn url_busca(termo: &str) -> Result<Url, String> {
    let mut url = Url::parse("https://www.aliexpress.com").unwrap();
    let slug = termo.split_whitespace().collect::<Vec<_>>().join("-");
    url.path_segments_mut()
        .map_err(|_| "URL de busca AliExpress inválida.")?
        .extend(["w", &format!("wholesale-{slug}.html")]);
    Ok(url)
}
pub struct FonteAliexpressFixture;
#[async_trait]
impl FonteLoja for FonteAliexpressFixture {
    async fn listar(&self, _: &str) -> Result<Vec<OfertaBruta>, String> {
        parse_busca(
            serde_json::from_str(include_str!("../../tests/fixtures/aliexpress/query.json"))
                .unwrap(),
        )
    }
}
