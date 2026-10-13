use super::{Database, StorageErro};
use crate::lojas::{ConfigLoja, LojaId, OfertaNormalizada};
use chrono::NaiveDate;
use rusqlite::{Connection, params};
use rust_decimal::Decimal;
use std::str::FromStr;

pub(super) fn id_produto_ml(id: &str) -> String {
    if id
        .split_once(':')
        .is_some_and(|(loja, _)| loja.parse::<LojaId>().is_ok())
    {
        id.into()
    } else {
        format!("ml:{id}")
    }
}
pub(super) fn chave_fonte(fonte: &str) -> String {
    id_produto_ml(fonte)
}

pub(super) fn migrate_v3(connection: &Connection) -> Result<(), StorageErro> {
    let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 3 {
        return Ok(());
    }
    let transaction = connection.unchecked_transaction()?;
    transaction.execute_batch(
        "PRAGMA defer_foreign_keys = ON;
         CREATE TABLE lojas (id TEXT PRIMARY KEY, nome TEXT NOT NULL, ativa INTEGER NOT NULL, padrao INTEGER NOT NULL, moeda TEXT NOT NULL, requer_credencial INTEGER NOT NULL);
         ALTER TABLE produtos ADD COLUMN loja TEXT NOT NULL DEFAULT 'ml';
         ALTER TABLE produtos ADD COLUMN gtin TEXT;
         ALTER TABLE produtos ADD COLUMN importado INTEGER NOT NULL DEFAULT 0;
         ALTER TABLE produtos ADD COLUMN oferta_json TEXT;
         ALTER TABLE snapshots ADD COLUMN moeda_origem TEXT DEFAULT 'BRL';
         ALTER TABLE snapshots ADD COLUMN preco_origem TEXT;
         ALTER TABLE snapshots ADD COLUMN frete_brl TEXT;
         ALTER TABLE snapshots ADD COLUMN imposto_brl TEXT;
         ALTER TABLE snapshots ADD COLUMN preco_final_brl TEXT;
         ALTER TABLE snapshots ADD COLUMN cotacao TEXT;
         ALTER TABLE snapshots ADD COLUMN oferta_json TEXT;
         CREATE TABLE cotacoes (data TEXT NOT NULL, moeda TEXT NOT NULL, valor TEXT NOT NULL, PRIMARY KEY (data,moeda));
         CREATE TABLE grupos (id INTEGER PRIMARY KEY, chave TEXT NOT NULL UNIQUE, titulo TEXT NOT NULL);
         CREATE TABLE grupo_ofertas (grupo_id INTEGER NOT NULL REFERENCES grupos(id), produto_id TEXT NOT NULL REFERENCES produtos(id), PRIMARY KEY (grupo_id,produto_id));
         UPDATE produtos SET id = 'ml:' || id WHERE substr(id,1,3) <> 'ml:';
         UPDATE snapshots SET produto_id = 'ml:' || produto_id WHERE substr(produto_id,1,3) <> 'ml:';
         UPDATE snapshots SET preco_origem = CAST(preco AS TEXT), preco_final_brl = CAST(preco AS TEXT);
         UPDATE saude_fontes SET fonte = 'ml:' || fonte;
         UPDATE circuitos_fontes SET fonte = 'ml:' || fonte;"
    )?;
    for id in LojaId::TODAS {
        let loja = ConfigLoja::inicial(id);
        transaction.execute(
            "INSERT INTO lojas VALUES (?1,?2,?3,?4,?5,?6)",
            params![
                id.as_str(),
                loja.nome,
                loja.ativa,
                loja.padrao,
                "BRL",
                false
            ],
        )?;
    }
    transaction.pragma_update(None, "user_version", 3)?;
    transaction.commit()?;
    Ok(())
}

