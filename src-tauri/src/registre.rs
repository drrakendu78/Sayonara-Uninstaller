// SPDX-FileCopyrightText: © 2026 Djamel Lazreg (drrakendu78) <https://djamellazreg.fr>
// SPDX-License-Identifier: LicenseRef-PolyForm-Strict-1.0.0
//
// Le registre : lecture (programmes installés, clés laissées par un programme), sauvegarde en
// .reg AVANT toute suppression, suppression d'une clé et de ses sous-clés. Toujours la vue 64 bits :
// la partie 32 bits se lit à son vrai chemin, SOFTWARE\WOW6432Node.

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::System::Environment::ExpandEnvironmentStringsW;
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegDeleteTreeW, RegEnumKeyExW, RegEnumValueW, RegOpenKeyExW, RegQueryValueExW, HKEY,
    HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_ALL_ACCESS, KEY_READ, KEY_WOW64_64KEY, REG_DWORD, REG_EXPAND_SZ,
    REG_SZ,
};

fn large(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(Some(0)).collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ruche {
    Hklm,
    Hkcu,
}

impl Ruche {
    fn hkey(self) -> HKEY {
        match self {
            Ruche::Hklm => HKEY_LOCAL_MACHINE,
            Ruche::Hkcu => HKEY_CURRENT_USER,
        }
    }
    pub fn prefixe(self) -> &'static str {
        match self {
            Ruche::Hklm => "HKLM",
            Ruche::Hkcu => "HKCU",
        }
    }
}

/// Une clé ouverte, refermée toute seule.
pub struct Cle(HKEY);

impl Drop for Cle {
    fn drop(&mut self) {
        unsafe {
            RegCloseKey(self.0);
        }
    }
}

pub fn ouvrir(ruche: Ruche, chemin: &str) -> Option<Cle> {
    let mut h: HKEY = std::ptr::null_mut();
    let r = unsafe { RegOpenKeyExW(ruche.hkey(), large(chemin).as_ptr(), 0, KEY_READ | KEY_WOW64_64KEY, &mut h) };
    if r == ERROR_SUCCESS { Some(Cle(h)) } else { None }
}

pub fn existe(ruche: Ruche, chemin: &str) -> bool {
    ouvrir(ruche, chemin).is_some()
}

fn expanser(s: &str) -> String {
    let src = large(s);
    let n = unsafe { ExpandEnvironmentStringsW(src.as_ptr(), std::ptr::null_mut(), 0) };
    if n == 0 {
        return s.to_string();
    }
    let mut buf = vec![0u16; n as usize];
    let m = unsafe { ExpandEnvironmentStringsW(src.as_ptr(), buf.as_mut_ptr(), n) };
    if m == 0 || m > n {
        return s.to_string();
    }
    String::from_utf16_lossy(&buf[..(m as usize).saturating_sub(1)])
}

impl Cle {
    pub fn ouvrir_sous(&self, nom: &str) -> Option<Cle> {
        let mut h: HKEY = std::ptr::null_mut();
        let r = unsafe { RegOpenKeyExW(self.0, large(nom).as_ptr(), 0, KEY_READ | KEY_WOW64_64KEY, &mut h) };
        if r == ERROR_SUCCESS { Some(Cle(h)) } else { None }
    }

    pub fn sous_cles(&self) -> Vec<String> {
        let mut noms = Vec::new();
        let mut i = 0u32;
        loop {
            let mut buf = [0u16; 256];
            let mut n = buf.len() as u32;
            let r = unsafe {
                RegEnumKeyExW(self.0, i, buf.as_mut_ptr(), &mut n, std::ptr::null(), std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut())
            };
            if r != ERROR_SUCCESS {
                break;
            }
            noms.push(String::from_utf16_lossy(&buf[..n as usize]));
            i += 1;
        }
        noms
    }

