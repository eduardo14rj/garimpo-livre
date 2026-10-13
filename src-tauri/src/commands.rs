use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

use chrono::Utc;
use tauri::{
    AppHandle, Emitter, Manager, State, WebviewUrl, Wry,
    webview::{NewWindowFeatures, NewWindowResponse, PageLoadEvent, WebviewWindowBuilder},
};
use tauri_plugin_notification::NotificationExt;
use url::Url;

use crate::{
    coleta::{FonteWebviewAssistida, RelogioSistema},
    models::{AppSettings, SearchConfig},
    storage::{self, Database, SearchFile},
};

pub struct AppState {
    pub data_dir: PathBuf,
    pub database: Arc<Database>,
    pub recursos_fontes: Arc<crate::lojas::RecursosFontes>,
    pub webview_limiter: crate::coleta::RateLimiter,
    running: Mutex<bool>,
}

impl AppState {
    pub fn new(data_dir: PathBuf, database: Database) -> Result<Self, String> {
        let seconds = storage::carregar_config(&data_dir)
            .map(|settings| settings.intervalo_coleta_segundos.max(2))
            .unwrap_or(2);
        let database = Arc::new(database);
        let recursos_fontes = Arc::new(crate::lojas::RecursosFontes::novos(
            database.clone(),
            data_dir.clone(),
        ));
        recursos_fontes.definir_intervalo(seconds);
        Ok(Self {
            data_dir,
            database,
            recursos_fontes,
            webview_limiter: crate::coleta::RateLimiter::new(
                Arc::new(RelogioSistema),
                Duration::from_secs(seconds),
            ),
            running: Mutex::new(false),
        })
    }
}

#[tauri::command]
pub fn continue_store_collector(id: crate::lojas::LojaId, app: AppHandle) -> Result<(), String> {
    let prefixo = format!("store-collector-{id}-");
    let mut encontrou = false;
    for window in app
        .webview_windows()
        .values()
        .filter(|window| window.label().starts_with(&prefixo))
    {
        window
            .eval("window.__garimpoRunStoreExtraction?.()")
            .map_err(|error| error.to_string())?;
        encontrou = true;
    }
    if !encontrou {
        return Err("A janela desta loja não está aberta. Execute a busca novamente.".into());
    }
    Ok(())
}

