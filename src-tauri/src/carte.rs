// SPDX-FileCopyrightText: © 2026 Djamel Lazreg (drrakendu78) <https://djamellazreg.fr>
// SPDX-License-Identifier: LicenseRef-PolyForm-Strict-1.0.0
//
// « Où part ma place » : TOUT le disque C:, mesuré une fois, puis parcouru dossier par dossier, du
// plus gros au plus petit — ce que les catégories ne voient pas (dossiers de développement, modèles
// d'IA, vidéos…) apparaît ici. Lecture seule : d'ici, on ne libère que ce qui se refait tout seul
// (un dossier de développement, un cache connu du profil), après un clic, avec les revérifications
// du reste de l'appli. L'interface ne connaît que des NUMÉROS de nœud, jamais un chemin.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime};

use serde::Serialize;
use walkdir::WalkDir;

use crate::{categories, dev, nettoyage, systeme};

/// Un fichier à partir de cette taille a sa propre ligne (un modèle, une vidéo, hiberfil.sys) ; les
/// plus petits sont comptés avec leur dossier.
const GROS_FICHIER: u64 = 200 * 1024 * 1024;
/// Les lignes montrées pour un dossier ; les plus petites sont résumées en une seule.
const LIGNES: usize = 60;
const PAS_DE_PARENT: u32 = u32::MAX;

/// « Annuler » pendant la mesure.
pub static ANNULER: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Genre {
    Dossier,
    Fichier,
    /// un dossier que Windows ne nous laisse pas lire (taille inconnue)
    Illisible,
}

struct Noeud {
    nom: String,
    parent: u32,
    taille: u64,
    enfants: Vec<u32>,
    genre: Genre,
}

pub struct Carte {
    racine: PathBuf,
    noeuds: Vec<Noeud>,
    /// la place que Windows dit utilisée au moment de la mesure
    utilise: u64,
    partielle: bool,
    /// la mesure part de la racine d'un disque (« C:\ ») : la place illisible y a sa ligne
    disque: bool,
}

#[derive(Serialize)]
pub struct Entree {
    index: usize,
    nom: String,
    /// « dossier », « fichier », « illisible », « fichiers » (ceux posés dans ce dossier), « systeme »
    genre: &'static str,
    taille: u64,
    /// ce que « Libérer » ferait : « dev » (supprimé pour de bon) ou « cache » (vidé, sauf le récent)
    liberer: Option<&'static str>,
    ouvrable: bool,
}

#[derive(Serialize)]
pub struct Niveau {
    index: usize,
    /// du disque jusqu'à ce dossier : (numéro, nom)
    fil: Vec<(usize, String)>,
    taille: u64,
    entrees: Vec<Entree>,
    /// les plus petites lignes, résumées
    autres: u64,
    nb_autres: usize,
    utilise: u64,
    partielle: bool,
}

