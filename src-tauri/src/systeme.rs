// SPDX-FileCopyrightText: © 2026 Djamel Lazreg (drrakendu78) <https://djamellazreg.fr>
// SPDX-License-Identifier: LicenseRef-PolyForm-Strict-1.0.0
//
// Les appels à Windows, et rien d'autre : place sur le disque, Corbeille, dossiers connus,
// envoi à la Corbeille, relance élevée (UAC). Le reste du programme passe par ici.

use std::ffi::{OsStr, OsString};
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::os::windows::fs::MetadataExt;
use std::path::{Component, Path, PathBuf, Prefix};

use windows_sys::core::GUID;
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_CANCELLED};
use windows_sys::Win32::Storage::FileSystem::{GetDiskFreeSpaceExW, FILE_ATTRIBUTE_REPARSE_POINT};
use windows_sys::Win32::System::Com::{
    CoInitializeEx, CoTaskMemFree, CoUninitialize, COINIT_APARTMENTTHREADED,
};
use windows_sys::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject, INFINITE};
use windows_sys::Win32::UI::Shell::{
    FOLDERID_Downloads, FOLDERID_LocalAppData, FOLDERID_LocalAppDataLow, SHEmptyRecycleBinW,
    SHFileOperationW, SHGetKnownFolderPath, SHQueryRecycleBinW, ShellExecuteExW, FOF_ALLOWUNDO,
    FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT, FOF_WANTNUKEWARNING, FO_DELETE,
    SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, SHERB_NOCONFIRMATION,
    SHERB_NOPROGRESSUI, SHERB_NOSOUND, SHFILEOPSTRUCTW, SHQUERYRBINFO,
};
use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

/// Une chaîne pour Windows : UTF-16 terminé par un zéro.
fn large(s: &OsStr) -> Vec<u16> {
    s.encode_wide().chain(Some(0)).collect()
}

/// La racine du disque système (« C:\ ») : %SystemDrive%, C: à défaut.
pub fn disque_systeme() -> PathBuf {
    let lecteur = std::env::var("SystemDrive").unwrap_or_default();
    let lecteur = if lecteur.len() == 2 && lecteur.ends_with(':') {
        lecteur
    } else {
        "C:".into()
    };
    PathBuf::from(format!("{lecteur}\\"))
}

/// La lettre du disque d'un chemin absolu (« C »), ou rien (chemin relatif, réseau…).
fn lettre(p: &Path) -> Option<char> {
    match p.components().next() {
        Some(Component::Prefix(pre)) => match pre.kind() {
            Prefix::Disk(l) | Prefix::VerbatimDisk(l) => Some((l as char).to_ascii_uppercase()),
            _ => None,
        },
        _ => None,
    }
}

