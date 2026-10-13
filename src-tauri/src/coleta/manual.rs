use url::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntradaManual {
    Busca { termo: String },
    Produto { id: String, url: String },
}

pub struct FonteManual;

impl FonteManual {
    pub fn analisar(link: &str) -> Result<EntradaManual, String> {
        let url = Url::parse(link.trim())
            .map_err(|_| "Cole um link válido do Mercado Livre.".to_owned())?;
        let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
        if url.scheme() != "https" || !mercado_livre_host(&host) {
            return Err("O link precisa ser HTTPS e pertencer ao Mercado Livre.".into());
        }
        if host == "lista.mercadolivre.com.br" {
            let query_term = url
                .query_pairs()
                .find(|(key, _)| key == "q")
                .map(|(_, value)| value.into_owned())
                .filter(|value| !value.trim().is_empty());
            let path = url.path().trim_matches('/');
            let decoded_path = url::form_urlencoded::parse(format!("q={path}").as_bytes())
                .next()
                .map(|(_, value)| value.into_owned())
                .unwrap_or_default();
            let term = query_term.unwrap_or(decoded_path).replace(['-', '+'], " ");
            let term = term.split_whitespace().collect::<Vec<_>>().join(" ");
            if term.is_empty() {
                return Err("O link de busca não contém um termo.".into());
            }
            return Ok(EntradaManual::Busca { termo: term });
        }
        if let Some(id) = extract_item_id(link) {
            return Ok(EntradaManual::Produto {
                id,
                url: link.trim().to_owned(),
            });
        }
        Err("O link não parece ser uma busca nem um anúncio do Mercado Livre.".into())
    }
}

fn mercado_livre_host(host: &str) -> bool {
    ["mercadolivre.com.br", "mercadolibre.com"]
        .iter()
        .any(|domain| host == *domain || host.ends_with(&format!(".{domain}")))
}

fn extract_item_id(value: &str) -> Option<String> {
    let url = Url::parse(value).ok()?;
    if !mercado_livre_host(&url.host_str()?.to_ascii_lowercase()) {
        return None;
    }
    let text = format!(
        "{}?{}#{}",
        url.path(),
        url.query().unwrap_or_default(),
        url.fragment().unwrap_or_default()
    );
    regex::Regex::new(r"(?i)MLB-?(\d+)")
        .ok()?
        .captures(&text)
        .and_then(|capture| capture.get(0))
        .map(|matched| matched.as_str().replace('-', "").to_uppercase())
}

#[cfg(test)]
mod tests {
    use super::{EntradaManual, FonteManual};

    #[test]
    fn identifica_link_de_busca_e_decodifica_o_termo() {
        assert_eq!(
            FonteManual::analisar("https://lista.mercadolivre.com.br/fone-bluetooth-anc").unwrap(),
            EntradaManual::Busca {
                termo: "fone bluetooth anc".into()
            }
        );
    }

    #[test]
    fn identifica_link_de_produto_e_extrai_o_id() {
        assert_eq!(
            FonteManual::analisar(
                "https://www.mercadolivre.com.br/fone/p/MLB-12345678?wid=MLB99887766"
            )
            .unwrap(),
            EntradaManual::Produto {
                id: "MLB12345678".into(),
                url: "https://www.mercadolivre.com.br/fone/p/MLB-12345678?wid=MLB99887766".into()
            }
        );
    }

    #[test]
    fn rejeita_links_de_outros_sites() {
        assert!(FonteManual::analisar("https://example.com/fone").is_err());
    }
}
