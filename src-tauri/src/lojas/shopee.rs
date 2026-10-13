use async_trait::async_trait;
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::{Value, json};
use url::Url;

use super::{FonteDaLoja, FonteLoja, Loja, LojaId, OfertaBruta};

pub const EXTRATOR: &str = include_str!("shopee_extractor.js");

pub struct LojaShopee {
    fontes: Vec<FonteDaLoja>,
}
impl LojaShopee {
    pub fn nova(fontes: Vec<FonteDaLoja>) -> Self {
        Self { fontes }
    }
}
impl Loja for LojaShopee {
    fn id(&self) -> LojaId {
        LojaId::Shopee
    }
    fn fontes(&self) -> Vec<FonteDaLoja> {
        self.fontes.clone()
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Id {
    Numero(u64),
    Texto(String),
}
impl Id {
    fn texto(self) -> Result<String, String> {
        let valor = match self {
            Self::Numero(numero) => numero.to_string(),
            Self::Texto(texto) => texto,
        };
        if valor.is_empty()
            || !valor.bytes().all(|byte| byte.is_ascii_digit())
            || valor.bytes().all(|byte| byte == b'0')
        {
            return Err("Shopee retornou ID inválido.".into());
        }
        Ok(valor)
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Item {
    item_id: Id,
    shop_id: Id,
    product_name: String,
    product_link: String,
    price_min: String,
    price_max: String,
    rating_star: Option<String>,
    sales: Option<u32>,
    #[serde(default)]
    shop_name: String,
}
fn mapear(item: Item) -> Result<OfertaBruta, String> {
    let url = Url::parse(&item.product_link).map_err(|_| "URL de oferta Shopee inválida.")?;
    if !url_shopee(&url) || item.product_name.trim().is_empty() {
        return Err("Oferta Shopee sem título ou URL válida.".into());
    }
    let minimo = item
        .price_min
        .parse::<Decimal>()
        .map_err(|_| "Preço mínimo Shopee inválido.")?;
    let maximo = item
        .price_max
        .parse::<Decimal>()
        .map_err(|_| "Preço máximo Shopee inválido.")?;
    if minimo <= Decimal::ZERO || maximo < minimo {
        return Err("Faixa de preço Shopee inválida.".into());
    }
    let nota = item
        .rating_star
        .as_deref()
        .and_then(|nota| nota.parse::<f64>().ok())
        .filter(|nota| nota.is_finite() && *nota > 0.0 && *nota <= 5.0);
    Ok(OfertaBruta {
        id_externo: format!("{}:{}", item.shop_id.texto()?, item.item_id.texto()?),
        titulo: item.product_name,
        url: item.product_link,
        gtin: None,
        modelo: None,
        marca: None,
        preco: minimo,
        frete: None,
        condicao: "unknown".into(),
        vendedor: item.shop_name,
        nota,
        qtd_avaliacoes: None,
        vendas: item.sales,
        reputacao: None,
        preco_variavel: maximo > minimo,
        preco_parcelado: None,
    })
}
fn conexao(valor: &Value) -> Result<&Value, String> {
    if valor.get("errors").is_some_and(|errors| {
        !errors.is_null() && errors.as_array().is_none_or(|array| !array.is_empty())
    }) {
        return Err("A página da Shopee retornou uma mensagem de erro.".into());
    }
    valor
        .get("data")
        .and_then(|data| data.get("productOfferV2"))
        .filter(|connection| connection.is_object())
        .ok_or_else(|| "Cards de produto não reconhecidos na página da Shopee.".into())
}
pub fn parse_busca(valor: Value) -> Result<Vec<OfertaBruta>, String> {
    let nodes = conexao(&valor)?
        .get("nodes")
        .ok_or("Resposta Shopee sem nodes.")?;
    let items: Vec<Item> = serde_json::from_value(nodes.clone())
        .map_err(|_| "Campos de productOfferV2 inválidos.".to_owned())?;
    items.into_iter().map(mapear).collect()
}

// Dados opcionais exclusivos da extração da página, separados dos dados estruturados.
#[derive(Default, Deserialize)]
struct DadosPagina {
    frete: Option<Decimal>,
    qtd_avaliacoes: Option<u32>,
    nota_loja: Option<f64>,
    #[serde(default)]
    oficial: bool,
    #[serde(default)]
    preferida: bool,
    condicao: Option<String>,
    gtin: Option<String>,
    modelo: Option<String>,
    marca: Option<String>,
}
pub fn parse_webview(valor: Value) -> Result<Vec<OfertaBruta>, String> {
    let mut ofertas = parse_busca(valor.clone())?;
    let nodes = conexao(&valor)?
        .get("nodes")
        .and_then(Value::as_array)
        .ok_or("Cards Shopee inválidos.")?;
    for (oferta, node) in ofertas.iter_mut().zip(nodes) {
        let pagina: DadosPagina =
            serde_json::from_value(node.get("_garimpo").cloned().unwrap_or_else(|| json!({})))
                .map_err(|_| "Dados complementares da página Shopee inválidos.")?;
        if pagina.frete.is_some_and(|valor| valor < Decimal::ZERO) {
            return Err("Frete Shopee inválido.".into());
        }
        oferta.frete = pagina.frete;
        oferta.qtd_avaliacoes = pagina.qtd_avaliacoes;
        oferta.reputacao =
            crate::core::reputacao_shopee(pagina.nota_loja, pagina.oficial, pagina.preferida);
        oferta.condicao = pagina.condicao.unwrap_or_else(|| "unknown".into());
        oferta.gtin = pagina.gtin.filter(|valor| {
            [8, 12, 13, 14].contains(&valor.len())
                && valor.bytes().all(|byte| byte.is_ascii_digit())
        });
        oferta.modelo = pagina.modelo;
        oferta.marca = pagina.marca;
    }
    Ok(ofertas)
}
pub fn url_shopee(url: &Url) -> bool {
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url
            .host_str()
            .is_some_and(|host| host == "shopee.com.br" || host.ends_with(".shopee.com.br"))
}
pub fn url_busca(termo: &str) -> Result<Url, String> {
    let mut url = Url::parse("https://shopee.com.br/search_user").unwrap();
    url.query_pairs_mut().append_pair("keyword", termo);
    Ok(url)
}
pub struct FonteShopeeFixture;
#[async_trait]
impl FonteLoja for FonteShopeeFixture {
    async fn listar(&self, _: &str) -> Result<Vec<OfertaBruta>, String> {
        parse_busca(
            serde_json::from_str(include_str!(
                "../../tests/fixtures/shopee/product_offer.json"
            ))
            .unwrap(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::url_busca;

    #[test]
    fn url_busca_usa_a_rota_atual_da_shopee_e_codifica_o_termo() {
        let url = url_busca("ryzen 7 5700g").unwrap();

        assert_eq!(url.path(), "/search_user");
        assert_eq!(
            url.query_pairs()
                .find(|(key, _)| key == "keyword")
                .unwrap()
                .1,
            "ryzen 7 5700g"
        );
    }
}
