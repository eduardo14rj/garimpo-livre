use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
};

mod legacy_credentials_cleanup;
mod multiloja;
mod ranking_multi;
pub use legacy_credentials_cleanup::limpar_configuracoes_anteriores_uma_vez;
pub use ranking_multi::{
    HistoricoMulti, PontoMulti, carregar_ranking_multi, excluir_ranking_multi, exportar_multi,
    resumir_multi, salvar_ranking_multi,
};

use chrono::{DateTime, NaiveDate, Utc};
use rusqlite::{Connection, OptionalExtension, params};

use crate::{
    core::calcular_historico,
    models::{AnuncioBruto, HistoricoProduto, PontoHistorico, ResultadoBusca},
};

#[derive(Debug, thiserror::Error)]
pub enum StorageErro {
    #[error("falha ao acessar disco: {0}")]
    Io(#[from] std::io::Error),
    #[error("falha no SQLite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("falha no JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("falha ao gravar CSV: {0}")]
    Csv(#[from] csv::Error),
    #[error("configuração inválida: {0}")]
    Config(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EstadoCircuito {
    Fechado,
    Aberto,
    MeioAberto,
}

impl EstadoCircuito {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fechado => "fechado",
            Self::Aberto => "aberto",
            Self::MeioAberto => "meio_aberto",
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EstadoSaudeFonte {
    pub loja: crate::lojas::LojaId,
    pub fonte: String,
    pub estado_circuito: EstadoCircuito,
    pub sucesso_7d: f64,
    pub tentativas_7d: u64,
    pub ultima_falha: Option<String>,
    pub atualizado_em: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct PermissaoTentativaFonte {
    pub permitido: bool,
    pub estado: EstadoCircuito,
}

pub fn initialize_data_dir(root: &Path) -> Result<(), StorageErro> {
    for path in [
        "buscas",
        "data",
        "data/resultados",
        "cache/paginas",
        "cache/resultados",
        "exports",
        "logs",
    ] {
        fs::create_dir_all(root.join(path))?;
    }
    let config_path = root.join("config.json");
    if !config_path.exists() {
        fs::write(
            &config_path,
            serde_json::to_vec_pretty(&crate::models::AppSettings::default())?,
        )?;
    }
    let synonyms_path = root.join("sinonimos.json");
    if !synonyms_path.exists() {
        fs::write(
            synonyms_path,
            br#"{
  "capa": ["capa", "capinha", "case", "cover", "capas"]
}
"#,
        )?;
    }
    Ok(())
}

pub fn carregar_sinonimos(
    root: &Path,
) -> Result<std::collections::BTreeMap<String, Vec<String>>, StorageErro> {
    let path = root.join("sinonimos.json");
    let synonyms = serde_json::from_slice::<std::collections::BTreeMap<String, Vec<String>>>(
        &fs::read(path)?,
    )?;
    Ok(synonyms)
}

pub fn slugificar(text: &str) -> String {
    let normalized = crate::core::normalizar(text);
    let slug = normalized.split_whitespace().collect::<Vec<_>>().join("-");
    if slug.is_empty() {
        "busca".into()
    } else {
        slug.chars().take(80).collect()
    }
}

pub fn validar_slug(slug: &str) -> Result<(), StorageErro> {
    if slug.is_empty()
        || slug.len() > 80
        || !slug
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
    {
        return Err(StorageErro::Config(
            "identificador de busca inválido".into(),
        ));
    }
    Ok(())
}

pub fn carregar_config(root: &Path) -> Result<crate::models::AppSettings, StorageErro> {
    let path = root.join("config.json");
    let settings = if path.exists() {
        serde_json::from_slice(&fs::read(path)?)?
    } else {
        crate::models::AppSettings::default()
    };
    Ok(settings)
}

pub fn salvar_config(
    root: &Path,
    settings: &crate::models::AppSettings,
) -> Result<(), StorageErro> {
    if settings.intervalo_coleta_segundos < 2 {
        return Err(StorageErro::Config(
            "o intervalo mínimo de coleta é de 2 segundos".into(),
        ));
    }
    let path = root.join("config.json");
    fs::write(path, serde_json::to_vec_pretty(settings)?)?;
    Ok(())
}

pub fn listar_buscas(root: &Path) -> Result<Vec<SearchFile>, StorageErro> {
    let mut searches = Vec::new();
    for entry in fs::read_dir(root.join("buscas"))? {
        let entry = entry?;
        if entry.path().extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let slug = entry
            .path()
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_owned();
        let config = serde_json::from_slice(&fs::read(entry.path())?)?;
        searches.push(SearchFile { slug, config });
    }
    searches.sort_by(|left, right| left.config.termo.cmp(&right.config.termo));
    Ok(searches)
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SearchFile {
    pub slug: String,
    pub config: crate::models::SearchConfig,
}

pub fn salvar_busca(
    root: &Path,
    slug: &str,
    config: &crate::models::SearchConfig,
) -> Result<(), StorageErro> {
    validar_slug(slug)?;
    config.validate().map_err(StorageErro::Config)?;
    let path = root.join("buscas").join(format!("{slug}.json"));
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, serde_json::to_vec_pretty(config)?)?;
    fs::rename(temporary, path)?;
    Ok(())
}

pub fn excluir_busca(root: &Path, slug: &str) -> Result<(), StorageErro> {
    validar_slug(slug)?;
    excluir_ranking_multi(root, slug)?;
    for directory in ["data/resultados", "cache/resultados"] {
        remover_arquivo_se_existir(root.join(directory).join(format!("{slug}.json")))?;
        remover_arquivo_se_existir(root.join(directory).join(format!("{slug}.json.tmp")))?;
    }
    let path = root.join("buscas").join(format!("{slug}.json"));
    remover_arquivo_se_existir(path)?;
    remover_arquivo_se_existir(root.join("buscas").join(format!("{slug}.json.tmp")))?;
    Ok(())
}

fn remover_arquivo_se_existir(path: PathBuf) -> Result<(), StorageErro> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

pub fn salvar_resultado(
    root: &Path,
    slug: &str,
    result: &ResultadoBusca,
) -> Result<PathBuf, StorageErro> {
    validar_slug(slug)?;
    let path = root
        .join("data")
        .join("resultados")
        .join(format!("{slug}.json"));
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, serde_json::to_vec_pretty(result)?)?;
    fs::rename(temporary, &path)?;
    Ok(path)
}

pub fn carregar_resultado(root: &Path, slug: &str) -> Result<Option<ResultadoBusca>, StorageErro> {
    validar_slug(slug)?;
    let path = root
        .join("data")
        .join("resultados")
        .join(format!("{slug}.json"));
    if !path.exists() {
        return Ok(None);
    }
    Ok(Some(serde_json::from_slice(&fs::read(path)?)?))
}

pub fn salvar_resultado_cache(
    root: &Path,
    slug: &str,
    result: &ResultadoBusca,
) -> Result<PathBuf, StorageErro> {
    validar_slug(slug)?;
    let directory = root.join("cache").join("resultados");
    fs::create_dir_all(&directory)?;
    let path = directory.join(format!("{slug}.json"));
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, serde_json::to_vec_pretty(result)?)?;
    fs::rename(temporary, &path)?;
    Ok(path)
}

pub fn carregar_resultado_cache(
    root: &Path,
    slug: &str,
) -> Result<Option<ResultadoBusca>, StorageErro> {
    validar_slug(slug)?;
    let path = root
        .join("cache")
        .join("resultados")
        .join(format!("{slug}.json"));
    if !path.exists() {
        return Ok(None);
    }
    Ok(Some(serde_json::from_slice(&fs::read(path)?)?))
}

pub fn registrar_log_execucao(
    root: &Path,
    result: &ResultadoBusca,
    duracao_ms: u64,
) -> Result<PathBuf, StorageErro> {
    let directory = root.join("logs");
    fs::create_dir_all(&directory)?;
    let date = Utc::now().format("%Y-%m-%d");
    let path = directory.join(format!("{date}.jsonl"));
    let record = serde_json::json!({
        "timestamp": Utc::now().to_rfc3339(),
        "busca": result.busca,
        "fonte": result.fonte,
        "tentativas": result.tentativas,
        "duracao_ms": duracao_ms,
        "degradado": result.degradado,
    });
    let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
    serde_json::to_writer(&mut file, &record)?;
    file.write_all(b"\n")?;
    Ok(path)
}

pub fn registrar_log_extracao(
    root: &Path,
    url: &str,
    estrategias: &serde_json::Value,
) -> Result<PathBuf, StorageErro> {
    let directory = root.join("logs");
    fs::create_dir_all(&directory)?;
    let date = Utc::now().format("%Y-%m-%d");
    let path = directory.join(format!("{date}.jsonl"));
    let record = serde_json::json!({
        "timestamp": Utc::now().to_rfc3339(),
        "tipo": "estrategias_extracao",
        "url": url,
        "estrategias": estrategias,
    });
    let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
    serde_json::to_writer(&mut file, &record)?;
    file.write_all(b"\n")?;
    Ok(path)
}

pub struct Database {
    connection: Mutex<Connection>,
}

impl Database {
    pub fn open(path: PathBuf) -> Result<Self, StorageErro> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let connection = Connection::open(path)?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        migrate(&connection)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    pub fn open_in_memory() -> Result<Self, StorageErro> {
        let connection = Connection::open_in_memory()?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        migrate(&connection)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    pub fn user_version(&self) -> Result<i64, StorageErro> {
        let connection = self
            .connection
            .lock()
            .expect("lock do SQLite não deve envenenar");
        Ok(connection.query_row("PRAGMA user_version", [], |row| row.get(0))?)
    }

    pub fn salvar_snapshot(
        &self,
        busca: &str,
        anuncio: &AnuncioBruto,
        data: NaiveDate,
    ) -> Result<(), StorageErro> {
        let mut connection = self
            .connection
            .lock()
            .expect("lock do SQLite não deve envenenar");
        let transaction = connection.transaction()?;
        let id = multiloja::id_produto_ml(&anuncio.id);
        transaction.execute(
            "INSERT INTO produtos (id, titulo, titulo_norm, modelo, vendedor_id, vendedor, reputacao, url, busca, visto_em) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10) ON CONFLICT(id) DO UPDATE SET titulo=excluded.titulo, titulo_norm=excluded.titulo_norm, modelo=excluded.modelo, vendedor_id=excluded.vendedor_id, vendedor=excluded.vendedor, reputacao=excluded.reputacao, url=excluded.url, busca=excluded.busca, visto_em=excluded.visto_em",
            params![id, anuncio.titulo, crate::core::normalizar(&anuncio.titulo), anuncio.modelo, anuncio.vendedor_id, anuncio.vendedor, anuncio.vendedor_score, anuncio.url, busca, Utc::now().to_rfc3339()],
        )?;
        transaction.execute(
            "INSERT INTO snapshots (produto_id, data, preco, preco_original, nota, qtd_avaliacoes, condicao, preco_brl, preco_final_brl) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8) ON CONFLICT(produto_id,data) DO UPDATE SET preco=excluded.preco,preco_original=excluded.preco_original,nota=excluded.nota,qtd_avaliacoes=excluded.qtd_avaliacoes,condicao=excluded.condicao,preco_brl=excluded.preco_brl,preco_final_brl=excluded.preco_final_brl",
            params![id, data.to_string(), anuncio.preco, anuncio.preco_original, anuncio.nota, anuncio.qtd_avaliacoes, anuncio.condicao, anuncio.preco.to_string()],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn contar_snapshots(&self) -> Result<u64, StorageErro> {
        let connection = self
            .connection
            .lock()
            .expect("lock do SQLite não deve envenenar");
        Ok(
            connection.query_row("SELECT COUNT(*) FROM snapshots", [], |row| {
                row.get::<_, i64>(0).map(|count| count as u64)
            })?,
        )
    }

    pub fn historico(&self, product_id: &str) -> Result<Vec<PontoHistorico>, StorageErro> {
        let connection = self
            .connection
            .lock()
            .expect("lock do SQLite não deve envenenar");
        let mut statement = connection.prepare("SELECT data, preco, preco_original, nota, qtd_avaliacoes FROM snapshots WHERE produto_id = ?1 ORDER BY data ASC")?;
        let rows = statement.query_map([multiloja::id_produto_ml(product_id)], |row| {
            Ok(PontoHistorico {
                data: row.get(0)?,
                preco: row.get(1)?,
                preco_original: row.get(2)?,
                nota: row.get(3)?,
                qtd_avaliacoes: row.get(4)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn todos_anuncios(&self) -> Result<Vec<AnuncioBruto>, StorageErro> {
        let connection = self
            .connection
            .lock()
            .expect("lock do SQLite não deve envenenar");
        let mut statement = connection.prepare("SELECT substr(p.id,4), p.titulo, s.preco, s.preco_original, p.url, s.condicao, p.vendedor, p.vendedor_id, p.reputacao, s.nota, s.qtd_avaliacoes, p.modelo FROM produtos p JOIN snapshots s ON p.id = s.produto_id WHERE p.loja = 'ml' AND s.data = (SELECT MAX(s2.data) FROM snapshots s2 WHERE s2.produto_id = s.produto_id)")?;
        let rows = statement.query_map([], |row| {
            Ok(AnuncioBruto {
                id: row.get(0)?,
                titulo: row.get(1)?,
                preco: row.get(2)?,
                preco_original: row.get(3)?,
                url: row.get(4)?,
                condicao: row.get(5)?,
                vendedor: row.get(6)?,
                vendedor_id: row.get(7)?,
                vendedor_score: row.get(8)?,
                vendedor_nivel: None,
                nota: row.get(9)?,
                qtd_avaliacoes: row.get(10)?,
                modelo: row.get(11)?,
                gtin: None,
                marca: None,
                frete: None,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn tentar_fonte(
        &self,
        fonte: &str,
        agora_ms: i64,
    ) -> Result<PermissaoTentativaFonte, StorageErro> {
        let fonte = multiloja::chave_fonte(fonte);
        let fonte = fonte.as_str();
        let mut connection = self
            .connection
            .lock()
            .expect("lock do SQLite não deve envenenar");
        let transaction = connection.transaction()?;
        let saved = transaction
            .query_row(
                "SELECT falhas_seguidas, aberto_ate_ms, sondagem_meio_aberto FROM circuitos_fontes WHERE fonte = ?1",
                [fonte],
                |row| {
                    Ok((
                        row.get::<_, u32>(0)?,
                        row.get::<_, Option<i64>>(1)?,
                        row.get::<_, bool>(2)?,
                    ))
                },
            )
            .optional()?;
        let (failures, open_until, probe_claimed) = saved.unwrap_or((0, None, false));
        let permission = match open_until {
            Some(until) if until > agora_ms => PermissaoTentativaFonte {
                permitido: false,
                estado: EstadoCircuito::Aberto,
            },
            Some(_) if probe_claimed => PermissaoTentativaFonte {
                permitido: false,
                estado: EstadoCircuito::MeioAberto,
            },
            Some(_) => {
                transaction.execute(
                    "UPDATE circuitos_fontes SET sondagem_meio_aberto = 1 WHERE fonte = ?1",
                    [fonte],
                )?;
                PermissaoTentativaFonte {
                    permitido: true,
                    estado: EstadoCircuito::MeioAberto,
                }
            }
            None => PermissaoTentativaFonte {
                permitido: true,
                estado: if failures >= 3 {
                    EstadoCircuito::MeioAberto
                } else {
                    EstadoCircuito::Fechado
                },
            },
        };
        transaction.commit()?;
        Ok(permission)
    }

    pub fn registrar_tentativa_fonte(
        &self,
        fonte: &str,
        sucesso: bool,
        latencia_ms: u64,
        erro: Option<&str>,
        agora_ms: i64,
    ) -> Result<EstadoCircuito, StorageErro> {
        let fonte = multiloja::chave_fonte(fonte);
        let fonte = fonte.as_str();
        let mut connection = self
            .connection
            .lock()
            .expect("lock do SQLite não deve envenenar");
        let transaction = connection.transaction()?;
        let saved = transaction
            .query_row(
                "SELECT falhas_seguidas, aberto_ate_ms FROM circuitos_fontes WHERE fonte = ?1",
                [fonte],
                |row| Ok((row.get::<_, u32>(0)?, row.get::<_, Option<i64>>(1)?)),
            )
            .optional()?;
        let (failures, open_until) = saved.unwrap_or((0, None));
        let state = if sucesso {
            transaction.execute(
                "INSERT INTO circuitos_fontes (fonte, falhas_seguidas, aberto_ate_ms, sondagem_meio_aberto) VALUES (?1, 0, NULL, 0) ON CONFLICT(fonte) DO UPDATE SET falhas_seguidas = 0, aberto_ate_ms = NULL, sondagem_meio_aberto = 0",
                [fonte],
            )?;
            EstadoCircuito::Fechado
        } else {
            let is_half_open_probe = open_until.is_some_and(|until| until <= agora_ms);
            let next_failures = if is_half_open_probe {
                3
            } else {
                failures.saturating_add(1)
            };
            if next_failures >= 3 {
                let retry_at = agora_ms.saturating_add(60 * 60 * 1000);
                transaction.execute(
                    "INSERT INTO circuitos_fontes (fonte, falhas_seguidas, aberto_ate_ms, sondagem_meio_aberto) VALUES (?1, ?2, ?3, 0) ON CONFLICT(fonte) DO UPDATE SET falhas_seguidas = excluded.falhas_seguidas, aberto_ate_ms = excluded.aberto_ate_ms, sondagem_meio_aberto = 0",
                    rusqlite::params![fonte, next_failures, retry_at],
                )?;
                EstadoCircuito::Aberto
            } else {
                transaction.execute(
                    "INSERT INTO circuitos_fontes (fonte, falhas_seguidas, aberto_ate_ms, sondagem_meio_aberto) VALUES (?1, ?2, NULL, 0) ON CONFLICT(fonte) DO UPDATE SET falhas_seguidas = excluded.falhas_seguidas, aberto_ate_ms = NULL, sondagem_meio_aberto = 0",
                    rusqlite::params![fonte, next_failures],
                )?;
                EstadoCircuito::Fechado
            }
        };
        let timestamp = DateTime::<Utc>::from_timestamp_millis(agora_ms)
            .unwrap_or_default()
            .to_rfc3339();
        let latencia_sql = i64::try_from(latencia_ms).unwrap_or(i64::MAX);
        transaction.execute(
            "INSERT INTO saude_fontes (fonte, timestamp, timestamp_ms, sucesso, latencia_ms, erro, estado_circuito) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![fonte, timestamp, agora_ms, sucesso, latencia_sql, erro, state.as_str()],
        )?;
        transaction.commit()?;
        Ok(state)
    }

    pub fn saude_fontes(&self, agora_ms: i64) -> Result<Vec<EstadoSaudeFonte>, StorageErro> {
        let connection = self
            .connection
            .lock()
            .expect("lock do SQLite não deve envenenar");
        let mut statement = connection.prepare(
            "SELECT fonte FROM circuitos_fontes UNION SELECT fonte FROM saude_fontes ORDER BY fonte",
        )?;
        let sources = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        let since = agora_ms.saturating_sub(7 * 24 * 60 * 60 * 1000);
        sources
            .into_iter()
            .map(|fonte| {
                let (attempts, successes, last_failure, updated): (
                    i64,
                    i64,
                    Option<String>,
                    Option<String>,
                ) = connection.query_row(
                    "SELECT COUNT(*), COALESCE(SUM(CASE WHEN sucesso = 1 THEN 1 ELSE 0 END), 0), (SELECT timestamp FROM saude_fontes WHERE fonte = ?1 AND sucesso = 0 ORDER BY timestamp_ms DESC LIMIT 1), (SELECT timestamp FROM saude_fontes WHERE fonte = ?1 ORDER BY timestamp_ms DESC LIMIT 1) FROM saude_fontes WHERE fonte = ?1 AND timestamp_ms >= ?2",
                    rusqlite::params![fonte, since],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )?;
                let open_until = connection
                    .query_row(
                        "SELECT aberto_ate_ms FROM circuitos_fontes WHERE fonte = ?1",
                        [&fonte],
                        |row| row.get::<_, Option<i64>>(0),
                    )
                    .optional()?
                    .flatten();
                let breaker_state = match open_until {
                    Some(until) if until > agora_ms => EstadoCircuito::Aberto,
                    Some(_) => EstadoCircuito::MeioAberto,
                    None => EstadoCircuito::Fechado,
                };
                Ok(EstadoSaudeFonte {
                    loja: fonte.split_once(':').and_then(|(id,_)|id.parse().ok()).unwrap_or(crate::lojas::LojaId::Ml),
                    fonte: fonte.split_once(':').map_or(fonte.clone(),|(_,nome)|nome.to_owned()),
                    estado_circuito: breaker_state,
                    sucesso_7d: if attempts == 0 {
                        0.0
                    } else {
                        successes as f64 / attempts as f64
                    },
                    tentativas_7d: attempts as u64,
                    ultima_falha: last_failure,
                    atualizado_em: updated,
                })
            })
            .collect()
    }

    pub fn anuncios_da_busca(&self, busca: &str) -> Result<Vec<AnuncioBruto>, StorageErro> {
        let connection = self
            .connection
            .lock()
            .expect("lock do SQLite não deve envenenar");
        let mut statement = connection.prepare("SELECT substr(p.id,4), p.titulo, s.preco, s.preco_original, p.url, s.condicao, p.vendedor, p.vendedor_id, p.reputacao, s.nota, s.qtd_avaliacoes, p.modelo FROM produtos p JOIN snapshots s ON p.id = s.produto_id WHERE p.loja = 'ml' AND p.busca = ?1 AND s.data = (SELECT MAX(s2.data) FROM snapshots s2 WHERE s2.produto_id = s.produto_id) ORDER BY p.id")?;
        let rows = statement.query_map([busca], |row| {
            Ok(AnuncioBruto {
                id: row.get(0)?,
                titulo: row.get(1)?,
                preco: row.get(2)?,
                preco_original: row.get(3)?,
                url: row.get(4)?,
                condicao: row.get(5)?,
                vendedor: row.get(6)?,
                vendedor_id: row.get(7)?,
                vendedor_score: row.get(8)?,
                vendedor_nivel: None,
                nota: row.get(9)?,
                qtd_avaliacoes: row.get(10)?,
                modelo: row.get(11)?,
                gtin: None,
                marca: None,
                frete: None,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}

fn migrate(connection: &Connection) -> Result<(), StorageErro> {
    let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version < 1 {
        connection.execute_batch(
            "BEGIN;
             CREATE TABLE produtos (
               id TEXT PRIMARY KEY, titulo TEXT NOT NULL, titulo_norm TEXT NOT NULL, modelo TEXT,
               vendedor_id TEXT, vendedor TEXT NOT NULL DEFAULT '', reputacao REAL NOT NULL DEFAULT 0.5,
               url TEXT NOT NULL, busca TEXT NOT NULL, visto_em TEXT NOT NULL
             );
             CREATE TABLE snapshots (
               produto_id TEXT NOT NULL REFERENCES produtos(id) ON DELETE CASCADE, data TEXT NOT NULL,
               preco REAL NOT NULL, preco_original REAL, nota REAL, qtd_avaliacoes INTEGER NOT NULL DEFAULT 0,
               condicao TEXT NOT NULL DEFAULT 'new', PRIMARY KEY (produto_id, data)
             );
             CREATE INDEX idx_snapshots_data ON snapshots(data);
             PRAGMA user_version = 1;
             COMMIT;"
        )?;
    }
    let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version < 2 {
        connection.execute_batch(
            "BEGIN;
             CREATE TABLE saude_fontes (
               id INTEGER PRIMARY KEY AUTOINCREMENT,
               fonte TEXT NOT NULL,
               timestamp TEXT NOT NULL,
               timestamp_ms INTEGER NOT NULL,
               sucesso INTEGER NOT NULL,
               latencia_ms INTEGER NOT NULL,
               erro TEXT,
               estado_circuito TEXT NOT NULL
             );
             CREATE INDEX idx_saude_fontes_timestamp ON saude_fontes(fonte, timestamp_ms);
             CREATE TABLE circuitos_fontes (
               fonte TEXT PRIMARY KEY,
               falhas_seguidas INTEGER NOT NULL DEFAULT 0,
               aberto_ate_ms INTEGER,
               sondagem_meio_aberto INTEGER NOT NULL DEFAULT 0
             );
             PRAGMA user_version = 2;
             COMMIT;",
        )?;
    }
    multiloja::migrate_v3(connection)?;
    multiloja::migrate_v4(connection)?;
    connection.execute_batch("CREATE INDEX IF NOT EXISTS idx_grupo_produto ON grupo_ofertas(produto_id); CREATE TABLE IF NOT EXISTS alertas_multi(entidade TEXT NOT NULL, grupo INTEGER NOT NULL, data TEXT NOT NULL, PRIMARY KEY(entidade,grupo,data));")?;
    Ok(())
}

pub fn resumir_historico(product_id: &str, points: Vec<PontoHistorico>) -> HistoricoProduto {
    calcular_historico(product_id, &points, Utc::now().date_naive())
}

pub fn exportar_csv(
    root: &Path,
    slug: &str,
    result: &ResultadoBusca,
) -> Result<PathBuf, StorageErro> {
    validar_slug(slug)?;
    let date = Utc::now().format("%Y-%m-%d");
    let path = root.join("exports").join(format!("{slug}_{date}.csv"));
    let mut file = fs::File::create(&path)?;
    use std::io::Write;
    file.write_all(&[0xEF, 0xBB, 0xBF])?;
    let mut writer = csv::Writer::from_writer(file);
    writer.write_record([
        "posição",
        "score",
        "título",
        "preço",
        "nota",
        "vendedor",
        "suspeito",
        "pareto",
        "url",
    ])?;
    for (index, group) in result.produtos.iter().enumerate() {
        writer.write_record([
            (index + 1).to_string(),
            format!("{:.4}", group.score),
            group.principal.anuncio.titulo.clone(),
            format!("{:.2}", group.principal.anuncio.preco),
            format!("{:.2}", group.principal.nota_bayesiana),
            group.principal.anuncio.vendedor.clone(),
            group.principal.suspeito.to_string(),
            group.pareto.to_string(),
            group.principal.anuncio.url.clone(),
        ])?;
    }
    writer.flush().map_err(std::io::Error::other)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn estrutura_e_config_nao_sao_sobrescritas() {
        let directory = tempdir().unwrap();
        initialize_data_dir(directory.path()).unwrap();
        let config_path = directory.path().join("config.json");
        let original = fs::read(&config_path).unwrap();
        fs::write(&config_path, b"config do usuario").unwrap();
        initialize_data_dir(directory.path()).unwrap();
        assert_eq!(fs::read(config_path).unwrap(), b"config do usuario");
        assert!(directory.path().join("cache/paginas").is_dir());
        assert!(!original.is_empty());
        assert_eq!(
            carregar_sinonimos(directory.path()).unwrap()["capa"],
            ["capa", "capinha", "case", "cover", "capas"]
        );
        let synonyms_path = directory.path().join("sinonimos.json");
        fs::write(&synonyms_path, br#"{"fone":["headphone"]}"#).unwrap();
        initialize_data_dir(directory.path()).unwrap();
        assert_eq!(
            carregar_sinonimos(directory.path()).unwrap()["fone"],
            ["headphone"]
        );
    }

    #[test]
    fn migration_cria_schema_versionado_e_wal() {
        let directory = tempdir().unwrap();
        let database = Database::open(directory.path().join("data/garimpo.db")).unwrap();
        assert_eq!(database.user_version().unwrap(), 4);
    }

    #[test]
    fn circuito_abre_apos_tres_falhas_e_permite_sondagem_apos_uma_hora() {
        let database = Database::open_in_memory().unwrap();
        for second in 0..3 {
            assert_eq!(
                database.tentar_fonte("API", second * 1000).unwrap().estado,
                EstadoCircuito::Fechado
            );
            assert_eq!(
                database
                    .registrar_tentativa_fonte("API", false, 8, Some("offline"), second * 1000)
                    .unwrap(),
                if second == 2 {
                    EstadoCircuito::Aberto
                } else {
                    EstadoCircuito::Fechado
                }
            );
        }
        let before_expiry = database.tentar_fonte("API", 3_599_999).unwrap();
        assert!(!before_expiry.permitido);
        assert_eq!(before_expiry.estado, EstadoCircuito::Aberto);
        let half_open = database.tentar_fonte("API", 3_602_000).unwrap();
        assert!(half_open.permitido);
        assert_eq!(half_open.estado, EstadoCircuito::MeioAberto);
        let second_probe = database.tentar_fonte("API", 3_602_001).unwrap();
        assert!(!second_probe.permitido);
        assert_eq!(second_probe.estado, EstadoCircuito::MeioAberto);
        assert_eq!(
            database
                .registrar_tentativa_fonte("API", true, 12, None, 3_602_002)
                .unwrap(),
            EstadoCircuito::Fechado
        );
    }

    #[test]
    fn saude_de_fontes_calcula_taxa_de_sucesso_e_ultima_falha() {
        let database = Database::open_in_memory().unwrap();
        let now = 10_000_000_i64;
        database
            .registrar_tentativa_fonte("WebView", true, 30, None, now - 1000)
            .unwrap();
        database
            .registrar_tentativa_fonte("WebView", false, 40, Some("verificação"), now)
            .unwrap();
        let status = database.saude_fontes(now).unwrap();
        assert_eq!(status.len(), 1);
        assert_eq!(status[0].fonte, "WebView");
        assert_eq!(status[0].tentativas_7d, 2);
        assert_eq!(status[0].sucesso_7d, 0.5);
        let latest_failure = status[0].ultima_falha.as_deref().unwrap();
        assert_eq!(
            DateTime::parse_from_rfc3339(latest_failure)
                .unwrap()
                .timestamp_millis(),
            now
        );
    }

    #[test]
    fn snapshot_diario_e_idempotente() {
        let database = Database::open_in_memory().unwrap();
        let ad =
            crate::coleta::parse_listagem_html(include_str!("../../tests/fixtures/listagem.html"))
                .unwrap()
                .remove(0);
        let day = NaiveDate::from_ymd_opt(2026, 10, 11).unwrap();
        database.salvar_snapshot("fone", &ad, day).unwrap();
        database.salvar_snapshot("fone", &ad, day).unwrap();
        assert_eq!(database.contar_snapshots().unwrap(), 1);
    }

    #[test]
    fn export_csv_tem_bom_cabecalho_e_acentos() {
        let directory = tempdir().unwrap();
        initialize_data_dir(directory.path()).unwrap();
        let mut ad =
            crate::coleta::parse_listagem_html(include_str!("../../tests/fixtures/listagem.html"))
                .unwrap()
                .remove(0);
        ad.titulo = "Fone Ruído ANC".into();
        let result = crate::core::executar_pipeline(
            "fone",
            &crate::models::SearchConfig {
                termo: "fone".into(),
                min_avaliacoes: 0,
                ..Default::default()
            },
            vec![ad],
        )
        .unwrap();
        let path = exportar_csv(directory.path(), "fone", &result).unwrap();
        let bytes = fs::read(path).unwrap();
        assert_eq!(&bytes[..3], &[0xEF, 0xBB, 0xBF]);
        assert!(
            String::from_utf8(bytes[3..].to_vec())
                .unwrap()
                .contains("Ruído")
        );
    }
}
