// SPDX-FileCopyrightText: © 2026 Djamel Lazreg (drrakendu78) <https://djamellazreg.fr>
// SPDX-License-Identifier: LicenseRef-PolyForm-Strict-1.0.0
//
// Les sept catégories de la maquette. L'interface ne connaît que leurs IDENTIFIANTS : les
// dossiers sont calculés ici, à partir des dossiers connus de Windows — jamais reçus du front.
//
// Trois garde-fous valent pour tout ce qui suit :
//   · on ne descend que dans des dossiers dont le NOM FINAL est un nom de cache connu
//     (`racine_sure`) : une variable d'environnement tordue ne peut pas faire vider un profil ;
//   · rien hors du disque système, aucun lien ni jonction suivi ou supprimé ;
//   · un fichier touché dans les dernières 24 h reste (il sert peut-être encore).

use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime};

use walkdir::WalkDir;

use crate::systeme;

pub const IDS: [&str; 7] = ["temp", "corbeille", "maj", "nav", "jeux", "install", "gros"];

/// Un fichier modifié (ou créé) depuis moins de 24 h n'est jamais touché.
pub const RECENT: Duration = Duration::from_secs(24 * 3600);
/// « Vieux installateurs » : pas utilisés depuis plus de 3 mois.
pub const AGE_INSTALL: Duration = Duration::from_secs(90 * 24 * 3600);
/// « Gros fichiers oubliés » : plus de 1 Go, pas utilisés depuis un an.
pub const AGE_GROS: Duration = Duration::from_secs(365 * 24 * 3600);
pub const TAILLE_GROS: u64 = 1 << 30;
const EXT_INSTALL: [&str; 6] = ["exe", "msi", "msix", "msixbundle", "appx", "appxbundle"];

/// Les seuls NOMS de dossier dans lesquels le Nettoyeur supprime quelque chose.
const NOMS_SURS: [&str; 16] = [
    "temp",
    "download",
    "cache",
    "code cache",
    "gpucache",
    "grshadercache",
    "shadercache",
    "cache2",
    "dxcache",
    "glcache",
    "dxccache",
    "vkcache",
    "d3dscache",
    "shaders",
    "oglcache",
    "cache_data",
];

pub fn connu(id: &str) -> bool {
    IDS.contains(&id)
}

/// Un dossier où l'on peut vider le CONTENU : absolu, sur le disque système, au moins deux
/// niveaux sous la racine, et dont le nom final est un nom de cache connu.
pub fn racine_sure(p: &Path) -> bool {
    if !p.is_absolute() || !systeme::sur_disque_systeme(p) {
        return false;
    }
    let niveaux = p
        .components()
        .filter(|c| matches!(c, Component::Normal(_)))
        .count();
    let nom = p
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    niveaux >= 2 && NOMS_SURS.contains(&nom.as_str())
}

pub struct Racine {
    pub chemin: PathBuf,
    /// il faut être administrateur pour y supprimer (Windows\Temp, mises à jour)
    pub admin: bool,
}

fn windows() -> PathBuf {
    std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| systeme::disque_systeme().join("Windows"))
}

/// Les sous-dossiers d'un dossier (jamais les liens).
fn sous_dossiers(p: &Path) -> Vec<PathBuf> {
    let Ok(lecture) = fs::read_dir(p) else {
        return Vec::new();
    };
    lecture
        .flatten()
        .filter(|e| {
            e.file_type()
                .map(|t| t.is_dir() && !t.is_symlink())
                .unwrap_or(false)
        })
        .map(|e| e.path())
        .collect()
}

/// Les profils d'un navigateur Chromium : « Default », « Profile 1 », « Profile 2 »…
fn profils_chromium(donnees: &Path) -> Vec<PathBuf> {
    sous_dossiers(donnees)
        .into_iter()
        .filter(|p| {
            let n = p
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            n == "Default" || n.starts_with("Profile ")
        })
        .collect()
}

