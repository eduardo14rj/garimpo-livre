//! Transporte compartilhado; URL, parser e extrator pertencem a cada loja.
use super::{FonteLoja, LojaId, OfertaBruta};
use crate::coleta::RateLimiter;
use async_trait::async_trait;
use serde_json::Value;
use std::time::Duration;
use tauri::{
    AppHandle, Emitter, Listener, Manager, WebviewUrl, Wry,
    webview::{NewWindowFeatures, NewWindowResponse, PageLoadEvent, WebviewWindowBuilder},
};
use tokio::sync::mpsc;
use url::Url;

#[derive(Clone, Copy)]
pub struct ExtratorLoja {
    pub script: &'static str,
    pub permite_url: fn(&Url) -> bool,
    pub url_busca: fn(&str) -> Result<Url, String>,
    pub parse_lista: fn(Value) -> Result<Vec<OfertaBruta>, String>,
    pub parse_detalhe: Option<fn(Value) -> Result<OfertaBruta, String>>,
}

pub struct FonteWebviewLoja {
    app: AppHandle,
    loja: LojaId,
    extrator: ExtratorLoja,
    limite: RateLimiter,
    assistida: bool,
}
impl FonteWebviewLoja {
    pub fn nova(app: AppHandle, loja: LojaId, extrator: ExtratorLoja, limite: RateLimiter) -> Self {
        Self {
            app,
            loja,
            extrator,
            limite,
            assistida: false,
        }
    }
    pub fn assistida(mut self) -> Self {
        self.assistida = true;
        self
    }
    async fn coletar(&self, url: Url, kind: &str) -> Result<Value, String> {
        if !(self.extrator.permite_url)(&url) {
            return Err("Endereço fora dos domínios da loja.".into());
        }
        self.limite.wait_turn().await;
        let label = format!("store-collector-{}-{}", self.loja, fastrand::u128(..));
        let evento = format!("garimpo:store-extraction-{}", fastrand::u128(..));
        let script = script_transporte(self.extrator.script, &evento);
        let (sender, mut receiver) = mpsc::unbounded_channel::<Value>();
        let closed_sender = sender.clone();
        let listener = self.app.listen(evento, move |event| {
            if let Ok(payload) = serde_json::from_str(event.payload()) {
                let _ = sender.send(payload);
            }
        });
        let _sessao = Sessao {
            app: self.app.clone(),
            label: label.clone(),
            listener,
        };
        let app_popup = self.app.clone();
        let label_popup = label.clone();
        let script_popup = script.clone();
        let permite = self.extrator.permite_url;
        let app_navegacao = self.app.clone();
        let loja = self.loja;
        let profile_dir = self
            .app
            .path()
            .app_data_dir()
            .map_err(|error| error.to_string())?
            .join("webview")
            .join(loja.as_str());
        std::fs::create_dir_all(&profile_dir).map_err(|error| error.to_string())?;
        let window =
            WebviewWindowBuilder::new(&self.app, &label, WebviewUrl::External(url.clone()))
                .title(format!("{} — Garimpo Livre", loja.nome()))
                .inner_size(1180.0, 760.0)
                .data_directory(profile_dir)
                .visible(self.assistida)
                .focused(self.assistida)
                .on_navigation(move |url| {
                    permitir_navegacao(&app_navegacao, loja, url, permite, false)
                })
                .on_new_window(move |url, features| {
                    popup(
                        &app_popup,
                        loja,
                        &label_popup,
                        &script_popup,
                        permite,
                        url,
                        features,
                    )
                })
                .on_page_load(move |window, payload| {
                    if payload.event() == PageLoadEvent::Finished {
                        let _ = window.eval(&script);
                    }
                })
                .build()
                .map_err(|error| format!("Não foi possível abrir {}: {error}", self.loja.nome()))?;
        let closed_url = url.to_string();
        window.on_window_event(move |event| {
            if matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
                let _ = closed_sender.send(serde_json::json!({"url":closed_url,"cancelled":true}));
            }
        });
        let mut aguardando = false;
        loop {
            let payload = if aguardando {
                receiver.recv().await
            } else {
                tokio::time::timeout(Duration::from_secs(30), receiver.recv())
                    .await
                    .map_err(|_| format!("Tempo limite ao ler {}.", self.loja.nome()))?
            }
            .ok_or("A janela de coleta foi encerrada.")?;
            let pagina_valida = payload
                .get("url")
                .and_then(Value::as_str)
                .and_then(|valor| Url::parse(valor).ok())
                .is_some_and(|url| permite(&url));
            if !pagina_valida {
                continue;
            }
            if payload.get("cancelled").and_then(Value::as_bool) == Some(true) {
                return Err(format!(
                    "Coleta de {} cancelada ao fechar a janela.",
                    self.loja.nome()
                ));
            }
            if payload.get("verification").and_then(Value::as_bool) == Some(true)
                || payload.get("login_required").and_then(Value::as_bool) == Some(true)
            {
                aguardando = true;
                window.show().map_err(|error| error.to_string())?;
                let _ = window.set_focus();
                let _ = self.app.emit("garimpo:loja-requer-acao", serde_json::json!({
                    "loja":self.loja, "nome":self.loja.nome(), "url":url.as_str(),
                    "tipo":if payload.get("login_required").and_then(Value::as_bool)==Some(true) {"login"} else {"verification"}
                }));
                continue;
            }
            if let Some(error) = payload.get("error").and_then(Value::as_str) {
                if aguardando {
                    continue;
                }
                return Err(error.to_owned());
            }
            if payload.get("kind").and_then(Value::as_str) == Some(kind) {
                return payload
                    .get("data")
                    .cloned()
                    .ok_or_else(|| "Extrator não retornou os dados da loja.".into());
            }
        }
    }
}
#[async_trait]
impl FonteLoja for FonteWebviewLoja {
    async fn listar(&self, termo: &str) -> Result<Vec<OfertaBruta>, String> {
        let dados = self
            .coletar((self.extrator.url_busca)(termo)?, "list")
            .await?;
        (self.extrator.parse_lista)(dados)
    }
    async fn detalhar(&self, endereco: &str) -> Result<OfertaBruta, String> {
        let parser = self
            .extrator
            .parse_detalhe
            .ok_or("Extrator de detalhe indisponível.")?;
        let url = Url::parse(endereco).map_err(|_| "URL de detalhe inválida.")?;
        parser(self.coletar(url, "product").await?)
    }
    fn controla_rate_limit(&self) -> bool {
        true
    }
}

