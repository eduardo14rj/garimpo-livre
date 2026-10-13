pub mod coleta;
pub mod commands;
pub mod core;
pub mod lojas;
pub mod models;
pub mod storage;

use std::path::PathBuf;

use tauri::{Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::commands::AppState;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let data_dir: PathBuf = app.path().app_data_dir()?;
            storage::initialize_data_dir(&data_dir)?;
            let database = storage::Database::open(data_dir.join("data").join("garimpo.db"))?;
            storage::limpar_configuracoes_anteriores_uma_vez(&data_dir)
                .map_err(std::io::Error::other)?;
            app.manage(AppState::new(data_dir, database)?);

            let headless = std::env::args().any(|argument| argument == "--run-all");
            if headless {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
                let source = std::env::args()
                    .skip_while(|argument| argument != "--fonte")
                    .nth(1)
                    .unwrap_or_else(|| "fixture".into());
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    let outcome = commands::executar_todas_multi(&handle, Some(source)).await;
                    if let Err(error) = &outcome {
                        eprintln!("{error}");
                    }
                    let needs_assistance = outcome.as_ref().is_ok_and(|results| {
                        results.iter().any(|result| {
                            result.status.values().any(|s| {
                                !matches!(s, lojas::StatusLoja::Ok | lojas::StatusLoja::Desativada)
                            })
                        })
                    });
                    if !needs_assistance {
                        handle.exit(if outcome.is_ok() { 0 } else { 1 });
                    }
                });
            } else if let Some(window) = app.get_webview_window("main") {
                window.show()?;
            }

            let handle = app.handle().clone();
            let run_now = tauri::menu::MenuItem::with_id(
                &handle,
                "run-now",
                "Executar agora",
                true,
                None::<&str>,
            )?;
            let show = tauri::menu::MenuItem::with_id(
                &handle,
                "show",
                "Abrir Garimpo Livre",
                true,
                None::<&str>,
            )?;
            let quit = tauri::menu::MenuItem::with_id(&handle, "quit", "Sair", true, None::<&str>)?;
            let menu = tauri::menu::Menu::with_items(&handle, &[&run_now, &show, &quit])?;
            let notification_handle = handle.clone();
            let _ = handle.notification().on_action(move |action| {
                let is_assistance = action
                    .notification()
                    .and_then(|notification| notification.title())
                    .is_some_and(|title| title == "Coletas precisam de ajuda");
                if action.action_id() == "tap" && is_assistance {
                    if let Some(window) = notification_handle.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                    let _ = notification_handle.emit("garimpo:open-assistance", ());
                }
            });
            tauri::tray::TrayIconBuilder::new()
                .tooltip("Garimpo Livre")
                .menu(&menu)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "run-now" => {
                        let _ = app.emit("garimpo:run-now", ());
                    }
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_searches,
            commands::list_stores,
            commands::configure_store,
            commands::continue_store_collector,
            commands::open_store,
            commands::clear_store_session,
            commands::clear_store_cache,
            commands::save_manual_offer,
            commands::run_all_multi_searches,
            commands::run_multi_search,
            commands::get_multi_results,
            commands::get_multi_history,
            commands::export_multi_ranking,
            commands::save_search,
            commands::delete_search,
            commands::get_settings,
            commands::get_source_health,
            commands::save_settings,
            commands::debug_notification
        ])
        .on_window_event(|window, event| {
            if window.label() == "main"
                && let tauri::WindowEvent::CloseRequested { api, .. } = event
            {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("falha ao iniciar o Garimpo Livre");
}
