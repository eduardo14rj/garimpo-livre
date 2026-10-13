mod cache;
mod fixture;
mod manual;
mod orchestrator;
mod parser;
mod timing;
mod webview;

pub use cache::{CacheErro, CachePaginas};
pub use fixture::FonteFixture;
pub use manual::{EntradaManual, FonteManual};
pub use orchestrator::{FonteCache, FonteCandidata, Orquestrador, OrquestradorErro};
pub use parser::{ParserErro, detectar_verificacao, parse_detalhe_html, parse_listagem_html};
pub use timing::{Clock, RateLimiter, RelogioSistema, retry_com_backoff};
pub use webview::{EVENTO_COLETA, FonteWebview, FonteWebviewAssistida, script_extracao};

use crate::models::{AnuncioBruto, ProdutoBruto};
use async_trait::async_trait;

#[derive(Debug, thiserror::Error)]
pub enum ColetaErro {
    #[error("falha ao interpretar o HTML: {0}")]
    Parser(#[from] ParserErro),
    #[error("falha ao acessar o arquivo de fixture: {0}")]
    Arquivo(#[from] std::io::Error),
    #[error("página do Mercado Livre exige verificação manual")]
    Verificacao,
    #[error("tempo limite aguardando a página do Mercado Livre")]
    TempoLimite,
    #[error("falha no WebView: {0}")]
    Webview(String),
    #[error("falha ao acessar fonte ou cache: {0}")]
    Fonte(String),
    #[error("a fonte retornou anúncios inválidos: {0}")]
    ResultadoInvalido(String),
    #[error("a coleta assistida só pode ser iniciada com o app em primeiro plano")]
    UsuarioAusente,
}

#[async_trait]
pub trait Fonte: Send + Sync {
    async fn listar(&self, termo: &str) -> Result<Vec<AnuncioBruto>, ColetaErro>;
    async fn detalhar(&self, url: &str) -> Result<ProdutoBruto, ColetaErro>;

    fn cache_timestamp(&self) -> Option<String> {
        None
    }
}