#[tauri::command]
pub fn list_stores(state: State<'_, AppState>) -> Result<Vec<crate::lojas::ConfigLoja>, String> {
    state
        .database
        .listar_lojas()
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn configure_store(
    id: crate::lojas::LojaId,
    ativa: bool,
    padrao: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .database
        .configurar_loja(id, ativa, padrao)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn open_store(id: crate::lojas::LojaId, app: AppHandle) -> Result<(), String> {
    let label = format!("store-session-{id}");
    let url = Url::parse(home_store_url(id))
        .map_err(|error| format!("Endereço inicial da loja inválido: {error}"))?;
    if let Some(window) = app.get_webview_window(&label) {
        if window
            .url()
            .map(|current| current.as_str() == "about:blank")
            .unwrap_or(false)
        {
            window
                .navigate(url.clone())
                .map_err(|error| error.to_string())?;
        }
        window.show().map_err(|error| error.to_string())?;
        window.set_focus().map_err(|error| error.to_string())?;
        return Ok(());
    }
    let profile = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("webview")
        .join(id.as_str());
    std::fs::create_dir_all(&profile).map_err(|error| error.to_string())?;
    let app_popup = app.clone();
    let root_label = label.clone();
    let popup_profile = profile.clone();
    WebviewWindowBuilder::new(&app, &label, WebviewUrl::External(url))
        .title(format!("{} — Garimpo Livre", id.nome()))
        .inner_size(1180.0, 760.0)
        .data_directory(profile)
        .visible(true)
        .focused(true)
        .on_navigation(|url| url.scheme() == "https" || url.as_str() == "about:blank")
        .on_new_window(move |url, features| {
            open_store_popup(
                &app_popup,
                id,
                &root_label,
                popup_profile.clone(),
                url,
                features,
            )
        })
        .on_page_load(move |window, payload| {
            if payload.event() == PageLoadEvent::Finished {
                let _ = window.show();
                let _ = window.set_focus();
            }
        })
        .build()
        .map_err(|error| format!("Não foi possível abrir {}: {error}", id.nome()))?;
    Ok(())
}

fn open_store_popup(
    app: &AppHandle,
    id: crate::lojas::LojaId,
    root_label: &str,
    profile: PathBuf,
    url: Url,
    features: NewWindowFeatures,
) -> NewWindowResponse<Wry> {
    if url.scheme() != "https" && url.as_str() != "about:blank" {
        return NewWindowResponse::Deny;
    }

    let label = format!("{root_label}-popup-{}", fastrand::u64(..));
    let app_popup = app.clone();
    let nested_root = root_label.to_owned();
    let nested_profile = profile.clone();
    match WebviewWindowBuilder::new(app, &label, WebviewUrl::External(url))
        .window_features(features)
        .title(format!("Acesso {} — Garimpo Livre", id.nome()))
        .data_directory(profile)
        .visible(true)
        .focused(true)
        .on_navigation(|url| url.scheme() == "https" || url.as_str() == "about:blank")
        .on_new_window(move |url, features| {
            open_store_popup(
                &app_popup,
                id,
                &nested_root,
                nested_profile.clone(),
                url,
                features,
            )
        })
        .build()
    {
        Ok(window) => NewWindowResponse::Create { window },
        Err(error) => {
            let _ = app.emit(
                "garimpo:loja-janela-falhou",
                format!("Falha ao abrir a janela de {}: {error}", id.nome()),
            );
            NewWindowResponse::Deny
        }
    }
}

#[tauri::command]
pub async fn clear_store_session(id: crate::lojas::LojaId, app: AppHandle) -> Result<(), String> {
    let collector_prefix = format!("store-collector-{id}-");
    let popup_prefix = format!("store-session-{id}-popup-");
    for window in app.webview_windows().values().filter(|window| {
        window.label() == format!("store-session-{id}")
            || window.label().starts_with(&collector_prefix)
            || window.label().starts_with(&popup_prefix)
    }) {
        let _ = window.close();
    }

    let profile = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("webview")
        .join(id.as_str());
    if !profile.exists() {
        return Ok(());
    }
    let mut last_error = None;
    for _ in 0..10 {
        match std::fs::remove_dir_all(&profile) {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => last_error = Some(error),
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err(format!(
        "Não foi possível limpar a sessão de {}: {}",
        id.nome(),
        last_error.map_or_else(|| "erro desconhecido".into(), |error| error.to_string())
    ))
}

#[tauri::command]
pub fn clear_store_cache(
    id: crate::lojas::LojaId,
    state: State<'_, AppState>,
) -> Result<(), String> {
    {
        let mut running = state
            .running
            .lock()
            .map_err(|_| "Estado da coleta indisponível")?;
        if *running {
            return Err("Aguarde a coleta terminar antes de limpar o cache.".into());
        }
        *running = true;
    }
    let _guard = ColetaEmAndamento(&state.running);
    state.recursos_fontes.invalidar_cache(id)
}

fn home_store_url(id: crate::lojas::LojaId) -> &'static str {
    match id {
        crate::lojas::LojaId::Ml => "https://www.mercadolivre.com.br/",
        crate::lojas::LojaId::Ebay => "https://www.ebay.com/",
        crate::lojas::LojaId::Amazon => "https://www.amazon.com.br/",
        crate::lojas::LojaId::Shopee => "https://shopee.com.br/",
        crate::lojas::LojaId::Aliexpress => "https://pt.aliexpress.com/",
        crate::lojas::LojaId::Kabum => "https://www.kabum.com.br/",
        crate::lojas::LojaId::Pichau => "https://www.pichau.com.br/",
    }
}

struct ColetaEmAndamento<'a>(&'a Mutex<bool>);

#[tauri::command]
pub async fn save_manual_offer(
    slug: String,
    id: crate::lojas::LojaId,
    entrada: crate::lojas::manual::EntradaOfertaManual,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<crate::core::multiloja::RankingMultiLoja, String> {
    storage::validar_slug(&slug).map_err(|e| e.to_string())?;
    {
        let mut running = state.running.lock().map_err(|_| "Estado indisponível.")?;
        if *running {
            return Err("Aguarde a coleta terminar para informar uma oferta manual.".into());
        }
        *running = true;
    }
    let _guard = ColetaEmAndamento(&state.running);
    let mut config: SearchConfig = serde_json::from_slice(
        &std::fs::read(state.data_dir.join("buscas").join(format!("{slug}.json")))
            .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    config.sinonimos = storage::carregar_sinonimos(&state.data_dir).map_err(|e| e.to_string())?;
    config.validate()?;
    let lojas = state.database.listar_lojas().map_err(|e| e.to_string())?;
    if !lojas.iter().any(|loja| {
        loja.id == id
            && loja.ativa
            && (config.lojas.contains(&id) || (config.lojas.is_empty() && loja.padrao))
    }) {
        return Err("Ative e selecione essa loja na busca antes de informar a oferta.".into());
    }
    let bruto = crate::lojas::manual::validar_manual(id, entrada)?;
    let contexto = crate::lojas::ContextoNormalizacao;
    let oferta = contexto
        .normalizar(id, bruto.clone())
        .map_err(|e| e.to_string())?;
    crate::lojas::manual::salvar_captura(&state.data_dir, &slug, &config.termo, id, bruto)?;
    state
        .database
        .salvar_oferta(&slug, &oferta, Utc::now().date_naive())
        .map_err(|e| e.to_string())?;
    let anterior = storage::carregar_ranking_multi(&state.data_dir, &slug)
        .map_err(|e| e.to_string())?
        .filter(|r| !r.demonstracao)
        .unwrap_or_default();
    let mut resultado = crate::lojas::ResultadoMultiLoja {
        ofertas: anterior
            .produtos
            .into_iter()
            .flat_map(|g| g.ofertas.into_iter().map(|o| o.oferta))
            .filter(|o| o.id != oferta.id)
            .collect(),
        status: anterior.status,
    };
    resultado.ofertas.push(oferta);
    resultado.status.insert(
        id,
        crate::lojas::StatusLoja::Degradado {
            fonte: "Manual".into(),
            capturado_em: None,
        },
    );
    let mut ranking = crate::core::multiloja::ranquear_multi(&slug, &config, resultado)?;
    ranking.executada_em = Utc::now().to_rfc3339();
    state
        .database
        .salvar_grupos(&mut ranking)
        .map_err(|e| e.to_string())?;
    storage::salvar_ranking_multi(&state.data_dir, &ranking).map_err(|e| e.to_string())?;
    notificar_minimas_multi(&app, &state, &ranking)?;
    let _ = app.emit("garimpo:ranking-multi", &ranking);
    Ok(ranking)
}
impl Drop for ColetaEmAndamento<'_> {
    fn drop(&mut self) {
        if let Ok(mut running) = self.0.lock() {
            *running = false;
        }
    }
}

#[tauri::command]
pub async fn run_multi_search(
    slug: String,
    source: Option<String>,
    retry_store: Option<crate::lojas::LojaId>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<crate::core::multiloja::RankingMultiLoja, String> {
    executar_busca_multi(&slug, source, retry_store, app, &state).await
}

async fn executar_busca_multi(
    slug: &str,
    source: Option<String>,
    retry_store: Option<crate::lojas::LojaId>,
    app: AppHandle,
    state: &AppState,
) -> Result<crate::core::multiloja::RankingMultiLoja, String> {
    use crate::lojas::{ContextoNormalizacao, OrquestradorMultiLoja};
    storage::validar_slug(slug).map_err(|error| error.to_string())?;
    {
        let mut running = state
            .running
            .lock()
            .map_err(|_| "Estado da coleta indisponível")?;
        if *running {
            return Err("Já existe uma coleta em andamento.".into());
        }
        *running = true;
    }
    let _guard = ColetaEmAndamento(&state.running);
    let prazo = tokio::time::Instant::now() + Duration::from_secs(60);
    let mut config: SearchConfig = serde_json::from_slice(
        &std::fs::read(state.data_dir.join("buscas").join(format!("{slug}.json")))
            .map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    config.sinonimos = storage::carregar_sinonimos(&state.data_dir).map_err(|e| e.to_string())?;
    let settings = storage::carregar_config(&state.data_dir).map_err(|error| error.to_string())?;
    let fonte = source.as_deref().unwrap_or(&settings.fonte);
    if !matches!(fonte, "fixture" | "webview") {
        return Err("Fonte desconhecida.".into());
    }
    if fonte == "fixture" {
        let lojas = state.database.listar_lojas().map_err(|e| e.to_string())?;
        let ranking = crate::lojas::fixture::executar_fixture(
            slug,
            &config,
            &lojas,
            state.recursos_fontes.clone(),
            |evento, ranking| {
                let _ = app.emit(
                    "garimpo:loja-concluida",
                    serde_json::json!({"busca":slug,"resultado":evento}),
                );
                let _ = app.emit("garimpo:ranking-parcial", ranking);
            },
        )
        .await?;
        storage::salvar_ranking_multi(&state.data_dir, &ranking).map_err(|e| e.to_string())?;
        let _ = app.emit("garimpo:ranking-multi", &ranking);
        return Ok(ranking);
    }
    let registro = registrar_fontes_webview(&app, state)?;
    let lojas = state
        .database
        .listar_lojas()
        .map_err(|error| error.to_string())?;
    let contexto = ContextoNormalizacao;
    let runner = OrquestradorMultiLoja::novo(registro, state.recursos_fontes.clone());
    let executada_em = Utc::now().to_rfc3339();
    let mut parcial = crate::lojas::ResultadoMultiLoja::default();
    let result = runner
        .executar_ate_com_retry_manual(&config, &lojas, contexto, prazo, retry_store, |evento| {
            let _ = app.emit(
                "garimpo:loja-concluida",
                serde_json::json!({"busca":slug,"resultado":evento}),
            );
            parcial.status.insert(evento.loja, evento.status.clone());
            parcial.ofertas.extend(evento.ofertas);
            if let Ok(mut ranking) =
                crate::core::multiloja::ranquear_multi(slug, &config, parcial.clone())
            {
                ranking.executada_em = executada_em.clone();
                ranking.demonstracao = fonte == "fixture";
                let _ = app.emit("garimpo:ranking-parcial", ranking);
            }
        })
        .await?;
    for oferta in &result.ofertas {
        if result
            .status
            .get(&oferta.loja)
            .is_some_and(crate::lojas::StatusLoja::observacao_nova)
        {
            state
                .database
                .salvar_oferta(slug, oferta, Utc::now().date_naive())
                .map_err(|error| error.to_string())?;
        }
    }
    let mut ranking = crate::core::multiloja::ranquear_multi(slug, &config, result)?;
    ranking.executada_em = executada_em;
    ranking.demonstracao = fonte == "fixture";
    if !ranking.demonstracao {
        state
            .database
            .salvar_grupos(&mut ranking)
            .map_err(|e| e.to_string())?;
        notificar_minimas_multi(&app, state, &ranking)?;
    }
    storage::salvar_ranking_multi(&state.data_dir, &ranking).map_err(|e| e.to_string())?;
    let _ = app.emit("garimpo:ranking-multi", &ranking);
    Ok(ranking)
}

fn registrar_fontes_webview(
    app: &AppHandle,
    state: &AppState,
) -> Result<crate::lojas::RegistroLojas, String> {
    use crate::lojas::{FonteDaLoja, Loja, LojaId, LojaMl, RegistroLojas, webview::ExtratorLoja};
    let mut registro = RegistroLojas::default();
    for id in LojaId::TODAS {
        if id == LojaId::Ml {
            let mut fonte = LojaMl::adaptar_fonte(
                "WebView",
                Arc::new(FonteWebviewAssistida::new(
                    app.clone(),
                    state.webview_limiter.clone(),
                )),
            );
            fonte.timeout = Duration::from_secs(60);
            registro.registrar(Box::new(LojaMl::nova(vec![fonte])))?;
            continue;
        }

        let extrator = match id {
            LojaId::Ebay => ExtratorLoja {
                script: crate::lojas::ebay::EXTRATOR,
                permite_url: crate::lojas::ebay::url_ebay,
                url_busca: crate::lojas::ebay::url_busca,
                parse_lista: crate::lojas::ebay::parse_busca,
                parse_detalhe: None,
            },
            LojaId::Amazon => ExtratorLoja {
                script: crate::lojas::amazon::EXTRATOR,
                permite_url: crate::lojas::amazon::url_amazon,
                url_busca: crate::lojas::amazon::url_busca,
                parse_lista: crate::lojas::amazon::parse_webview,
                parse_detalhe: None,
            },
            LojaId::Shopee => ExtratorLoja {
                script: crate::lojas::shopee::EXTRATOR,
                permite_url: crate::lojas::shopee::url_shopee,
                url_busca: crate::lojas::shopee::url_busca,
                parse_lista: crate::lojas::shopee::parse_webview,
                parse_detalhe: None,
            },
            LojaId::Aliexpress => ExtratorLoja {
                script: crate::lojas::aliexpress::EXTRATOR,
                permite_url: crate::lojas::aliexpress::url_aliexpress,
                url_busca: crate::lojas::aliexpress::url_busca,
                parse_lista: crate::lojas::aliexpress::parse_webview,
                parse_detalhe: None,
            },
            LojaId::Kabum => ExtratorLoja {
                script: crate::lojas::kabum::EXTRATOR,
                permite_url: crate::lojas::kabum::url_kabum,
                url_busca: crate::lojas::kabum::url_busca,
                parse_lista: crate::lojas::kabum::parse_lista,
                parse_detalhe: Some(crate::lojas::kabum::parse_detalhe),
            },
            LojaId::Pichau => ExtratorLoja {
                script: crate::lojas::pichau::EXTRATOR,
                permite_url: crate::lojas::pichau::url_pichau,
                url_busca: crate::lojas::pichau::url_busca,
                parse_lista: crate::lojas::pichau::parse_lista,
                parse_detalhe: Some(crate::lojas::pichau::parse_detalhe),
            },
            LojaId::Ml => unreachable!(),
        };
        let mut fonte = FonteDaLoja::nova(
            "WebView",
            Arc::new(crate::lojas::webview::FonteWebviewLoja::nova(
                app.clone(),
                id,
                extrator,
                state.recursos_fontes.limitador(id, "WebView"),
            )),
        );
        fonte.timeout = Duration::from_secs(60);
        let loja: Box<dyn Loja> = match id {
            LojaId::Ebay => Box::new(crate::lojas::ebay::LojaEbay::nova(vec![fonte])),
            LojaId::Amazon => Box::new(crate::lojas::amazon::LojaAmazon::nova(vec![fonte])),
            LojaId::Shopee => Box::new(crate::lojas::shopee::LojaShopee::nova(vec![fonte])),
            LojaId::Aliexpress => {
                Box::new(crate::lojas::aliexpress::LojaAliexpress::nova(vec![fonte]))
            }
            LojaId::Kabum => Box::new(crate::lojas::kabum::LojaKabum::nova(vec![fonte])),
            LojaId::Pichau => Box::new(crate::lojas::pichau::LojaPichau::nova(vec![fonte])),
            LojaId::Ml => unreachable!(),
        };
        registro.registrar(loja)?;
    }
    Ok(registro)
}

fn notificar_minimas_multi(
    app: &AppHandle,
    state: &AppState,
    ranking: &crate::core::multiloja::RankingMultiLoja,
) -> Result<(), String> {
    let hoje = Utc::now().date_naive();
    let mut novas = 0;
    for grupo in &ranking.produtos {
        for (entidade, agrupado) in std::iter::once((grupo.chave.as_str(), true))
            .chain(grupo.ofertas.iter().map(|o| (o.oferta.id.as_str(), false)))
        {
            let h = state
                .database
                .historico_multi(entidade, agrupado, hoje)
                .map_err(|e| e.to_string())?;
            if h.alerta_minima
                && state
                    .database
                    .registrar_alerta_multi(entidade, agrupado, hoje)
                    .map_err(|e| e.to_string())?
            {
                novas += 1;
            }
        }
    }
    if novas > 0 {
        let _ = app.notification().builder().title("Mínimas de 30 dias").body(format!("{novas} oferta(s)/grupo(s) atingiram a mínima observada. Confira o histórico no Garimpo Livre.")).show();
    }
    Ok(())
}

#[tauri::command]
pub fn get_multi_results(
    slug: String,
    state: State<'_, AppState>,
) -> Result<Option<crate::core::multiloja::RankingMultiLoja>, String> {
    storage::carregar_ranking_multi(&state.data_dir, &slug).map_err(|e| e.to_string())
}
#[tauri::command]
pub fn get_multi_history(
    entidade: String,
    grupo: bool,
    state: State<'_, AppState>,
) -> Result<storage::HistoricoMulti, String> {
    state
        .database
        .historico_multi(&entidade, grupo, Utc::now().date_naive())
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub fn export_multi_ranking(slug: String, state: State<'_, AppState>) -> Result<String, String> {
    let ranking = storage::carregar_ranking_multi(&state.data_dir, &slug)
        .map_err(|e| e.to_string())?
        .ok_or("Execute a busca antes de exportar.")?;
    storage::exportar_multi(&state.data_dir, &ranking)
        .map(|p| p.to_string_lossy().into())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_searches(state: State<'_, AppState>) -> Result<Vec<SearchFile>, String> {
    storage::listar_buscas(&state.data_dir).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn save_search(
    slug: Option<String>,
    config: SearchConfig,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let slug = slug.unwrap_or_else(|| storage::slugificar(&config.termo));
    storage::salvar_busca(&state.data_dir, &slug, &config).map_err(|error| error.to_string())?;
    Ok(slug)
}

#[tauri::command]
pub fn delete_search(slug: String, state: State<'_, AppState>) -> Result<(), String> {
    {
        let mut running = state
            .running
            .lock()
            .map_err(|_| "Estado da coleta indisponível")?;
        if *running {
            return Err("Aguarde a coleta terminar antes de excluir uma busca.".into());
        }
        *running = true;
    }
    let _guard = ColetaEmAndamento(&state.running);
    storage::excluir_busca(&state.data_dir, &slug).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn save_settings(settings: AppSettings, state: State<'_, AppState>) -> Result<(), String> {
    storage::salvar_config(&state.data_dir, &settings).map_err(|error| error.to_string())?;
    state.webview_limiter.set_interval(Duration::from_secs(
        settings.intervalo_coleta_segundos.max(2),
    ));
    state
        .recursos_fontes
        .definir_intervalo(settings.intervalo_coleta_segundos);
    Ok(())
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Result<AppSettings, String> {
    storage::carregar_config(&state.data_dir).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn get_source_health(
    state: State<'_, AppState>,
) -> Result<Vec<storage::EstadoSaudeFonte>, String> {
    state
        .database
        .saude_fontes(Utc::now().timestamp_millis())
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn debug_notification(app: AppHandle) -> Result<(), String> {
    app.notification()
        .builder()
        .title("Garimpo Livre")
        .body("Notificação de teste enviada.")
        .show()
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn run_all_multi_searches(
    source: Option<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<crate::core::multiloja::RankingMultiLoja>, String> {
    executar_todas_multi_com_state(&app, &state, source).await
}
pub async fn executar_todas_multi(
    app: &AppHandle,
    source: Option<String>,
) -> Result<Vec<crate::core::multiloja::RankingMultiLoja>, String> {
    executar_todas_multi_com_state(app, &app.state::<AppState>(), source).await
}
async fn executar_todas_multi_com_state(
    app: &AppHandle,
    state: &AppState,
    source: Option<String>,
) -> Result<Vec<crate::core::multiloja::RankingMultiLoja>, String> {
    let buscas = storage::listar_buscas(&state.data_dir).map_err(|e| e.to_string())?;
    let mut resultados = Vec::new();
    let mut erros = Vec::new();
    for busca in buscas {
        match executar_busca_multi(&busca.slug, source.clone(), None, app.clone(), state).await {
            Ok(r) => resultados.push(r),
            Err(e) => erros.push(format!("{}: {e}", busca.slug)),
        }
    }
    if resultados.is_empty() && !erros.is_empty() {
        return Err(erros.join("; "));
    }
    let ajuda = resultados.iter().any(|r| {
        r.status.values().any(|s| {
            !matches!(
                s,
                crate::lojas::StatusLoja::Ok | crate::lojas::StatusLoja::Desativada
            )
        })
    });
    if ajuda || !erros.is_empty() {
        let _=app.notification().builder().title("Coletas precisam de ajuda").body("Abra o Garimpo Livre para conferir os status, retomar o acesso ou informar ofertas manualmente.").show();
    }
    Ok(resultados)
}
