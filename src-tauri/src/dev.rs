// SPDX-FileCopyrightText: © 2026 Djamel Lazreg (drrakendu78) <https://djamellazreg.fr>
// SPDX-License-Identifier: LicenseRef-PolyForm-Strict-1.0.0
//
// Les dossiers de DÉVELOPPEMENT : ce qu'un compilateur ou un gestionnaire de paquets refait tout
// seul — le `target` d'un projet Rust (à côté de son Cargo.toml), le `node_modules` d'un projet Node
// (à côté de son package.json). Des dizaines de Go chez un développeur, rien chez les autres.
//
// Garde-fous, en plus de ceux de toute l'appli :
//   · on ne cherche que dans le profil (sans AppData ni les dossiers « . ») et dans les dossiers
//     posés à la racine de C: — jamais dans Windows, Program Files ou ProgramData : une appli
//     installée peut porter un node_modules dont elle a besoin ;
//   · le dossier doit porter les marques de son outil (CACHEDIR.TAG ou .rustc_info.json pour Cargo),
//     et son projet (Cargo.toml, package.json) est revérifié au moment de supprimer ;
//   · aucun lien ni jonction suivi ni supprimé (pnpm en met partout dans node_modules).
//
// Ils sont supprimés POUR DE BON, pas mis à la Corbeille : trop gros pour elle, et ils se refont à la
// prochaine compilation (`cargo build`, `npm install`).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::SystemTime;

use walkdir::{DirEntry, WalkDir};

use crate::systeme;

/// Assez profond pour « Documents\site et appi\Projet\src-tauri\target » et ses voisins.
const PROFONDEUR: usize = 9;
/// Les dossiers de la racine de C: où l'on ne cherche jamais.
const RACINE_INTERDITE: [&str; 14] = [
    "windows",
    "program files",
    "program files (x86)",
    "programdata",
    "users",
    "$recycle.bin",
    "system volume information",
    "recovery",
    "perflogs",
    "config.msi",
    "$winreagent",
    "$sysreset",
    "documents and settings",
    "onedrivetemp",
];

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Genre {
    Rust,
    Node,
}

