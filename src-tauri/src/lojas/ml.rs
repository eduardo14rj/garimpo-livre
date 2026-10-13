use super::{FonteDaLoja, FonteLoja, Loja, LojaId, OfertaBruta};
use crate::{coleta::Fonte, models::AnuncioBruto};
use async_trait::async_trait;
use std::sync::Arc;

/// A fronteira mantém as fontes v1/v2 intactas; toda aritmética v3 usa Decimal.
pub struct LojaMl {
    fontes: Vec<FonteDaLoja>,
}
impl LojaMl {
    pub fn nova(fontes: Vec<FonteDaLoja>) -> Self {
        Self { fontes }
    }
    pub fn adaptar_fonte(nome: impl Into<String>, fonte: Arc<dyn Fonte>) -> FonteDaLoja {
        FonteDaLoja::nova(nome, Arc::new(FonteMlLegada { fonte }))
    }
    pub fn adaptar_anuncio(anuncio: AnuncioBruto) -> Result<OfertaBruta, String> {
        if !anuncio.preco.is_finite() {
            return Err("Preço inválido no anúncio legado do Mercado Livre.".into());
        }
        let preco = anuncio
            .preco
            .to_string()
            .parse()
            .map_err(|_| "Preço fora da precisão decimal.".to_owned())?;
        Ok(OfertaBruta {
            id_externo: anuncio.id,
            titulo: anuncio.titulo,
            url: anuncio.url,
            gtin: anuncio.gtin,
            modelo: anuncio.modelo,
            marca: anuncio.marca,
            preco,
            frete: anuncio.frete,
            condicao: anuncio.condicao,
            vendedor: anuncio.vendedor,
            nota: anuncio.nota,
            qtd_avaliacoes: Some(anuncio.qtd_avaliacoes),
            vendas: None,
            reputacao: reputacao_nivel(anuncio.vendedor_nivel.as_deref())
                .or(Some(anuncio.vendedor_score)),
            preco_variavel: false,
            preco_parcelado: None,
        })
    }
}
/// Os cinco níveis oficiais são convertidos para a escala especificada no ranking v3.
pub fn reputacao_nivel(nivel: Option<&str>) -> Option<f64> {
    match nivel? {
        "1_red" => Some(0.0),
        "2_orange" => Some(0.25),
        "3_yellow" => Some(0.5),
        "4_light_green" => Some(0.75),
        "5_green" => Some(1.0),
        _ => None,
    }
}
impl Loja for LojaMl {
    fn id(&self) -> LojaId {
        LojaId::Ml
    }
    fn fontes(&self) -> Vec<FonteDaLoja> {
        self.fontes.clone()
    }
}
struct FonteMlLegada {
    fonte: Arc<dyn Fonte>,
}
#[async_trait]
impl FonteLoja for FonteMlLegada {
    async fn listar(&self, termo: &str) -> Result<Vec<OfertaBruta>, String> {
        self.fonte
            .listar(termo)
            .await
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(LojaMl::adaptar_anuncio)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lojas::{ConfigLoja, ContextoNormalizacao, RegistroLojas};
    #[test]
    fn cinco_niveis_oficiais_e_fixture_antiga_preservada() {
        for (id, esperado) in [
            ("1_red", 0.0),
            ("2_orange", 0.25),
            ("3_yellow", 0.5),
            ("4_light_green", 0.75),
            ("5_green", 1.0),
        ] {
            assert_eq!(reputacao_nivel(Some(id)), Some(esperado));
        }
        assert_eq!(reputacao_nivel(None), None);
        assert_eq!(reputacao_nivel(Some("desconhecido")), None);
        let mut a: AnuncioBruto=serde_json::from_str(r#"{"id":"MLB1","titulo":"SSD","preco":1,"url":"https://www.mercadolivre.com.br/MLB1","condicao":"new","vendedor":"loja","vendedor_id":null,"vendedor_score":0.91,"nota":null,"qtd_avaliacoes":0,"modelo":null}"#).unwrap();
        assert_eq!(
            LojaMl::adaptar_anuncio(a.clone()).unwrap().reputacao,
            Some(0.91)
        );
        a.vendedor_nivel = Some("4_light_green".into());
        assert_eq!(LojaMl::adaptar_anuncio(a).unwrap().reputacao, Some(0.75));
    }
    #[tokio::test]
    async fn adapta_fixture_sem_mudar_precos_e_registro_resolve_padrao() {
        let fonte = LojaMl::adaptar_fonte("Fixture", Arc::new(crate::coleta::FonteFixture));
        let anuncios = fonte.fonte.listar("fone").await.unwrap();
        assert_eq!(anuncios.len(), 15);
        let mut registro = RegistroLojas::default();
        registro
            .registrar(Box::new(LojaMl::nova(vec![fonte])))
            .unwrap();
        let ids = registro
            .selecionar(&[], &[ConfigLoja::inicial(LojaId::Ml)])
            .unwrap();
        assert_eq!(ids, vec![LojaId::Ml]);
        assert!(registro.obter(LojaId::Ebay).is_err());
        let oferta = registro
            .obter(LojaId::Ml)
            .unwrap()
            .normalizar(anuncios[0].clone(), &ContextoNormalizacao)
            .unwrap();
        assert_eq!(oferta.id, "ml:MLB1001");
        assert_eq!(oferta.preco_final_brl, "249".parse().unwrap());
        assert_eq!(oferta.frete_brl, None);
    }
}
