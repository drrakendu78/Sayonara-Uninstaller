// SPDX-FileCopyrightText: © 2026 Djamel Lazreg (drrakendu78) <https://djamellazreg.fr>
// SPDX-License-Identifier: LicenseRef-PolyForm-Strict-1.0.0
//
// Où mène un raccourci (.lnk) ? Lecture directe du format « Shell Link » de Microsoft
// (MS-SHLLINK), sans COM : la cible locale (LinkInfo), sinon le chemin relatif, sinon le bloc de
// variables d'environnement. Les raccourcis « annoncés » de MSI (sans cible lisible) sont ignorés.

use std::fs;
use std::path::{Path, PathBuf};

use windows_sys::Win32::Globalization::{MultiByteToWideChar, CP_ACP};

fn u16_le(b: &[u8], i: usize) -> Option<u16> {
    Some(u16::from_le_bytes([*b.get(i)?, *b.get(i + 1)?]))
}
fn u32_le(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_le_bytes([*b.get(i)?, *b.get(i + 1)?, *b.get(i + 2)?, *b.get(i + 3)?]))
}

/// Une chaîne ANSI terminée par un zéro, dans la page de code de Windows.
fn ansi(b: &[u8], debut: usize) -> Option<String> {
    let fin = b.get(debut..)?.iter().position(|&c| c == 0)? + debut;
    let octets = &b[debut..fin];
    if octets.is_empty() {
        return Some(String::new());
    }
    let n = unsafe { MultiByteToWideChar(CP_ACP, 0, octets.as_ptr(), octets.len() as i32, std::ptr::null_mut(), 0) };
    if n <= 0 {
        return None;
    }
    let mut buf = vec![0u16; n as usize];
    unsafe { MultiByteToWideChar(CP_ACP, 0, octets.as_ptr(), octets.len() as i32, buf.as_mut_ptr(), n) };
    Some(String::from_utf16_lossy(&buf))
}

/// Une chaîne UTF-16 terminée par un zéro.
fn unicode(b: &[u8], debut: usize) -> Option<String> {
    let mut mots = Vec::new();
    let mut i = debut;
    loop {
        let c = u16_le(b, i)?;
        if c == 0 {
            break;
        }
        mots.push(c);
        i += 2;
    }
    Some(String::from_utf16_lossy(&mots))
}

/// La cible d'un raccourci, ou rien (raccourci annoncé de MSI, illisible, adresse web…).
pub fn cible(lnk: &Path) -> Option<PathBuf> {
    let b = fs::read(lnk).ok()?;
    if b.len() < 0x4C || u32_le(&b, 0)? != 0x4C {
        return None;
    }
    let drapeaux = u32_le(&b, 0x14)?;
    let unicode_ok = drapeaux & 0x80 != 0;
    let mut i = 0x4Cusize;
    if drapeaux & 0x1 != 0 {
        i += 2 + u16_le(&b, i)? as usize; // liste d'identifiants de l'Explorateur
    }
    let mut trouve: Option<String> = None;
    if drapeaux & 0x2 != 0 && drapeaux & 0x100 == 0 {
        let debut = i;
        let taille = u32_le(&b, i)? as usize;
        let entete = u32_le(&b, i + 4)? as usize;
        let options = u32_le(&b, i + 8)?;
        if options & 0x1 != 0 {
            let base = if entete >= 0x24 {
                let o = u32_le(&b, i + 28)? as usize;
                unicode(&b, debut + o)
            } else {
                let o = u32_le(&b, i + 16)? as usize;
                ansi(&b, debut + o)
            };
            let suffixe = if entete >= 0x24 {
                u32_le(&b, i + 32).and_then(|o| unicode(&b, debut + o as usize))
            } else {
                u32_le(&b, i + 24).and_then(|o| ansi(&b, debut + o as usize))
            };
            if let Some(base) = base.filter(|s| !s.is_empty()) {
                trouve = Some(base + suffixe.as_deref().unwrap_or(""));
            }
        }
        i = debut + taille;
    }
    if let Some(t) = trouve {
        return Some(PathBuf::from(t));
    }
    // les chaînes qui suivent : nom, chemin relatif, dossier de travail…
    let lire_chaine = |i: &mut usize| -> Option<String> {
        let n = u16_le(&b, *i)? as usize;
        *i += 2;
        let s = if unicode_ok {
            let mots: Vec<u16> = (0..n).map(|k| u16_le(&b, *i + 2 * k)).collect::<Option<_>>()?;
            *i += 2 * n;
            String::from_utf16_lossy(&mots)
        } else {
            let s = String::from_utf8_lossy(b.get(*i..*i + n)?).to_string();
            *i += n;
            s
        };
        Some(s)
    };
    if drapeaux & 0x4 != 0 {
        lire_chaine(&mut i)?;
    }
    if drapeaux & 0x8 != 0 {
        let relatif = lire_chaine(&mut i)?;
        let p = lnk.parent()?.join(relatif);
        return Some(p);
    }
    None
}

/// Les raccourcis du menu Démarrer (utilisateur et commun, avec leurs sous-dossiers) et des
/// bureaux (utilisateur et public, sans sous-dossiers).
pub fn tous() -> Vec<PathBuf> {
    let mut liste = Vec::new();
    let appdata = std::env::var_os("APPDATA").map(PathBuf::from);
    let programdata = std::env::var_os("ProgramData").map(PathBuf::from);
    for menu in [appdata.map(|a| a.join("Microsoft\\Windows\\Start Menu\\Programs")), programdata.map(|p| p.join("Microsoft\\Windows\\Start Menu\\Programs"))].into_iter().flatten() {
        for e in walkdir::WalkDir::new(menu).follow_links(false).max_depth(4).into_iter().flatten() {
            if e.file_type().is_file() && e.path().extension().map(|x| x.eq_ignore_ascii_case("lnk")).unwrap_or(false) {
                liste.push(e.into_path());
            }
        }
    }
    for bureau in crate::systeme::bureaux() {
        if let Ok(l) = fs::read_dir(bureau) {
            for e in l.flatten() {
                let p = e.path();
                if p.extension().map(|x| x.eq_ignore_ascii_case("lnk")).unwrap_or(false) && e.file_type().map(|t| t.is_file()).unwrap_or(false) {
                    liste.push(p);
                }
            }
        }
    }
    liste
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Les raccourcis du menu Démarrer de cette machine : la plupart doivent se lire.
    #[test]
    fn les_raccourcis_reels_se_lisent() {
        let liste = tous();
        assert!(!liste.is_empty(), "aucun raccourci trouvé");
        let lus = liste.iter().filter(|l| cible(l).is_some()).count();
        assert!(lus * 2 >= liste.len(), "{lus} lus sur {}", liste.len());
    }

    #[test]
    fn un_fichier_qui_n_est_pas_un_raccourci_ne_donne_rien() {
        let f = std::env::temp_dir().join(format!("nettoyeur-faux-{}.lnk", std::process::id()));
        fs::write(&f, b"pas un raccourci").unwrap();
        assert_eq!(cible(&f), None);
        let _ = fs::remove_file(&f);
    }
}
