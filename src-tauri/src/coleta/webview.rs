use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use tauri::{
    AppHandle, Emitter, Listener, Manager, WebviewUrl, Wry,
    webview::{NewWindowFeatures, NewWindowResponse, PageLoadEvent, WebviewWindowBuilder},
};
use tokio::sync::mpsc;
use url::Url;

use super::{ColetaErro, Fonte, RateLimiter};
use crate::models::{AnuncioBruto, ProdutoBruto};

pub const EVENTO_COLETA: &str = "garimpo:coleta";
const TIMEOUT_PAGINA: Duration = Duration::from_secs(30);

/// Ao cancelar o future (timeout multi-loja), remove listeners e janelas da tentativa.
struct SessaoColetor {
    app: AppHandle,
    listener: tauri::EventId,
    fechar_ao_sair: bool,
}
impl Drop for SessaoColetor {
    fn drop(&mut self) {
        self.app.unlisten(self.listener);
        if self.fechar_ao_sair {
            for window in self.app.webview_windows().into_values().filter(|window| {
                window.label() == "collector" || window.label().starts_with("collector-popup-")
            }) {
                let _ = window.close();
            }
        }
    }
}

pub fn script_extracao() -> &'static str {
    include_str!("extractor.js")
}

pub struct FonteWebview {
    app: AppHandle,
    limite: RateLimiter,
}

pub struct FonteWebviewAssistida {
    fonte: FonteWebview,
}

impl FonteWebviewAssistida {
    pub fn new(app: AppHandle, limite: RateLimiter) -> Self {
        Self {
            fonte: FonteWebview::new(app.clone(), limite),
        }
    }
}

impl FonteWebview {
    pub fn new(app: AppHandle, limite: RateLimiter) -> Self {
        Self { app, limite }
    }

    async fn abrir_e_aguardar(&self, url: &str, esperado: &str) -> Result<Value, ColetaErro> {
        let parsed = Url::parse(url).map_err(|error| ColetaErro::Webview(error.to_string()))?;
        if !mercado_livre_url(&parsed) {
            return Err(ColetaErro::Webview(
                "a navegação foi bloqueada fora dos domínios do Mercado Livre".into(),
            ));
        }

        self.limite.wait_turn().await;
        if let Some(old) = self.app.get_webview_window("collector") {
            let _ = old.close();
        }
        let (sender, mut receiver) = mpsc::unbounded_channel::<Value>();
        let closed_sender = sender.clone();
        let listener = self.app.listen(EVENTO_COLETA, move |event| {
            if let Ok(payload) = serde_json::from_str::<Value>(event.payload()) {
                let _ = sender.send(payload);
            }
        });
        let mut sessao = SessaoColetor {
            app: self.app.clone(),
            listener,
            fechar_ao_sair: true,
        };
        let app = self.app.clone();
        let popup_app = self.app.clone();
        let navigation_app = self.app.clone();
        let profile_dir = self
            .app
            .path()
            .app_data_dir()
            .map_err(|error| ColetaErro::Webview(error.to_string()))?
            .join("webview")
            .join("ml");
        std::fs::create_dir_all(&profile_dir)
            .map_err(|error| ColetaErro::Webview(error.to_string()))?;
        let window =
            WebviewWindowBuilder::new(&self.app, "collector", WebviewUrl::External(parsed))
                .title("Mercado Livre — Garimpo Livre")
                .inner_size(1180.0, 760.0)
                .data_directory(profile_dir)
                .visible(false)
                .focused(false)
                .initialization_script(include_str!("login_navigation.js"))
                .on_navigation(move |url| permitir_navegacao(&navigation_app, url))
                .on_new_window(move |url, features| {
                    abrir_popup_mercado_livre(&popup_app, url, features)
                })
                .on_page_load(|window, payload| {
                    if payload.event() == PageLoadEvent::Finished {
                        let _ = window.eval(script_extracao());
                    }
                })
                .build()
                .map_err(|error| ColetaErro::Webview(error.to_string()))?;
        window.on_window_event(move |event| {
            if matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
                let _ = closed_sender.send(serde_json::json!({"error":"Coleta cancelada ao fechar a janela do Mercado Livre."}));
            }
        });

        let result = async {
            let mut awaiting_user_action = false;
            loop {
                let payload = if awaiting_user_action {
                    receiver.recv().await
                } else {
                    tokio::time::timeout(TIMEOUT_PAGINA, receiver.recv())
                        .await
                        .map_err(|_| ColetaErro::TempoLimite)?
                }
                .ok_or(ColetaErro::TempoLimite)?;
                if payload.get("login_required").and_then(Value::as_bool) == Some(true) {
                    awaiting_user_action = true;
                    if let Some(main) = app.get_webview_window("main") {
                        let _ = main.show();
                    }
                    let _ = window.show();
                    let _ = app.emit("garimpo:login-required", url);
                    continue;
                }
                if payload.get("verification").and_then(Value::as_bool) == Some(true) {
                    awaiting_user_action = true;
                    if let Some(main) = app.get_webview_window("main") {
                        let _ = main.show();
                    }
                    let _ = app.emit("garimpo:verification", url);
                    let _ = window.show();
                    let _ = window.set_focus();
                    continue;
                }
                if let Some(error) = payload.get("error").and_then(Value::as_str) {
                    if awaiting_user_action
                        && error == "Não foi possível ler os anúncios da página do Mercado Livre. Nenhum resultado foi salvo."
                    {
                        continue;
                    }
                    let _ = window.show();
                    return Err(ColetaErro::Webview(error.to_owned()));
                }
                if payload.get("kind").and_then(Value::as_str) == Some(esperado) {
                    if let Some(strategies) = payload.get("estrategias")
                        && let Ok(data_dir) = app.path().app_data_dir()
                    {
                        let _ = crate::storage::registrar_log_extracao(&data_dir, url, strategies);
                    }
                    return Ok(payload);
                }
            }
        }
        .await;

