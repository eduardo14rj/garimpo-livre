use std::path::Path;

const SERVICE: &str = "com.eduardo.garimpolivre";
const CLEANUP_MARKER: &str = "cleanup-previous-store-settings-v4.done";
const ACCOUNT: &str = concat!("mercadolivre-o", "auth");

fn cleanup_once(
    data_dir: &Path,
    mut delete: impl FnMut(&str) -> Result<(), u32>,
) -> Result<(), String> {
    let marker = data_dir.join(CLEANUP_MARKER);
    if marker.exists() {
        return Ok(());
    }

    for username in crate::lojas::LojaId::TODAS
        .map(|id| format!("loja-{id}"))
        .into_iter()
        .chain([ACCOUNT.to_owned()])
    {
        let target = format!("{username}.{SERVICE}");
        if let Err(code) = delete(&target)
            && code != 1168
        {
            return Err(format!(
                "Não foi possível limpar uma configuração antiga ({code})."
            ));
        }
    }

    std::fs::write(marker, b"v4\n").map_err(|error| error.to_string())
}

#[cfg(windows)]
pub fn limpar_configuracoes_anteriores_uma_vez(data_dir: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::{
        Foundation::GetLastError,
        Security::Credentials::{CRED_TYPE_GENERIC, CredDeleteW},
    };

    cleanup_once(data_dir, |target| {
        let wide = std::ffi::OsStr::new(target)
            .encode_wide()
            .chain([0])
            .collect::<Vec<_>>();
        let result = unsafe { CredDeleteW(wide.as_ptr(), CRED_TYPE_GENERIC, 0) };
        if result != 0 {
            Ok(())
        } else {
            Err(unsafe { GetLastError() })
        }
    })
}

#[cfg(not(windows))]
pub fn limpar_configuracoes_anteriores_uma_vez(_data_dir: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::{ACCOUNT, cleanup_once};

    #[test]
    fn limpa_alvos_legados_uma_vez_sem_consultar_o_cofre_real() {
        let directory = tempfile::tempdir().unwrap();
        let seen = RefCell::new(Vec::new());
        for _ in 0..2 {
            cleanup_once(directory.path(), |target| {
                seen.borrow_mut().push(target.to_owned());
                Err(1168)
            })
            .unwrap();
        }
        let seen = seen.into_inner();
        assert_eq!(seen.len(), 8);
        assert!(
            seen.iter()
                .any(|target| target == "loja-shopee.com.eduardo.garimpolivre")
        );
        assert!(
            seen.iter()
                .any(|target| target.as_str() == format!("{}.com.eduardo.garimpolivre", ACCOUNT))
        );
        assert!(
            directory
                .path()
                .join("cleanup-previous-store-settings-v4.done")
                .exists()
        );
    }

    #[test]
    fn erro_ao_apagar_nao_grava_marcador_e_permite_nova_tentativa() {
        let directory = tempfile::tempdir().unwrap();
        let mut calls = 0;
        let error = cleanup_once(directory.path(), |_| {
            calls += 1;
            Err(5)
        })
        .unwrap_err();
        assert!(error.contains("(5)"));
        assert_eq!(calls, 1);
        assert!(
            !directory
                .path()
                .join("cleanup-previous-store-settings-v4.done")
                .exists()
        );
    }
}
