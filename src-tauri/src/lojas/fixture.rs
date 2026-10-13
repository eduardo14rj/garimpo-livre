//! Mesmo registro usado pelo comando desktop e pelo smoke headless, sem janela/rede.
use super::*;
use std::sync::Arc;

pub fn registro_fixture() -> Result<RegistroLojas, String> {
    let mut r = RegistroLojas::default();
    r.registrar(Box::new(LojaMl::nova(vec![LojaMl::adaptar_fonte(
        "Fixture",
        Arc::new(crate::coleta::FonteFixture),
    )])))?;
    r.registrar(Box::new(ebay::LojaEbay::nova(vec![FonteDaLoja::nova(
        "Fixture",
        Arc::new(ebay::FonteEbayFixture),
    )])))?;
    r.registrar(Box::new(shopee::LojaShopee::nova(vec![FonteDaLoja::nova(
        "Fixture",
        Arc::new(shopee::FonteShopeeFixture),
    )])))?;
    r.registrar(Box::new(aliexpress::LojaAliexpress::nova(vec![
        FonteDaLoja::nova("Fixture", Arc::new(aliexpress::FonteAliexpressFixture)),
    ])))?;
    r.registrar(Box::new(amazon::LojaAmazon::nova(vec![FonteDaLoja::nova(
        "Fixture",
        Arc::new(amazon::FonteAmazonFixture),
    )])))?;
    r.registrar(Box::new(kabum::LojaKabum::nova(vec![FonteDaLoja::nova(
        "Fixture",
        Arc::new(FonteVarejoFixture {
            dados: kabum::FIXTURE,
            parser: kabum::parse_lista,
        }),
    )])))?;
    r.registrar(Box::new(pichau::LojaPichau::nova(vec![FonteDaLoja::nova(
        "Fixture",
        Arc::new(FonteVarejoFixture {
            dados: pichau::FIXTURE,
            parser: pichau::parse_lista,
        }),
    )])))?;
    // Demonstração não reutiliza nem atualiza o cache das coletas reais.
    for id in LojaId::TODAS {
        r.desabilitar_cache(id)?;
    }
    Ok(r)
}
pub async fn executar_fixture(
    slug: &str,
    config: &crate::models::SearchConfig,
    lojas: &[ConfigLoja],
    recursos: Arc<RecursosFontes>,
    mut publicar: impl FnMut(orchestrator::EventoLoja, crate::core::multiloja::RankingMultiLoja),
) -> Result<crate::core::multiloja::RankingMultiLoja, String> {
    let runner = OrquestradorMultiLoja::novo(registro_fixture()?, recursos);
    let contexto = ContextoNormalizacao;
    let instante = chrono::Utc::now().to_rfc3339();
    let mut parcial = ResultadoMultiLoja::default();
    let resultado = runner
        .executar(config, lojas, contexto, |evento| {
            parcial.status.insert(evento.loja, evento.status.clone());
            parcial.ofertas.extend(evento.ofertas.clone());
            if let Ok(mut r) = crate::core::multiloja::ranquear_multi(slug, config, parcial.clone())
            {
                r.demonstracao = true;
                r.executada_em = instante.clone();
                publicar(evento, r);
            }
        })
        .await?;
    let mut ranking = crate::core::multiloja::ranquear_multi(slug, config, resultado)?;
    ranking.demonstracao = true;
    ranking.executada_em = instante;
    Ok(ranking)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn headless_sete_lojas_usa_mesmo_fluxo_do_desktop_sem_rede_janelas_ou_snapshots_reais() {
        let dir = tempfile::tempdir().unwrap();
        let db = Arc::new(crate::storage::Database::open_in_memory().unwrap());
        let recursos = Arc::new(RecursosFontes::novos(db.clone(), dir.path().into()));
        let config = crate::models::SearchConfig {
            termo: "ssd nvme".into(),
            lojas: LojaId::TODAS.to_vec(),
            condicao: "any".into(),
            min_avaliacoes: 0,
            ..Default::default()
        };
        for _ in 0..2 {
            let mut eventos = vec![];
            let r = executar_fixture(
                "ssd",
                &config,
                &LojaId::TODAS.map(ConfigLoja::inicial),
                recursos.clone(),
                |_, r| eventos.push(r),
            )
            .await
            .unwrap();
            assert!(r.demonstracao);
            assert_eq!(r.anuncios_coletados, 26);
            assert!(
                !r.produtos
                    .iter()
                    .flat_map(|grupo| &grupo.ofertas)
                    .any(|oferta| oferta.oferta.id == "aliexpress:1005001002")
            );
            assert_eq!(eventos.len(), 7);
            assert!(r.status.values().all(|s| *s == StatusLoja::Ok));
            assert!(r.produtos.iter().any(|g| g.ofertas.len() >= 2));
            crate::storage::salvar_ranking_multi(dir.path(), &r).unwrap();
            assert!(
                crate::storage::carregar_ranking_multi(dir.path(), "ssd")
                    .unwrap()
                    .unwrap()
                    .demonstracao
            );
        }
        assert_eq!(db.contar_snapshots().unwrap(), 0);
        assert!(!dir.path().join("cache/lojas").exists());
    }
}