        sessao.fechar_ao_sair = false;
        if result.is_ok() {
            let _ = window.close();
            for popup in self
                .app
                .webview_windows()
                .into_values()
                .filter(|window| window.label().starts_with("collector-popup-"))
            {
                let _ = popup.close();
            }
        }
        result
    }
}

fn abrir_popup_mercado_livre(
    app: &AppHandle,
    url: Url,
    features: NewWindowFeatures,
) -> NewWindowResponse<Wry> {
    if !url_popup_mercado_livre_permitida(&url) {
        let host = url.host_str().unwrap_or("desconhecido");
        let _ = app.emit(
            "garimpo:popup-failed",
            format!(
                "O Mercado Livre tentou abrir uma página fora dos domínios permitidos ({host})."
            ),
        );
        return NewWindowResponse::Deny;
    }

    let label = format!("collector-popup-{}", fastrand::u64(..));
    let popup_app = app.clone();
    let navigation_app = app.clone();
    // O WebView2 exige que o WebView associado ao evento NewWindowRequested
    // ainda esteja em about:blank; ele próprio navega esse WebView para `url`.
    let blank_url = Url::parse("about:blank").expect("URL about:blank válida");
    match WebviewWindowBuilder::new(app, &label, WebviewUrl::External(blank_url))
        .window_features(features)
        .title("Acessar sua conta do Mercado Livre")
        .initialization_script(include_str!("login_navigation.js"))
        .on_navigation(move |url| permitir_navegacao_popup(&navigation_app, url))
        .on_new_window(move |url, features| abrir_popup_mercado_livre(&popup_app, url, features))
        .on_page_load(|window, payload| {
            if payload.event() == PageLoadEvent::Finished {
                let _ = window.eval(script_extracao());
            }
        })
        .build()
    {
        Ok(window) => NewWindowResponse::Create { window },
        Err(error) => {
            let host = url.host_str().unwrap_or("desconhecido");
            let message =
                format!("Não foi possível preparar a janela do Mercado Livre ({host}): {error}");
            eprintln!("{message}");
            let _ = app.emit("garimpo:popup-failed", message);
            // Deixa o WebView2 abrir a janela padrão como contingência, em vez
            // de transformar uma falha de criação em um clique sem resposta.
            NewWindowResponse::Allow
        }
    }
}

fn mercado_livre_url(url: &Url) -> bool {
    url.scheme() == "https" && url.host_str().is_some_and(mercado_livre_host)
}

fn url_popup_mercado_livre_permitida(url: &Url) -> bool {
    url.as_str() == "about:blank" || mercado_livre_url(url)
}

fn permitir_navegacao(app: &AppHandle, url: &Url) -> bool {
    permitir_url(app, url, mercado_livre_url(url))
}

fn permitir_navegacao_popup(app: &AppHandle, url: &Url) -> bool {
    permitir_url(app, url, url_popup_mercado_livre_permitida(url))
}

fn permitir_url(app: &AppHandle, url: &Url, permitida: bool) -> bool {
    if !permitida {
        let host = url.host_str().unwrap_or("sem domínio");
        let _ = app.emit(
            "garimpo:navigation-blocked",
            format!("{}://{}{}", url.scheme(), host, url.path()),
        );
    }
    permitida
}

fn mercado_livre_host(host: &str) -> bool {
    [
        "mercadolivre.com.br",
        "mercadolivre.com",
        "mercadolibre.com",
    ]
    .iter()
    .any(|domain| host == *domain || host.ends_with(&format!(".{domain}")))
}

#[derive(Debug, Deserialize)]
struct ExtracaoLista {
    anuncios: Vec<AnuncioBruto>,
}

#[derive(Debug, Deserialize)]
struct ExtracaoDetalhe {
    produto: ProdutoBruto,
}

#[async_trait]
impl Fonte for FonteWebview {
    async fn listar(&self, termo: &str) -> Result<Vec<AnuncioBruto>, ColetaErro> {
        let slug = crate::core::normalizar(termo).replace(' ', "-");
        let encoded: String = url::form_urlencoded::byte_serialize(slug.as_bytes()).collect();
        let url = format!("https://lista.mercadolivre.com.br/{encoded}");
        let payload = self.abrir_e_aguardar(&url, "list").await?;
        let result: ExtracaoLista = serde_json::from_value(payload)
            .map_err(|error| ColetaErro::Webview(error.to_string()))?;
        Ok(result.anuncios)
    }

    async fn detalhar(&self, url: &str) -> Result<ProdutoBruto, ColetaErro> {
        let payload = self.abrir_e_aguardar(url, "product").await?;
        let result: ExtracaoDetalhe = serde_json::from_value(payload)
            .map_err(|error| ColetaErro::Webview(error.to_string()))?;
        Ok(result.produto)
    }
}

#[async_trait]
impl Fonte for FonteWebviewAssistida {
    async fn listar(&self, termo: &str) -> Result<Vec<AnuncioBruto>, ColetaErro> {
        self.fonte.listar(termo).await
    }

    async fn detalhar(&self, url: &str) -> Result<ProdutoBruto, ColetaErro> {
        self.fonte.detalhar(url).await
    }
}
