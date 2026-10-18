// SPDX-FileCopyrightText: © 2026 Djamel Lazreg (drrakendu78) <https://djamellazreg.fr>
// SPDX-License-Identifier: LicenseRef-PolyForm-Strict-1.0.0
//
// La suppression. Elle reprend EXACTEMENT les fichiers que la mesure a comptés
// (`categories::fichiers_a_vider` : ni lien, ni récent, seulement dans un dossier de cache
// connu), un par un ; un fichier verrouillé (programme ouvert) est simplement laissé.

use std::fs;
use std::path::Path;
use std::time::SystemTime;

use crate::categories::{self, Mesure};

/// Supprime un fichier ; s'il est en lecture seule (fréquent dans Temp), lève l'attribut et
/// réessaie une fois.
fn supprimer(f: &Path) -> bool {
    if fs::remove_file(f).is_ok() {
        return true;
    }
    let Ok(meta) = fs::symlink_metadata(f) else {
        return false;
    };
    let mut perms = meta.permissions();
    if !perms.readonly() {
        return false;
    }
    #[allow(clippy::permissions_set_readonly_false)]
    perms.set_readonly(false);
    fs::set_permissions(f, perms).is_ok() && fs::remove_file(f).is_ok()
}

/// Vide le contenu d'un dossier de cache : les fichiers d'abord, puis les sous-dossiers devenus
/// vides (du plus profond au moins profond). Le dossier lui-même reste.
pub fn vider(racine: &Path, maintenant: SystemTime) -> Mesure {
    let mut fait = Mesure::default();
    let fichiers: Vec<_> = categories::fichiers_a_vider(racine, maintenant).collect();
    for (f, taille) in fichiers {
        if supprimer(&f) {
            fait += Mesure { taille, nombre: 1 };
        }
    }
    // les sous-dossiers vides : remove_dir refuse un dossier qui contient encore quelque chose,
    // et ne suit jamais un lien (il le refuserait aussi : ce n'est pas un dossier vide à nous)
    if categories::racine_sure(racine) {
        let mut dossiers: Vec<_> = walkdir::WalkDir::new(racine)
            .follow_links(false)
            .same_file_system(true)
            .min_depth(1)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_dir() && !e.path_is_symlink())
            .map(|e| e.into_path())
            .collect();
        dossiers.sort_by_key(|d| std::cmp::Reverse(d.components().count()));
        for d in dossiers {
            let _ = fs::remove_dir(&d);
        }
    }
    fait
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn dossier_test(nom: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir()
            .join(format!("nettoyeur-test-{nom}-{}", std::process::id()))
            .join("Temp");
        let _ = fs::remove_dir_all(d.parent().unwrap());
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn vider_garde_la_racine_le_recent_et_ce_qui_est_derriere_une_jonction() {
        let d = dossier_test("vider");
        fs::create_dir_all(d.join("a").join("b")).unwrap();
        fs::write(d.join("a").join("b").join("vieux.tmp"), vec![0u8; 100]).unwrap();
        fs::write(d.join("lecture-seule.tmp"), vec![0u8; 50]).unwrap();
        let mut p = fs::metadata(d.join("lecture-seule.tmp"))
            .unwrap()
            .permissions();
        p.set_readonly(true);
        fs::set_permissions(d.join("lecture-seule.tmp"), p).unwrap();
        // derrière une jonction : un « document » qui ne doit JAMAIS disparaître
        let ailleurs = d.parent().unwrap().join("ailleurs");
        fs::create_dir_all(&ailleurs).unwrap();
        fs::write(ailleurs.join("precieux.doc"), b"garde-moi").unwrap();
        let ok = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(d.join("lien"))
            .arg(&ailleurs)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        assert!(ok, "mklink /J a échoué");

        // aujourd'hui : tout est récent, rien ne part
        let m = vider(&d, SystemTime::now());
        assert_eq!(m.nombre, 0);
        assert!(d.join("a").join("b").join("vieux.tmp").exists());

        // dans deux jours : les fichiers partent, les dossiers vides aussi, la racine reste
        let plus_tard = SystemTime::now() + Duration::from_secs(2 * 24 * 3600);
        let m = vider(&d, plus_tard);
        assert_eq!(
            m,
            Mesure {
                taille: 150,
                nombre: 2
            }
        );
        assert!(d.is_dir(), "la racine reste");
        assert!(!d.join("a").exists(), "les sous-dossiers vides partent");
        assert!(
            ailleurs.join("precieux.doc").exists(),
            "rien derrière la jonction n'est touché"
        );
        let _ = fs::remove_dir(d.join("lien"));
        let _ = fs::remove_dir_all(d.parent().unwrap());
    }

    #[test]
    fn vider_refuse_un_dossier_qui_n_est_pas_un_cache() {
        let d = std::env::temp_dir()
            .join(format!("nettoyeur-test-pas-cache-{}", std::process::id()))
            .join("Documents");
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join("lettre.txt"), b"x").unwrap();
        let plus_tard = SystemTime::now() + Duration::from_secs(2 * 24 * 3600);
        let m = vider(&d, plus_tard);
        assert_eq!(m.nombre, 0);
        assert!(d.join("lettre.txt").exists());
        let _ = fs::remove_dir_all(d.parent().unwrap());
    }
}
