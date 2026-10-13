use super::*;
use crate::{
    core::bm25,
    lojas::{self, ContextoNormalizacao},
    models::SearchWeights,
    storage::{self, Database},
};
use chrono::{Days, NaiveDate};

fn oferta(loja: LojaId, id: &str, preco: &str) -> OfertaNormalizada {
    let mut bruto = lojas::kabum::parse_lista(serde_json::from_str(lojas::kabum::FIXTURE).unwrap())
        .unwrap()
        .remove(0);
    bruto.id_externo = id.into();
    bruto.preco = preco.parse().unwrap();
    ContextoNormalizacao.normalizar(loja, bruto).unwrap()
}
fn config() -> SearchConfig {
    SearchConfig {
        termo: "ssd nvme".into(),
        condicao: "any".into(),
        min_avaliacoes: 0,
        ..Default::default()
    }
}
fn ranking(ofertas: Vec<OfertaNormalizada>) -> RankingMultiLoja {
    ranquear_multi(
        "ssd",
        &config(),
        ResultadoMultiLoja {
            ofertas,
            ..Default::default()
        },
    )
    .unwrap()
}

#[test]
fn gtin_modelo_jaccard_e_menor_final_agrupam_tres_lojas() {
    let a = oferta(LojaId::Ml, "1", "405");
    let b = oferta(LojaId::Kabum, "2", "389");
    let c = oferta(LojaId::Pichau, "3", "399");
    let r = ranking(vec![a.clone(), b.clone(), c.clone()]);
    assert_eq!(r.produtos.len(), 1);
    assert_eq!(r.produtos[0].ofertas.len(), 3);
    assert_eq!(r.produtos[0].principal.oferta.id, "kabum:2");
    let mut modelo = vec![a, b, c];
    for o in &mut modelo {
        o.gtin = None;
    }
    assert_eq!(ranking(modelo.clone()).produtos.len(), 1);
    for o in &mut modelo {
        o.modelo = None;
        o.marca = None;
    }
    assert_eq!(ranking(modelo).produtos.len(), 1);
}
#[test]
fn capacidade_voltagem_gtin_marca_e_acessorio_conflitantes_nao_unem() {
    let a = oferta(LojaId::Ml, "1", "400");
    let mut b = a.clone();
    b.titulo = b.titulo.replace("1TB", "500GB").replace("1 TB", "500 GB");
    assert!(!mesmo_produto(&a, &b));
    b = a.clone();
    b.titulo.push_str(" 220V");
    let mut v = a.clone();
    v.titulo.push_str(" 127V");
    assert!(!mesmo_produto(&v, &b));
    b = a.clone();
    b.gtin = Some("1234567890123".into());
    assert!(!mesmo_produto(&a, &b));
    b = a.clone();
    b.gtin = None;
    let mut sem_gtin = a.clone();
    sem_gtin.gtin = None;
    b.marca = Some("outra".into());
    assert!(!mesmo_produto(&sem_gtin, &b));
    b = a.clone();
    b.titulo = format!("Capa para {}", b.titulo);
    assert!(!mesmo_produto(&a, &b));
}
#[test]
fn falta_de_capacidade_nao_cria_ponte_entre_variantes() {
    let mut a = oferta(LojaId::Ml, "1", "400");
    a.gtin = None;
    let mut b = a.clone();
    b.id = "kabum:2".into();
    b.titulo = "SSD Kingston NV2 PCIe NVMe".into();
    let mut c = a.clone();
    c.id = "pichau:3".into();
    c.titulo = "SSD Kingston NV2 500GB PCIe NVMe".into();
    assert_eq!(ranking(vec![a, b, c]).produtos.len(), 2);
    let mut equivalente = oferta(LojaId::Pichau, "4", "399");
    equivalente.titulo = equivalente
        .titulo
        .replace("1TB", "1000GB")
        .replace("1 TB", "1000 GB");
    assert!(mesmo_produto(&oferta(LojaId::Ml, "1", "400"), &equivalente));
}
#[test]
fn oferta_com_frete_usa_soma_observada_no_ranking() {
    let mut bruto = lojas::kabum::parse_lista(serde_json::from_str(lojas::kabum::FIXTURE).unwrap())
        .unwrap()
        .remove(0);
    bruto.id_externo = "1001".into();
    bruto.titulo = "SSD Kingston NV2 1TB NVMe".into();
    bruto.preco = 100.into();
    bruto.frete = Some(10.into());
    bruto.gtin = None;
    bruto.modelo = Some("SNV2S/1000G".into());
    bruto.marca = Some("Kingston".into());
    bruto.nota = Some(4.8);
    bruto.qtd_avaliacoes = Some(120);
    bruto.reputacao = Some(1.0);
    let oferta_com_frete = ContextoNormalizacao
        .normalizar(LojaId::Ebay, bruto)
        .unwrap();
    assert_eq!(oferta_com_frete.preco_brl, Decimal::from(100));
    assert_eq!(oferta_com_frete.frete_brl, Some(Decimal::from(10)));
    assert_eq!(oferta_com_frete.preco_final_brl, Decimal::from(110));
    let nacional = oferta(LojaId::Pichau, "local", "800");
    let mut outra = oferta(LojaId::Kabum, "outra", "850");
    outra.gtin = Some("1234567890123".into());
    outra.modelo = Some("outro modelo".into());
    let mut busca = config();
    busca.pesos = SearchWeights {
        preco: 1.0,
        nota: 0.0,
        vendedor: 0.0,
        relevancia: 0.0,
    };
    let r = ranquear_multi(
        "ssd",
        &busca,
        ResultadoMultiLoja {
            ofertas: vec![oferta_com_frete, nacional, outra],
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(r.produtos[0].principal.oferta.loja, LojaId::Ebay);
    assert!(r.produtos[0].pareto);
    assert_eq!(r.produtos[0].ofertas.len(), 2);
    assert!(r.produtos[1].principal.oferta.id == "kabum:outra");
}
#[test]
fn filtros_final_decimal_reviews_reais_e_termos_com_espacos() {
    let mut a = oferta(LojaId::Ml, "1", "50");
    a.preco_final_brl = "50.01".parse().unwrap();
    let mut c = config();
    c.preco_max = Some(50.into());
    let r = ranquear_multi(
        "ssd",
        &c,
        ResultadoMultiLoja {
            ofertas: vec![a],
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(r.excluidos["preco_final"], 1);
    let mut a = oferta(LojaId::Shopee, "2", "40");
    a.qtd_avaliacoes = None;
    a.vendas = Some(10000);
    c.preco_max = None;
    c.min_avaliacoes = 1;
    c.condicao = "new".into();
    let mut incompleta = a.clone();
    incompleta.condicao = "unknown".into();
    let ranqueamento = ranquear_multi(
        "ssd",
        &c,
        ResultadoMultiLoja {
            ofertas: vec![incompleta],
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(ranqueamento.produtos.len(), 1);
    assert_eq!(
        ranqueamento.produtos[0].principal.avisos,
        vec![
            "Condição não informada".to_owned(),
            "Avaliações não informadas (mínimo: 1)".to_owned()
        ]
    );

    let mut incompativel = a.clone();
    incompativel.condicao = "used".into();
    incompativel.qtd_avaliacoes = Some(100);
    assert!(
        ranquear_multi(
            "ssd",
            &c,
            ResultadoMultiLoja {
                ofertas: vec![incompativel],
                ..Default::default()
            }
        )
        .unwrap()
        .produtos
        .is_empty()
    );

    let mut poucas_avaliacoes = a.clone();
    poucas_avaliacoes.condicao = "new".into();
    poucas_avaliacoes.qtd_avaliacoes = Some(0);
    assert!(
        ranquear_multi(
            "ssd",
            &c,
            ResultadoMultiLoja {
                ofertas: vec![poucas_avaliacoes],
                ..Default::default()
            }
        )
        .unwrap()
        .produtos
        .is_empty()
    );
    c.min_avaliacoes = 0;
    c.termo = "ssd 1 tb".into();
    c.obrigatorio = vec!["kingston nv2".into()];
    assert_eq!(
        ranquear_multi(
            "ssd",
            &c,
            ResultadoMultiLoja {
                ofertas: vec![a],
                ..Default::default()
            }
        )
        .unwrap()
        .produtos
        .len(),
        1
    );
    let a = oferta(LojaId::Ml, "1", "12345678901234567890.12");
    let b = oferta(LojaId::Kabum, "2", "12345678901234567890.11");
    assert_eq!(
        ranking(vec![a, b]).produtos[0].principal.oferta.id,
        "kabum:2"
    );
}
#[test]
fn vendas_tem_teto_observado_sem_virar_contagem_de_reviews_e_bm25_corpus_unico() {
    let a = oferta(LojaId::Ml, "1", "400");
    let mut b = oferta(LojaId::Shopee, "2", "399");
    b.qtd_avaliacoes = None;
    b.vendas = Some(50000);
    let r = ranking(vec![a.clone(), b.clone()]);
    let o = r.produtos[0]
        .ofertas
        .iter()
        .find(|o| o.oferta.loja == LojaId::Shopee)
        .unwrap();
    assert!(o.confianca_por_vendas);
    assert_eq!(o.votos_confianca, 120);
    assert_eq!(o.oferta.qtd_avaliacoes, None);
    assert_eq!(
        o.relevancia,
        bm25(
            "ssd nvme",
            &b.titulo,
            &[a.titulo, b.titulo.clone()],
            1.2,
            0.75
        )
    );
    assert_eq!(ranking(vec![b]).produtos[0].principal.votos_confianca, 0);
}
#[test]
fn grupos_persistem_chave_e_historico_minimo_entre_lojas_por_dia() {
    let db = Database::open_in_memory().unwrap();
    let hoje = NaiveDate::from_ymd_opt(2026, 10, 12).unwrap();
    let a = oferta(LojaId::Ml, "1", "405");
    let mut b = oferta(LojaId::Pichau, "2", "399");
    for n in 0..30 {
        let dia = hoje.checked_sub_days(Days::new(n)).unwrap();
        db.salvar_oferta("ssd", &a, dia).unwrap();
        b.preco_final_brl = if n == 0 { 389.into() } else { 399.into() };
        db.salvar_oferta("ssd", &b, dia).unwrap();
    }
    let mut r = ranking(vec![a.clone(), b.clone()]);
    db.salvar_grupos(&mut r).unwrap();
    let chave = r.produtos[0].chave.clone();
    let h = db.historico_multi(&chave, true, hoje).unwrap();
    assert_eq!(h.pontos.len(), 30);
    assert_eq!(h.minima_30d, Some(389.into()));
    assert!(h.alerta_minima);
    assert_eq!(h.pontos.last().unwrap().loja, LojaId::Pichau);
    assert!(
        db.historico_multi(&a.id, false, hoje)
            .unwrap()
            .alerta_minima
    );
    assert!(db.registrar_alerta_multi(&chave, true, hoje).unwrap());
    assert!(!db.registrar_alerta_multi(&chave, true, hoje).unwrap());
    b.gtin = None;
    let mut r2 = ranking(vec![b]);
    db.salvar_grupos(&mut r2).unwrap();
    assert_eq!(r2.produtos[0].chave, chave);
    assert_eq!(
        db.historico_multi(&chave, true, hoje).unwrap().pontos.len(),
        30
    );
    let dir = tempfile::tempdir().unwrap();
    storage::salvar_ranking_multi(dir.path(), &r2).unwrap();
    let carregado = storage::carregar_ranking_multi(dir.path(), "ssd")
        .unwrap()
        .unwrap();
    assert_eq!(carregado.produtos[0].chave, r2.produtos[0].chave);
    assert_eq!(
        carregado.produtos[0].principal.oferta,
        r2.produtos[0].principal.oferta
    );
    assert!((carregado.produtos[0].score - r2.produtos[0].score).abs() < 1e-12);
    let csv = storage::exportar_multi(dir.path(), &r2).unwrap();
    assert!(
        std::fs::read_to_string(csv)
            .unwrap()
            .contains("preco_final_brl")
    );
}
#[test]
fn historico_exclui_futuro_e_dados_fora_30_dias_e_nao_alerta_cache_ou_amostra_curta() {
    let hoje = NaiveDate::from_ymd_opt(2026, 10, 12).unwrap();
    let p = |n: u64, preco: i32| storage::PontoMulti {
        data: hoje.checked_sub_days(Days::new(n)).unwrap().to_string(),
        produto_id: "ml:1".into(),
        loja: LojaId::Ml,
        preco_final_brl: preco.into(),
    };
    let mut pontos = (0..6).map(|n| p(n, 100)).collect::<Vec<_>>();
    pontos.push(p(31, 1));
    assert!(!storage::resumir_multi("x", pontos.clone(), hoje).alerta_minima);
    pontos.push(p(6, 110));
    let h = storage::resumir_multi("x", pontos.clone(), hoje);
    assert!(h.alerta_minima);
    assert_eq!(h.minima_30d, Some(100.into()));
    pontos.retain(|p| p.data != hoje.to_string());
    assert!(!storage::resumir_multi("x", pontos, hoje).alerta_minima);
}

#[test]
fn catalogo_corrigido_divide_grupo_sem_colidir_chave_ou_misturar_historicos() {
    let db = Database::open_in_memory().unwrap();
    let dia = NaiveDate::from_ymd_opt(2026, 10, 12).unwrap();
    let a = oferta(LojaId::Ml, "1", "405");
    let mut b = oferta(LojaId::Kabum, "2", "389");
    for o in [&a, &b] {
        db.salvar_oferta("ssd", o, dia).unwrap();
    }
    let mut antigo = ranking(vec![a.clone(), b.clone()]);
    db.salvar_grupos(&mut antigo).unwrap();
    assert_eq!(antigo.produtos.len(), 1);
    b.gtin = Some("1234567890123".into());
    db.salvar_oferta("ssd", &b, dia).unwrap();
    let mut atual = ranking(vec![a.clone(), b.clone()]);
    db.salvar_grupos(&mut atual).unwrap();
    assert_eq!(atual.produtos.len(), 2);
    assert_ne!(atual.produtos[0].chave, atual.produtos[1].chave);
    for g in &atual.produtos {
        let h = db.historico_multi(&g.chave, true, dia).unwrap();
        assert_eq!(h.pontos.len(), 1);
        assert_eq!(h.pontos[0].produto_id, g.principal.oferta.id);
    }
    let mut repetido = ranking(vec![a, b]);
    db.salvar_grupos(&mut repetido).unwrap();
    for g in &repetido.produtos {
        assert_eq!(
            g.chave,
            atual
                .produtos
                .iter()
                .find(|a| a.principal.oferta.id == g.principal.oferta.id)
                .unwrap()
                .chave
        );
    }
}
