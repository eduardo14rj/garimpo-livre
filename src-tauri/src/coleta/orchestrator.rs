use std::{
    future::Future,
    path::PathBuf,
    sync::Mutex,
    time::{Duration, Instant},
};

use async_trait::async_trait;
use thiserror::Error;

use crate::{
    core::executar_pipeline,
    models::{AnuncioBruto, ProdutoBruto, ResultadoBusca, SearchConfig, TentativaFonte},
    storage::{Database, EstadoCircuito},
};

use super::{ColetaErro, Fonte};

const LIMITE_ANUNCIOS: usize = 50;
const LIMITE_PRECOS_AUSENTES: f64 = 0.5;

pub struct FonteCandidata {
    pub nome: String,
    pub fonte: Box<dyn Fonte>,
    pub timeout: Option<Duration>,
    pub degradada: bool,
    pub usa_circuito: bool,
}

impl FonteCandidata {
    pub fn nova(
        nome: impl Into<String>,
        fonte: Box<dyn Fonte>,
        timeout: Duration,
        degradada: bool,
    ) -> Self {
        Self {
            nome: nome.into(),
            fonte,
            timeout: Some(timeout),
            degradada,
            usa_circuito: true,
        }
    }

    pub fn sem_circuito(mut self) -> Self {
        self.usa_circuito = false;
        self
    }

    pub fn sem_timeout_externo(mut self) -> Self {
        self.timeout = None;
        self
    }
}

pub struct Orquestrador {
    fontes: Vec<FonteCandidata>,
}

#[derive(Debug, Clone)]
pub struct DetalheFonte {
    pub produto: ProdutoBruto,
    pub fonte: String,
    pub tentativas: Vec<TentativaFonte>,
    pub cache_origem: Option<String>,
}

impl Orquestrador {
    pub fn novo(fontes: Vec<FonteCandidata>) -> Self {
        Self { fontes }
    }

    pub async fn executar(
        &self,
        database: &Database,
        slug: &str,
        config: &SearchConfig,
    ) -> Result<ResultadoBusca, OrquestradorErro> {
        self.executar_em(
            database,
            slug,
            config,
            chrono::Utc::now().timestamp_millis(),
        )
        .await
    }

    pub async fn detalhar(
        &self,
        database: &Database,
        url: &str,
    ) -> Result<DetalheFonte, OrquestradorErro> {
        let agora_ms = chrono::Utc::now().timestamp_millis();
        let mut tentativas = Vec::new();
        let mut ultimo_erro = None;
        for candidata in &self.fontes {
            let permissao = if candidata.usa_circuito {
                database.tentar_fonte(&candidata.nome, agora_ms)?
            } else {
                crate::storage::PermissaoTentativaFonte {
                    permitido: true,
                    estado: EstadoCircuito::Fechado,
                }
            };
            if !permissao.permitido {
                let erro = format!(
                    "circuito {} está {}",
                    candidata.nome,
                    permissao.estado.as_str()
                );
                tentativas.push(TentativaFonte {
                    fonte: candidata.nome.clone(),
                    sucesso: false,
                    duracao_ms: 0,
                    erro: Some(erro.clone()),
                });
                ultimo_erro = Some(erro);
                continue;
            }
            let iniciou = Instant::now();
            let resposta = limitar(candidata.timeout, candidata.fonte.detalhar(url))
                .await
                .and_then(validar_detalhe);
            let duracao_ms = iniciou.elapsed().as_millis().min(u64::MAX as u128) as u64;
            match resposta {
                Ok(produto) => {
                    if candidata.usa_circuito {
                        database.registrar_tentativa_fonte(
                            &candidata.nome,
                            true,
                            duracao_ms,
                            None,
                            agora_ms,
                        )?;
                    }
                    tentativas.push(TentativaFonte {
                        fonte: candidata.nome.clone(),
                        sucesso: true,
                        duracao_ms,
                        erro: None,
                    });
                    return Ok(DetalheFonte {
                        produto,
                        fonte: candidata.nome.clone(),
                        tentativas,
                        cache_origem: candidata.fonte.cache_timestamp(),
                    });
                }
                Err(error) => {
                    let message = error.to_string();
                    if candidata.usa_circuito {
                        database.registrar_tentativa_fonte(
                            &candidata.nome,
                            false,
                            duracao_ms,
                            Some(&message),
                            agora_ms,
                        )?;
                    }
                    tentativas.push(TentativaFonte {
                        fonte: candidata.nome.clone(),
                        sucesso: false,
                        duracao_ms,
                        erro: Some(message.clone()),
                    });
                    ultimo_erro = Some(message);
                }
            }
        }
        Err(OrquestradorErro::FontesIndisponiveis(
            ultimo_erro.unwrap_or_else(|| "não há fonte disponível para detalhar o link".into()),
        ))
    }

