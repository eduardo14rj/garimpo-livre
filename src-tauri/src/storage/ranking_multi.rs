use super::{Database, StorageErro};
use crate::{core::multiloja::RankingMultiLoja, lojas::LojaId};
use chrono::{Days, NaiveDate};
use rusqlite::{OptionalExtension, params};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::Path};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PontoMulti {
    pub data: String,
    pub produto_id: String,
    pub loja: LojaId,
    pub preco_final_brl: Decimal,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HistoricoMulti {
    pub entidade: String,
    pub pontos: Vec<PontoMulti>,
    pub minima_30d: Option<Decimal>,
    pub media_movel_7d: Option<Decimal>,
    pub dias_observados_30d: usize,
    pub alerta_minima: bool,
}

pub fn resumir_multi(entidade: &str, pontos: Vec<PontoMulti>, hoje: NaiveDate) -> HistoricoMulti {
    let inicio = hoje
        .checked_sub_days(Days::new(29))
        .unwrap_or(NaiveDate::MIN);
    let inicio_media = hoje
        .checked_sub_days(Days::new(6))
        .unwrap_or(NaiveDate::MIN);
    let mut dias: BTreeMap<NaiveDate, PontoMulti> = BTreeMap::new();
    for p in pontos {
        if let Ok(dia) = NaiveDate::parse_from_str(&p.data, "%Y-%m-%d")
            && dia <= hoje
            && p.preco_final_brl > Decimal::ZERO
            && dias
                .get(&dia)
                .is_none_or(|antiga| p.preco_final_brl < antiga.preco_final_brl)
        {
            dias.insert(dia, p);
        }
    }
    let janela = dias
        .range(inicio..=hoje)
        .map(|(_, p)| p)
        .collect::<Vec<_>>();
    let minima_30d = janela.iter().map(|p| p.preco_final_brl).min();
    let recentes = dias
        .range(inicio_media..=hoje)
        .map(|(_, p)| p.preco_final_brl)
        .collect::<Vec<_>>();
    // Média incremental evita overflow na soma de valores monetários altos.
    let media_movel_7d = recentes
        .iter()
        .enumerate()
        .try_fold(None, |media, (i, &v)| {
            let anterior = media.unwrap_or(v);
            anterior
                .checked_add(v.checked_sub(anterior)?.checked_div(Decimal::from(i + 1))?)
                .map(Some)
        })
        .flatten()
        .map(|v| v.round_dp(2));
    let dias_observados_30d = janela.len();
    let alerta_minima = dias_observados_30d >= 7
        && dias
            .get(&hoje)
            .is_some_and(|p| Some(p.preco_final_brl) == minima_30d);
    HistoricoMulti {
        entidade: entidade.into(),
        pontos: dias.into_values().collect(),
        minima_30d,
        media_movel_7d,
        dias_observados_30d,
        alerta_minima,
    }
}

pub fn salvar_ranking_multi(root: &Path, ranking: &RankingMultiLoja) -> Result<(), StorageErro> {
    super::validar_slug(&ranking.busca)?;
    let dir = root.join("data/resultados-multi");
    fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}.json", ranking.busca));
    let temp = path.with_extension("json.tmp");
    fs::write(&temp, serde_json::to_vec_pretty(ranking)?)?;
    fs::rename(temp, path)?;
    Ok(())
}
pub fn excluir_ranking_multi(root: &Path, slug: &str) -> Result<(), StorageErro> {
    super::validar_slug(slug)?;
    let path = root
        .join("data/resultados-multi")
        .join(format!("{slug}.json"));
    super::remover_arquivo_se_existir(path)
}
pub fn carregar_ranking_multi(
    root: &Path,
    slug: &str,
) -> Result<Option<RankingMultiLoja>, StorageErro> {
    super::validar_slug(slug)?;
    let path = root
        .join("data/resultados-multi")
        .join(format!("{slug}.json"));
    if !path.exists() {
        return Ok(None);
    }
    Ok(Some(serde_json::from_slice(&fs::read(path)?)?))
}
pub fn exportar_multi(
    root: &Path,
    ranking: &RankingMultiLoja,
) -> Result<std::path::PathBuf, StorageErro> {
    super::validar_slug(&ranking.busca)?;
    let path = root
        .join("exports")
        .join(format!("{}-multi.csv", ranking.busca));
    fs::create_dir_all(root.join("exports"))?;
    let mut csv = csv::Writer::from_path(&path)?;
    csv.write_record([
        "grupo",
        "principal",
        "loja",
        "id",
        "titulo",
        "preco_brl",
        "frete_brl",
        "preco_final_brl",
        "score",
        "url",
    ])?;
    for g in &ranking.produtos {
        for o in &g.ofertas {
            let p = &o.oferta;
            csv.write_record([
                g.chave.clone(),
                (p.id == g.principal.oferta.id).to_string(),
                p.loja.to_string(),
                p.id.clone(),
                p.titulo.clone(),
                p.preco_brl.to_string(),
                p.frete_brl.map(|v| v.to_string()).unwrap_or_default(),
                p.preco_final_brl.to_string(),
                o.score.to_string(),
                p.url.clone(),
            ])?;
        }
    }
    csv.flush()?;
    Ok(path)
}