/// Les dossiers d'une catégorie « à vider » (temp, maj, nav, jeux). Seuls ceux qui existent et
/// passent `racine_sure` sont rendus.
pub fn racines(id: &str) -> Vec<Racine> {
    let mut r: Vec<Racine> = Vec::new();
    let mut ajouter = |chemin: PathBuf, admin: bool| r.push(Racine { chemin, admin });
    let local = systeme::appdata_local();
    let local_low = systeme::appdata_local_low();
    match id {
        "temp" => {
            ajouter(std::env::temp_dir(), false);
            ajouter(windows().join("Temp"), true);
        }
        "maj" => ajouter(
            windows().join("SoftwareDistribution").join("Download"),
            true,
        ),
        "nav" => {
            if let Some(local) = &local {
                // Chrome, Edge, Brave : le cache des pages et du code, par profil, et les caches de
                // shaders communs. Jamais les cookies ni les mots de passe : « vous restez connecté ».
                for donnees in [
                    local.join("Google").join("Chrome").join("User Data"),
                    local.join("Microsoft").join("Edge").join("User Data"),
                    local
                        .join("BraveSoftware")
                        .join("Brave-Browser")
                        .join("User Data"),
                ] {
                    for profil in profils_chromium(&donnees) {
                        for c in ["Cache", "Code Cache", "GPUCache"] {
                            ajouter(profil.join(c), false);
                        }
                    }
                    for c in ["GrShaderCache", "ShaderCache"] {
                        ajouter(donnees.join(c), false);
                    }
                }
                // Opera et Opera GX gardent leur cache dans AppData\Local
                for o in ["Opera Stable", "Opera GX Stable"] {
                    let base = local.join("Opera Software").join(o);
                    for c in ["Cache", "Code Cache", "GPUCache"] {
                        ajouter(base.join(c), false);
                    }
                }
                // Firefox : cache2 de chaque profil
                for profil in sous_dossiers(&local.join("Mozilla").join("Firefox").join("Profiles"))
                {
                    ajouter(profil.join("cache2"), false);
                }
            }
        }
        "jeux" => {
            if let Some(local) = &local {
                for (a, b) in [
                    ("NVIDIA", "DXCache"),
                    ("NVIDIA", "GLCache"),
                    ("AMD", "DxCache"),
                    ("AMD", "DxcCache"),
                    ("AMD", "VkCache"),
                    ("AMD", "OglCache"),
                ] {
                    ajouter(local.join(a).join(b), false);
                }
                ajouter(local.join("D3DSCache"), false);
                // Star Citizen : les shaders de chaque version, refaits au lancement suivant
                for version in sous_dossiers(&local.join("Star Citizen")) {
                    ajouter(version.join("shaders"), false);
                }
            }
            if let Some(low) = &local_low {
                ajouter(
                    low.join("NVIDIA").join("PerDriverVersion").join("DXCache"),
                    false,
                );
                ajouter(low.join("Intel").join("ShaderCache"), false);
            }
        }
        _ => {}
    }
    r.retain(|x| racine_sure(&x.chemin) && x.chemin.is_dir());
    // un dossier compté une seule fois (Temp peut apparaître sous deux noms)
    let mut vus: Vec<String> = Vec::new();
    r.retain(|x| {
        let cle = x.chemin.to_string_lossy().to_lowercase();
        if vus.contains(&cle) {
            false
        } else {
            vus.push(cle);
            true
        }
    });
    r
}

/// La dernière fois qu'un fichier a servi : la plus récente de sa création, de sa modification
/// et de son dernier accès. Un téléchargement d'hier daté de 2019 par le serveur est « d'hier ».
pub fn derniere_utilisation(meta: &fs::Metadata) -> SystemTime {
    [
        meta.created().ok(),
        meta.modified().ok(),
        meta.accessed().ok(),
    ]
    .into_iter()
    .flatten()
    .max()
    .unwrap_or_else(SystemTime::now)
}

fn age(meta: &fs::Metadata, maintenant: SystemTime) -> Duration {
    maintenant
        .duration_since(derniere_utilisation(meta))
        .unwrap_or(Duration::ZERO)
}

/// Pour les caches : la création ou la modification compte, pas la simple lecture (un cache
/// est relu sans arrêt).
fn age_cache(meta: &fs::Metadata, maintenant: SystemTime) -> Duration {
    let date = [meta.created().ok(), meta.modified().ok()]
        .into_iter()
        .flatten()
        .max()
        .unwrap_or_else(SystemTime::now);
    maintenant.duration_since(date).unwrap_or(Duration::ZERO)
}

#[derive(Default, Clone, Copy, Debug, PartialEq)]
pub struct Mesure {
    pub taille: u64,
    pub nombre: u64,
}

impl std::ops::AddAssign for Mesure {
    fn add_assign(&mut self, o: Mesure) {
        self.taille += o.taille;
        self.nombre += o.nombre;
    }
}

