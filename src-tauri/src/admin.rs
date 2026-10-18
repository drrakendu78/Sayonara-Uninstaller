// SPDX-FileCopyrightText: © 2026 Djamel Lazreg (drrakendu78) <https://djamellazreg.fr>
// SPDX-License-Identifier: LicenseRef-PolyForm-Strict-1.0.0
//
// Ce qui demande l'administrateur — Windows\Temp, restes des mises à jour, dossiers de Program
// Files ou ProgramData, clés de HKLM — passe par UN plan, déposé dans le dossier de l'appli, et
// par UNE demande d'autorisation (UAC). Le programme relancé ne fait confiance à personne : il ne
// lit que ce fichier-là, et seulement si son empreinte SHA-256 est celle reçue en argument (un
// autre programme ne peut pas l'échanger entre l'écriture et la lecture) ; il revérifie chaque
// élément avec les règles de l'appli, envoie les dossiers à la Corbeille et sauvegarde chaque
// clé en .reg avant de la supprimer.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use windows_sys::Win32::Foundation::SYSTEMTIME;
use windows_sys::Win32::Security::Cryptography::{BCryptHash, BCRYPT_SHA256_ALG_HANDLE};
use windows_sys::Win32::System::SystemInformation::GetLocalTime;

use crate::restes::{self, Genre};
use crate::{categories, nettoyage, programmes, registre, systeme};

#[derive(Default, Serialize, Deserialize)]
pub struct Plan {
    /// catégories de l'analyse à vider côté administrateur : « temp », « maj »
    pub categories: Vec<String>,
    /// dossiers (restes, orphelins) à envoyer à la Corbeille
    pub dossiers: Vec<String>,
    /// raccourcis morts du menu Démarrer commun ou du bureau public, à la Corbeille
    #[serde(default)]
    pub raccourcis: Vec<String>,
    /// clés de HKLM à sauvegarder puis supprimer
    pub cles: Vec<String>,
}

/// L'identifiant de l'appli (tauri.conf.json) : le nom de son dossier de données. Le programme relancé
/// en administrateur n'a pas la configuration de Tauri sous la main, d'où la constante (un test la
/// garde alignée).
pub const IDENTIFIANT: &str = "com.drrakendu78.sayonara";
/// Le nom montré à l'utilisateur, aussi celui du dossier des sauvegardes dans Documents.
pub const NOM: &str = "Sayonara Uninstaller";

/// Le seul fichier de plan accepté.
pub fn fichier_plan() -> Option<PathBuf> {
    Some(systeme::appdata_local()?.join(IDENTIFIANT).join("plan-admin.json"))
}

/// Où vont les sauvegardes du registre : Documents\Sayonara Uninstaller\Sauvegardes\<date>.
pub fn dossier_sauvegardes() -> Option<PathBuf> {
    let mut maintenant: SYSTEMTIME = unsafe { std::mem::zeroed() };
    unsafe { GetLocalTime(&mut maintenant) };
    let d = systeme::documents()?.join(NOM).join("Sauvegardes").join(horodatage(&maintenant));
    std::fs::create_dir_all(&d).ok()?;
    Some(d)
}

/// « 2026-10-18 21h30 », à l'heure de l'horloge du PC : celle que l'utilisateur a en tête.
fn horodatage(t: &SYSTEMTIME) -> String {
    format!("{:04}-{:02}-{:02} {:02}h{:02}", t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute)
}

/// SHA-256 en hexadécimal (BCrypt de Windows), pour reconnaître le plan qu'on a écrit.
fn empreinte(octets: &[u8]) -> Option<String> {
    let mut h = [0u8; 32];
    let statut = unsafe { BCryptHash(BCRYPT_SHA256_ALG_HANDLE, std::ptr::null(), 0, octets.as_ptr(), u32::try_from(octets.len()).ok()?, h.as_mut_ptr(), 32) };
    (statut == 0).then(|| h.iter().map(|o| format!("{o:02x}")).collect())
}

/// Un nom de fichier sûr pour la sauvegarde d'une clé.
pub fn nom_sauvegarde(cle: &str) -> String {
    let s: String = cle.chars().map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' || c == '.' { c } else { '_' }).collect();
    format!("{}.reg", s.chars().take(150).collect::<String>())
}

/// Sauvegarde puis supprime une clé ; rien n'est supprimé si la sauvegarde a échoué.
pub fn retirer_cle(cle: &str, sauvegardes: &Path) -> bool {
    registre::cle_supprimable(cle) && registre::sauvegarder(cle, &sauvegardes.join(nom_sauvegarde(cle))) && registre::supprimer(cle)
}