/// Mesure tout ce qui est sous `racine` (le disque C:), sans suivre ni compter un lien, une jonction
/// ou un fichier dans le nuage, ni quitter le disque. `progression(octets lus, place utilisée)`.
pub fn mesurer(racine: &Path, progression: impl Fn(u64, u64)) -> Carte {
    ANNULER.store(false, Ordering::SeqCst);
    let utilise = systeme::espace(racine).map(|e| e.total.saturating_sub(e.libre)).unwrap_or(0);
    let nom_racine = racine.to_string_lossy().trim_end_matches('\\').to_string();
    let mut noeuds = vec![Noeud { nom: nom_racine, parent: PAS_DE_PARENT, taille: 0, enfants: Vec::new(), genre: Genre::Dossier }];
    // pile[p] = le nœud du dossier ouvert à la profondeur p
    let mut pile: Vec<u32> = vec![0];
    let (mut lus, mut signal, mut partielle) = (0u64, Instant::now(), false);
    let mut marche = WalkDir::new(racine).follow_links(false).same_file_system(true).min_depth(1).into_iter();
    while let Some(entree) = marche.next() {
        if ANNULER.load(Ordering::Relaxed) {
            partielle = true;
            break;
        }
        let e = match entree {
            Ok(e) => e,
            Err(err) => {
                // le dossier a été rendu, mais Windows refuse de l'ouvrir : il reste, « illisible »
                let d = err.depth();
                if let (Some(p), Some(&i)) = (err.path(), pile.get(d)) {
                    if d > 0 && p.file_name().map(|n| n.to_string_lossy() == noeuds[i as usize].nom).unwrap_or(false) {
                        noeuds[i as usize].genre = Genre::Illisible;
                    }
                }
                continue;
            }
        };
        let profondeur = e.depth();
        pile.truncate(profondeur);
        let parent = *pile.last().unwrap_or(&0);
        let Ok(meta) = e.metadata() else { continue };
        if systeme::est_reparse(&meta) {
            if e.file_type().is_dir() {
                marche.skip_current_dir();
            }
            continue;
        }
        let nom = || e.file_name().to_string_lossy().into_owned();
        if e.file_type().is_dir() {
            let i = noeuds.len() as u32;
            noeuds.push(Noeud { nom: nom(), parent, taille: 0, enfants: Vec::new(), genre: Genre::Dossier });
            noeuds[parent as usize].enfants.push(i);
            pile.push(i);
        } else if e.file_type().is_file() {
            let t = meta.len();
            lus += t;
            if t >= GROS_FICHIER {
                let i = noeuds.len() as u32;
                noeuds.push(Noeud { nom: nom(), parent, taille: t, enfants: Vec::new(), genre: Genre::Fichier });
                noeuds[parent as usize].enfants.push(i);
            } else {
                noeuds[parent as usize].taille += t;
            }
            if signal.elapsed() >= Duration::from_millis(200) {
                progression(lus, utilise);
                signal = Instant::now();
            }
        }
    }
    // chaque nœud ajoute sa taille à son parent, des feuilles vers le disque : un enfant a toujours un
    // numéro plus grand que son parent
    for i in (1..noeuds.len()).rev() {
        let (t, p) = (noeuds[i].taille, noeuds[i].parent as usize);
        noeuds[p].taille += t;
    }
    progression(lus, utilise);
    Carte { racine: racine.to_path_buf(), noeuds, utilise, partielle, disque: racine.parent().is_none() }
}

impl Carte {
    fn chemin(&self, i: usize) -> PathBuf {
        let mut noms = Vec::new();
        let mut k = i;
        while k != 0 && k < self.noeuds.len() {
            noms.push(self.noeuds[k].nom.as_str());
            k = self.noeuds[k].parent as usize;
        }
        let mut p = self.racine.clone();
        for n in noms.iter().rev() {
            p.push(n);
        }
        p
    }

