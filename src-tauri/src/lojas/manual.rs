//! Entrada explícita da pessoa: fallback final sem controlar o navegador.
use super::{LojaId, OfertaBruta};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{path::Path, sync::Arc};

#[derive(Serialize, Deserialize)]
struct CapturaManual {
    termo: String,
    loja: LojaId,
    capturado_em: String,
    ofertas: Vec<OfertaBruta>,
}
pub fn salvar_captura(
    root: &Path,
    slug: &str,
    termo: &str,
    loja: LojaId,
    oferta: OfertaBruta,
) -> Result<(), String> {
    crate::storage::validar_slug(slug).map_err(|e| e.to_string())?;
    let dir = root.join("cache/manual").join(slug);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{loja}.json"));
    let anterior = std::fs::read(&path)
        .ok()
        .and_then(|v| serde_json::from_slice::<CapturaManual>(&v).ok())
        .filter(|c| c.termo == termo && c.loja == loja);
    let data_anterior = anterior.as_ref().map(|c| c.capturado_em.clone());
    let mut ofertas = anterior.map(|c| c.ofertas).unwrap_or_default();
    ofertas.retain(|o| o.id_externo != oferta.id_externo);
    // A data do lote é conservadora: acrescentar um item não atualiza os preços antigos.
    let capturado_em = if ofertas.is_empty() {
        None
    } else {
        data_anterior
    }
    .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
    ofertas.push(oferta);
    let captura = CapturaManual {
        termo: termo.into(),
        loja,
        capturado_em,
        ofertas,
    };
    let temp = path.with_extension("json.tmp");
    std::fs::write(
        &temp,
        serde_json::to_vec_pretty(&captura).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    std::fs::rename(temp, path).map_err(|e| e.to_string())?;
    Ok(())
}
pub fn fallback(root: &Path, slug: &str, termo: &str, loja: LojaId) -> Option<super::FonteDaLoja> {
    crate::storage::validar_slug(slug).ok()?;
    let raw = std::fs::read(
        root.join("cache/manual")
            .join(slug)
            .join(format!("{loja}.json")),
    )
    .ok()?;
    let captura: CapturaManual = serde_json::from_slice(&raw).ok()?;
    if captura.loja != loja
        || captura.termo != termo
        || chrono::DateTime::parse_from_rfc3339(&captura.capturado_em)
            .ok()?
            .timestamp()
            > chrono::Utc::now().timestamp()
    {
        return None;
    }
    let mut fonte = super::FonteDaLoja::nova("Manual", Arc::new(FonteManualLoja(captura.ofertas)));
    fonte.degradada = true;
    fonte.capturado_em = Some(captura.capturado_em);
    Some(fonte)
}
pub struct FonteSemJanela;
#[async_trait::async_trait]
impl super::FonteLoja for FonteSemJanela {
    async fn listar(&self, _: &str) -> Result<Vec<OfertaBruta>, String> {
        Err("A coleta assistida exige o app aberto.".into())
    }
}
struct FonteManualLoja(Vec<OfertaBruta>);
#[async_trait::async_trait]
impl super::FonteLoja for FonteManualLoja {
    async fn listar(&self, _: &str) -> Result<Vec<OfertaBruta>, String> {
        Ok(self.0.clone())
    }
}

#[derive(Deserialize)]
pub struct EntradaOfertaManual {
    pub url: String,
    pub titulo: String,
    pub preco: String,
    pub frete: Option<String>,
    pub condicao: String,
    pub vendedor: String,
    pub nota: Option<f64>,
    pub qtd_avaliacoes: Option<u32>,
    pub gtin: Option<String>,
    pub modelo: Option<String>,
    pub marca: Option<String>,
}
pub fn validar_manual(loja: LojaId, entrada: EntradaOfertaManual) -> Result<OfertaBruta, String> {
    let mut url = url::Url::parse(entrada.url.trim()).map_err(|_| "Link inválido.")?;
    let permitida = match loja {
        LojaId::Ml => url
            .host_str()
            .is_some_and(|h| h == "mercadolivre.com.br" || h.ends_with(".mercadolivre.com.br")),
        LojaId::Ebay => super::ebay::url_ebay(&url),
        LojaId::Amazon => super::amazon::url_amazon(&url),
        LojaId::Shopee => super::shopee::url_shopee(&url),
        LojaId::Aliexpress => super::aliexpress::url_aliexpress(&url),
        LojaId::Kabum => super::kabum::url_kabum(&url),
        LojaId::Pichau => super::pichau::url_pichau(&url),
    };
    if !permitida
        || url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("O link precisa ser HTTPS e pertencer à loja selecionada.".into());
    }
    if entrada.titulo.trim().is_empty() {
        return Err("Informe o título observado na página.".into());
    }
    let preco: Decimal = entrada
        .preco
        .parse()
        .map_err(|_| "Preço decimal inválido.")?;
    let frete = entrada
        .frete
        .map(|s| s.parse::<Decimal>())
        .transpose()
        .map_err(|_| "Frete decimal inválido.")?;
    if preco <= Decimal::ZERO || frete.is_some_and(|v| v < Decimal::ZERO) {
        return Err("Preço deve ser positivo e frete não negativo.".into());
    }
    if entrada
        .nota
        .is_some_and(|v| !v.is_finite() || !(0.0..=5.0).contains(&v))
    {
        return Err("Nota deve estar entre 0 e 5.".into());
    }
    if !["new", "used", "unknown"].contains(&entrada.condicao.as_str()) {
        return Err("Condição inválida.".into());
    }
    let gtin = entrada
        .gtin
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().to_owned());
    if gtin.as_ref().is_some_and(|s| {
        ![8, 12, 13, 14].contains(&s.len()) || !s.bytes().all(|c| c.is_ascii_digit())
    }) {
        return Err("GTIN/EAN precisa conter 8, 12, 13 ou 14 dígitos.".into());
    }
    url.set_fragment(None);
    let hash = Sha256::digest(url.as_str().as_bytes());
    Ok(OfertaBruta {
        id_externo: format!("manual:{hash:x}"),
        titulo: entrada.titulo.trim().into(),
        url: url.to_string(),
        gtin,
        modelo: entrada.modelo,
        marca: entrada.marca,
        preco,
        frete,
        condicao: entrada.condicao,
        vendedor: entrada.vendedor,
        nota: entrada.nota,
        qtd_avaliacoes: entrada.qtd_avaliacoes,
        vendas: None,
        reputacao: None,
        preco_variavel: false,
        preco_parcelado: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn entrada() -> EntradaOfertaManual {
        serde_json::from_str(r#"{"url":"https://www.amazon.com.br/dp/B0FIX00001","titulo":"SSD NV2 1TB","preco":"389.123456789","frete":null,"condicao":"new","vendedor":"Loja informada","nota":4.8,"qtd_avaliacoes":120,"gtin":"0740617329919","modelo":null,"marca":"Kingston"}"#).unwrap()
    }
    #[test]
    fn manual_preserva_decimal_origem_e_desconhecidos_sem_inventar_reputacao() {
        let p = validar_manual(LojaId::Amazon, entrada()).unwrap();
        assert_eq!(p.preco.to_string(), "389.123456789");
        assert_eq!(p.frete, None);
        assert_eq!(p.reputacao, None);
        let mut e = entrada();
        e.url = "https://www.ebay.com/itm/1".into();
        assert_eq!(
            validar_manual(LojaId::Ebay, e).unwrap().preco,
            "389.123456789".parse().unwrap()
        );
    }
    #[test]
    fn manual_rejeita_host_enganoso_senha_gtin_e_dinheiro_invalido() {
        for endereco in [
            "https://amazon.com.br.evil.invalid/dp/x",
            "https://usuario:senha@amazon.com.br/dp/x",
            "http://amazon.com.br/dp/x",
        ] {
            let mut e = entrada();
            e.url = endereco.into();
            assert!(validar_manual(LojaId::Amazon, e).is_err());
        }
        let mut e = entrada();
        e.gtin = Some("interno".into());
        assert!(validar_manual(LojaId::Amazon, e).is_err());
        let mut e = entrada();
        e.preco = "-1".into();
        assert!(validar_manual(LojaId::Amazon, e).is_err());
    }
    #[tokio::test]
    async fn fallback_manual_em_segundo_plano_preserva_data_e_nao_vira_snapshot_novo() {
        let dir = tempfile::tempdir().unwrap();
        let bruto = validar_manual(LojaId::Amazon, entrada()).unwrap();
        salvar_captura(dir.path(), "ssd", "ssd nvme", LojaId::Amazon, bruto).unwrap();
        let path = dir.path().join("cache/manual/ssd/amazon.json");
        let mut c: CapturaManual = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        c.capturado_em = "2026-09-30T12:00:00Z".into();
        std::fs::write(&path, serde_json::to_vec(&c).unwrap()).unwrap();
        let mut segunda = entrada();
        segunda.url = "https://www.amazon.com.br/dp/B0FIX00002".into();
        salvar_captura(
            dir.path(),
            "ssd",
            "ssd nvme",
            LojaId::Amazon,
            validar_manual(LojaId::Amazon, segunda).unwrap(),
        )
        .unwrap();
        let lote: CapturaManual = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(lote.capturado_em, "2026-09-30T12:00:00Z");
        assert!(fallback(dir.path(), "ssd", "outra busca", LojaId::Amazon).is_none());
        let mut registro = super::super::RegistroLojas::default();
        registro
            .registrar(Box::new(super::super::amazon::LojaAmazon::nova(vec![])))
            .unwrap();
        registro
            .acrescentar_fallback(
                LojaId::Amazon,
                fallback(dir.path(), "ssd", "ssd nvme", LojaId::Amazon).unwrap(),
            )
            .unwrap();
        let db = Arc::new(crate::storage::Database::open_in_memory().unwrap());
        let runner = super::super::OrquestradorMultiLoja::novo(
            registro,
            Arc::new(super::super::RecursosFontes::novos(
                db.clone(),
                dir.path().into(),
            )),
        );
        let config = crate::models::SearchConfig {
            termo: "ssd nvme".into(),
            lojas: vec![LojaId::Amazon],
            ..Default::default()
        };
        let r = runner
            .executar(
                &config,
                &[super::super::ConfigLoja::inicial(LojaId::Amazon)],
                super::super::ContextoNormalizacao,
                |_| {},
            )
            .await
            .unwrap();
        assert_eq!(r.ofertas.len(), 2);
        assert!(!r.status[&LojaId::Amazon].observacao_nova());
        assert!(
            matches!(&r.status[&LojaId::Amazon],super::super::StatusLoja::Degradado {capturado_em:Some(d),..} if d=="2026-09-30T12:00:00Z")
        );
        assert_eq!(db.contar_snapshots().unwrap(), 0);
    }
}
