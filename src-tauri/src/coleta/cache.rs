use std::{future::Future, path::PathBuf, sync::Arc, time::Duration};

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};

use super::Clock;

#[derive(Debug, thiserror::Error)]
pub enum CacheErro {
    #[error("falha no cache: {0}")]
    Io(#[from] std::io::Error),
    #[error("falha ao serializar o cache: {0}")]
    Json(#[from] serde_json::Error),
    #[error("a fonte falhou: {0}")]
    Fonte(String),
}

#[derive(Debug, Serialize, Deserialize)]
struct PaginaCache<T> {
    capturado_em_ms: u64,
    dados: T,
}

pub struct CachePaginas {
    diretorio: PathBuf,
    ttl: Duration,
    clock: Arc<dyn Clock>,
}

impl CachePaginas {
    pub fn new(diretorio: PathBuf, ttl: Duration, clock: Arc<dyn Clock>) -> Self {
        Self {
            diretorio,
            ttl,
            clock,
        }
    }

    pub fn caminho(&self, url: &str) -> PathBuf {
        let digest = Sha256::digest(url.as_bytes());
        let filename = digest
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        self.diretorio.join(format!("{filename}.json"))
    }

    pub async fn buscar_ou_coletar<T, F, Fut>(&self, url: &str, coletar: F) -> Result<T, CacheErro>
    where
        T: Serialize + DeserializeOwned,
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T, String>>,
    {
        let path = self.caminho(url);
        if let Ok(raw) = std::fs::read(&path)
            && let Ok(entry) = serde_json::from_slice::<PaginaCache<T>>(&raw)
        {
            let age = self.clock.now_ms().saturating_sub(entry.capturado_em_ms);
            if age <= self.ttl.as_millis().min(u64::MAX as u128) as u64 {
                return Ok(entry.dados);
            }
        }
        let dados = coletar().await.map_err(CacheErro::Fonte)?;
        std::fs::create_dir_all(&self.diretorio)?;
        let raw = serde_json::to_vec(&PaginaCache {
            capturado_em_ms: self.clock.now_ms(),
            dados: &dados,
        })?;
        std::fs::write(path, raw)?;
        Ok(dados)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::{
        Arc,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    };
    use tempfile::tempdir;

    #[derive(Default)]
    struct RelogioFake(AtomicU64);
    #[async_trait]
    impl Clock for RelogioFake {
        fn now_ms(&self) -> u64 {
            self.0.load(Ordering::SeqCst)
        }
        async fn sleep(&self, duration: Duration) {
            self.0
                .fetch_add(duration.as_millis() as u64, Ordering::SeqCst);
        }
    }

    #[tokio::test]
    async fn cache_reusa_ate_expirar_e_tem_nome_sha256_deterministico() {
        let directory = tempdir().unwrap();
        let clock = Arc::new(RelogioFake::default());
        let cache = CachePaginas::new(
            directory.path().to_owned(),
            Duration::from_secs(3600),
            clock.clone(),
        );
        let url = "https://lista.mercadolivre.com.br/fone";
        assert_eq!(
            cache
                .caminho(url)
                .file_name()
                .unwrap()
                .to_string_lossy()
                .len(),
            69
        );
        let calls = AtomicUsize::new(0);
        for expected in ["primeira", "primeira"] {
            let value = cache
                .buscar_ou_coletar(url, || async {
                    calls.fetch_add(1, Ordering::SeqCst);
                    Ok::<_, String>(expected.to_owned())
                })
                .await
                .unwrap();
            assert_eq!(value, expected);
        }
        clock.0.store(3_600_001, Ordering::SeqCst);
        let value = cache
            .buscar_ou_coletar(url, || async {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok::<_, String>("renovada".to_owned())
            })
            .await
            .unwrap();
        assert_eq!(value, "renovada");
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }
}