/// Les fichiers qu'on supprimerait dans un dossier à vider : tout ce qui n'est ni un lien, ni
/// récent. La racine elle-même reste.
pub fn fichiers_a_vider(
    racine: &Path,
    maintenant: SystemTime,
) -> impl Iterator<Item = (PathBuf, u64)> {
    let ok = racine_sure(racine)
        && fs::symlink_metadata(racine)
            .map(|m| m.is_dir() && !systeme::est_reparse(&m))
            .unwrap_or(false);
    let marche = if ok {
        Some(
            WalkDir::new(racine)
                .follow_links(false)
                .same_file_system(true)
                .min_depth(1),
        )
    } else {
        None
    };
    marche
        .into_iter()
        .flat_map(|w| w.into_iter())
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter_map(move |e| {
            let meta = e.metadata().ok()?;
            if systeme::est_reparse(&meta) || age_cache(&meta, maintenant) < RECENT {
                return None;
            }
            Some((e.into_path(), meta.len()))
        })
}

pub fn mesurer(racine: &Path, maintenant: SystemTime) -> Mesure {
    let mut m = Mesure::default();
    for (_, taille) in fichiers_a_vider(racine, maintenant) {
        m.taille += taille;
        m.nombre += 1;
    }
    m
}

#[derive(Clone, Debug)]
pub struct Fichier {
    pub chemin: PathBuf,
    pub taille: u64,
    pub derniere: SystemTime,
}

/// Le fichier de Téléchargements entre-t-il dans la catégorie ? (règle commune à l'analyse et
/// à la vérification qui précède l'envoi à la Corbeille)
pub fn categorie_du_telechargement(
    meta: &fs::Metadata,
    nom: &Path,
    maintenant: SystemTime,
) -> Option<&'static str> {
    if !meta.is_file() || systeme::est_reparse(meta) {
        return None;
    }
    let ext = nom
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let a = age(meta, maintenant);
    if EXT_INSTALL.contains(&ext.as_str()) {
        // un installateur n'est jamais compté deux fois : s'il est récent, il n'est pas « gros oublié »
        return if a >= AGE_INSTALL {
            Some("install")
        } else {
            None
        };
    }
    if meta.len() >= TAILLE_GROS && a >= AGE_GROS {
        return Some("gros");
    }
    None
}