    /// Ce que « Libérer » ferait pour ce nœud, s'il se refait tout seul.
    fn liberable(&self, i: usize) -> Option<&'static str> {
        let n = self.noeuds.get(i)?;
        if n.genre != Genre::Dossier || n.taille == 0 || i == 0 {
            return None;
        }
        let p = self.chemin(i);
        if dev::genre_de(&p).is_some() && dev::autorise(&p) {
            return Some("dev");
        }
        // un cache connu (Temp, caches de navigateur ou de jeu…), mais seulement dans le profil :
        // ailleurs, il faudrait l'administrateur
        let profil = std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_default();
        if categories::racine_sure(&p) && !profil.as_os_str().is_empty() && p.starts_with(&profil) {
            return Some("cache");
        }
        None
    }

    fn genre_texte(g: Genre) -> &'static str {
        match g {
            Genre::Dossier => "dossier",
            Genre::Fichier => "fichier",
            Genre::Illisible => "illisible",
        }
    }

    pub fn niveau(&self, i: usize) -> Option<Niveau> {
        let n = self.noeuds.get(i)?;
        if n.genre == Genre::Fichier {
            return None;
        }
        let mut enfants: Vec<usize> = n.enfants.iter().map(|&k| k as usize).collect();
        enfants.sort_by(|a, b| self.noeuds[*b].taille.cmp(&self.noeuds[*a].taille));
        let dans_enfants: u64 = enfants.iter().map(|&k| self.noeuds[k].taille).sum();
        let mut entrees: Vec<Entree> = enfants
            .iter()
            .map(|&k| {
                let e = &self.noeuds[k];
                Entree {
                    index: k,
                    nom: e.nom.clone(),
                    genre: Self::genre_texte(e.genre),
                    taille: e.taille,
                    liberer: self.liberable(k),
                    ouvrable: e.genre == Genre::Dossier && !e.enfants.is_empty(),
                }
            })
            .collect();
        // les petits fichiers posés directement dans ce dossier
        let ici = n.taille.saturating_sub(dans_enfants);
        if ici > 0 {
            entrees.push(Entree { index: i, nom: "Fichiers de ce dossier".into(), genre: "fichiers", taille: ici, liberer: None, ouvrable: false });
        }
        // à la racine : ce que Windows compte mais qu'on ne peut pas lire (fichiers du système,
        // points de restauration, zones protégées)
        if i == 0 && self.disque && !self.partielle && self.utilise > n.taille {
            entrees.push(Entree { index: 0, nom: "Système et zones protégées".into(), genre: "systeme", taille: self.utilise - n.taille, liberer: None, ouvrable: false });
        }
        entrees.sort_by(|a, b| b.taille.cmp(&a.taille));
        let (mut autres, mut nb_autres) = (0u64, 0usize);
        if entrees.len() > LIGNES {
            for e in entrees.drain(LIGNES..) {
                autres += e.taille;
                nb_autres += 1;
            }
        }
        let mut fil = Vec::new();
        let mut k = i;
        loop {
            fil.push((k, self.noeuds[k].nom.clone()));
            if k == 0 {
                break;
            }
            k = self.noeuds[k].parent as usize;
        }
        fil.reverse();
        Some(Niveau { index: i, fil, taille: n.taille, entrees, autres, nb_autres, utilise: self.utilise, partielle: self.partielle })
    }

    /// Le chemin d'un nœud pour « Afficher » dans l'Explorateur (le disque lui-même : rien).
    pub fn chemin_a_montrer(&self, i: usize) -> Option<PathBuf> {
        (i > 0 && i < self.noeuds.len()).then(|| self.chemin(i))
    }

    /// Libère un nœud qui se refait tout seul : un dossier de développement part pour de bon, un
    /// cache est vidé de ce qui n'a pas servi depuis 24 h. Rend les octets libérés et le dossier
    /// parent à remontrer. Les tailles de la carte suivent.
    pub fn liberer(&mut self, i: usize) -> Result<(u64, usize), String> {
        let quoi = self.liberable(i).ok_or("Ce dossier ne se libère pas d'ici.")?;
        let p = self.chemin(i);
        let libere = if quoi == "dev" {
            let genre = dev::genre_de(&p).ok_or("Ce n'est plus un dossier de développement.")?;
            dev::supprimer(&dev::Dossier { chemin: p, genre, taille: 0, nombre: 0, derniere: SystemTime::UNIX_EPOCH })
        } else {
            nettoyage::vider(&p, SystemTime::now()).taille
        };
        // ce qui reste dedans n'a plus de détail juste : on le garde en un seul bloc
        self.noeuds[i].enfants.clear();
        let mut k = i;
        loop {
            self.noeuds[k].taille = self.noeuds[k].taille.saturating_sub(libere);
            if k == 0 {
                break;
            }
            k = self.noeuds[k].parent as usize;
        }
        Ok((libere, self.noeuds[i].parent as usize))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn dossier_test(nom: &str) -> PathBuf {
        let d = Path::new(env!("CARGO_MANIFEST_DIR")).join("target").join("tests-sayonara").join(format!("carte-{nom}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn les_tailles_remontent_et_les_lignes_sont_triees() {
        let d = dossier_test("tailles");
        fs::create_dir_all(d.join("a").join("b")).unwrap();
        fs::create_dir_all(d.join("c")).unwrap();
        fs::write(d.join("a").join("b").join("x.bin"), vec![0u8; 3000]).unwrap();
        fs::write(d.join("a").join("y.bin"), vec![0u8; 1000]).unwrap();
        fs::write(d.join("c").join("z.bin"), vec![0u8; 500]).unwrap();
        fs::write(d.join("ici.txt"), vec![0u8; 10]).unwrap();
        let carte = mesurer(&d, |_, _| {});
        let n = carte.niveau(0).unwrap();
        assert_eq!(n.taille, 4510);
        let noms: Vec<(&str, u64)> = n.entrees.iter().map(|e| (e.nom.as_str(), e.taille)).collect();
        assert_eq!(noms, vec![("a", 4000), ("c", 500), ("Fichiers de ce dossier", 10)]);
        let a = n.entrees[0].index;
        assert!(n.entrees[0].ouvrable);
        let na = carte.niveau(a).unwrap();
        assert_eq!(na.fil.iter().map(|(_, nom)| nom.as_str()).collect::<Vec<_>>()[1..], ["a"]);
        assert_eq!(na.entrees.iter().map(|e| (e.nom.as_str(), e.taille)).collect::<Vec<_>>(), vec![("b", 3000), ("Fichiers de ce dossier", 1000)]);
        assert_eq!(carte.chemin_a_montrer(a), Some(d.join("a")));
        assert_eq!(carte.chemin_a_montrer(0), None);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn rien_derriere_une_jonction_n_est_compte() {
        let d = dossier_test("jonction");
        let ailleurs = d.join("ailleurs");
        let mesure = d.join("mesure");
        fs::create_dir_all(&ailleurs).unwrap();
        fs::create_dir_all(&mesure).unwrap();
        fs::write(ailleurs.join("gros.bin"), vec![0u8; 5000]).unwrap();
        fs::write(mesure.join("petit.bin"), vec![0u8; 100]).unwrap();
        let ok = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(mesure.join("lien"))
            .arg(&ailleurs)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        assert!(ok, "mklink /J a échoué");
        let carte = mesurer(&mesure, |_, _| {});
        assert_eq!(carte.niveau(0).unwrap().taille, 100);
        let _ = fs::remove_dir(mesure.join("lien"));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn liberer_un_target_le_retire_de_la_carte_et_du_disque() {
        let d = dossier_test("liberer");
        let projet = d.join("projet");
        let t = projet.join("target");
        fs::create_dir_all(t.join("debug")).unwrap();
        fs::write(projet.join("Cargo.toml"), "[package]").unwrap();
        fs::write(projet.join("main.rs"), vec![0u8; 20]).unwrap();
        fs::write(t.join("CACHEDIR.TAG"), "x").unwrap();
        fs::write(t.join("debug").join("app.exe"), vec![0u8; 4000]).unwrap();
        let mut carte = mesurer(&d, |_, _| {});
        let np = carte.niveau(0).unwrap();
        let ip = np.entrees.iter().find(|e| e.nom == "projet").unwrap().index;
        let nt = carte.niveau(ip).unwrap();
        let cible = nt.entrees.iter().find(|e| e.nom == "target").unwrap();
        assert_eq!(cible.liberer, Some("dev"));
        // un dossier ordinaire ne se libère pas d'ici
        assert!(carte.liberer(ip).is_err());
        let (libere, parent) = carte.liberer(cible.index).unwrap();
        assert_eq!((libere, parent), (4001, ip));
        assert!(!t.exists());
        assert!(projet.join("main.rs").exists(), "le projet reste");
        assert_eq!(carte.niveau(0).unwrap().taille, 4030 - 4001, "il reste Cargo.toml et main.rs");
        let _ = fs::remove_dir_all(&d);
    }

    /// Sur la VRAIE machine, en LECTURE SEULE : `cargo test -- --ignored --nocapture carte_reelle`
    #[test]
    #[ignore]
    fn carte_reelle() {
        let debut = Instant::now();
        let carte = mesurer(&systeme::disque_systeme(), |_, _| {});
        let n = carte.niveau(0).unwrap();
        println!("{} nœuds, {:.1} Go mesurés ({:.1} Go utilisés), en {:?}", carte.noeuds.len(), n.taille as f64 / (1u64 << 30) as f64, n.utilise as f64 / (1u64 << 30) as f64, debut.elapsed());
        for e in n.entrees.iter().take(15) {
            println!("{:>8.1} Go  {:<10} {}", e.taille as f64 / (1u64 << 30) as f64, e.genre, e.nom);
        }
    }
}