    pub fn noms_valeurs(&self) -> Vec<String> {
        let mut noms = Vec::new();
        let mut i = 0u32;
        loop {
            let mut buf = [0u16; 16384];
            let mut n = buf.len() as u32;
            let r = unsafe {
                RegEnumValueW(self.0, i, buf.as_mut_ptr(), &mut n, std::ptr::null(), std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut())
            };
            if r != ERROR_SUCCESS {
                break;
            }
            noms.push(String::from_utf16_lossy(&buf[..n as usize]));
            i += 1;
        }
        noms
    }

    fn brut(&self, nom: &str) -> Option<(u32, Vec<u8>)> {
        let n = large(nom);
        let (mut genre, mut taille) = (0u32, 0u32);
        let r = unsafe { RegQueryValueExW(self.0, n.as_ptr(), std::ptr::null(), &mut genre, std::ptr::null_mut(), &mut taille) };
        if r != ERROR_SUCCESS || taille > 1 << 20 {
            return None;
        }
        let mut buf = vec![0u8; taille as usize];
        let r = unsafe { RegQueryValueExW(self.0, n.as_ptr(), std::ptr::null(), &mut genre, buf.as_mut_ptr(), &mut taille) };
        if r != ERROR_SUCCESS {
            return None;
        }
        buf.truncate(taille as usize);
        Some((genre, buf))
    }

    /// Une valeur texte (REG_SZ, ou REG_EXPAND_SZ développée) ; `""` pour la valeur par défaut.
    pub fn texte(&self, nom: &str) -> Option<String> {
        let (genre, buf) = self.brut(nom)?;
        if genre != REG_SZ && genre != REG_EXPAND_SZ {
            return None;
        }
        let mots: Vec<u16> = buf.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).take_while(|&c| c != 0).collect();
        let s = String::from_utf16_lossy(&mots).trim().to_string();
        let s = if genre == REG_EXPAND_SZ { expanser(&s) } else { s };
        if s.is_empty() { None } else { Some(s) }
    }

    pub fn nombre(&self, nom: &str) -> Option<u32> {
        let (genre, buf) = self.brut(nom)?;
        if genre != REG_DWORD || buf.len() < 4 {
            return None;
        }
        Some(u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]))
    }
}

/// « HKCU\Software\X » → (Hkcu, « Software\X »).
pub fn decouper(complet: &str) -> Option<(Ruche, String)> {
    let (tete, reste) = complet.split_once('\\')?;
    let ruche = match tete.to_ascii_uppercase().as_str() {
        "HKLM" | "HKEY_LOCAL_MACHINE" => Ruche::Hklm,
        "HKCU" | "HKEY_CURRENT_USER" => Ruche::Hkcu,
        _ => return None,
    };
    Some((ruche, reste.trim_matches('\\').to_string()))
}

/// Les premiers niveaux de SOFTWARE qu'on ne supprime JAMAIS, quoi qu'en dise l'analyse.
const INTOUCHABLES: [&str; 8] = ["microsoft", "windows", "classes", "clients", "policies", "wow6432node", "registeredapplications", "wow6432node\\microsoft"];

/// Une clé qu'on accepte de supprimer : HKCU ou HKLM, sous SOFTWARE (ou SOFTWARE\WOW6432Node),
/// au moins un niveau en dessous (« Software\Éditeur »), jamais Microsoft, Windows, Classes…
pub fn cle_supprimable(complet: &str) -> bool {
    let Some((_, chemin)) = decouper(complet) else { return false };
    let parties: Vec<String> = chemin.split('\\').filter(|p| !p.is_empty()).map(|p| p.to_lowercase()).collect();
    if parties.first().map(String::as_str) != Some("software") || parties.iter().any(|p| p == "..") {
        return false;
    }
    let sous: Vec<&String> = if parties.get(1).map(String::as_str) == Some("wow6432node") { parties.iter().skip(2).collect() } else { parties.iter().skip(1).collect() };
    match sous.first() {
        None => false,
        Some(premier) => !INTOUCHABLES.contains(&premier.as_str()),
    }
}