struct Sessao {
    app: AppHandle,
    label: String,
    listener: tauri::EventId,
}
impl Drop for Sessao {
    fn drop(&mut self) {
        self.app.unlisten(self.listener);
        let popup_prefix = format!("{}-popup-", self.label);
        for window in self.app.webview_windows().values().filter(|window| {
            window.label() == self.label || window.label().starts_with(&popup_prefix)
        }) {
            let _ = window.close();
        }
    }
}
fn permitir_navegacao(
    app: &AppHandle,
    loja: LojaId,
    url: &Url,
    permite: fn(&Url) -> bool,
    blank: bool,
) -> bool {
    let permitida = permite(url) || (blank && url.as_str() == "about:blank");
    if !permitida {
        let _ = app.emit(
            "garimpo:loja-navegacao-bloqueada",
            serde_json::json!({"loja":loja,"dominio":url.host_str()}),
        );
    }
    permitida
}
fn popup(
    app: &AppHandle,
    loja: LojaId,
    root: &str,
    script: &str,
    permite: fn(&Url) -> bool,
    url: Url,
    features: NewWindowFeatures,
) -> NewWindowResponse<Wry> {
    if !(permite(&url) || url.as_str() == "about:blank") {
        return NewWindowResponse::Deny;
    }
    let label = format!("{root}-popup-{}", fastrand::u64(..));
    let app_navegacao = app.clone();
    let app_popup = app.clone();
    let root = root.to_owned();
    let script_pagina = script.to_owned();
    let script_popup = script.to_owned();
    match WebviewWindowBuilder::new(
        app,
        &label,
        WebviewUrl::External(Url::parse("about:blank").unwrap()),
    )
    .window_features(features)
    .title(format!("Acesso {} — Garimpo Livre", loja.nome()))
    .on_navigation(move |url| permitir_navegacao(&app_navegacao, loja, url, permite, true))
    .on_new_window(move |url, features| {
        popup(
            &app_popup,
            loja,
            &root,
            &script_popup,
            permite,
            url,
            features,
        )
    })
    .on_page_load(move |window, payload| {
        if payload.event() == PageLoadEvent::Finished {
            let _ = window.eval(&script_pagina);
        }
    })
    .build()
    {
        Ok(window) => NewWindowResponse::Create { window },
        Err(error) => {
            let _ = app.emit(
                "garimpo:loja-janela-falhou",
                format!("Falha ao abrir a janela de {}: {error}", loja.nome()),
            );
            NewWindowResponse::Deny
        }
    }
}

pub fn script_transporte(extrator: &str, evento: &str) -> String {
    include_str!("webview_transport.js")
        .replace(
            "__GARIMPO_STORE_EXTRACTOR__",
            extrator.trim().trim_end_matches(';'),
        )
        .replace(
            "__GARIMPO_STORE_EVENT__",
            &serde_json::to_string(evento).unwrap(),
        )
}
