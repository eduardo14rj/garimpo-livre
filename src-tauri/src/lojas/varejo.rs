//! Dados canônicos dos extratores nacionais; cada módulo restringe seus próprios URLs.
use async_trait::async_trait;
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::Value;
use url::Url;

use super::{FonteLoja, LojaId, OfertaBruta};

#[derive(Deserialize)]
struct OfertaPagina {
    id_externo: String,
    titulo: String,
    url: String,
    preco: String,
    frete: Option<String>,
    preco_parcelado: Option<String>,
    gtin: Option<String>,
    modelo: Option<String>,
    marca: Option<String>,
    #[serde(default)]
    vendedor: String,
    #[serde(default)]
    venda_direta: bool,
    #[serde(default)]
    condicao: String,
    nota: Option<f64>,
    qtd_avaliacoes: Option<u32>,
}
fn dinheiro(valor: &str) -> Result<Decimal, String> {
    valor
        .parse()
        .map_err(|_| "Preço ou frete da loja em formato decimal inválido.".into())
}
pub fn parse_lista(
    valor: Value,
    loja: LojaId,
    permite_url: fn(&Url) -> bool,
) -> Result<Vec<OfertaBruta>, String> {
    let ofertas: Vec<OfertaPagina> = serde_json::from_value(valor)
        .map_err(|_| "A página da loja não retornou ofertas reconhecidas.".to_owned())?;
    ofertas
        .into_iter()
        .map(|pagina| {
            let url = Url::parse(&pagina.url).map_err(|_| "URL do produto inválida.")?;
            if !permite_url(&url)
                || pagina.id_externo.trim().is_empty()
                || pagina.titulo.trim().is_empty()
            {
                return Err("Oferta sem identidade, título ou URL permitida.".into());
            }
            let preco = dinheiro(&pagina.preco)?;
            let frete = pagina.frete.as_deref().map(dinheiro).transpose()?;
            let parcelado = pagina
                .preco_parcelado
                .as_deref()
                .map(dinheiro)
                .transpose()?;
            if preco <= Decimal::ZERO
                || frete.is_some_and(|valor| valor < Decimal::ZERO)
                || parcelado.is_some_and(|valor| valor <= Decimal::ZERO)
            {
                return Err("Preço precisa ser positivo e frete não pode ser negativo.".into());
            }
            let vendedor_direto = match loja {
                LojaId::Kabum => pagina.vendedor.eq_ignore_ascii_case("KaBuM!"),
                LojaId::Pichau => ["Pichau", "Pichau Informática"]
                    .iter()
                    .any(|nome| pagina.vendedor.eq_ignore_ascii_case(nome)),
                _ => false,
            };
            Ok(OfertaBruta {
                id_externo: pagina.id_externo,
                titulo: pagina.titulo,
                url: pagina.url,
                preco,
                frete,
                preco_parcelado: parcelado,
                gtin: pagina.gtin.filter(|gtin| {
                    [8, 12, 13, 14].contains(&gtin.len())
                        && gtin.bytes().all(|byte| byte.is_ascii_digit())
                }),
                modelo: pagina.modelo.filter(|modelo| !modelo.trim().is_empty()),
                marca: pagina.marca.filter(|marca| !marca.trim().is_empty()),
                vendedor: pagina.vendedor,
                condicao: match pagina.condicao.as_str() {
                    "new" => "new",
                    "used" => "used",
                    "refurbished" => "refurbished",
                    _ => "unknown",
                }
                .into(),
                nota: pagina
                    .nota
                    .filter(|nota| nota.is_finite() && (0.0..=5.0).contains(nota)),
                qtd_avaliacoes: pagina.qtd_avaliacoes,
                vendas: None,
                reputacao: (pagina.venda_direta && vendedor_direto).then_some(1.0),
                preco_variavel: false,
            })
        })
        .collect()
}
pub fn parse_detalhe(
    valor: Value,
    loja: LojaId,
    permite_url: fn(&Url) -> bool,
) -> Result<OfertaBruta, String> {
    let mut ofertas = parse_lista(serde_json::json!([valor]), loja, permite_url)?;
    Ok(ofertas.remove(0))
}
pub struct FonteVarejoFixture {
    pub dados: &'static str,
    pub parser: fn(Value) -> Result<Vec<OfertaBruta>, String>,
}
#[async_trait]
impl FonteLoja for FonteVarejoFixture {
    async fn listar(&self, _: &str) -> Result<Vec<OfertaBruta>, String> {
        (self.parser)(serde_json::from_str(self.dados).map_err(|_| "Fixture da loja inválida.")?)
    }
}