pub(super) fn migrate_v4(connection: &Connection) -> Result<(), StorageErro> {
    let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 4 {
        return Ok(());
    }
    if version != 3 {
        return Err(StorageErro::Config(format!(
            "migração v4 esperava banco v3, encontrou v{version}"
        )));
    }

    let transaction = connection.unchecked_transaction()?;
    transaction.execute_batch(
        "PRAGMA defer_foreign_keys = ON;
         ALTER TABLE produtos DROP COLUMN importado;
         ALTER TABLE lojas RENAME TO lojas_v3;
         CREATE TABLE lojas (
           id TEXT PRIMARY KEY, nome TEXT NOT NULL, ativa INTEGER NOT NULL, padrao INTEGER NOT NULL
         );
         INSERT INTO lojas (id,nome,ativa,padrao)
           SELECT id,nome,ativa,padrao FROM lojas_v3;
         DROP TABLE lojas_v3;

         CREATE TABLE snapshots_v4 (
           produto_id TEXT NOT NULL REFERENCES produtos(id) ON DELETE CASCADE,
           data TEXT NOT NULL, preco REAL NOT NULL, preco_original REAL, nota REAL,
           qtd_avaliacoes INTEGER NOT NULL DEFAULT 0, condicao TEXT NOT NULL DEFAULT 'new',
           preco_brl TEXT NOT NULL, frete_brl TEXT, preco_final_brl TEXT NOT NULL,
           oferta_json TEXT, PRIMARY KEY (produto_id,data)
         );",
    )?;

    let historico = {
        let mut query = transaction.prepare(
            "SELECT produto_id,data,preco,preco_original,nota,qtd_avaliacoes,
                    COALESCE(condicao,'new'),
                    moeda_origem,preco_origem,frete_brl,oferta_json FROM snapshots",
        )?;
        query
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, f64>(2)?,
                    row.get::<_, Option<f64>>(3)?,
                    row.get::<_, Option<f64>>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, Option<String>>(8)?,
                    row.get::<_, Option<String>>(9)?,
                    row.get::<_, Option<String>>(10)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?
    };
    for (
        produto_id,
        data,
        preco_legado,
        preco_original,
        nota,
        qtd_avaliacoes,
        condicao,
        moeda,
        preco_origem,
        frete_origem,
        oferta_json,
    ) in historico
    {
        let preco = if moeda.as_deref() == Some("BRL") {
            preco_origem
                .as_deref()
                .map(Decimal::from_str)
                .transpose()
                .map_err(|error| StorageErro::Config(format!("preço histórico inválido: {error}")))?
                .unwrap_or(
                    Decimal::from_str(&preco_legado.to_string()).map_err(|error| {
                        StorageErro::Config(format!("preço histórico inválido: {error}"))
                    })?,
                )
        } else {
            // Valores antigos fora de BRL já foram convertidos no total do ranking.
            Decimal::from_str(&preco_legado.to_string()).map_err(|error| {
                StorageErro::Config(format!("preço histórico inválido: {error}"))
            })?
        };
        let frete = if moeda.as_deref() == Some("BRL") {
            frete_origem
                .as_deref()
                .map(Decimal::from_str)
                .transpose()
                .map_err(|error| {
                    StorageErro::Config(format!("frete histórico inválido: {error}"))
                })?
        } else {
            None
        };
        let total = preco
            .checked_add(frete.unwrap_or_default())
            .ok_or_else(|| {
                StorageErro::Config("preço histórico fora da precisão decimal".into())
            })?;
        transaction.execute(
            "INSERT INTO snapshots_v4 (
               produto_id,data,preco,preco_original,nota,qtd_avaliacoes,condicao,
               preco_brl,frete_brl,preco_final_brl,oferta_json
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            params![
                produto_id,
                data,
                preco_legado,
                preco_original,
                nota,
                qtd_avaliacoes,
                condicao,
                preco.to_string(),
                frete.map(|valor| valor.to_string()),
                total.to_string(),
                oferta_json,
            ],
        )?;
    }
    transaction.execute_batch(
        "DROP TABLE snapshots;
         ALTER TABLE snapshots_v4 RENAME TO snapshots;
         CREATE INDEX idx_snapshots_data ON snapshots(data);
         DROP TABLE cotacoes;
         PRAGMA user_version = 4;",
    )?;
    transaction.commit()?;
    Ok(())
}