    async fn executar_em(
        &self,
        database: &Database,
        slug: &str,
        config: &SearchConfig,
        agora_ms: i64,
    ) -> Result<ResultadoBusca, OrquestradorErro> {
        if self.fontes.is_empty() {
            return Err(OrquestradorErro::SemFontes);
        }

        let mut tentativas = Vec::new();
        let mut fontes_sem_resultado = 0;
        let mut ultimo_erro = None;
        for candidata in &self.fontes {
            let permissao = if candidata.usa_circuito {
                database.tentar_fonte(&candidata.nome, agora_ms)?
            } else {
                crate::storage::PermissaoTentativaFonte {
                    permitido: true,
                    estado: EstadoCircuito::Fechado,
                }
            };
            if !permissao.permitido {
                let erro = format!(
                    "circuito {} está {}",
                    candidata.nome,
                    permissao.estado.as_str()
                );
                tentativas.push(TentativaFonte {
                    fonte: candidata.nome.clone(),
                    sucesso: false,
                    duracao_ms: 0,
                    erro: Some(erro.clone()),
                });
                ultimo_erro = Some(erro);
                continue;
            }

            let iniciou = Instant::now();
            let coleta = limitar(candidata.timeout, candidata.fonte.listar(&config.termo)).await;
            let duracao_ms = iniciou.elapsed().as_millis().min(u64::MAX as u128) as u64;
            let anuncios = coleta.and_then(validar_anuncios);

            match anuncios {
                Ok(anuncios) => {
                    let estado = if candidata.usa_circuito {
                        database.registrar_tentativa_fonte(
                            &candidata.nome,
                            true,
                            duracao_ms,
                            None,
                            agora_ms,
                        )?
                    } else {
                        EstadoCircuito::Fechado
                    };
                    tentativas.push(TentativaFonte {
                        fonte: candidata.nome.clone(),
                        sucesso: true,
                        duracao_ms,
                        erro: None,
                    });
                    if anuncios.is_empty() {
                        fontes_sem_resultado += 1;
                        if fontes_sem_resultado < 2 {
                            continue;
                        }
                    }
                    let mut result = executar_pipeline(slug, config, anuncios)?;
                    result.fonte = candidata.nome.clone();
                    result.degradado = candidata.degradada;
                    result.tentativas = tentativas;
                    result.cache_origem = candidata.fonte.cache_timestamp();
                    if let Some(original) = &result.cache_origem {
                        result.executada_em = original.clone();
                    }
                    debug_assert!(matches!(estado, EstadoCircuito::Fechado));
                    return Ok(result);
                }
                Err(error) => {
                    let message = error.to_string();
                    if candidata.usa_circuito {
                        database.registrar_tentativa_fonte(
                            &candidata.nome,
                            false,
                            duracao_ms,
                            Some(&message),
                            agora_ms,
                        )?;
                    }
                    tentativas.push(TentativaFonte {
                        fonte: candidata.nome.clone(),
                        sucesso: false,
                        duracao_ms,
                        erro: Some(message.clone()),
                    });
                    ultimo_erro = Some(message);
                }
            }
        }
        Err(OrquestradorErro::FontesIndisponiveis(
            ultimo_erro.unwrap_or_else(|| {
                format!(
                    "{} fontes não foram suficientes para validar a resposta",
                    self.fontes.len()
                )
            }),
        ))
    }
}