/// Écrit le plan et relance le programme en administrateur. `Refuse` si l'utilisateur dit non.
pub fn executer(plan: &Plan) -> systeme::Eleve {
    let Some(fichier) = fichier_plan() else { return systeme::Eleve::Echec };
    if let Some(d) = fichier.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let Ok(json) = serde_json::to_vec(plan) else { return systeme::Eleve::Echec };
    let Some(e) = empreinte(&json) else { return systeme::Eleve::Echec };
    if std::fs::write(&fichier, json).is_err() {
        return systeme::Eleve::Echec;
    }
    let r = systeme::relancer_eleve(&format!("--admin {e}"));
    let _ = std::fs::remove_file(&fichier);
    r
}

/// Le plan, seulement si le fichier est EXACTEMENT celui qu'on a écrit.
fn plan_verifie(octets: &[u8], attendue: &str) -> Option<Plan> {
    if empreinte(octets)? != attendue.to_ascii_lowercase() {
        return None;
    }
    serde_json::from_slice(octets).ok()
}

/// Côté administrateur : lit le plan, revérifie TOUT, exécute. Code de sortie 0.
pub fn programme_eleve(attendue: &str) -> i32 {
    let Some(fichier) = fichier_plan() else { return 2 };
    let Ok(octets) = std::fs::read(&fichier) else { return 2 };
    let Some(plan) = plan_verifie(&octets, attendue) else { return 3 };
    let maintenant = std::time::SystemTime::now();
    for id in &plan.categories {
        if id != "temp" && id != "maj" {
            continue;
        }
        for r in categories::racines(id).into_iter().filter(|r| r.admin) {
            nettoyage::vider(&r.chemin, maintenant);
        }
    }
    let installes = programmes::lister();
    let mut corbeille: Vec<PathBuf> = plan.dossiers.iter().filter(|d| restes::suppression_sure(Genre::Dossier, d, &installes)).map(PathBuf::from).collect();
    corbeille.extend(plan.raccourcis.iter().filter(|r| restes::suppression_sure(Genre::Raccourci, r, &installes)).map(PathBuf::from));
    if !corbeille.is_empty() {
        systeme::vers_corbeille(&corbeille);
    }
    if !plan.cles.is_empty() {
        if let Some(sauvegardes) = dossier_sauvegardes() {
            for c in plan.cles.iter().filter(|c| c.to_uppercase().starts_with("HKLM\\")) {
                retirer_cle(c, &sauvegardes);
            }
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn horodatage_de_reference() {
        let t = SYSTEMTIME { wYear: 2026, wMonth: 10, wDayOfWeek: 0, wDay: 8, wHour: 9, wMinute: 5, wSecond: 0, wMilliseconds: 0 };
        assert_eq!(horodatage(&t), "2026-10-08 09h05");
    }

    /// Le programme relancé en administrateur cherche son plan dans le dossier de l'appli : même
    /// identifiant et même nom que tauri.conf.json, sinon il ne le trouverait pas.
    #[test]
    fn identifiant_et_nom_alignes_sur_la_configuration() {
        let conf: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert_eq!(conf["identifier"], IDENTIFIANT);
        assert_eq!(conf["productName"], NOM);
    }

    #[test]
    fn empreinte_de_reference() {
        // vecteur de test officiel de SHA-256 (FIPS 180-2)
        assert_eq!(empreinte(b"abc").unwrap(), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    #[test]
    fn un_plan_echange_est_refuse() {
        let ecrit = serde_json::to_vec(&Plan { categories: vec!["temp".into()], ..Default::default() }).unwrap();
        let e = empreinte(&ecrit).unwrap();
        assert!(plan_verifie(&ecrit, &e).is_some());
        assert!(plan_verifie(&ecrit, &e.to_uppercase()).is_some());
        let echange = serde_json::to_vec(&Plan { cles: vec!["HKLM\\Software\\Editeur".into()], ..Default::default() }).unwrap();
        assert!(plan_verifie(&echange, &e).is_none());
        assert!(plan_verifie(&ecrit, "").is_none());
    }

    #[test]
    fn noms_de_sauvegarde_sans_caractere_interdit() {
        let n = nom_sauvegarde("HKCU\\Software\\Snap Inc.\\Snap Camera");
        assert!(n.ends_with(".reg"));
        assert!(!n.contains('\\') && !n.contains(':'));
    }
}