impl Database {
    pub fn salvar_grupos(&self, ranking: &mut RankingMultiLoja) -> Result<(), StorageErro> {
        let mut connection = self.connection.lock().expect("lock SQLite");
        let tx = connection.transaction()?;
        let mut usados = std::collections::BTreeSet::new();
        for grupo in &mut ranking.produtos {
            let mut antigos = std::collections::BTreeSet::new();
            for o in &grupo.ofertas {
                let mut q = tx.prepare("SELECT grupo_id FROM grupo_ofertas WHERE produto_id=?1")?;
                for id in q.query_map([&o.oferta.id], |r| r.get::<_, i64>(0))? {
                    antigos.insert(id?);
                }
            }
            if let Some(id) = tx
                .query_row(
                    "SELECT id FROM grupos WHERE chave=?1",
                    [&grupo.chave],
                    |r| r.get::<_, i64>(0),
                )
                .optional()?
            {
                antigos.insert(id);
            }
            antigos.retain(|id| !usados.contains(id));
            let mut compativeis = std::collections::BTreeSet::new();
            for antigo in antigos {
                let mut q=tx.prepare("SELECT p.oferta_json FROM grupo_ofertas go JOIN produtos p ON p.id=go.produto_id WHERE go.grupo_id=?1 AND p.oferta_json IS NOT NULL")?;
                let jsons = q
                    .query_map([antigo], |r| r.get::<_, String>(0))?
                    .collect::<Result<Vec<_>, _>>()?;
                let ofertas = jsons
                    .iter()
                    .map(|s| serde_json::from_str::<crate::lojas::OfertaNormalizada>(s))
                    .collect::<Result<Vec<_>, _>>()?;
                if ofertas.iter().all(|a| {
                    grupo
                        .ofertas
                        .iter()
                        .all(|b| crate::core::multiloja::sem_conflito(a, &b.oferta))
                }) {
                    compativeis.insert(antigo);
                }
            }
            let antigos = compativeis;
            let id = if let Some(&id) = antigos.first() {
                id
            } else {
                // Catálogo corrigido pode dividir um grupo antigo cuja chave já existe.
                // Preserva a chave histórica e cria outra para o novo conjunto incompatível.
                let base = grupo.chave.clone();
                let mut tentativa = 0;
                while tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM grupos WHERE chave=?1)",
                    [&grupo.chave],
                    |r| r.get::<_, bool>(0),
                )? {
                    tentativa += 1;
                    grupo.chave = format!("{base}:revisao:{tentativa}");
                }
                tx.execute(
                    "INSERT INTO grupos(chave,titulo) VALUES (?1,?2)",
                    params![grupo.chave, grupo.principal.oferta.titulo],
                )?;
                tx.last_insert_rowid()
            };
            usados.insert(id);
            // Quando surge GTIN, grupos provisórios podem convergir. Mantém todos os vínculos/históricos.
            for antigo in antigos.into_iter().filter(|&a| a != id) {
                tx.execute("INSERT OR IGNORE INTO grupo_ofertas SELECT ?1,produto_id FROM grupo_ofertas WHERE grupo_id=?2",params![id,antigo])?;
                tx.execute("DELETE FROM grupo_ofertas WHERE grupo_id=?1", [antigo])?;
                tx.execute("DELETE FROM grupos WHERE id=?1", [antigo])?;
            }
            for o in &grupo.ofertas {
                let p = &o.oferta;
                // Cache antigo pode ainda não ter metadados no SQLite. Não cria snapshot de hoje.
                tx.execute("INSERT OR IGNORE INTO produtos(id,titulo,titulo_norm,modelo,vendedor,reputacao,url,busca,visto_em,loja,gtin,oferta_json) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",params![p.id,p.titulo,crate::core::normalizar(&p.titulo),p.modelo,p.vendedor,p.reputacao.unwrap_or(0.5),p.url,ranking.busca,ranking.executada_em,p.loja.as_str(),p.gtin,serde_json::to_string(p)?])?;
                tx.execute(
                    "DELETE FROM grupo_ofertas WHERE produto_id=?1 AND grupo_id<>?2",
                    params![p.id, id],
                )?;
                tx.execute(
                    "INSERT OR IGNORE INTO grupo_ofertas VALUES (?1,?2)",
                    params![id, p.id],
                )?;
            }
            // Chave existente preserva continuidade entre título, modelo e GTIN descoberto depois.
            grupo.chave =
                tx.query_row("SELECT chave FROM grupos WHERE id=?1", [id], |r| r.get(0))?;
            tx.execute(
                "UPDATE grupos SET titulo=?2 WHERE id=?1",
                params![id, grupo.principal.oferta.titulo],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn historico_multi(
        &self,
        entidade: &str,
        grupo: bool,
        hoje: NaiveDate,
    ) -> Result<HistoricoMulti, StorageErro> {
        let connection = self.connection.lock().expect("lock SQLite");
        let sql = if grupo {
            "SELECT s.data,s.produto_id,p.loja,s.preco_final_brl FROM snapshots s JOIN produtos p ON p.id=s.produto_id JOIN grupo_ofertas go ON go.produto_id=p.id JOIN grupos g ON g.id=go.grupo_id WHERE g.chave=?1 AND s.preco_final_brl IS NOT NULL ORDER BY s.data,s.produto_id"
        } else {
            "SELECT s.data,s.produto_id,p.loja,s.preco_final_brl FROM snapshots s JOIN produtos p ON p.id=s.produto_id WHERE p.id=?1 AND s.preco_final_brl IS NOT NULL ORDER BY s.data"
        };
        let mut q = connection.prepare(sql)?;
        let dados = q.query_map([entidade], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?;
        let pontos = dados
            .map(|r| {
                let (data, produto_id, loja, preco) = r?;
                Ok(PontoMulti {
                    data,
                    produto_id,
                    loja: loja.parse().map_err(StorageErro::Config)?,
                    preco_final_brl: preco.parse().map_err(|e| {
                        StorageErro::Config(format!("preço histórico inválido: {e}"))
                    })?,
                })
            })
            .collect::<Result<Vec<_>, StorageErro>>()?;
        Ok(resumir_multi(entidade, pontos, hoje))
    }
    pub fn registrar_alerta_multi(
        &self,
        entidade: &str,
        grupo: bool,
        dia: NaiveDate,
    ) -> Result<bool, StorageErro> {
        let connection = self.connection.lock().expect("lock SQLite");
        Ok(connection.execute(
            "INSERT OR IGNORE INTO alertas_multi(entidade,grupo,data) VALUES (?1,?2,?3)",
            params![entidade, grupo, dia.to_string()],
        )? == 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn carrega_ranking_legado_com_status_sem_credencial() {
        let dir = tempfile::tempdir().unwrap();
        let resultados = dir.path().join("data/resultados-multi");
        fs::create_dir_all(&resultados).unwrap();
        fs::write(
            resultados.join("fone.json"),
            r#"{
                "busca": "fone",
                "executada_em": "2026-10-12T12:00:00Z",
                "demonstracao": false,
                "anuncios_coletados": 0,
                "anuncios_filtrados": 0,
                "excluidos": {},
                "produtos": [],
                "status": { "ml": { "estado": "sem_credencial" } }
            }"#,
        )
        .unwrap();

        let ranking = carregar_ranking_multi(dir.path(), "fone").unwrap().unwrap();

        assert_eq!(
            ranking.status[&LojaId::Ml],
            crate::lojas::StatusLoja::Degradado {
                fonte: "Resultado antigo; execute nova coleta".into(),
                capturado_em: None,
            }
        );
    }
}
