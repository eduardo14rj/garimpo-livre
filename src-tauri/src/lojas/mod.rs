pub mod aliexpress;
pub mod amazon;
pub mod fixture;
pub mod kabum;
pub mod manual;
pub mod pichau;
mod varejo;
pub use varejo::FonteVarejoFixture;
pub mod ebay;
mod ml;
mod orchestrator;
pub mod shopee;
pub mod webview;

pub use ml::LojaMl;
pub use orchestrator::{EventoLoja, OrquestradorMultiLoja, RecursosFontes};

use async_trait::async_trait;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt, str::FromStr, sync::Arc, time::Duration};

#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum LojaId {
    Ml,
    Ebay,
    Amazon,
    Shopee,
    Aliexpress,
    Kabum,
    Pichau,
}

impl LojaId {
    pub const TODAS: [Self; 7] = [
        Self::Ml,
        Self::Ebay,
        Self::Amazon,
        Self::Shopee,
        Self::Aliexpress,
        Self::Kabum,
        Self::Pichau,
    ];
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ml => "ml",
            Self::Ebay => "ebay",
            Self::Amazon => "amazon",
            Self::Shopee => "shopee",
            Self::Aliexpress => "aliexpress",
            Self::Kabum => "kabum",
            Self::Pichau => "pichau",
        }
    }
    pub fn nome(self) -> &'static str {
        match self {
            Self::Ml => "Mercado Livre",
            Self::Ebay => "eBay",
            Self::Amazon => "Amazon",
            Self::Shopee => "Shopee",
            Self::Aliexpress => "AliExpress",
            Self::Kabum => "KaBuM!",
            Self::Pichau => "Pichau",
        }
    }
}
impl fmt::Display for LojaId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
impl From<LojaId> for String {
    fn from(value: LojaId) -> Self {
        value.to_string()
    }
}
impl FromStr for LojaId {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::TODAS
            .into_iter()
            .find(|id| id.as_str() == value)
            .ok_or_else(|| {
                format!(
                    "Loja desconhecida: {value}. Escolha {}.",
                    Self::TODAS.map(|id| id.as_str()).join(", ")
                )
            })
    }
}
impl TryFrom<String> for LojaId {
    type Error = String;
    fn try_from(value: String) -> Result<Self, String> {
        value.parse()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConfigLoja {
    pub id: LojaId,
    pub nome: String,
    pub ativa: bool,
    pub padrao: bool,
}
impl ConfigLoja {
    pub fn inicial(id: LojaId) -> Self {
        Self {
            id,
            nome: id.nome().into(),
            ativa: true,
            padrao: id == LojaId::Ml,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OfertaBruta {
    pub id_externo: String,
    pub titulo: String,
    pub url: String,
    pub gtin: Option<String>,
    pub modelo: Option<String>,
    pub marca: Option<String>,
    pub preco: Decimal,
    pub frete: Option<Decimal>,
    pub condicao: String,
    pub vendedor: String,
    pub nota: Option<f64>,
    pub qtd_avaliacoes: Option<u32>,
    pub vendas: Option<u32>,
    pub reputacao: Option<f64>,
    pub preco_variavel: bool,
    pub preco_parcelado: Option<Decimal>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OfertaNormalizada {
    pub id: String,
    pub loja: LojaId,
    pub titulo: String,
    pub url: String,
    pub gtin: Option<String>,
    pub modelo: Option<String>,
    pub marca: Option<String>,
    pub preco_brl: Decimal,
    pub frete_brl: Option<Decimal>,
    pub preco_final_brl: Decimal,
    pub condicao: String,
    pub vendedor: String,
    pub nota: Option<f64>,
    pub qtd_avaliacoes: Option<u32>,
    pub vendas: Option<u32>,
    pub reputacao: Option<f64>,
    pub preco_variavel: bool,
    pub preco_parcelado: Option<Decimal>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "estado", rename_all = "snake_case")]
pub enum StatusLoja {
    Ok,
    Degradado {
        fonte: String,
        capturado_em: Option<String>,
    },
    Falhou {
        erro: String,
    },
    Desativada,
}

// Resultados antigos ainda podem conter `sem_credencial`. Convertemos esse
// estado removido em um aviso visível sem descartar os anúncios salvos.
#[derive(Deserialize)]
#[serde(tag = "estado", rename_all = "snake_case")]
enum StatusLojaPersistido {
    Ok,
    Degradado {
        fonte: String,
        capturado_em: Option<String>,
    },
    Falhou {
        erro: String,
    },
    Desativada,
    SemCredencial,
}

impl<'de> Deserialize<'de> for StatusLoja {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(match StatusLojaPersistido::deserialize(deserializer)? {
            StatusLojaPersistido::Ok => Self::Ok,
            StatusLojaPersistido::Degradado {
                fonte,
                capturado_em,
            } => Self::Degradado {
                fonte,
                capturado_em,
            },
            StatusLojaPersistido::Falhou { erro } => Self::Falhou { erro },
            StatusLojaPersistido::Desativada => Self::Desativada,
            StatusLojaPersistido::SemCredencial => Self::Degradado {
                fonte: "Resultado antigo; execute nova coleta".into(),
                capturado_em: None,
            },
        })
    }
}

impl StatusLoja {
    pub fn observacao_nova(&self) -> bool {
        matches!(
            self,
            Self::Ok
                | Self::Degradado {
                    capturado_em: None,
                    ..
                }
        )
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ResultadoMultiLoja {
    pub ofertas: Vec<OfertaNormalizada>,
    pub status: BTreeMap<LojaId, StatusLoja>,
}

#[derive(Debug, thiserror::Error)]
#[error("não foi possível normalizar a oferta: {0}")]
pub struct ErroNormalizacao(pub String);

#[derive(Clone, Default)]
pub struct ContextoNormalizacao;

impl ContextoNormalizacao {
    pub fn normalizar(
        &self,
        loja: LojaId,
        bruto: OfertaBruta,
    ) -> Result<OfertaNormalizada, ErroNormalizacao> {
        if bruto.id_externo.is_empty()
            || bruto.titulo.trim().is_empty()
            || !url::Url::parse(&bruto.url).is_ok_and(|url| url.scheme() == "https")
            || bruto.preco <= Decimal::ZERO
            || bruto.frete.is_some_and(|frete| frete < Decimal::ZERO)
            || bruto
                .reputacao
                .is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value))
            || bruto
                .nota
                .is_some_and(|value| !value.is_finite() || !(0.0..=5.0).contains(&value))
        {
            return Err(ErroNormalizacao(
                "campos obrigatórios, preço, URL, nota ou reputação inválidos".into(),
            ));
        }
        let total = bruto
            .preco
            .checked_add(bruto.frete.unwrap_or_default())
            .ok_or_else(|| {
                ErroNormalizacao("preço mais frete excedem a precisão decimal".into())
            })?;
        Ok(OfertaNormalizada {
            id: format!(
                "{loja}:{}",
                bruto
                    .id_externo
                    .strip_prefix(&format!("{loja}:"))
                    .unwrap_or(&bruto.id_externo)
            ),
            loja,
            titulo: bruto.titulo,
            url: bruto.url,
            gtin: bruto.gtin,
            modelo: bruto.modelo,
            marca: bruto.marca,
            preco_brl: bruto.preco,
            frete_brl: bruto.frete,
            preco_final_brl: total,
            condicao: bruto.condicao,
            vendedor: bruto.vendedor,
            nota: bruto.nota,
            qtd_avaliacoes: bruto.qtd_avaliacoes,
            vendas: bruto.vendas,
            reputacao: bruto.reputacao,
            preco_variavel: bruto.preco_variavel,
            preco_parcelado: bruto.preco_parcelado,
        })
    }
}

#[async_trait]
pub trait FonteLoja: Send + Sync {
    async fn listar(&self, termo: &str) -> Result<Vec<OfertaBruta>, String>;
    async fn detalhar(&self, _url_ou_id: &str) -> Result<OfertaBruta, String> {
        Err("Esta fonte não fornece detalhe de produto.".into())
    }
    fn controla_rate_limit(&self) -> bool {
        false
    }
}

#[derive(Clone)]
pub struct FonteDaLoja {
    pub nome: String,
    pub fonte: Arc<dyn FonteLoja>,
    pub timeout: Duration,
    pub degradada: bool,
    pub disponivel: bool,
    pub capturado_em: Option<String>,
    pub cacheavel: bool,
}
impl FonteDaLoja {
    pub fn nova(nome: impl Into<String>, fonte: Arc<dyn FonteLoja>) -> Self {
        Self {
            nome: nome.into(),
            fonte,
            timeout: Duration::from_secs(15),
            degradada: false,
            disponivel: true,
            capturado_em: None,
            cacheavel: true,
        }
    }
}

pub trait Loja: Send + Sync {
    fn id(&self) -> LojaId;
    fn fontes(&self) -> Vec<FonteDaLoja>;
    fn normalizar(
        &self,
        bruto: OfertaBruta,
        contexto: &ContextoNormalizacao,
    ) -> Result<OfertaNormalizada, ErroNormalizacao> {
        contexto.normalizar(self.id(), bruto)
    }
    fn reputacao_0_1(&self, bruto: &OfertaBruta) -> Option<f64> {
        bruto.reputacao
    }
}

#[derive(Default)]
pub struct RegistroLojas {
    lojas: BTreeMap<LojaId, Arc<dyn Loja>>,
}
impl RegistroLojas {
    pub(crate) fn desabilitar_cache(&mut self, id: LojaId) -> Result<(), String> {
        let loja = self.obter(id)?;
        let fontes = loja
            .fontes()
            .into_iter()
            .map(|mut f| {
                f.cacheavel = false;
                f
            })
            .collect();
        self.lojas
            .insert(id, Arc::new(LojaComFontes { loja, fontes }));
        Ok(())
    }
    pub fn acrescentar_fallback(&mut self, id: LojaId, fonte: FonteDaLoja) -> Result<(), String> {
        let loja = self.obter(id)?;
        self.lojas
            .insert(id, Arc::new(LojaComFallback { loja, fonte }));
        Ok(())
    }
    pub fn registrar(&mut self, loja: Box<dyn Loja>) -> Result<(), String> {
        let id = loja.id();
        if self.lojas.contains_key(&id) {
            return Err(format!("Loja {id} já registrada."));
        }
        self.lojas.insert(id, Arc::from(loja));
        Ok(())
    }
    pub fn obter(&self, id: LojaId) -> Result<Arc<dyn Loja>, String> {
        self.lojas
            .get(&id)
            .cloned()
            .ok_or_else(|| format!("Loja {id} não está disponível nesta versão."))
    }
    pub fn selecionar(
        &self,
        solicitadas: &[LojaId],
        config: &[ConfigLoja],
    ) -> Result<Vec<LojaId>, String> {
        let ids = if solicitadas.is_empty() {
            config
                .iter()
                .filter(|loja| loja.padrao)
                .map(|loja| loja.id)
                .collect()
        } else {
            solicitadas.to_vec()
        };
        let ids = ids.into_iter().collect::<std::collections::BTreeSet<_>>();
        if ids.is_empty() {
            return Err("Escolha ao menos uma loja ou configure as lojas padrão.".into());
        }
        for id in &ids {
            self.obter(*id)?;
        }
        Ok(ids.into_iter().collect())
    }
}

struct LojaComFontes {
    loja: Arc<dyn Loja>,
    fontes: Vec<FonteDaLoja>,
}
impl Loja for LojaComFontes {
    fn id(&self) -> LojaId {
        self.loja.id()
    }
    fn fontes(&self) -> Vec<FonteDaLoja> {
        self.fontes.clone()
    }
    fn normalizar(
        &self,
        bruto: OfertaBruta,
        contexto: &ContextoNormalizacao,
    ) -> Result<OfertaNormalizada, ErroNormalizacao> {
        self.loja.normalizar(bruto, contexto)
    }
    fn reputacao_0_1(&self, bruto: &OfertaBruta) -> Option<f64> {
        self.loja.reputacao_0_1(bruto)
    }
}
struct LojaComFallback {
    loja: Arc<dyn Loja>,
    fonte: FonteDaLoja,
}
impl Loja for LojaComFallback {
    fn id(&self) -> LojaId {
        self.loja.id()
    }
    fn fontes(&self) -> Vec<FonteDaLoja> {
        let mut fontes = self.loja.fontes();
        fontes.push(self.fonte.clone());
        fontes
    }
    fn normalizar(
        &self,
        bruto: OfertaBruta,
        contexto: &ContextoNormalizacao,
    ) -> Result<OfertaNormalizada, ErroNormalizacao> {
        self.loja.normalizar(bruto, contexto)
    }
    fn reputacao_0_1(&self, bruto: &OfertaBruta) -> Option<f64> {
        self.loja.reputacao_0_1(bruto)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn busca_antiga_e_desconhecida() {
        let busca: crate::models::SearchConfig =
            serde_json::from_str(r#"{"termo":"ssd"}"#).unwrap();
        assert!(busca.lojas.is_empty());
        let erro = serde_json::from_str::<crate::models::SearchConfig>(
            r#"{"termo":"ssd","lojas":["ml","xpto"]}"#,
        )
        .unwrap_err();
        assert!(erro.to_string().contains("Loja desconhecida: xpto"));
        for id in LojaId::TODAS {
            assert_eq!(id.to_string().parse::<LojaId>().unwrap(), id);
        }
        let busca: crate::models::SearchConfig =
            serde_json::from_str(r#"{"termo":"ssd","preco_max":1234567890123.123456789}"#).unwrap();
        assert_eq!(
            busca.preco_max.unwrap().to_string(),
            "1234567890123.123456789"
        );
        assert!(
            serde_json::to_string(&busca)
                .unwrap()
                .contains("\"1234567890123.123456789\"")
        );
    }

    #[test]
    fn normalizacao_preserva_preco_e_frete_mostrados_sem_calculo_externo() {
        let ad =
            crate::coleta::parse_listagem_html(include_str!("../../tests/fixtures/listagem.html"))
                .unwrap()
                .remove(0);
        let mut bruto = LojaMl::adaptar_anuncio(ad).unwrap();
        bruto.id_externo = "123".into();
        bruto.preco = "99.90".parse().unwrap();
        bruto.frete = Some("10.01".parse().unwrap());
        let nacional = ContextoNormalizacao
            .normalizar(LojaId::Kabum, bruto)
            .unwrap();
        assert_eq!(nacional.preco_final_brl, "109.91".parse().unwrap());
        assert_eq!(nacional.preco_brl, "99.90".parse().unwrap());
        assert_eq!(nacional.frete_brl, Some("10.01".parse().unwrap()));
    }
}
