use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SearchWeights {
    pub preco: f64,
    pub nota: f64,
    pub vendedor: f64,
    pub relevancia: f64,
}

impl Default for SearchWeights {
    fn default() -> Self {
        Self {
            preco: 0.4,
            nota: 0.3,
            vendedor: 0.2,
            relevancia: 0.1,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SearchConfig {
    pub termo: String,
    #[serde(default)]
    pub lojas: Vec<crate::lojas::LojaId>,
    #[serde(
        default,
        deserialize_with = "rust_decimal::serde::arbitrary_precision_option::deserialize"
    )]
    pub preco_max: Option<rust_decimal::Decimal>,
    #[serde(default = "default_condition")]
    pub condicao: String,
    #[serde(default)]
    pub obrigatorio: Vec<String>,
    #[serde(default)]
    pub proibido: Vec<String>,
    #[serde(default = "default_min_reviews")]
    pub min_avaliacoes: u32,
    #[serde(default)]
    pub pesos: SearchWeights,
    #[serde(skip)]
    pub sinonimos: BTreeMap<String, Vec<String>>,
}

fn default_condition() -> String {
    "new".to_owned()
}
fn default_min_reviews() -> u32 {
    10
}

impl SearchConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.termo.trim().is_empty() {
            return Err("Informe o termo da busca.".into());
        }
        if self
            .preco_max
            .is_some_and(|value| value <= rust_decimal::Decimal::ZERO)
        {
            return Err("O preço máximo precisa ser maior que zero.".into());
        }
        let weights = [
            &self.pesos.preco,
            &self.pesos.nota,
            &self.pesos.vendedor,
            &self.pesos.relevancia,
        ];
        if weights
            .iter()
            .any(|weight| !weight.is_finite() || **weight < 0.0)
        {
            return Err("Os pesos devem ser números não negativos.".into());
        }
        let sum: f64 = weights.iter().map(|weight| **weight).sum();
        if (sum - 1.0).abs() > 0.0001 {
            return Err(format!("A soma dos pesos deve ser 1,00 (atual: {sum:.2})."));
        }
        Ok(())
    }
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            termo: String::new(),
            lojas: vec![],
            preco_max: None,
            condicao: default_condition(),
            obrigatorio: vec![],
            proibido: vec![],
            min_avaliacoes: default_min_reviews(),
            pesos: SearchWeights::default(),
            sinonimos: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AppSettings {
    pub intervalo_coleta_segundos: u64,
    pub pesos_padrao: SearchWeights,
    pub fonte: String,
    pub coleta_periodica_minutos: u64,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            intervalo_coleta_segundos: 2,
            pesos_padrao: SearchWeights::default(),
            fonte: "webview".into(),
            coleta_periodica_minutos: 60,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AnuncioBruto {
    pub id: String,
    pub titulo: String,
    pub preco: f64,
    pub preco_original: Option<f64>,
    pub url: String,
    pub condicao: String,
    pub vendedor: String,
    pub vendedor_id: Option<String>,
    pub vendedor_score: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vendedor_nivel: Option<String>,
    pub nota: Option<f64>,
    pub qtd_avaliacoes: u32,
    pub modelo: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gtin: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub marca: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frete: Option<rust_decimal::Decimal>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProdutoBruto {
    pub id: String,
    pub titulo: String,
    pub preco: f64,
    pub nota: Option<f64>,
    pub qtd_avaliacoes: u32,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gtin: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub marca: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modelo: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frete: Option<rust_decimal::Decimal>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AtributosProduto {
    pub capacidade: Option<f64>,
    pub unidade: Option<String>,
    pub voltagem: Option<u16>,
    pub bluetooth: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OfertaRanking {
    pub anuncio: AnuncioBruto,
    pub titulo_normalizado: String,
    pub nota_bayesiana: f64,
    pub relevancia: f64,
    pub suspeito: bool,
    pub score: f64,
    pub atributos: AtributosProduto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GrupoRanking {
    pub principal: OfertaRanking,
    pub ofertas: Vec<OfertaRanking>,
    pub score: f64,
    pub pareto: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResultadoBusca {
    pub busca: String,
    #[serde(default)]
    pub fonte: String,
    pub executada_em: String,
    pub anuncios_coletados: usize,
    pub anuncios_filtrados: usize,
    pub produtos: Vec<GrupoRanking>,
    #[serde(default)]
    pub degradado: bool,
    #[serde(default)]
    pub tentativas: Vec<TentativaFonte>,
    #[serde(default)]
    pub cache_origem: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TentativaFonte {
    pub fonte: String,
    pub sucesso: bool,
    pub duracao_ms: u64,
    pub erro: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PontoHistorico {
    pub data: String,
    pub preco: f64,
    pub preco_original: Option<f64>,
    pub nota: Option<f64>,
    pub qtd_avaliacoes: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HistoricoProduto {
    pub produto_id: String,
    pub pontos: Vec<PontoHistorico>,
    pub minima_30d: Option<f64>,
    pub media_movel_7d: Option<f64>,
    pub alerta_minima: bool,
    pub promo_suspeita: bool,
}