/// Sauvegarde une clé (et ses sous-clés) dans un fichier .reg, avec l'outil de Windows.
/// Un double-clic sur le fichier la remet.
pub fn sauvegarder(complet: &str, fichier: &Path) -> bool {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    std::process::Command::new("reg.exe")
        .args(["export", complet])
        .arg(fichier)
        .args(["/y", "/reg:64"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map(|o| o.status.success() && fichier.is_file())
        .unwrap_or(false)
}

/// Supprime une clé et toutes ses sous-clés. Refuse tout ce que `cle_supprimable` refuse.
pub fn supprimer(complet: &str) -> bool {
    if !cle_supprimable(complet) {
        return false;
    }
    let Some((ruche, chemin)) = decouper(complet) else { return false };
    let Some((parent, nom)) = chemin.rsplit_once('\\') else { return false };
    let mut h: HKEY = std::ptr::null_mut();
    let r = unsafe { RegOpenKeyExW(ruche.hkey(), large(parent).as_ptr(), 0, KEY_ALL_ACCESS | KEY_WOW64_64KEY, &mut h) };
    if r != ERROR_SUCCESS {
        return false;
    }
    let parent = Cle(h);
    let r = unsafe { RegDeleteTreeW(parent.0, large(nom).as_ptr()) };
    r == ERROR_SUCCESS && !existe(ruche, &chemin)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seules_les_cles_d_editeurs_sont_supprimables() {
        assert!(cle_supprimable("HKCU\\Software\\Snap Inc.\\Snap Camera"));
        assert!(cle_supprimable("HKCU\\Software\\Gloam"));
        assert!(cle_supprimable("HKLM\\SOFTWARE\\WOW6432Node\\Insta360"));
        assert!(!cle_supprimable("HKCU\\Software"));
        assert!(!cle_supprimable("HKCU\\Software\\Microsoft\\Windows"));
        assert!(!cle_supprimable("HKLM\\SOFTWARE\\Classes\\.txt"));
        assert!(!cle_supprimable("HKLM\\SOFTWARE\\WOW6432Node"));
        assert!(!cle_supprimable("HKLM\\SOFTWARE\\WOW6432Node\\Microsoft\\Office"));
        assert!(!cle_supprimable("HKLM\\SYSTEM\\CurrentControlSet\\Services\\X"));
        assert!(!cle_supprimable("HKCR\\Software\\X"));
        assert!(!cle_supprimable("HKCU\\Software\\..\\System"));
    }

    #[test]
    fn lecture_de_la_cle_des_programmes() {
        let cle = ouvrir(Ruche::Hklm, "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall").expect("clé Uninstall");
        assert!(!cle.sous_cles().is_empty());
    }

    /// Aller-retour sur une clé de TEST de HKCU : création, sauvegarde .reg, suppression.
    #[test]
    fn sauvegarde_puis_suppression_d_une_cle_de_test() {
        use std::os::windows::process::CommandExt;
        let nom = format!("HKCU\\Software\\NettoyeurTest{}", std::process::id());
        let ok = std::process::Command::new("reg.exe")
            .args(["add", &format!("{nom}\\Sous"), "/v", "InstallDir", "/d", "C:\\Test", "/f"])
            .creation_flags(0x0800_0000)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        assert!(ok, "reg add a échoué");
        let fichier = std::env::temp_dir().join(format!("nettoyeur-test-{}.reg", std::process::id()));
        assert!(sauvegarder(&nom, &fichier), "la sauvegarde .reg a échoué");
        let contenu = std::fs::read(&fichier).unwrap();
        let texte = String::from_utf16_lossy(&contenu.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect::<Vec<_>>());
        assert!(texte.contains("InstallDir"), "la sauvegarde ne contient pas la valeur");
        assert!(supprimer(&nom));
        assert!(!existe(Ruche::Hkcu, nom.trim_start_matches("HKCU\\")));
        let _ = std::fs::remove_file(&fichier);
    }
}