pub(crate) fn validar_anuncios(
    anuncios: Vec<AnuncioBruto>,
) -> Result<Vec<AnuncioBruto>, ColetaErro> {
    if anuncios.len() > LIMITE_ANUNCIOS {
        return Err(ColetaErro::ResultadoInvalido(format!(
            "{} anúncios excedem o limite de {LIMITE_ANUNCIOS}",
            anuncios.len()
        )));
    }
    if anuncios.is_empty() {
        return Ok(anuncios);
    }
    if anuncios
        .iter()
        .any(|anuncio| anuncio.id.trim().is_empty() || anuncio.titulo.trim().is_empty())
    {
        return Err(ColetaErro::ResultadoInvalido(
            "algum anúncio não contém id ou título".into(),
        ));
    }
    let sem_preco = anuncios
        .iter()
        .filter(|anuncio| !anuncio.preco.is_finite() || anuncio.preco <= 0.0)
        .count();
    if sem_preco as f64 / anuncios.len() as f64 > LIMITE_PRECOS_AUSENTES {
        return Err(ColetaErro::ResultadoInvalido(format!(
            "{sem_preco} de {} anúncios não têm preço",
            anuncios.len()
        )));
    }
    Ok(anuncios)
}

fn validar_detalhe(produto: ProdutoBruto) -> Result<ProdutoBruto, ColetaErro> {
    if produto.id.trim().is_empty()
        || produto.titulo.trim().is_empty()
        || !produto.preco.is_finite()
        || produto.preco <= 0.0
    {
        return Err(ColetaErro::ResultadoInvalido(
            "o detalhe precisa conter id, título e preço válido".into(),
        ));
    }
    Ok(produto)
}

async fn limitar<T, F>(timeout: Option<Duration>, future: F) -> Result<T, ColetaErro>
where
    F: Future<Output = Result<T, ColetaErro>>,
{
    match timeout {
        Some(duration) => tokio::time::timeout(duration, future)
            .await
            .map_err(|_| ColetaErro::TempoLimite)?,
        None => future.await,
    }
}

#[derive(Debug, Error)]
pub enum OrquestradorErro {
    #[error("nenhuma fonte foi configurada")]
    SemFontes,
    #[error("nenhuma fonte disponível: {0}")]
    FontesIndisponiveis(String),
    #[error(transparent)]
    Storage(#[from] crate::storage::StorageErro),
    #[error("falha ao calcular o ranking: {0}")]
    Pipeline(String),
}

impl From<String> for OrquestradorErro {
    fn from(value: String) -> Self {
        Self::Pipeline(value)
    }
}

pub struct FonteCache {
    diretorio_dados: PathBuf,
    origem: Mutex<Option<String>>,
}

impl FonteCache {
    pub fn nova(diretorio_dados: PathBuf) -> Self {
        Self {
            diretorio_dados,
            origem: Mutex::new(None),
        }
    }

    fn carregar(&self, termo: &str) -> Result<ResultadoBusca, ColetaErro> {
        let slug = crate::storage::slugificar(termo);
        let result = crate::storage::carregar_resultado_cache(&self.diretorio_dados, &slug)
            .map_err(|error| ColetaErro::Fonte(error.to_string()))?
            .ok_or_else(|| ColetaErro::Fonte("não existe resultado anterior no cache".into()))?;
        *self
            .origem
            .lock()
            .expect("lock do cache não deve envenenar") = Some(result.executada_em.clone());
        Ok(result)
    }
}

#[async_trait]
impl Fonte for FonteCache {
    async fn listar(&self, termo: &str) -> Result<Vec<AnuncioBruto>, ColetaErro> {
        let result = self.carregar(termo)?;
        let mut anuncios = result
            .produtos
            .into_iter()
            .flat_map(|grupo| grupo.ofertas.into_iter().map(|oferta| oferta.anuncio))
            .collect::<Vec<_>>();
        anuncios.sort_by(|left, right| left.id.cmp(&right.id));
        anuncios.dedup_by(|left, right| left.id == right.id);
        Ok(anuncios)
    }

