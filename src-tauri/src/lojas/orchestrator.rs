use super::{
    ConfigLoja, ContextoNormalizacao, Loja, LojaId, OfertaBruta, OfertaNormalizada, RegistroLojas,
    ResultadoMultiLoja, StatusLoja,
};
use crate::{
    coleta::{RateLimiter, RelogioSistema},
    models::SearchConfig,
    storage::Database,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::{
    task::JoinSet,
    time::{Instant, timeout, timeout_at},
};

const TIMEOUT_GLOBAL: Duration = Duration::from_secs(60);
const CACHE_TTL_MS: i64 = 60 * 60 * 1000;

pub struct RecursosFontes {
    pub database: Arc<Database>,
    cache_dir: PathBuf,
    limitadores: Mutex<HashMap<(LojaId, String), RateLimiter>>,
    intervalo_segundos: AtomicU64,
}
impl RecursosFontes {
    pub fn novos(database: Arc<Database>, data_dir: PathBuf) -> Self {
        Self {
            database,
            cache_dir: data_dir.join("cache/lojas"),
            limitadores: Mutex::new(HashMap::new()),
            intervalo_segundos: AtomicU64::new(2),
        }
    }
    pub fn limitador(&self, loja: LojaId, fonte: &str) -> RateLimiter {
        self.limitadores
            .lock()
            .expect("lock de limitadores")
            .entry((loja, fonte.into()))
            .or_insert_with(|| {
                RateLimiter::new(
                    Arc::new(RelogioSistema),
                    Duration::from_secs(self.intervalo_segundos.load(Ordering::Relaxed)),
                )
            })
            .clone()
    }
    pub fn definir_intervalo(&self, segundos: u64) {
        let segundos = segundos.max(2);
        let limitadores = self.limitadores.lock().expect("lock de limitadores");
        self.intervalo_segundos.store(segundos, Ordering::Relaxed);
        for limitador in limitadores.values() {
            limitador.set_interval(Duration::from_secs(segundos));
        }
    }
    pub fn invalidar_cache(&self, loja: LojaId) -> Result<(), String> {
        // LojaId só produz um dos sete nomes fixos, sem componentes de caminho.
        let pasta = self.cache_dir.join(loja.as_str());
        if pasta.exists() {
            fs::remove_dir_all(pasta)
                .map_err(|error| format!("Falha ao invalidar cache de {loja}: {error}"))?;
        }
        Ok(())
    }
    fn cache_path(&self, loja: LojaId, fonte: &str, termo: &str) -> PathBuf {
        let digest = Sha256::digest(format!("{fonte}\0{termo}").as_bytes());
        self.cache_dir
            .join(loja.as_str())
            .join(format!("{digest:x}.json"))
    }
    fn ler_cache(&self, loja: LojaId, fonte: &str, termo: &str) -> Option<CacheLoja> {
        let path = self.cache_path(loja, fonte, termo);
        let saved: CacheLoja = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
        (saved.loja == loja
            && saved.fonte == fonte
            && saved.termo == termo
            && saved.capturado_em_ms <= Utc::now().timestamp_millis())
        .then_some(saved)
    }
    fn salvar_cache(
        &self,
        loja: LojaId,
        fonte: &str,
        termo: &str,
        ofertas: Vec<OfertaBruta>,
    ) -> Result<(), String> {
        let path = self.cache_path(loja, fonte, termo);
        fs::create_dir_all(path.parent().ok_or("Caminho de cache inválido")?)
            .map_err(|error| error.to_string())?;
        let cache = CacheLoja {
            loja,
            fonte: fonte.into(),
            termo: termo.into(),
            capturado_em_ms: Utc::now().timestamp_millis(),
            ofertas,
        };
        let raw = serde_json::to_vec(&cache).map_err(|error| error.to_string())?;
        let temp = path.with_extension("json.tmp");
        fs::write(&temp, raw).map_err(|error| error.to_string())?;
        fs::rename(temp, path).map_err(|error| error.to_string())
    }
}

#[derive(Serialize, Deserialize)]
struct CacheLoja {
    loja: LojaId,
    fonte: String,
    termo: String,
    capturado_em_ms: i64,
    ofertas: Vec<OfertaBruta>,
}

struct TentativaEmAndamento {
    database: Arc<Database>,
    chave: String,
    inicio: Instant,
    concluida: bool,
}
impl Drop for TentativaEmAndamento {
    fn drop(&mut self) {
        if !self.concluida {
            let _ = self.database.registrar_tentativa_fonte(
                &self.chave,
                false,
                self.inicio.elapsed().as_millis().min(u64::MAX as u128) as u64,
                Some("Coleta cancelada ou tempo limite global atingido"),
                Utc::now().timestamp_millis(),
            );
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EventoLoja {
    pub loja: LojaId,
    pub status: StatusLoja,
    pub ofertas: Vec<OfertaNormalizada>,
}

pub struct OrquestradorMultiLoja {
    registro: RegistroLojas,
    recursos: Arc<RecursosFontes>,
}
impl OrquestradorMultiLoja {
    pub fn novo(registro: RegistroLojas, recursos: Arc<RecursosFontes>) -> Self {
        Self { registro, recursos }
    }
    pub async fn executar(
        &self,
        busca: &SearchConfig,
        config: &[ConfigLoja],
        contexto: ContextoNormalizacao,
        publicar: impl FnMut(EventoLoja),
    ) -> Result<ResultadoMultiLoja, String> {
        self.executar_ate(
            busca,
            config,
            contexto,
            Instant::now() + TIMEOUT_GLOBAL,
            publicar,
        )
        .await
    }
    pub async fn executar_ate(
        &self,
        busca: &SearchConfig,
        config: &[ConfigLoja],
        contexto: ContextoNormalizacao,
        deadline: Instant,
        publicar: impl FnMut(EventoLoja),
    ) -> Result<ResultadoMultiLoja, String> {
        self.executar_ate_com_retry_manual(busca, config, contexto, deadline, None, publicar)
            .await
    }
    pub async fn executar_ate_com_retry_manual(
        &self,
        busca: &SearchConfig,
        config: &[ConfigLoja],
        contexto: ContextoNormalizacao,
        deadline: Instant,
        retry_manual: Option<LojaId>,
        mut publicar: impl FnMut(EventoLoja),
    ) -> Result<ResultadoMultiLoja, String> {
        busca.validate()?;
        let ids = self.registro.selecionar(&busca.lojas, config)?;
        let mut jobs = JoinSet::new();
        let mut task_lojas = HashMap::new();
        let mut resultado = ResultadoMultiLoja::default();
        for id in ids {
            if config.iter().any(|loja| loja.id == id && !loja.ativa) {
                let evento = EventoLoja {
                    loja: id,
                    status: StatusLoja::Desativada,
                    ofertas: vec![],
                };
                resultado.status.insert(id, evento.status.clone());
                publicar(evento);
                continue;
            }
            let loja = self.registro.obter(id)?;
            let termo = busca.termo.clone();
            let contexto = contexto.clone();
            let recursos = self.recursos.clone();
            let libera_circuito_webview = retry_manual == Some(id);
            let task = jobs.spawn(async move {
                match timeout_at(
                    deadline,
                    executar_loja(
                        loja.clone(),
                        termo.clone(),
                        recursos.clone(),
                        contexto.clone(),
                        libera_circuito_webview,
                    ),
                )
                .await
                {
                    Ok(evento) => evento,
                    Err(_) => cache_de_contingencia(loja.as_ref(), &termo, &recursos, &contexto)
                        .unwrap_or(EventoLoja {
                            loja: id,
                            status: StatusLoja::Falhou {
                                erro: "Tempo limite global de 60 segundos".into(),
                            },
                            ofertas: vec![],
                        }),
                }
            });
            task_lojas.insert(task.id(), id);
        }
        while let Some(job) = jobs.join_next_with_id().await {
            let evento = match job {
                Ok((_, evento)) => evento,
                Err(error) => EventoLoja {
                    loja: *task_lojas
                        .get(&error.id())
                        .ok_or("Tarefa sem identificação de loja")?,
                    status: StatusLoja::Falhou {
                        erro: "Falha interna durante a coleta desta loja".into(),
                    },
                    ofertas: vec![],
                },
            };
            resultado.ofertas.extend(evento.ofertas.clone());
            resultado.status.insert(evento.loja, evento.status.clone());
            publicar(evento);
        }
        Ok(resultado)
    }
}

fn cache_de_contingencia(
    loja: &dyn Loja,
    termo: &str,
    recursos: &RecursosFontes,
    contexto: &ContextoNormalizacao,
) -> Option<EventoLoja> {
    let mut caches = loja
        .fontes()
        .iter()
        .filter(|fonte| fonte.cacheavel)
        .filter_map(|fonte| recursos.ler_cache(loja.id(), &fonte.nome, termo))
        .collect::<Vec<_>>();
    caches.sort_by_key(|cache| std::cmp::Reverse(cache.capturado_em_ms));
    for cache in caches {
        if let Ok(ofertas) = normalizar(loja, cache.ofertas, contexto) {
            return Some(EventoLoja {
                loja: loja.id(),
                status: StatusLoja::Degradado {
                    fonte: format!("Cache ({})", cache.fonte),
                    capturado_em: chrono::DateTime::from_timestamp_millis(cache.capturado_em_ms)
                        .map(|data| data.to_rfc3339()),
                },
                ofertas,
            });
        }
    }
    None
}

fn normalizar(
    loja: &dyn Loja,
    brutos: Vec<OfertaBruta>,
    contexto: &ContextoNormalizacao,
) -> Result<Vec<OfertaNormalizada>, String> {
    brutos
        .into_iter()
        .map(|bruto| {
            loja.normalizar(bruto, contexto)
                .map_err(|error| error.to_string())
        })
        .collect()
}

async fn executar_loja(
    loja: Arc<dyn Loja>,
    termo: String,
    recursos: Arc<RecursosFontes>,
    contexto: ContextoNormalizacao,
    libera_circuito_webview: bool,
) -> EventoLoja {
    let id = loja.id();
    let mut erros = Vec::new();
    let mut caches = Vec::new();
    for candidata in loja.fontes() {
        let forcar_sondagem_manual = libera_circuito_webview && candidata.nome == "WebView";
        if candidata.cacheavel
            && let Some(cache) = recursos.ler_cache(id, &candidata.nome, &termo)
        {
            if !forcar_sondagem_manual
                && Utc::now().timestamp_millis() - cache.capturado_em_ms <= CACHE_TTL_MS
                && let Ok(ofertas) = normalizar(loja.as_ref(), cache.ofertas.clone(), &contexto)
            {
                return EventoLoja {
                    loja: id,
                    status: StatusLoja::Degradado {
                        fonte: format!("Cache ({})", candidata.nome),
                        capturado_em: chrono::DateTime::from_timestamp_millis(
                            cache.capturado_em_ms,
                        )
                        .map(|data| data.to_rfc3339()),
                    },
                    ofertas,
                };
            }
            caches.push(cache);
        }
        if !candidata.disponivel {
            continue;
        }
        let chave = format!("{id}:{}", candidata.nome);
        match recursos
            .database
            .tentar_fonte(&chave, Utc::now().timestamp_millis())
        {
            Ok(permissao) if permissao.permitido || forcar_sondagem_manual => {}
            Ok(_) => {
                erros.push(format!("{}: circuito aberto", candidata.nome));
                continue;
            }
            Err(error) => {
                erros.push(error.to_string());
                continue;
            }
        }
        let iniciou = Instant::now();
        let mut tentativa = TentativaEmAndamento {
            database: recursos.database.clone(),
            chave: chave.clone(),
            inicio: iniciou,
            concluida: false,
        };
        let result = timeout(candidata.timeout, async {
            if !candidata.fonte.controla_rate_limit() {
                recursos.limitador(id, &candidata.nome).wait_turn().await;
            }
            let brutos = candidata.fonte.listar(&termo).await?;
            let ofertas = normalizar(loja.as_ref(), brutos.clone(), &contexto)?;
            Ok::<_, String>((brutos, ofertas))
        })
        .await
        .unwrap_or_else(|_| Err(format!("Tempo limite da fonte {}", candidata.nome)));
        let duracao = iniciou.elapsed().as_millis().min(u64::MAX as u128) as u64;
        let error = result.as_ref().err().map(String::as_str);
        if let Err(error) = recursos.database.registrar_tentativa_fonte(
            &chave,
            result.is_ok(),
            duracao,
            error,
            Utc::now().timestamp_millis(),
        ) {
            erros.push(error.to_string());
        }
        tentativa.concluida = true;
        match result {
            Ok((brutos, ofertas)) => {
                if candidata.cacheavel
                    && candidata.capturado_em.is_none()
                    && let Err(error) = recursos.salvar_cache(id, &candidata.nome, &termo, brutos)
                {
                    eprintln!("Falha ao salvar cache de {id}: {error}");
                }
                let status = if candidata.capturado_em.is_some() {
                    StatusLoja::Degradado {
                        fonte: candidata.nome,
                        capturado_em: candidata.capturado_em,
                    }
                } else if candidata.degradada {
                    StatusLoja::Degradado {
                        fonte: candidata.nome,
                        capturado_em: None,
                    }
                } else {
                    StatusLoja::Ok
                };
                return EventoLoja {
                    loja: id,
                    status,
                    ofertas,
                };
            }
            Err(error) => erros.push(format!("{}: {error}", candidata.nome)),
        }
    }
    caches.sort_by_key(|cache| std::cmp::Reverse(cache.capturado_em_ms));
    for cache in caches {
        if let Ok(ofertas) = normalizar(loja.as_ref(), cache.ofertas, &contexto) {
            return EventoLoja {
                loja: id,
                status: StatusLoja::Degradado {
                    fonte: format!("Cache ({})", cache.fonte),
                    capturado_em: chrono::DateTime::from_timestamp_millis(cache.capturado_em_ms)
                        .map(|data| data.to_rfc3339()),
                },
                ofertas,
            };
        }
    }
    let status = StatusLoja::Falhou {
        erro: if erros.is_empty() {
            "Nenhuma fonte disponível para esta loja".into()
        } else {
            erros.join("; ")
        },
    };
    EventoLoja {
        loja: id,
        status,
        ofertas: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::super::{FonteDaLoja, FonteLoja, LojaMl};
    use super::*;
    use crate::coleta::FonteFixture;
    use async_trait::async_trait;
    struct Fake {
        delay: u64,
        erro: bool,
    }
    #[async_trait]
    impl FonteLoja for Fake {
        async fn listar(&self, _termo: &str) -> Result<Vec<OfertaBruta>, String> {
            tokio::time::sleep(Duration::from_secs(self.delay)).await;
            if self.erro {
                return Err("erro fake".into());
            }
            let ads = crate::coleta::parse_listagem_html(include_str!(
                "../../tests/fixtures/listagem.html"
            ))
            .unwrap();
            Ok(vec![LojaMl::adaptar_anuncio(ads[0].clone()).unwrap()])
        }
    }
    struct LojaFake {
        id: LojaId,
        fontes: Vec<FonteDaLoja>,
    }
    impl Loja for LojaFake {
        fn id(&self) -> LojaId {
            self.id
        }
        fn fontes(&self) -> Vec<FonteDaLoja> {
            self.fontes.clone()
        }
    }
    fn fonte(delay: u64, erro: bool) -> FonteDaLoja {
        let mut fonte = FonteDaLoja::nova("API", Arc::new(Fake { delay, erro }));
        fonte.timeout = Duration::from_secs(120);
        fonte
    }
    fn fonte_com_nome(nome: &str, erro: bool) -> FonteDaLoja {
        let mut fonte = FonteDaLoja::nova(nome, Arc::new(Fake { delay: 0, erro }));
        fonte.timeout = Duration::from_secs(120);
        fonte
    }
    fn setup(
        lojas: Vec<LojaFake>,
        root: PathBuf,
    ) -> (OrquestradorMultiLoja, SearchConfig, Vec<ConfigLoja>) {
        let mut registro = RegistroLojas::default();
        let mut ids = Vec::new();
        for loja in lojas {
            ids.push(loja.id);
            registro.registrar(Box::new(loja)).unwrap();
        }
        let config = ids.iter().map(|id| ConfigLoja::inicial(*id)).collect();
        let busca = SearchConfig {
            termo: "fone".into(),
            lojas: ids,
            ..Default::default()
        };
        let recursos = Arc::new(RecursosFontes::novos(
            Arc::new(Database::open_in_memory().unwrap()),
            root,
        ));
        (
            OrquestradorMultiLoja::novo(registro, recursos),
            busca,
            config,
        )
    }
    #[tokio::test]
    async fn retry_manual_sonda_webview_da_loja_sem_liberar_outras_e_ignora_cache_fresco() {
        let dir = tempfile::tempdir().unwrap();
        let (runner, busca, config) = setup(
            vec![
                LojaFake {
                    id: LojaId::Shopee,
                    fontes: vec![fonte_com_nome("WebView", false)],
                },
                LojaFake {
                    id: LojaId::Amazon,
                    fontes: vec![fonte_com_nome("WebView", false)],
                },
            ],
            dir.path().into(),
        );
        let db = &runner.recursos.database;
        let agora_ms = Utc::now().timestamp_millis() - 1000;
        for n in 0..3 {
            db.registrar_tentativa_fonte(
                "shopee:WebView",
                false,
                1,
                Some("falha anterior"),
                agora_ms + n,
            )
            .unwrap();
            db.registrar_tentativa_fonte(
                "amazon:WebView",
                false,
                1,
                Some("falha anterior"),
                agora_ms + n,
            )
            .unwrap();
        }
        runner
            .recursos
            .salvar_cache(LojaId::Shopee, "WebView", "fone", vec![])
            .unwrap();

        let result = runner
            .executar_ate_com_retry_manual(
                &busca,
                &config,
                ContextoNormalizacao,
                Instant::now() + TIMEOUT_GLOBAL,
                Some(LojaId::Shopee),
                |_| {},
            )
            .await
            .unwrap();

        assert_eq!(result.status[&LojaId::Shopee], StatusLoja::Ok);
        assert!(matches!(
            &result.status[&LojaId::Amazon],
            StatusLoja::Falhou { erro } if erro.contains("circuito aberto")
        ));
        assert!(
            db.tentar_fonte("shopee:WebView", Utc::now().timestamp_millis())
                .unwrap()
                .permitido
        );
        assert!(
            !db.tentar_fonte("amazon:WebView", Utc::now().timestamp_millis())
                .unwrap()
                .permitido
        );
    }
    #[tokio::test(start_paused = true)]
    async fn paralelo_entrega_parcial_falha_timeout_e_eventos_na_ordem() {
        let dir = tempfile::tempdir().unwrap();
        let (runner, busca, config) = setup(
            vec![
                LojaFake {
                    id: LojaId::Ml,
                    fontes: vec![fonte(1, false)],
                },
                LojaFake {
                    id: LojaId::Ebay,
                    fontes: vec![fonte(2, true)],
                },
                LojaFake {
                    id: LojaId::Kabum,
                    fontes: vec![fonte(120, false)],
                },
            ],
            dir.path().into(),
        );
        let mut eventos = Vec::new();
        let inicio = Instant::now();
        let result = runner
            .executar(&busca, &config, ContextoNormalizacao, |evento| {
                eventos.push(evento)
            })
            .await
            .unwrap();
        assert_eq!(inicio.elapsed(), Duration::from_secs(60));
        assert_eq!(result.ofertas.len(), 1);
        assert_eq!(result.status[&LojaId::Ml], StatusLoja::Ok);
        assert!(
            matches!(&result.status[&LojaId::Ebay],StatusLoja::Falhou {erro} if erro.contains("erro fake"))
        );
        assert!(
            matches!(&result.status[&LojaId::Kabum],StatusLoja::Falhou {erro} if erro.contains("60 segundos"))
        );
        assert_eq!(
            eventos.iter().map(|evento| evento.loja).collect::<Vec<_>>(),
            vec![LojaId::Ml, LojaId::Ebay, LojaId::Kabum]
        );
    }
    #[tokio::test(start_paused = true)]
    async fn tres_fontes_respondem_abaixo_de_trinta_segundos() {
        let dir = tempfile::tempdir().unwrap();
        let (runner, busca, config) = setup(
            vec![
                LojaFake {
                    id: LojaId::Ml,
                    fontes: vec![fonte(5, false)],
                },
                LojaFake {
                    id: LojaId::Ebay,
                    fontes: vec![fonte(8, false)],
                },
                LojaFake {
                    id: LojaId::Kabum,
                    fontes: vec![fonte(12, false)],
                },
            ],
            dir.path().into(),
        );
        let inicio = Instant::now();
        let mut eventos = Vec::new();
        let result = runner
            .executar(&busca, &config, ContextoNormalizacao, |evento| {
                eventos.push(evento)
            })
            .await
            .unwrap();
        assert_eq!(result.ofertas.len(), 3);
        assert_eq!(inicio.elapsed(), Duration::from_secs(12));
        assert_eq!(eventos.len(), 3);
    }
    #[tokio::test(start_paused = true)]
    async fn prazo_global_inclui_tempo_gasto_antes_das_fontes() {
        let dir = tempfile::tempdir().unwrap();
        let (runner, busca, config) = setup(
            vec![LojaFake {
                id: LojaId::Ebay,
                fontes: vec![fonte(120, false)],
            }],
            dir.path().into(),
        );
        let inicio = Instant::now();
        let prazo = inicio + Duration::from_secs(60);
        tokio::time::sleep(Duration::from_secs(8)).await;
        let result = runner
            .executar_ate(&busca, &config, ContextoNormalizacao, prazo, |_| {})
            .await
            .unwrap();
        assert_eq!(inicio.elapsed(), Duration::from_secs(60));
        assert!(matches!(
            result.status[&LojaId::Ebay],
            StatusLoja::Falhou { .. }
        ));
    }
    #[test]
    fn invalidar_cache_de_uma_loja_preserva_outras() {
        let dir = tempfile::tempdir().unwrap();
        let recursos = RecursosFontes::novos(
            Arc::new(Database::open_in_memory().unwrap()),
            dir.path().into(),
        );
        recursos
            .salvar_cache(LojaId::Ml, "API", "ssd", vec![])
            .unwrap();
        recursos
            .salvar_cache(LojaId::Ebay, "API", "ssd", vec![])
            .unwrap();
        recursos.invalidar_cache(LojaId::Ebay).unwrap();
        assert!(recursos.ler_cache(LojaId::Ebay, "API", "ssd").is_none());
        assert!(recursos.ler_cache(LojaId::Ml, "API", "ssd").is_some());
    }
    #[tokio::test]
    async fn intervalo_configurado_atualiza_limitador_compartilhado() {
        #[derive(Default)]
        struct ClockFake(AtomicU64);
        #[async_trait::async_trait]
        impl crate::coleta::Clock for ClockFake {
            fn now_ms(&self) -> u64 {
                self.0.load(Ordering::Relaxed)
            }
            async fn sleep(&self, d: Duration) {
                self.0.fetch_add(d.as_millis() as u64, Ordering::Relaxed);
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let recursos = RecursosFontes::novos(
            Arc::new(Database::open_in_memory().unwrap()),
            dir.path().into(),
        );
        let clock = Arc::new(ClockFake::default());
        let limitador = RateLimiter::new(clock.clone(), Duration::from_secs(2));
        recursos
            .limitadores
            .lock()
            .unwrap()
            .insert((LojaId::Ml, "API".into()), limitador.clone());
        limitador.wait_turn().await;
        recursos.definir_intervalo(7);
        recursos.limitador(LojaId::Ml, "API").wait_turn().await;
        assert_eq!(clock.0.load(Ordering::Relaxed), 7000);
        recursos.definir_intervalo(0);
        limitador.wait_turn().await;
        assert_eq!(clock.0.load(Ordering::Relaxed), 9000);
    }
    #[tokio::test(start_paused = true)]
    async fn cache_isolado_por_loja_fonte_e_busca_e_fonte_indisponivel_usa_fallback() {
        let dir = tempfile::tempdir().unwrap();
        let mut indisponivel = fonte(1, false);
        indisponivel.disponivel = false;
        let fallback = LojaMl::adaptar_fonte("Fixture", Arc::new(FonteFixture));
        let (runner, busca, config) = setup(
            vec![LojaFake {
                id: LojaId::Amazon,
                fontes: vec![indisponivel, fallback],
            }],
            dir.path().into(),
        );
        let first = runner
            .executar(&busca, &config, ContextoNormalizacao, |_| {})
            .await
            .unwrap();
        assert!(matches!(
            first.status[&LojaId::Amazon],
            StatusLoja::Ok | StatusLoja::Degradado { .. }
        ));
        assert_eq!(first.ofertas.len(), 15);
        let second = runner
            .executar(&busca, &config, ContextoNormalizacao, |_| {})
            .await
            .unwrap();
        assert!(matches!(
            second.status[&LojaId::Amazon],
            StatusLoja::Degradado { .. }
        ));
        assert!(
            runner
                .recursos
                .ler_cache(LojaId::Ml, "Fixture", "fone")
                .is_none()
        );
        assert!(
            runner
                .recursos
                .ler_cache(LojaId::Amazon, "API", "fone")
                .is_none()
        );
        assert!(
            runner
                .recursos
                .ler_cache(LojaId::Amazon, "Fixture", "ssd")
                .is_none()
        );
    }

    #[test]
    fn registro_devolve_cada_id_e_rejeita_duplicados() {
        let mut registro = RegistroLojas::default();
        for id in LojaId::TODAS {
            registro
                .registrar(Box::new(LojaFake { id, fontes: vec![] }))
                .unwrap();
        }
        for id in LojaId::TODAS {
            assert_eq!(registro.obter(id).unwrap().id(), id);
        }
        assert!(
            registro
                .registrar(Box::new(LojaFake {
                    id: LojaId::Ml,
                    fontes: vec![]
                }))
                .is_err()
        );
        let config = LojaId::TODAS
            .into_iter()
            .map(ConfigLoja::inicial)
            .collect::<Vec<_>>();
        assert_eq!(registro.selecionar(&[], &config).unwrap(), vec![LojaId::Ml]);
    }

    #[tokio::test(start_paused = true)]
    async fn timeout_libera_sondagem_e_reusa_cache_antigo() {
        let dir = tempfile::tempdir().unwrap();
        let (runner, busca, config) = setup(
            vec![LojaFake {
                id: LojaId::Ml,
                fontes: vec![fonte(120, false)],
            }],
            dir.path().into(),
        );
        let ads =
            crate::coleta::parse_listagem_html(include_str!("../../tests/fixtures/listagem.html"))
                .unwrap();
        runner
            .recursos
            .salvar_cache(
                LojaId::Ml,
                "API",
                "fone",
                vec![LojaMl::adaptar_anuncio(ads[0].clone()).unwrap()],
            )
            .unwrap();
        let path = runner.recursos.cache_path(LojaId::Ml, "API", "fone");
        let mut cache: CacheLoja = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        cache.capturado_em_ms -= 2 * CACHE_TTL_MS;
        fs::write(path, serde_json::to_vec(&cache).unwrap()).unwrap();
        let db = &runner.recursos.database;
        let passado = Utc::now().timestamp_millis() - 2 * 60 * 60 * 1000;
        for n in 0..3 {
            db.registrar_tentativa_fonte("ml:API", false, 1, Some("fake"), passado + n)
                .unwrap();
        }
        let inicio = Instant::now();
        let result = runner
            .executar(&busca, &config, ContextoNormalizacao, |_| {})
            .await
            .unwrap();
        assert_eq!(inicio.elapsed(), Duration::from_secs(60));
        assert_eq!(result.ofertas.len(), 1);
        assert!(matches!(
            result.status[&LojaId::Ml],
            StatusLoja::Degradado { .. }
        ));
        assert!(
            !db.tentar_fonte("ml:API", Utc::now().timestamp_millis())
                .unwrap()
                .permitido
        );
        let health = db.saude_fontes(Utc::now().timestamp_millis()).unwrap();
        assert_eq!(
            health[0].estado_circuito,
            crate::storage::EstadoCircuito::Aberto
        );
    }

    #[tokio::test(start_paused = true)]
    async fn loja_desativada_nao_inicia_fonte_e_limitadores_sao_independentes() {
        let dir = tempfile::tempdir().unwrap();
        let (runner, busca, mut config) = setup(
            vec![LojaFake {
                id: LojaId::Ml,
                fontes: vec![fonte(120, false)],
            }],
            dir.path().into(),
        );
        config[0].ativa = false;
        let result = runner
            .executar(&busca, &config, ContextoNormalizacao, |_| {})
            .await
            .unwrap();
        assert_eq!(result.status[&LojaId::Ml], StatusLoja::Desativada);
        assert!(runner.recursos.database.saude_fontes(0).unwrap().is_empty());
        let ml = runner.recursos.limitador(LojaId::Ml, "API");
        ml.wait_turn().await;
        let inicio = Instant::now();
        runner
            .recursos
            .limitador(LojaId::Ebay, "API")
            .wait_turn()
            .await;
        runner
            .recursos
            .limitador(LojaId::Ml, "WebView")
            .wait_turn()
            .await;
        assert_eq!(inicio.elapsed(), Duration::ZERO);
        runner
            .recursos
            .limitador(LojaId::Ml, "API")
            .wait_turn()
            .await;
        assert!(inicio.elapsed() >= Duration::from_millis(1900));
    }
}