impl Genre {
    fn projet(self) -> &'static str {
        match self {
            Genre::Rust => "Cargo.toml",
            Genre::Node => "package.json",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Dossier {
    pub chemin: PathBuf,
    pub genre: Genre,
    pub taille: u64,
    pub nombre: u64,
    /// la dernière compilation (ou installation) : le fichier le plus récent qu'il contient
    pub derniere: SystemTime,
}

fn nom_de(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default()
}

/// Un chemin dans une zone où l'on ne touche jamais à un dossier de développement : Windows,
/// Program Files, ProgramData, AppData (les applis installées et les outils globaux de npm y vivent).
fn zone_interdite(p: &Path) -> bool {
    let t = p.to_string_lossy().to_lowercase().replace('/', "\\");
    ["\\windows\\", "\\program files", "\\programdata\\", "\\appdata\\"].iter().any(|z| t.contains(z))
}

/// Hors des zones interdites (pour la carte, qui voit tout le disque).
pub fn autorise(p: &Path) -> bool {
    p.is_absolute() && systeme::sur_disque_systeme(p) && !zone_interdite(p)
}

/// Le dossier est-il un dossier de développement, avec son projet à côté ? Rien ne le crée ni ne
/// le suit : on lit seulement.
pub fn genre_de(p: &Path) -> Option<Genre> {
    let genre = match nom_de(p).as_str() {
        "target" => Genre::Rust,
        "node_modules" => Genre::Node,
        _ => return None,
    };
    let parent = p.parent()?;
    if !parent.join(genre.projet()).is_file() {
        return None;
    }
    // un `target` qui n'a pas été fait par Cargo (des données d'un projet ?) n'est jamais proposé
    if genre == Genre::Rust && !p.join("CACHEDIR.TAG").is_file() && !p.join(".rustc_info.json").is_file() {
        return None;
    }
    Some(genre)
}

fn est_lien(e: &DirEntry) -> bool {
    e.path_is_symlink() || e.metadata().map(|m| systeme::est_reparse(&m)).unwrap_or(true)
}

/// Tout ce qu'il y a sous `racine`, sans jamais passer un lien ni une jonction (ni les suivre, ni
/// les rendre) ni quitter le disque.
fn sans_liens(racine: &Path) -> impl Iterator<Item = DirEntry> {
    WalkDir::new(racine)
        .follow_links(false)
        .same_file_system(true)
        .into_iter()
        .filter_entry(|e| e.depth() == 0 || !est_lien(e))
        .filter_map(|e| e.ok())
}

/// Où chercher : le profil de l'utilisateur et les dossiers posés à la racine de C:.
fn racines() -> Vec<(PathBuf, bool)> {
    let mut r = Vec::new();
    if let Some(profil) = std::env::var_os("USERPROFILE").map(PathBuf::from) {
        if profil.is_absolute() {
            r.push((profil, true));
        }
    }
    let c = systeme::disque_systeme();
    if let Ok(lecture) = fs::read_dir(&c) {
        for e in lecture.flatten() {
            let Ok(m) = e.metadata() else { continue };
            if !m.is_dir() || systeme::est_reparse(&m) || RACINE_INTERDITE.contains(&nom_de(&e.path()).as_str()) {
                continue;
            }
            r.push((e.path(), false));
        }
    }
    r
}

/// Le chemin de chaque dossier de développement trouvé sous `racine` (sans les mesurer).
fn trouver(racine: &Path, profil: bool, trouves: &mut Vec<(PathBuf, Genre)>) {
    let mut marche = WalkDir::new(racine).follow_links(false).same_file_system(true).max_depth(PROFONDEUR).into_iter();
    while let Some(entree) = marche.next() {
        let Ok(e) = entree else { continue };
        if e.depth() == 0 || !e.file_type().is_dir() {
            continue;
        }
        let nom = nom_de(e.path());
        if est_lien(&e) || nom.starts_with('.') || (profil && e.depth() == 1 && nom == "appdata") || zone_interdite(e.path()) {
            marche.skip_current_dir();
            continue;
        }
        if nom == "target" || nom == "node_modules" {
            if let Some(g) = genre_de(e.path()) {
                trouves.push((e.path().to_path_buf(), g));
            }
            // reconnu ou non, on n'y descend jamais : un node_modules en contient des milliers d'autres
            marche.skip_current_dir();
        }
    }
}

/// Taille, nombre de fichiers et dernière modification d'un dossier, liens exclus.
fn mesurer(d: &Path) -> (u64, u64, SystemTime) {
    let (mut taille, mut nombre, mut derniere) = (0u64, 0u64, SystemTime::UNIX_EPOCH);
    for e in sans_liens(d).filter(|e| e.file_type().is_file()) {
        let Ok(m) = e.metadata() else { continue };
        taille += m.len();
        nombre += 1;
        if let Ok(t) = m.modified() {
            derniere = derniere.max(t);
        }
    }
    (taille, nombre, derniere)
}

/// Tous les dossiers de développement de C:, du plus gros au plus petit. Lecture seule. Les
/// mesures se font à plusieurs (un `target` fait vite 30 Go et 100 000 fichiers).
pub fn chercher() -> Vec<Dossier> {
    let mut chemins = Vec::new();
    for (racine, profil) in racines() {
        if systeme::sur_disque_systeme(&racine) {
            trouver(&racine, profil, &mut chemins);
        }
    }
    chemins.sort();
    chemins.dedup_by(|a, b| a.0 == b.0);
    let suivant = AtomicUsize::new(0);
    let resultats = Mutex::new(Vec::with_capacity(chemins.len()));
    let fils = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(2, 6);
    std::thread::scope(|s| {
        for _ in 0..fils {
            s.spawn(|| loop {
                let i = suivant.fetch_add(1, Ordering::SeqCst);
                let Some((chemin, genre)) = chemins.get(i) else { break };
                let (taille, nombre, derniere) = mesurer(chemin);
                if nombre > 0 {
                    if let Ok(mut r) = resultats.lock() {
                        r.push(Dossier { chemin: chemin.clone(), genre: *genre, taille, nombre, derniere });
                    }
                }
            });
        }
    });
    let mut r = resultats.into_inner().unwrap_or_default();
    r.sort_by(|a, b| b.taille.cmp(&a.taille));
    r
}

/// Au moment de supprimer : toujours là, toujours un dossier (pas devenu un lien), toujours sur C:,
/// hors des zones interdites, et toujours le dossier de développement d'un projet.
pub fn encore_valable(d: &Dossier) -> bool {
    let Ok(m) = fs::symlink_metadata(&d.chemin) else { return false };
    m.is_dir()
        && !systeme::est_reparse(&m)
        && d.chemin.is_absolute()
        && systeme::sur_disque_systeme(&d.chemin)
        && !zone_interdite(&d.chemin)
        && genre_de(&d.chemin) == Some(d.genre)
}

/// Supprime le dossier POUR DE BON : ses fichiers un par un (un fichier verrouillé par un programme
/// ouvert reste), puis ses sous-dossiers vidés, puis lui-même s'il est vide. Rend les octets libérés.
pub fn supprimer(d: &Dossier) -> u64 {
    if !encore_valable(d) {
        return 0;
    }
    let mut libere = 0u64;
    let mut dossiers: Vec<PathBuf> = Vec::new();
    for e in sans_liens(&d.chemin) {
        if e.file_type().is_dir() {
            dossiers.push(e.into_path());
        } else if e.file_type().is_file() {
            let taille = e.metadata().map(|m| m.len()).unwrap_or(0);
            if crate::nettoyage::supprimer(e.path()) {
                libere += taille;
            }
        }
    }
    // du plus profond au moins profond ; remove_dir refuse ce qui n'est pas vide (un lien gardé,
    // un fichier verrouillé) : ce dossier-là reste, avec ce qu'il contient encore
    dossiers.sort_by_key(|p| std::cmp::Reverse(p.components().count()));
    for p in dossiers {
        let _ = fs::remove_dir(&p);
    }
    libere
}

#[cfg(test)]
mod tests {
    use super::*;

    // pas dans Temp : il est sous AppData, une zone où le module ne touche jamais rien
    fn dossier_test(nom: &str) -> PathBuf {
        let d = Path::new(env!("CARGO_MANIFEST_DIR")).join("target").join("tests-sayonara").join(format!("dev-{nom}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn projet_rust(d: &Path) -> PathBuf {
        fs::write(d.join("Cargo.toml"), "[package]").unwrap();
        let t = d.join("target");
        fs::create_dir_all(t.join("debug").join("deps")).unwrap();
        fs::write(t.join("CACHEDIR.TAG"), "Signature: 8a477f597d28d172789f06886806bc55").unwrap();
        fs::write(t.join("debug").join("deps").join("lib.rlib"), vec![0u8; 3000]).unwrap();
        t
    }

    #[test]
    fn seuls_les_vrais_dossiers_de_developpement_sont_reconnus() {
        let d = dossier_test("genre");
        let t = projet_rust(&d);
        assert_eq!(genre_de(&t), Some(Genre::Rust));
        // un `target` sans les marques de Cargo : des données, jamais proposées
        let autre = d.join("donnees");
        fs::create_dir_all(autre.join("target")).unwrap();
        fs::write(autre.join("Cargo.toml"), "").unwrap();
        assert_eq!(genre_de(&autre.join("target")), None);
        // un node_modules sans package.json à côté
        fs::create_dir_all(d.join("seul").join("node_modules")).unwrap();
        assert_eq!(genre_de(&d.join("seul").join("node_modules")), None);
        fs::write(d.join("seul").join("package.json"), "{}").unwrap();
        assert_eq!(genre_de(&d.join("seul").join("node_modules")), Some(Genre::Node));
        assert!(zone_interdite(Path::new("C:\\Users\\x\\AppData\\Roaming\\npm\\node_modules")));
        assert!(zone_interdite(Path::new("C:\\Program Files\\App\\resources\\node_modules")));
        assert!(!zone_interdite(Path::new("C:\\Users\\x\\Documents\\projet\\node_modules")));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn la_recherche_trouve_mesure_et_ne_descend_pas_dedans() {
        let d = dossier_test("trouver");
        let t = projet_rust(&d.join("a").join("projet").tap_mkdir());
        // un node_modules qui en contient d'autres : un seul est rendu
        let n = d.join("b").join("site");
        fs::create_dir_all(n.join("node_modules").join("x").join("node_modules")).unwrap();
        fs::write(n.join("package.json"), "{}").unwrap();
        fs::write(n.join("node_modules").join("x").join("package.json"), "{}").unwrap();
        fs::write(n.join("node_modules").join("x").join("index.js"), vec![0u8; 500]).unwrap();
        let mut trouves = Vec::new();
        trouver(&d, false, &mut trouves);
        trouves.sort();
        assert_eq!(trouves, vec![(t.clone(), Genre::Rust), (n.join("node_modules"), Genre::Node)]);
        let (taille, nombre, _) = mesurer(&t);
        assert_eq!((taille, nombre), (3000 + 43, 2));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn supprimer_revérifie_et_ne_touche_jamais_derriere_une_jonction() {
        let d = dossier_test("supprimer");
        let t = projet_rust(&d.join("projet").tap_mkdir());
        // derrière une jonction dans le target : un « document » qui ne doit JAMAIS disparaître
        let ailleurs = d.join("ailleurs");
        fs::create_dir_all(&ailleurs).unwrap();
        fs::write(ailleurs.join("precieux.doc"), b"garde-moi").unwrap();
        let ok = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(t.join("lien"))
            .arg(&ailleurs)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        assert!(ok, "mklink /J a échoué");
        let (taille, nombre, derniere) = mesurer(&t);
        let dossier = Dossier { chemin: t.clone(), genre: Genre::Rust, taille, nombre, derniere };

        // le projet a disparu entre l'analyse et le nettoyage : rien n'est supprimé
        fs::rename(d.join("projet").join("Cargo.toml"), d.join("projet").join("Cargo.toml.bak")).unwrap();
        assert_eq!(supprimer(&dossier), 0);
        assert!(t.join("debug").join("deps").join("lib.rlib").exists());
        fs::rename(d.join("projet").join("Cargo.toml.bak"), d.join("projet").join("Cargo.toml")).unwrap();

        assert_eq!(supprimer(&dossier), taille);
        assert!(!t.join("debug").exists(), "ses fichiers et dossiers partent");
        assert!(ailleurs.join("precieux.doc").exists(), "rien derrière la jonction n'est touché");
        assert!(d.join("projet").join("Cargo.toml").exists(), "le projet reste");
        let _ = fs::remove_dir(t.join("lien"));
        let _ = fs::remove_dir_all(&d);
    }

    trait Creer {
        fn tap_mkdir(self) -> PathBuf;
    }
    impl Creer for PathBuf {
        fn tap_mkdir(self) -> PathBuf {
            fs::create_dir_all(&self).unwrap();
            self
        }
    }

    /// Sur la VRAIE machine, en LECTURE SEULE : `cargo test -- --ignored --nocapture dev_reel`
    #[test]
    #[ignore]
    fn dev_reel() {
        let debut = std::time::Instant::now();
        let r = chercher();
        let total: u64 = r.iter().map(|d| d.taille).sum();
        println!("{} dossiers, {:.1} Go, en {:?}", r.len(), total as f64 / (1u64 << 30) as f64, debut.elapsed());
        for d in &r {
            println!("{:>8.2} Go  {:?}  {}", d.taille as f64 / (1u64 << 30) as f64, d.genre, d.chemin.display());
        }
    }
}
