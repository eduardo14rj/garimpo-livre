mod history;
pub mod multiloja;
mod normalize;
pub mod parsers;
mod pipeline;
mod ranking;

pub use history::calcular_historico;
pub use normalize::{
    extrair_atributos, normalizar, normalizar_com_sinonimos, tokenizar, tokenizar_com_sinonimos,
};
pub use pipeline::{executar_pipeline, filtrar_anuncios};
pub use ranking::{
    bayesiana, bm25, bm25_com_sinonimos, fronteira_pareto, indice_invertido, jaccard,
    levenshtein_similaridade, media_movel, min_max, outliers_iqr, promo_suspeita,
    reputacao_percentual, reputacao_positiva, reputacao_shopee, score_wilson,
    tukey_limite_inferior,
};