/// Les « à vérifier » : les fichiers posés DIRECTEMENT dans Téléchargements (jamais dans ses
/// sous-dossiers : une appli portable y vit peut-être).
pub fn telechargements(dossier: &Path, maintenant: SystemTime) -> (Vec<Fichier>, Vec<Fichier>) {
    let (mut install, mut gros) = (Vec::new(), Vec::new());
    if !dossier.is_absolute() || !systeme::sur_disque_systeme(dossier) {
        return (install, gros);
    }
    let Ok(lecture) = fs::read_dir(dossier) else {
        return (install, gros);
    };
    for e in lecture.flatten() {
        // DirEntry::metadata ne suit pas les liens sous Windows
        let Ok(meta) = e.metadata() else { continue };
        let chemin = e.path();
        let f = Fichier {
            taille: meta.len(),
            derniere: derniere_utilisation(&meta),
            chemin,
        };
        match categorie_du_telechargement(&meta, &f.chemin, maintenant) {
            Some("install") => install.push(f),
            Some("gros") => gros.push(f),
            _ => {}
        }
    }
    install.sort_by(|a, b| b.taille.cmp(&a.taille));
    gros.sort_by(|a, b| b.taille.cmp(&a.taille));
    (install, gros)
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn dossier_test(nom: &str) -> PathBuf {
        // sous un dossier « Temp » : les tests passent par les mêmes garde-fous que l'appli
        let d = std::env::temp_dir()
            .join(format!("nettoyeur-test-{nom}-{}", std::process::id()))
            .join("Temp");
        let _ = fs::remove_dir_all(d.parent().unwrap());
        fs::create_dir_all(&d).unwrap();
        d
    }

    pub fn vieillir(f: &Path, jours: u64) {
        let date = SystemTime::now() - Duration::from_secs(jours * 24 * 3600);
        let fichier = fs::OpenOptions::new().write(true).open(f).unwrap();
        fichier
            .set_times(fs::FileTimes::new().set_modified(date).set_accessed(date))
            .unwrap();
    }

    #[test]
    fn seuls_les_noms_de_cache_sont_des_racines_sures() {
        let c = systeme::disque_systeme();
        assert!(racine_sure(
            &c.join("Users")
                .join("x")
                .join("AppData")
                .join("Local")
                .join("Temp")
        ));
        assert!(racine_sure(
            &c.join("Windows")
                .join("SoftwareDistribution")
                .join("Download")
        ));
        assert!(racine_sure(
            &c.join("Users")
                .join("x")
                .join("AppData")
                .join("Local")
                .join("NVIDIA")
                .join("DXCache")
        ));
        // jamais un profil, un disque, le dossier Windows, un chemin relatif
        assert!(!racine_sure(&c));
        assert!(!racine_sure(&c.join("Users").join("x")));
        assert!(!racine_sure(&c.join("Windows")));
        assert!(
            !racine_sure(&c.join("Temp")),
            "un seul niveau sous la racine"
        );
        assert!(!racine_sure(Path::new("AppData\\Local\\Temp")));
        assert!(!racine_sure(&c.join("Users").join("x").join("Documents")));
    }

    #[test]
    fn identifiants_inconnus_refuses() {
        assert!(connu("temp") && connu("gros"));
        assert!(!connu("C:\\Windows") && !connu("") && !connu("TEMP"));
    }

    #[test]
    fn la_mesure_ignore_le_recent() {
        let d = dossier_test("mesure");
        fs::write(d.join("vieux.tmp"), vec![0u8; 1000]).unwrap();
        fs::write(d.join("neuf.tmp"), vec![0u8; 500]).unwrap();
        fs::create_dir_all(d.join("sous")).unwrap();
        fs::write(d.join("sous").join("vieux2.tmp"), vec![0u8; 300]).unwrap();
        vieillir(&d.join("vieux.tmp"), 3);
        vieillir(&d.join("sous").join("vieux2.tmp"), 3);
        // la création compte aussi : un fichier créé aujourd'hui reste récent même antidaté.
        // On mesure donc « dans 2 jours » pour que la création d'aujourd'hui soit vieille.
        let plus_tard = SystemTime::now() + Duration::from_secs(2 * 24 * 3600);
        let m = mesurer(&d, plus_tard);
        assert_eq!(
            m,
            Mesure {
                taille: 1800,
                nombre: 3
            },
            "tout a plus de 24 h dans 2 jours"
        );
        let m = mesurer(&d, SystemTime::now());
        assert_eq!(m.nombre, 0, "créés aujourd'hui : rien n'est mesuré");
        let _ = fs::remove_dir_all(d.parent().unwrap());
    }

    #[test]
    fn la_mesure_ne_suit_pas_les_jonctions() {
        let d = dossier_test("jonction");
        let ailleurs = d.parent().unwrap().join("ailleurs");
        fs::create_dir_all(&ailleurs).unwrap();
        fs::write(ailleurs.join("precieux.doc"), vec![0u8; 4096]).unwrap();
        let lien = d.join("lien");
        let ok = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&lien)
            .arg(&ailleurs)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        assert!(ok, "mklink /J a échoué");
        let plus_tard = SystemTime::now() + Duration::from_secs(2 * 24 * 3600);
        let vus: Vec<_> = fichiers_a_vider(&d, plus_tard).map(|(p, _)| p).collect();
        assert!(vus.is_empty(), "la jonction a été suivie : {vus:?}");
        // remove_dir retire la jonction seule, jamais ce qu'elle désigne
        let _ = fs::remove_dir(&lien);
        let _ = fs::remove_dir_all(d.parent().unwrap());
    }

    #[test]
    fn telechargements_tries_par_regle() {
        let d = dossier_test("telechargements");
        let maintenant = SystemTime::now() + Duration::from_secs(400 * 24 * 3600);
        for (nom, taille) in [("setup.exe", 10u64), ("photo.jpg", 20), ("film.mkv", 30)] {
            fs::write(d.join(nom), vec![0u8; taille as usize]).unwrap();
        }
        fs::create_dir_all(d.join("Appli portable")).unwrap();
        fs::write(d.join("Appli portable").join("appli.exe"), b"x").unwrap();
        let (install, gros) = telechargements(&d, maintenant);
        let noms: Vec<_> = install
            .iter()
            .map(|f| f.chemin.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert_eq!(
            noms,
            ["setup.exe"],
            "seuls les installateurs du niveau du dossier"
        );
        assert!(gros.is_empty(), "« gros » demande plus de 1 Go");
        // trop récent : rien
        let (install, _) = telechargements(&d, SystemTime::now());
        assert!(install.is_empty());
        let _ = fs::remove_dir_all(d.parent().unwrap());
    }
}
