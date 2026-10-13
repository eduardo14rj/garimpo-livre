use crate::{
    coleta::{ColetaErro, Fonte, parse_detalhe_html, parse_listagem_html},
    models::{AnuncioBruto, ProdutoBruto},
};
use async_trait::async_trait;

#[derive(Debug, Default, Clone, Copy)]
pub struct FonteFixture;

#[async_trait]
impl Fonte for FonteFixture {
    async fn listar(&self, _termo: &str) -> Result<Vec<AnuncioBruto>, ColetaErro> {
        parse_listagem_html(include_str!("../../tests/fixtures/listagem.html")).map_err(Into::into)
    }

    async fn detalhar(&self, url: &str) -> Result<ProdutoBruto, ColetaErro> {
        parse_detalhe_html(url, include_str!("../../tests/fixtures/produto.html"))
            .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn fonte_fixture_entrega_listagem_e_detalhe() {
        let source = FonteFixture;
        assert_eq!(source.listar("fone").await.unwrap().len(), 15);
        let detail = source
            .detalhar("https://produto.mercadolivre.com.br/MLB-9001")
            .await
            .unwrap();
        assert_eq!(detail.nota, Some(4.8));
        assert_eq!(detail.qtd_avaliacoes, 1250);
    }
}