impl Database {
    pub fn listar_lojas(&self) -> Result<Vec<ConfigLoja>, StorageErro> {
        let connection = self.connection.lock().expect("lock SQLite");
        let mut query = connection.prepare("SELECT id,nome,ativa,padrao FROM lojas ORDER BY id")?;
        let rows = query.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, bool>(2)?,
                row.get::<_, bool>(3)?,
            ))
        })?;
        rows.map(|row| {
            let (id, nome, ativa, padrao) = row?;
            let id: LojaId = id.parse().map_err(StorageErro::Config)?;
            Ok(ConfigLoja {
                id,
                nome,
                ativa,
                padrao,
            })
        })
        .collect()
    }
    pub fn configurar_loja(
        &self,
        id: LojaId,
        ativa: bool,
        padrao: bool,
    ) -> Result<(), StorageErro> {
        let connection = self.connection.lock().expect("lock SQLite");
        let count = connection.execute(
            "UPDATE lojas SET ativa=?2,padrao=?3 WHERE id=?1",
            params![id.as_str(), ativa, padrao],
        )?;
        if count != 1 {
            return Err(StorageErro::Config(format!("loja {id} não cadastrada")));
        }
        Ok(())
    }
    pub fn salvar_oferta(
        &self,
        busca: &str,
        oferta: &OfertaNormalizada,
        data: NaiveDate,
    ) -> Result<(), StorageErro> {
        if !oferta.id.starts_with(&format!("{}:", oferta.loja)) {
            return Err(StorageErro::Config(
                "ID da oferta não corresponde à loja".into(),
            ));
        }
        let mut connection = self.connection.lock().expect("lock SQLite");
        let transaction = connection.transaction()?;
        let json = serde_json::to_string(oferta)?;
        transaction.execute(
            "INSERT INTO produtos (id,titulo,titulo_norm,modelo,vendedor,reputacao,url,busca,visto_em,loja,gtin,oferta_json) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12) ON CONFLICT(id) DO UPDATE SET titulo=excluded.titulo,titulo_norm=excluded.titulo_norm,modelo=excluded.modelo,vendedor=excluded.vendedor,reputacao=excluded.reputacao,url=excluded.url,busca=excluded.busca,visto_em=excluded.visto_em,gtin=excluded.gtin,oferta_json=excluded.oferta_json",
            params![oferta.id,oferta.titulo,crate::core::normalizar(&oferta.titulo),oferta.modelo,oferta.vendedor,oferta.reputacao.unwrap_or(0.5),oferta.url,busca,chrono::Utc::now().to_rfc3339(),oferta.loja.as_str(),oferta.gtin,json]
        )?;
        let preco_legado = oferta
            .preco_final_brl
            .to_string()
            .parse::<f64>()
            .map_err(|error| {
                StorageErro::Config(format!("preço excede o histórico legado: {error}"))
            })?;
        transaction.execute(
            "INSERT INTO snapshots (produto_id,data,preco,nota,qtd_avaliacoes,condicao,preco_brl,frete_brl,preco_final_brl,oferta_json) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10) ON CONFLICT(produto_id,data) DO UPDATE SET preco=excluded.preco,nota=excluded.nota,qtd_avaliacoes=excluded.qtd_avaliacoes,condicao=excluded.condicao,preco_brl=excluded.preco_brl,frete_brl=excluded.frete_brl,preco_final_brl=excluded.preco_final_brl,oferta_json=excluded.oferta_json",
            params![oferta.id,data.to_string(),preco_legado,oferta.nota,oferta.qtd_avaliacoes.unwrap_or(0),oferta.condicao,oferta.preco_brl.to_string(),oferta.frete_brl.map(|value|value.to_string()),oferta.preco_final_brl.to_string(),json]
        )?;
        transaction.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    #[test]
    fn migration_v2_preserva_ids_vinculos_contagens() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("v2.db");
        let connection = Connection::open(&path).unwrap();
        connection.execute_batch("CREATE TABLE produtos (id TEXT PRIMARY KEY,titulo TEXT,titulo_norm TEXT,modelo TEXT,vendedor_id TEXT,vendedor TEXT,reputacao REAL,url TEXT,busca TEXT,visto_em TEXT); CREATE TABLE snapshots (produto_id TEXT REFERENCES produtos(id),data TEXT,preco REAL,preco_original REAL,nota REAL,qtd_avaliacoes INTEGER NOT NULL DEFAULT 0,condicao TEXT,PRIMARY KEY(produto_id,data)); CREATE TABLE saude_fontes (fonte TEXT); CREATE TABLE circuitos_fontes (fonte TEXT PRIMARY KEY); INSERT INTO produtos (id) VALUES ('MLB1'),('MLB2'); INSERT INTO snapshots (produto_id,data,preco) VALUES ('MLB1','2026-10-01',10.1),('MLB1','2026-10-02',11),('MLB2','2026-10-01',20); PRAGMA user_version=2;").unwrap();
        drop(connection);
        let db = Database::open(path).unwrap();
        assert_eq!(db.user_version().unwrap(), 4);
        assert_eq!(db.contar_snapshots().unwrap(), 3);
        let connection = db.connection.lock().unwrap();
        let count:i64=connection.query_row("SELECT COUNT(*) FROM snapshots s JOIN produtos p ON p.id=s.produto_id WHERE p.id LIKE 'ml:%'",[],|row|row.get(0)).unwrap();
        assert_eq!(count, 3);
        assert_eq!(connection.query_row("SELECT preco_final_brl FROM snapshots WHERE produto_id='ml:MLB1' AND data='2026-10-01'",[],|row|row.get::<_,String>(0)).unwrap(),"10.1");
        let violations: i64 = connection
            .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(violations, 0);
        drop(connection);
        assert_eq!(db.listar_lojas().unwrap().len(), 7);
        assert_eq!(db.historico("MLB1").unwrap().len(), 2);
        assert_eq!(db.historico("ml:MLB1").unwrap().len(), 2);
        let connection = db.connection.lock().unwrap();
        let columns = |table: &str| {
            let mut statement = connection
                .prepare(&format!("PRAGMA table_info({table})"))
                .unwrap();
            statement
                .query_map([], |row| row.get::<_, String>(1))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        };
        let snapshot_columns = columns("snapshots");
        assert!(snapshot_columns.contains(&"preco_brl".into()));
        assert!(snapshot_columns.contains(&"frete_brl".into()));
        for removed in ["moeda_origem", "preco_origem", "imposto_brl", "cotacao"] {
            assert!(!snapshot_columns.contains(&removed.into()));
        }
        assert!(!columns("produtos").contains(&"importado".into()));
        assert!(
            columns("lojas")
                .iter()
                .all(|column| { !["moeda", "requer_credencial"].contains(&column.as_str()) })
        );
        let cotacoes_existe: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='cotacoes')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!cotacoes_existe);
    }

    #[test]
    fn migration_v3_remove_campos_de_importacao_e_preserva_historico() {
        let connection = Connection::open_in_memory().unwrap();
        connection.execute_batch(
            "CREATE TABLE produtos (
               id TEXT PRIMARY KEY,titulo TEXT,titulo_norm TEXT,modelo TEXT,vendedor_id TEXT,
               vendedor TEXT,reputacao REAL,url TEXT,busca TEXT,visto_em TEXT,
               loja TEXT,gtin TEXT,importado INTEGER,oferta_json TEXT
             );
             CREATE TABLE snapshots (
               produto_id TEXT REFERENCES produtos(id),data TEXT,preco REAL,preco_original REAL,
               nota REAL,qtd_avaliacoes INTEGER,condicao TEXT,moeda_origem TEXT,preco_origem TEXT,
               frete_brl TEXT,imposto_brl TEXT,preco_final_brl TEXT,cotacao TEXT,oferta_json TEXT,
               PRIMARY KEY(produto_id,data)
             );
             CREATE TABLE lojas (
               id TEXT PRIMARY KEY,nome TEXT,ativa INTEGER,padrao INTEGER,moeda TEXT,
               requer_credencial INTEGER
             );
             CREATE TABLE cotacoes(data TEXT,moeda TEXT,valor TEXT,PRIMARY KEY(data,moeda));
             CREATE TABLE grupos(id INTEGER PRIMARY KEY,chave TEXT UNIQUE,titulo TEXT);
             CREATE TABLE grupo_ofertas(grupo_id INTEGER REFERENCES grupos(id),produto_id TEXT REFERENCES produtos(id),PRIMARY KEY(grupo_id,produto_id));
             INSERT INTO produtos VALUES ('ml:MLB1','Capa S25','capa s25',NULL,NULL,'Vendedor',0.8,'https://example.invalid/p','busca','data','ml',NULL,0,'{}');
             INSERT INTO snapshots VALUES ('ml:MLB1','2026-10-01',119.9,NULL,4.8,40,'new','BRL','100.00','10.00','8.00','118.00','5.00','{}');
             INSERT INTO lojas VALUES ('ml','Mercado Livre',1,1,'BRL',1);
             INSERT INTO cotacoes VALUES ('2026-10-01','USD','5.00');
             PRAGMA user_version=3;"
        ).unwrap();

        migrate_v4(&connection).unwrap();

        let snapshots: i64 = connection
            .query_row("SELECT COUNT(*) FROM snapshots", [], |row| row.get(0))
            .unwrap();
        assert_eq!(snapshots, 1);
        assert_eq!(
            connection
                .query_row(
                    "SELECT preco_brl,frete_brl,preco_final_brl FROM snapshots WHERE produto_id='ml:MLB1'",
                    [],
                    |row| Ok((row.get::<_, String>(0)?,row.get::<_, Option<String>>(1)?,row.get::<_, String>(2)?)),
                )
                .unwrap(),
            ("100.00".into(),Some("10.00".into()),"110.00".into())
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM produtos", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            connection
                .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            4
        );
    }

    #[test]
    fn migration_com_colisao_faz_rollback_integral() {
        let connection = Connection::open_in_memory().unwrap();
        connection.execute_batch("CREATE TABLE produtos (id TEXT PRIMARY KEY); CREATE TABLE snapshots (produto_id TEXT,preco REAL); CREATE TABLE saude_fontes (fonte TEXT); CREATE TABLE circuitos_fontes (fonte TEXT PRIMARY KEY); INSERT INTO produtos VALUES ('MLB1'),('ml:MLB1'); PRAGMA user_version=2;").unwrap();
        assert!(migrate_v3(&connection).is_err());
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 2);
        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM produtos", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 2);
        let tables: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name='lojas'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(tables, 0);
        let columns: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('produtos')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(columns, 1);
    }

    #[test]
    fn upsert_preserva_dias_e_dinheiro_decimal_no_snapshot() {
        let db = Database::open_in_memory().unwrap();
        let ad =
            crate::coleta::parse_listagem_html(include_str!("../../tests/fixtures/listagem.html"))
                .unwrap()
                .remove(0);
        let dia = NaiveDate::from_ymd_opt(2026, 10, 9).unwrap();
        db.salvar_snapshot("fone", &ad, dia).unwrap();
        db.salvar_snapshot("fone", &ad, dia.succ_opt().unwrap())
            .unwrap();
        assert_eq!(db.historico(&ad.id).unwrap().len(), 2);
        let mut oferta = crate::lojas::ContextoNormalizacao
            .normalizar(
                LojaId::Ml,
                crate::lojas::LojaMl::adaptar_anuncio(ad).unwrap(),
            )
            .unwrap();
        oferta.preco_final_brl = "1234567890123.12".parse().unwrap();
        db.salvar_oferta("fone", &oferta, dia).unwrap();
        db.salvar_oferta("fone", &oferta, dia).unwrap();
        assert_eq!(db.contar_snapshots().unwrap(), 2);
        let connection = db.connection.lock().unwrap();
        let raw: String = connection
            .query_row(
                "SELECT preco_final_brl FROM snapshots WHERE produto_id=?1 AND data=?2",
                params![oferta.id, dia.to_string()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(raw, "1234567890123.12");
    }
    #[test]
    fn circuito_e_saude_sao_isolados_por_loja_e_fonte() {
        let db = Database::open_in_memory().unwrap();
        for n in 0..3 {
            db.registrar_tentativa_fonte("ebay:API", false, 1, Some("falha fake"), n)
                .unwrap();
        }
        assert!(!db.tentar_fonte("ebay:API", 10).unwrap().permitido);
        assert!(db.tentar_fonte("ml:API", 10).unwrap().permitido);
        assert!(db.tentar_fonte("ebay:WebView", 10).unwrap().permitido);
        let health = db.saude_fontes(10).unwrap();
        assert_eq!(health[0].loja, LojaId::Ebay);
        assert_eq!(health[0].fonte, "API");
    }
}
