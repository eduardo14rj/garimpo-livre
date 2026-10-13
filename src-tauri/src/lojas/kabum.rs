use super::{FonteDaLoja, Loja, LojaId, OfertaBruta};
use serde_json::Value;
use url::Url;

pub const EXTRATOR: &str = include_str!("varejo_extractor.js");
pub const FIXTURE: &str = include_str!("../../tests/fixtures/kabum/ofertas.json");
pub struct LojaKabum {
    fontes: Vec<FonteDaLoja>,
}
impl LojaKabum {
    pub fn nova(fontes: Vec<FonteDaLoja>) -> Self {
        Self { fontes }
    }
}
impl Loja for LojaKabum {
    fn id(&self) -> LojaId {
        LojaId::Kabum
    }
    fn fontes(&self) -> Vec<FonteDaLoja> {
        self.fontes.clone()
    }
}
pub fn url_kabum(url: &Url) -> bool {
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url
            .host_str()
            .is_some_and(|host| host == "kabum.com.br" || host.ends_with(".kabum.com.br"))
}
pub fn url_busca(termo: &str) -> Result<Url, String> {
    let mut url = Url::parse("https://www.kabum.com.br").unwrap();
    let slug = termo.split_whitespace().collect::<Vec<_>>().join("-");
    url.path_segments_mut()
        .map_err(|_| "URL de busca KaBuM! inválida.")?
        .extend(["busca", &slug]);
    Ok(url)
}
pub fn parse_lista(valor: Value) -> Result<Vec<OfertaBruta>, String> {
    super::varejo::parse_lista(valor, LojaId::Kabum, url_kabum)
}
pub fn parse_detalhe(valor: Value) -> Result<OfertaBruta, String> {
    super::varejo::parse_detalhe(valor, LojaId::Kabum, url_kabum)
}
