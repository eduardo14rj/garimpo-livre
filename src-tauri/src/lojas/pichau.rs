use super::{FonteDaLoja, Loja, LojaId, OfertaBruta};
use serde_json::Value;
use url::Url;

pub const EXTRATOR: &str = include_str!("varejo_extractor.js");
pub const FIXTURE: &str = include_str!("../../tests/fixtures/pichau/ofertas.json");
pub struct LojaPichau {
    fontes: Vec<FonteDaLoja>,
}
impl LojaPichau {
    pub fn nova(fontes: Vec<FonteDaLoja>) -> Self {
        Self { fontes }
    }
}
impl Loja for LojaPichau {
    fn id(&self) -> LojaId {
        LojaId::Pichau
    }
    fn fontes(&self) -> Vec<FonteDaLoja> {
        self.fontes.clone()
    }
}
pub fn url_pichau(url: &Url) -> bool {
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url
            .host_str()
            .is_some_and(|host| host == "pichau.com.br" || host.ends_with(".pichau.com.br"))
}
pub fn url_busca(termo: &str) -> Result<Url, String> {
    let mut url = Url::parse("https://www.pichau.com.br/search").unwrap();
    url.query_pairs_mut().append_pair("q", termo);
    Ok(url)
}
pub fn parse_lista(valor: Value) -> Result<Vec<OfertaBruta>, String> {
    super::varejo::parse_lista(valor, LojaId::Pichau, url_pichau)
}
pub fn parse_detalhe(valor: Value) -> Result<OfertaBruta, String> {
    super::varejo::parse_detalhe(valor, LojaId::Pichau, url_pichau)
}