    async fn detalhar(&self, url: &str) -> Result<ProdutoBruto, ColetaErro> {
        let searches = crate::storage::listar_buscas(&self.diretorio_dados)
            .map_err(|error| ColetaErro::Fonte(error.to_string()))?;
        for search in searches {
            if let Ok(result) = self.carregar(&search.config.termo) {
                for offer in result.produtos.into_iter().flat_map(|group| group.ofertas) {
                    if offer.anuncio.url == url {
                        let ad = offer.anuncio;
                        return Ok(ProdutoBruto {
                            id: ad.id,
                            titulo: ad.titulo,
                            preco: ad.preco,
                            nota: ad.nota,
                            qtd_avaliacoes: ad.qtd_avaliacoes,
                            url: ad.url,
                            gtin: ad.gtin,
                            marca: ad.marca,
                            modelo: ad.modelo,
                            frete: ad.frete,
                        });
                    }
                }
            }
        }
        Err(ColetaErro::Fonte("produto não encontrado no cache".into()))
    }

    fn cache_timestamp(&self) -> Option<String> {
        self.origem
            .lock()
            .expect("lock do cache não deve envenenar")
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use super::*;

    struct FonteTeste {
        respostas: Mutex<Vec<Result<Vec<AnuncioBruto>, ColetaErro>>>,
        chamadas: Arc<AtomicUsize>,
        atraso: Duration,
        detalhe: Mutex<Vec<Result<ProdutoBruto, ColetaErro>>>,
    }

    impl FonteTeste {
        fn nova(respostas: Vec<Result<Vec<AnuncioBruto>, ColetaErro>>) -> (Self, Arc<AtomicUsize>) {
            let chamadas = Arc::new(AtomicUsize::new(0));
            (
                Self {
                    respostas: Mutex::new(respostas),
                    chamadas: chamadas.clone(),
                    atraso: Duration::ZERO,
                    detalhe: Mutex::new(Vec::new()),
                },
                chamadas,
            )
        }
    }

    #[async_trait]
    impl Fonte for FonteTeste {
        async fn listar(&self, _: &str) -> Result<Vec<AnuncioBruto>, ColetaErro> {
            self.chamadas.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(self.atraso).await;
            self.respostas.lock().unwrap().remove(0)
        }

        async fn detalhar(&self, _: &str) -> Result<ProdutoBruto, ColetaErro> {
            self.detalhe
                .lock()
                .unwrap()
                .pop()
                .unwrap_or_else(|| Err(ColetaErro::Fonte("não usado pelo teste".into())))
        }
    }

    fn config() -> SearchConfig {
        SearchConfig {
            termo: "fone bluetooth".into(),
            min_avaliacoes: 0,
            ..Default::default()
        }
    }

    fn anuncio(preco: f64) -> AnuncioBruto {
        AnuncioBruto {
            id: "MLB1".into(),
            titulo: "Fone Bluetooth".into(),
            preco,
            preco_original: None,
            url: "https://www.mercadolivre.com.br/MLB1".into(),
            condicao: "new".into(),
            vendedor: "Loja".into(),
            vendedor_id: None,
            vendedor_score: 0.5,
            vendedor_nivel: None,
            nota: Some(4.5),
            qtd_avaliacoes: 20,
            modelo: None,
            gtin: None,
            marca: None,
            frete: None,
        }
    }

    fn candidate(name: &str, source: FonteTeste, degraded: bool) -> FonteCandidata {
        FonteCandidata::nova(name, Box::new(source), Duration::from_millis(50), degraded)
    }

    #[tokio::test]
    async fn fallback_rejeita_precos_invalidos_e_aceita_a_fonte_seguinte() {
        let database = Database::open_in_memory().unwrap();
        let (invalid, _) =
            FonteTeste::nova(vec![Ok(vec![anuncio(0.0), anuncio(0.0), anuncio(30.0)])]);
        let (valid, _) = FonteTeste::nova(vec![Ok(vec![anuncio(30.0)])]);
        let orchestrator = Orquestrador::novo(vec![
            candidate("API", invalid, false),
            candidate("WebView", valid, false),
        ]);
        let result = orchestrator
            .executar_em(&database, "fone", &config(), 1_000)
            .await
            .unwrap();
        assert_eq!(result.fonte, "WebView");
        assert_eq!(result.tentativas.len(), 2);
        assert!(!result.tentativas[0].sucesso);
        assert!(result.tentativas[1].sucesso);
    }

    #[tokio::test]
    async fn duas_fontes_validas_sem_resultado_aceitam_lista_vazia() {
        let database = Database::open_in_memory().unwrap();
        let (empty_a, _) = FonteTeste::nova(vec![Ok(vec![])]);
        let (empty_b, _) = FonteTeste::nova(vec![Ok(vec![])]);
        let orchestrator = Orquestrador::novo(vec![
            candidate("API", empty_a, false),
            candidate("WebView", empty_b, false),
        ]);
        let result = orchestrator
            .executar_em(&database, "fone", &config(), 1_000)
            .await
            .unwrap();
        assert!(result.produtos.is_empty());
        assert_eq!(result.tentativas.len(), 2);
        assert!(result.tentativas.iter().all(|attempt| attempt.sucesso));
    }

    #[tokio::test]
    async fn timeout_registra_falha_e_usa_fallback() {
        let database = Database::open_in_memory().unwrap();
        let (mut slow, _) = FonteTeste::nova(vec![Ok(vec![anuncio(30.0)])]);
        slow.atraso = Duration::from_millis(100);
        let (backup, calls) = FonteTeste::nova(vec![Ok(vec![anuncio(30.0)])]);
        let mut slow = candidate("API", slow, false);
        slow.timeout = Some(Duration::from_millis(1));
        let orchestrator = Orquestrador::novo(vec![slow, candidate("WebView", backup, true)]);
        let result = orchestrator
            .executar_em(&database, "fone", &config(), 1_000)
            .await
            .unwrap();
        assert_eq!(result.fonte, "WebView");
        assert!(result.degradado);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(database.saude_fontes(1_000).unwrap()[0].tentativas_7d, 1);
    }

    #[tokio::test]
    async fn detalhe_usa_fallback_e_valida_campos_obrigatorios() {
        let database = Database::open_in_memory().unwrap();
        let (primary, _) = FonteTeste::nova(vec![]);
        primary
            .detalhe
            .lock()
            .unwrap()
            .push(Err(ColetaErro::Fonte("offline".into())));
        let (backup, _) = FonteTeste::nova(vec![]);
        backup.detalhe.lock().unwrap().push(Ok(ProdutoBruto {
            id: "MLB8".into(),
            titulo: "Teclado".into(),
            preco: 99.0,
            nota: None,
            qtd_avaliacoes: 0,
            url: "https://www.mercadolivre.com.br/MLB8".into(),
            gtin: None,
            marca: None,
            modelo: None,
            frete: None,
        }));
        let orchestrator = Orquestrador::novo(vec![
            candidate("API", primary, false),
            candidate("WebView", backup, false),
        ]);
        let result = orchestrator
            .detalhar(&database, "https://www.mercadolivre.com.br/MLB8")
            .await
            .unwrap();
        assert_eq!(result.fonte, "WebView");
        assert_eq!(result.produto.id, "MLB8");
        assert_eq!(result.tentativas.len(), 2);
    }
}