/// Le chemin est-il sur le disque système ? Le Nettoyeur ne travaille QUE sur C: — ni les
/// autres disques, ni le réseau.
pub fn sur_disque_systeme(p: &Path) -> bool {
    match (lettre(p), lettre(&disque_systeme())) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

/// Un point de réparation (lien symbolique, jonction, fichier cloud…) : jamais suivi, jamais
/// supprimé.
pub fn est_reparse(meta: &std::fs::Metadata) -> bool {
    meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

pub struct Espace {
    pub total: u64,
    pub libre: u64,
}

/// Taille et place libre d'un disque.
pub fn espace(racine: &Path) -> Option<Espace> {
    let chemin = large(racine.as_os_str());
    let (mut dispo, mut total, mut libre) = (0u64, 0u64, 0u64);
    let ok = unsafe { GetDiskFreeSpaceExW(chemin.as_ptr(), &mut dispo, &mut total, &mut libre) };
    if ok == 0 {
        None
    } else {
        Some(Espace { total, libre })
    }
}

/// Ce que contient la Corbeille d'un disque : (octets, éléments).
pub fn corbeille(racine: &Path) -> (u64, u64) {
    let chemin = large(racine.as_os_str());
    let mut info = SHQUERYRBINFO {
        cbSize: std::mem::size_of::<SHQUERYRBINFO>() as u32,
        i64Size: 0,
        i64NumItems: 0,
    };
    let hr = unsafe { SHQueryRecycleBinW(chemin.as_ptr(), &mut info) };
    if hr < 0 {
        (0, 0)
    } else {
        (info.i64Size.max(0) as u64, info.i64NumItems.max(0) as u64)
    }
}

/// Vide la Corbeille d'un disque, sans confirmation ni bruit (l'utilisateur a coché la case).
pub fn vider_corbeille(racine: &Path) -> bool {
    let chemin = large(racine.as_os_str());
    let hr = unsafe {
        SHEmptyRecycleBinW(
            std::ptr::null_mut(),
            chemin.as_ptr(),
            SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND,
        )
    };
    // une Corbeille déjà vide répond une erreur : ce n'en est pas une
    hr >= 0 || corbeille(racine).1 == 0
}

fn dossier_connu(id: &GUID) -> Option<PathBuf> {
    let mut brut: *mut u16 = std::ptr::null_mut();
    let hr = unsafe { SHGetKnownFolderPath(id, 0, std::ptr::null_mut(), &mut brut) };
    if hr < 0 || brut.is_null() {
        return None;
    }
    let chemin = unsafe {
        let mut n = 0usize;
        while *brut.add(n) != 0 {
            n += 1;
        }
        let texte = OsString::from_wide(std::slice::from_raw_parts(brut, n));
        CoTaskMemFree(brut as *const core::ffi::c_void);
        PathBuf::from(texte)
    };
    if chemin.is_absolute() {
        Some(chemin)
    } else {
        None
    }
}

/// %LOCALAPPDATA% (C:\Users\…\AppData\Local), demandé à Windows plutôt qu'à l'environnement.
pub fn appdata_local() -> Option<PathBuf> {
    dossier_connu(&FOLDERID_LocalAppData)
}

/// …\AppData\LocalLow (caches de shaders NVIDIA et Intel).
pub fn appdata_local_low() -> Option<PathBuf> {
    dossier_connu(&FOLDERID_LocalAppDataLow)
}

/// Le dossier Téléchargements, même déplacé par l'utilisateur.
pub fn telechargements() -> Option<PathBuf> {
    dossier_connu(&FOLDERID_Downloads)
}

/// Envoie des fichiers à la Corbeille (récupérables). Si l'un d'eux est trop gros pour elle,
/// Windows DEMANDE avant de le supprimer pour de bon (FOF_WANTNUKEWARNING) : rien ne disparaît
/// définitivement sans un oui.
pub fn vers_corbeille(fichiers: &[PathBuf]) -> bool {
    if fichiers.is_empty() {
        return true;
    }
    // une liste de chemins séparés par des zéros, terminée par un double zéro
    let mut liste: Vec<u16> = Vec::new();
    for f in fichiers {
        liste.extend(f.as_os_str().encode_wide());
        liste.push(0);
    }
    liste.push(0);
    unsafe {
        let com = CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32);
        let mut op: SHFILEOPSTRUCTW = std::mem::zeroed();
        op.wFunc = FO_DELETE;
        op.pFrom = liste.as_ptr();
        op.fFlags =
            (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_WANTNUKEWARNING | FOF_SILENT | FOF_NOERRORUI)
                as u16;
        let r = SHFileOperationW(&mut op);
        if com >= 0 {
            CoUninitialize();
        }
        r == 0 && op.fAnyOperationsAborted == 0
    }
}

pub enum Eleve {
    /// le programme élevé a tourné jusqu'au bout
    Fait,
    /// l'utilisateur a dit non à l'UAC
    Refuse,
    Echec,
}

/// Relance CE programme en administrateur (une seule demande UAC) avec des arguments fixes,
/// et attend qu'il ait fini. Aucun chemin ne passe : seulement des identifiants de catégorie.
pub fn relancer_eleve(arguments: &str) -> Eleve {
    let Ok(exe) = std::env::current_exe() else {
        return Eleve::Echec;
    };
    let verbe = large(OsStr::new("runas"));
    let fichier = large(exe.as_os_str());
    let params = large(OsStr::new(arguments));
    unsafe {
        let mut info: SHELLEXECUTEINFOW = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        info.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC;
        info.lpVerb = verbe.as_ptr();
        info.lpFile = fichier.as_ptr();
        info.lpParameters = params.as_ptr();
        info.nShow = SW_HIDE as i32;
        if ShellExecuteExW(&mut info) == 0 {
            return if GetLastError() == ERROR_CANCELLED {
                Eleve::Refuse
            } else {
                Eleve::Echec
            };
        }
        if info.hProcess.is_null() {
            return Eleve::Echec;
        }
        WaitForSingleObject(info.hProcess, INFINITE);
        let mut code = 1u32;
        GetExitCodeProcess(info.hProcess, &mut code);
        CloseHandle(info.hProcess);
        if code == 0 {
            Eleve::Fait
        } else {
            Eleve::Echec
        }
    }
}

/// Ouvre l'Explorateur sur un fichier, sélectionné.
pub fn montrer_dans_explorateur(fichier: &Path) -> bool {
    use std::os::windows::process::CommandExt;
    std::process::Command::new("explorer.exe")
        .raw_arg(format!("/select,\"{}\"", fichier.display()))
        .spawn()
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_disque_systeme_est_une_racine_de_lecteur() {
        let d = disque_systeme();
        assert!(d.is_absolute());
        assert_eq!(d.components().count(), 2, "{d:?}");
    }

    #[test]
    fn rien_hors_du_disque_systeme() {
        let sys = disque_systeme();
        assert!(sur_disque_systeme(&sys.join("Windows").join("Temp")));
        let autre = if lettre(&sys) == Some('D') {
            "E:\\x"
        } else {
            "D:\\x"
        };
        assert!(!sur_disque_systeme(Path::new(autre)));
        assert!(!sur_disque_systeme(Path::new("relatif\\Temp")));
        assert!(!sur_disque_systeme(Path::new("\\\\serveur\\partage\\Temp")));
    }
}
