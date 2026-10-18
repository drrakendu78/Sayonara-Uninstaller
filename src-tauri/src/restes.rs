// SPDX-FileCopyrightText: © 2026 Djamel Lazreg (drrakendu78) <https://djamellazreg.fr>
// SPDX-FileCopyrightText: © 2017 Marcin Szeniak (Bulk Crap Uninstaller)
// SPDX-License-Identifier: LicenseRef-PolyForm-Strict-1.0.0 AND Apache-2.0
//
// Ce que les programmes laissent derrière eux. Logique portée de Bulk Crap Uninstaller
// (© 2017 Marcin Szeniak, Apache License 2.0 — cf. LICENCES-TIERS/BCU.txt) : InstallLocation,
// CommonDriveJunkScanner, SoftwareRegKeyScanner, ShortcutJunk et ProgramFilesOrphans, avec leurs
// points de confiance (noms.rs). Modifiée : en Rust ; un dossier qui CONTIENT l'installation d'un
// autre programme n'est jamais proposé (BCU le notait seulement) ; rien hors du disque C: ;
// Documents, Bureau (hors raccourcis) et Téléchargements ne sont jamais fouillés.

use std::fs;
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::noms::{self, Cible, Niveau, Raison};
use crate::programmes::{self, Programme};
use crate::raccourcis;
use crate::registre::{self, Ruche};
use crate::systeme;

const FILE_ATTRIBUTE_SYSTEM: u32 = 0x4;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Genre {
    Dossier,
    Cle,
    Raccourci,
}

#[derive(Clone, Debug)]
pub struct Reste {
    pub genre: Genre,
    /// chemin complet, ou clé complète (« HKCU\Software\… »)
    pub chemin: String,
    pub raisons: Vec<Raison>,
    pub taille: u64,
    pub nombre: u64,
    /// le programme dont c'est le reste (dans la liste des désinstallés) ; rien pour un orphelin
    pub programme: Option<usize>,
}

impl Reste {
    pub fn niveau(&self) -> Niveau {
        noms::niveau(&self.raisons)
    }
    /// Faut-il être administrateur pour l'enlever ?
    pub fn admin(&self) -> bool {
        match self.genre {
            Genre::Cle => self.chemin.to_uppercase().starts_with("HKLM"),
            _ => !sous(Path::new(&self.chemin), &profil()),
        }
    }
}

fn env(nom: &str) -> Option<PathBuf> {
    std::env::var_os(nom).map(PathBuf::from).filter(|p| p.is_absolute())
}
fn profil() -> PathBuf {
    env("USERPROFILE").unwrap_or_default()
}
fn windows() -> PathBuf {
    env("SystemRoot").unwrap_or_else(|| PathBuf::from("C:\\Windows"))
}

/// `enfant` est-il `parent` ou sous lui ? (sans tenir compte de la casse)
pub fn sous(enfant: &Path, parent: &Path) -> bool {
    let (e, p) = (enfant.to_string_lossy().to_lowercase(), parent.to_string_lossy().to_lowercase());
    let p = p.trim_end_matches('\\');
    !p.is_empty() && (e == p || e.starts_with(&format!("{p}\\")))
}

/// Les dossiers « connus » de Windows : jamais proposés EUX-MÊMES (GetProhibitedLocations).
fn dossiers_connus() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = ["APPDATA", "LOCALAPPDATA", "ProgramData", "ProgramFiles", "ProgramFiles(x86)", "ProgramW6432",
        "USERPROFILE", "PUBLIC", "SystemRoot", "SystemDrive", "TEMP", "CommonProgramFiles", "CommonProgramFiles(x86)"]
        .iter()
        .filter_map(|n| env(n))
        .collect();
    let p = profil();
    for s in ["Documents", "Desktop", "Downloads", "Pictures", "Music", "Videos", "Saved Games", "Favorites", "Links", "Contacts", "OneDrive", "AppData", "AppData\\LocalLow"] {
        v.push(p.join(s));
    }
    for menu in [env("APPDATA"), env("ProgramData")].into_iter().flatten() {
        v.push(menu.join("Microsoft\\Windows\\Start Menu\\Programs"));
        v.push(menu.join("Microsoft\\Windows\\Start Menu"));
        v.push(menu.join("Microsoft"));
    }
    if let Some(dl) = systeme::telechargements() {
        v.push(dl);
    }
    v
}

fn est_connu(p: &Path, connus: &[PathBuf]) -> bool {
    connus.iter().any(|c| c.to_string_lossy().eq_ignore_ascii_case(p.to_string_lossy().trim_end_matches('\\')))
}

/// Un dossier qu'on n'ouvre ni ne propose jamais (IsSystemDirectory) : attribut Système, sous
/// Windows, Téléchargements, nom interdit, ou lien.
fn interdit(p: &Path) -> bool {
    let Ok(meta) = fs::symlink_metadata(p) else { return true };
    if systeme::est_reparse(&meta) || meta.file_attributes() & FILE_ATTRIBUTE_SYSTEM != 0 {
        return true;
    }
    if sous(p, &windows()) || systeme::telechargements().map(|d| sous(p, &d)).unwrap_or(false) {
        return true;
    }
    let nom = p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    // tout ce qui commence par « Windows » ou « Microsoft » appartient au système (ModifiableWindowsApps,
    // WindowsPowerShell, Microsoft Update Health Tools…) : jamais, même vide
    noms::NOMS_INTERDITS.contains(&nom.as_str()) || nom.starts_with("windows") || nom.starts_with("microsoft")
        || nom.contains("windowsapps") || ["msbuild", "iis", "iis express", "dotnet", "common files"].contains(&nom.as_str())
}

fn sous_dossiers(p: &Path) -> Vec<PathBuf> {
    let Ok(l) = fs::read_dir(p) else { return Vec::new() };
    l.flatten().filter(|e| e.file_type().map(|t| t.is_dir() && !t.is_symlink()).unwrap_or(false)).map(|e| e.path()).collect()
}

/// Taille et nombre de fichiers d'un dossier (sans suivre les liens), et s'il contient des programmes.
fn mesurer(p: &Path) -> (u64, u64, bool) {
    let (mut taille, mut nombre, mut exe) = (0u64, 0u64, false);
    for e in walkdir::WalkDir::new(p).follow_links(false).same_file_system(true).into_iter().flatten() {
        if e.file_type().is_file() {
            nombre += 1;
            taille += e.metadata().map(|m| m.len()).unwrap_or(0);
            let ext = e.path().extension().map(|x| x.to_string_lossy().to_lowercase()).unwrap_or_default();
            exe |= matches!(ext.as_str(), "exe" | "dll" | "msi" | "com" | "bat" | "cmd" | "ps1" | "sys" | "scr");
        }
    }
    (taille, nombre, exe)
}

/// Les dossiers où vivent les programmes encore installés (et leurs désinstalleurs).
fn dossiers_installes(installes: &[Programme]) -> Vec<PathBuf> {
    installes.iter().filter_map(programmes::dossier).filter(|d| d.components().count() > 2).collect()
}

/// Un autre programme vit-il dans ce dossier, ou dessous ?
fn contient_une_installation(p: &Path, installes: &[PathBuf]) -> bool {
    installes.iter().any(|d| sous(d, p))
}

/// Les dossiers fouillés pour un programme désinstallé (JunkSearchDirs + Program Files).
fn racines_fouillees() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = Vec::new();
    for menu in [env("APPDATA"), env("ProgramData")].into_iter().flatten() {
        v.push(menu.join("Microsoft\\Windows\\Start Menu\\Programs"));
    }
    v.extend(["APPDATA", "ProgramData", "LOCALAPPDATA"].iter().filter_map(|n| env(n)));
    if let Some(low) = systeme::appdata_local_low() {
        v.push(low);
    }
    v.extend(["ProgramW6432", "ProgramFiles", "ProgramFiles(x86)"].iter().filter_map(|n| env(n)));
    v.retain(|r| r.is_dir() && systeme::sur_disque_systeme(r));
    v.sort_by_key(|r| r.to_string_lossy().to_lowercase());
    v.dedup_by(|a, b| a.to_string_lossy().eq_ignore_ascii_case(&b.to_string_lossy()));
    v
}

/// Pénalité « nom voisin » : un AUTRE programme installé porte ce nom (« AppX Extended » quand on
/// retire « AppX ») — TestForSimilarNames.
fn nom_voisin(cible: &Cible, nom: &str, installes: &[Programme]) -> bool {
    let n = nom.to_lowercase();
    let c = cible.nom_court.to_lowercase();
    installes.iter().any(|p| {
        let a = p.cible.nom_court.to_lowercase();
        !a.contains(&c) && a.contains(&n) && n.chars().count() > 3
    })
}

fn fouiller(dossier: &Path, cible: &Cible, niveau: i32, connus: &[PathBuf], installes_dossiers: &[PathBuf], installes: &[Programme], sortie: &mut Vec<Reste>, programme: usize) -> Vec<PathBuf> {
    let mut trouves_ici = Vec::new();
    let parent_connu = est_connu(dossier, connus);
    let nom_parent = dossier.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    for sous_d in sous_dossiers(dossier) {
        if interdit(&sous_d) {
            continue;
        }
        let nom = sous_d.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let mut nouveau: Option<Reste> = None;
        if !est_connu(&sous_d, connus) {
            let mut r = noms::raisons_du_nom(cible, &nom, &nom_parent, niveau, parent_connu);
            if r.iter().any(|x| x.points() > 0) {
                if noms::NOMS_DOUTEUX.contains(&nom.to_lowercase().as_str()) {
                    r.push(Raison::NomDouteux);
                }
                if contient_une_installation(&sous_d, installes_dossiers) {
                    r.push(Raison::DossierEncoreUtilise);
                }
                if nom_voisin(cible, &nom, installes) {
                    r.push(Raison::NomVoisin);
                }
                nouveau = Some(Reste { genre: Genre::Dossier, chemin: sous_d.to_string_lossy().to_string(), raisons: r, taille: 0, nombre: 0, programme: Some(programme) });
            }
        }
        let mut dessous = Vec::new();
        if niveau <= 1 {
            dessous = fouiller(&sous_d, cible, niveau + 1, connus, installes_dossiers, installes, sortie, programme);
        }
        if let Some(mut n) = nouveau {
            // rien d'autre ne restera dedans après le nettoyage : un bon signe
            let sans_fichier = fs::read_dir(&sous_d).map(|l| l.flatten().all(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(true))).unwrap_or(false);
            if sans_fichier {
                let sd = sous_dossiers(&sous_d);
                if sd.is_empty() || sd.iter().all(|d| dessous.iter().any(|t: &PathBuf| t.to_string_lossy().eq_ignore_ascii_case(&d.to_string_lossy()))) {
                    n.raisons.push(Raison::TousSousDossiers);
                }
            }
            trouves_ici.push(sous_d.clone());
            sortie.push(n);
        }
        trouves_ici.extend(dessous);
    }
    trouves_ici
}

/// Les clés de SOFTWARE au nom du programme (SoftwareRegKeyScanner), jusqu'à trois niveaux.
fn cles(cible: &Cible, dossier_installe: Option<&Path>, installes: &[Programme], sortie: &mut Vec<Reste>, programme: usize) {
    const IGNOREES: [&str; 6] = ["microsoft", "wow6432node", "windows", "classes", "clients", "registeredapplications"];
    const VALEURS_DOSSIER: [&str; 15] = ["installdir", "install_dir", "install directory", "instdir", "applicationpath", "install folder",
        "last stable install path", "targetdir", "path", "path64", "apppath", "installlocation", "installpath", "exe64", "exepath"];
    fn parcourir(cle: &registre::Cle, complet: &str, niveau: i32, cible: &Cible, dossier: Option<&Path>, installes: &[Programme], sortie: &mut Vec<Reste>, programme: usize) {
        for nom in cle.sous_cles() {
            if IGNOREES.contains(&nom.to_lowercase().as_str()) {
                continue;
            }
            let Some(sc) = cle.ouvrir_sous(&nom) else { continue };
            let chemin = format!("{complet}\\{nom}");
            let parent = complet.rsplit('\\').next().unwrap_or("");
            let mut r = noms::raisons_du_nom(cible, &nom, parent, niveau, false);
            // une valeur qui pointe dans le dossier du programme : lien explicite
            if let Some(d) = dossier {
                let pointe = sc.noms_valeurs().iter().filter(|v| v.is_empty() || VALEURS_DOSSIER.contains(&v.to_lowercase().as_str())).any(|v| {
                    sc.texte(v).map(|t| sous(Path::new(t.trim_matches('"')), d)).unwrap_or(false)
                });
                if pointe {
                    r.push(Raison::LienExplicite);
                }
            }
            if !r.is_empty() && registre::cle_supprimable(&chemin) {
                if nom_voisin(cible, &nom, installes) {
                    r.push(Raison::NomVoisin);
                }
                sortie.push(Reste { genre: Genre::Cle, chemin: chemin.clone(), raisons: r, taille: 0, nombre: 0, programme: Some(programme) });
            }
            if niveau <= 1 {
                parcourir(&sc, &chemin, niveau + 1, cible, dossier, installes, sortie, programme);
            }
        }
    }
    for (ruche, racine) in [(Ruche::Hkcu, "Software"), (Ruche::Hklm, "SOFTWARE"), (Ruche::Hklm, "SOFTWARE\\WOW6432Node")] {
        if let Some(c) = registre::ouvrir(ruche, racine) {
            parcourir(&c, &format!("{}\\{}", ruche.prefixe(), racine), 0, cible, dossier_installe, installes, sortie, programme);
        }
    }
}

/// Les raccourcis qui mènent au programme retiré (ShortcutJunk).
fn raccourcis_morts(cible: &Cible, dossier: Option<&Path>, liens: &[(PathBuf, PathBuf)], installes_dossiers: &[PathBuf], sortie: &mut Vec<Reste>, programme: usize) {
    let Some(d) = dossier else { return };
    let partage = installes_dossiers.iter().any(|x| x.to_string_lossy().eq_ignore_ascii_case(&d.to_string_lossy()));
    for (lnk, cible_lnk) in liens {
        if !sous(cible_lnk, d) {
            continue;
        }
        let mut r = vec![Raison::LienExplicite];
        if partage {
            r.push(Raison::DossierEncoreUtilise);
        }
        let nom = lnk.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        r.extend(noms::raisons_du_nom(cible, &nom, "", 0, false));
        let taille = fs::metadata(lnk).map(|m| m.len()).unwrap_or(0);
        sortie.push(Reste { genre: Genre::Raccourci, chemin: lnk.to_string_lossy().to_string(), raisons: r, taille, nombre: 1, programme: Some(programme) });
    }
}

/// Le programme retiré, tel qu'on le connaissait avant (nom, éditeur, dossier).
pub struct Retire {
    pub cible: Cible,
    pub dossier: Option<PathBuf>,
}

/// Tous les restes des programmes retirés, fusionnés, mesurés, sans les douteux.
pub fn chercher(retires: &[Retire], installes: &[Programme]) -> Vec<Reste> {
    let connus = dossiers_connus();
    let installes_dossiers = dossiers_installes(installes);
    let racines = racines_fouillees();
    let liens: Vec<(PathBuf, PathBuf)> = raccourcis::tous().into_iter().filter_map(|l| raccourcis::cible(&l).map(|c| (l, c))).collect();
    let mut sortie = Vec::new();
    for (i, p) in retires.iter().enumerate() {
        // 1. le dossier d'installation, s'il est resté
        if let Some(d) = p.dossier.as_deref().filter(|d| d.is_dir() && !interdit(d) && !est_connu(d, &connus) && d.components().count() > 2) {
            let mut r = vec![Raison::LienExplicite];
            if contient_une_installation(d, &installes_dossiers) {
                r.push(Raison::DossierEncoreUtilise);
            }
            sortie.push(Reste { genre: Genre::Dossier, chemin: d.to_string_lossy().to_string(), raisons: r, taille: 0, nombre: 0, programme: Some(i) });
        }
        // 2. les dossiers à son nom
        for racine in &racines {
            fouiller(racine, &p.cible, 0, &connus, &installes_dossiers, installes, &mut sortie, i);
        }
        // 3. le registre
        cles(&p.cible, p.dossier.as_deref(), installes, &mut sortie, i);
        // 4. les raccourcis
        raccourcis_morts(&p.cible, p.dossier.as_deref(), &liens, &installes_dossiers, &mut sortie, i);
    }
    finaliser(sortie)
}

/// Fusionne les doublons (leurs raisons s'additionnent), écarte les douteux, ce qui est sous un
/// autre reste, et ce qui touche au Nettoyeur lui-même ; mesure les dossiers.
fn finaliser(entree: Vec<Reste>) -> Vec<Reste> {
    let mut fusion: Vec<Reste> = Vec::new();
    for r in entree {
        if let Some(x) = fusion.iter_mut().find(|x| x.programme == r.programme && x.chemin.eq_ignore_ascii_case(&r.chemin)) {
            for raison in r.raisons {
                if !x.raisons.contains(&raison) {
                    x.raisons.push(raison);
                }
            }
        } else {
            fusion.push(r);
        }
    }
    let moi = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf));
    fusion.retain(|r| r.niveau() != Niveau::Douteux);
    fusion.retain(|r| r.genre == Genre::Cle || !moi.as_deref().map(|m| sous(m, Path::new(&r.chemin)) || sous(Path::new(&r.chemin), m)).unwrap_or(false));
    // un dossier sous un autre reste retenu disparaît avec lui : on ne le montre pas deux fois
    let dossiers: Vec<String> = fusion.iter().filter(|r| r.genre == Genre::Dossier).map(|r| r.chemin.clone()).collect();
    fusion.retain(|r| r.genre != Genre::Dossier || !dossiers.iter().any(|d| !d.eq_ignore_ascii_case(&r.chemin) && sous(Path::new(&r.chemin), Path::new(d))));
    let cles: Vec<String> = fusion.iter().filter(|r| r.genre == Genre::Cle).map(|r| r.chemin.to_lowercase()).collect();
    fusion.retain(|r| r.genre != Genre::Cle || !cles.iter().any(|c| *c != r.chemin.to_lowercase() && r.chemin.to_lowercase().starts_with(&format!("{c}\\"))));
    for r in fusion.iter_mut().filter(|r| r.genre == Genre::Dossier) {
        let (t, n, _) = mesurer(Path::new(&r.chemin));
        r.taille = t;
        r.nombre = n;
    }
    fusion
}

/// Les dossiers oubliés par des programmes désinstallés depuis longtemps (ProgramFilesOrphans) :
/// dans Program Files et les dossiers « Programs », ce qu'aucun programme installé ne réclame.
pub fn orphelins(installes: &[Programme]) -> Vec<Reste> {
    let installes_dossiers = dossiers_installes(installes);
    let connus = dossiers_connus();
    let noms_utilises: Vec<String> = installes.iter().map(|p| p.cible.nom_court.to_lowercase()).filter(|n| n.chars().count() > 3).collect();
    let editeurs: Vec<String> = installes.iter().map(|p| p.cible.editeur_court.to_lowercase()).filter(|n| n.chars().count() > 3).collect();
    let mut racines: Vec<PathBuf> = ["ProgramW6432", "ProgramFiles", "ProgramFiles(x86)"].iter().filter_map(|n| env(n)).collect();
    racines.extend(["APPDATA", "LOCALAPPDATA", "ProgramData"].iter().filter_map(|n| env(n)).map(|d| d.join("Programs")));
    racines.retain(|r| r.is_dir() && systeme::sur_disque_systeme(r));
    racines.sort_by_key(|r| r.to_string_lossy().to_lowercase());
    racines.dedup_by(|a, b| a.to_string_lossy().eq_ignore_ascii_case(&b.to_string_lossy()));

    fn fouiller_orphelins(dossier: &Path, niveau: i32, ctx: (&[PathBuf], &[PathBuf], &[String], &[String]), sortie: &mut Vec<Reste>) {
        let (installes_dossiers, connus, noms_utilises, editeurs) = ctx;
        if fs::metadata(dossier).map(|m| m.file_attributes() & FILE_ATTRIBUTE_SYSTEM != 0).unwrap_or(true) {
            return;
        }
        for s in sous_dossiers(dossier) {
            if interdit(&s) || est_connu(&s, connus) {
                continue;
            }
            // ce qu'un programme installé occupe, ou contient : jamais
            if contient_une_installation(&s, installes_dossiers) || installes_dossiers.iter().any(|d| sous(&s, d)) {
                continue;
            }
            let nom = s.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
            let douteux = noms::NOMS_DOUTEUX.iter().any(|d| nom.contains(d));
            let utilise = noms_utilises.iter().any(|n| nom.contains(n.as_str()) || n.contains(nom.as_str()));
            let (taille, nombre, exe) = mesurer(&s);
            let directs = fs::read_dir(&s).map(|l| l.flatten().any(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))).unwrap_or(false);
            let premier = if directs {
                if exe { Raison::Executables } else { Raison::Fichiers }
            } else if nombre == 0 {
                Raison::DossierVide
            } else {
                if niveau < 1 && !douteux && !utilise {
                    fouiller_orphelins(&s, niveau + 1, ctx, sortie);
                }
                if exe { Raison::Executables } else { Raison::Fichiers }
            };
            let mut r = vec![premier];
            if editeurs.iter().any(|e| nom.contains(e.as_str())) {
                r.push(Raison::EditeurEncoreUtilise);
            }
            if utilise {
                r.push(Raison::NomEncoreUtilise);
            }
            if douteux {
                r.push(Raison::NomDouteux);
            }
            if nombre > 100 {
                r.push(Raison::BeaucoupDeFichiers);
            }
            r.push(Raison::Profondeur(-2 * niveau));
            if sous_dossiers(&s).is_empty() {
                r.push(Raison::SansSousDossier);
            }
            sortie.push(Reste { genre: Genre::Dossier, chemin: s.to_string_lossy().to_string(), raisons: r, taille, nombre, programme: None });
        }
    }
    let mut sortie = Vec::new();
    for r in &racines {
        fouiller_orphelins(r, 0, (&installes_dossiers, &connus, &noms_utilises, &editeurs), &mut sortie);
    }
    sortie.retain(|r| r.niveau() != Niveau::Douteux);
    sortie.sort_by(|a, b| b.taille.cmp(&a.taille));
    sortie
}

/// Le reste peut-il partir, ICI et MAINTENANT ? Revérifié juste avant la suppression (et par le
/// programme relancé en administrateur, qui ne fait confiance à personne).
pub fn suppression_sure(genre: Genre, chemin: &str, installes: &[Programme]) -> bool {
    match genre {
        Genre::Cle => registre::cle_supprimable(chemin),
        Genre::Raccourci => {
            let p = Path::new(chemin);
            p.is_absolute()
                && systeme::sur_disque_systeme(p)
                && p.extension().map(|e| e.eq_ignore_ascii_case("lnk")).unwrap_or(false)
                && fs::symlink_metadata(p).map(|m| m.is_file() && !systeme::est_reparse(&m)).unwrap_or(false)
                // seulement un raccourci des menus Démarrer ou des bureaux, là où on les cherche
                && raccourcis::tous().iter().any(|l| l.to_string_lossy().eq_ignore_ascii_case(chemin))
        }
        Genre::Dossier => {
            let p = Path::new(chemin);
            let connus = dossiers_connus();
            let dans_une_racine = racines_fouillees().iter().chain(
                ["APPDATA", "LOCALAPPDATA", "ProgramData"].iter().filter_map(|n| env(n)).map(|d| d.join("Programs")).collect::<Vec<_>>().iter(),
            ).any(|r| sous(p, r) && !r.to_string_lossy().eq_ignore_ascii_case(&p.to_string_lossy()));
            p.is_absolute()
                && systeme::sur_disque_systeme(p)
                && p.is_dir()
                && !interdit(p)
                && !est_connu(p, &connus)
                && dans_une_racine
                && !contient_une_installation(p, &dossiers_installes(installes))
        }
    }
}

/// Le chemin montré à l'utilisateur : sans son profil ni la lettre du disque.
pub fn affichage(r: &Reste) -> String {
    if r.genre == Genre::Cle {
        return r.chemin.clone();
    }
    let p = r.chemin.clone();
    let profil = profil().to_string_lossy().to_string();
    if !profil.is_empty() && p.to_lowercase().starts_with(&format!("{}\\", profil.to_lowercase())) {
        let reste = &p[profil.len() + 1..];
        return reste.replacen("Desktop\\", "Bureau\\", 1);
    }
    p.get(3..).filter(|_| p.get(1..3) == Some(":\\")).map(str::to_string).unwrap_or(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cible(nom: &str, editeur: &str) -> Cible {
        Cible { nom_court: noms::nom_court(nom), editeur_court: noms::editeur_court(editeur) }
    }

    #[test]
    fn sous_compare_sans_casse_et_par_dossier() {
        assert!(sous(Path::new("C:\\Program Files\\App\\bin"), Path::new("c:\\program files\\app")));
        assert!(sous(Path::new("C:\\Program Files\\App"), Path::new("C:\\Program Files\\App\\")));
        assert!(!sous(Path::new("C:\\Program Files\\Application"), Path::new("C:\\Program Files\\App")));
    }

    /// Un faux arbre : AppData\Snap Inc\Snap Camera (le reste), et un dossier voisin d'un
    /// programme encore installé (jamais proposé).
    #[test]
    fn la_fouille_trouve_le_reste_et_epargne_le_voisin() {
        let racine = std::env::temp_dir().join(format!("nettoyeur-restes-{}", std::process::id()));
        let _ = fs::remove_dir_all(&racine);
        let reste = racine.join("Snap Inc").join("Snap Camera");
        fs::create_dir_all(reste.join("cache")).unwrap();
        fs::write(reste.join("cache").join("x.bin"), vec![0u8; 100]).unwrap();
        let voisin = racine.join("Snap Camera Pro");
        fs::create_dir_all(&voisin).unwrap();
        let installe_ici = racine.join("Outil").join("Snap Camera");
        fs::create_dir_all(&installe_ici).unwrap();
        let installes = vec![Programme {
            cle: "HKCU\\x".into(), nom: "Snap Camera Pro".into(), editeur: String::new(), taille: None, date: None,
            dossier: Some(installe_ici.clone()), desinstaller: String::new(), icone_source: None,
            cible: cible("Snap Camera Pro", ""), nom_brut: "Snap Camera Pro".into(),
        }];
        let mut sortie = Vec::new();
        let c = cible("Snap Camera 1.20.0", "Snap Inc.");
        fouiller(&racine, &c, 0, &[racine.clone()], &[installe_ici.clone()], &installes, &mut sortie, 0);
        let f = finaliser(sortie);
        let chemins: Vec<String> = f.iter().map(|r| r.chemin.clone()).collect();
        assert!(chemins.iter().any(|p| p.eq_ignore_ascii_case(&reste.to_string_lossy())), "reste non trouvé : {chemins:?}");
        assert!(!chemins.iter().any(|p| p.eq_ignore_ascii_case(&installe_ici.to_string_lossy())), "dossier d'un programme installé proposé");
        let r = f.iter().find(|r| r.chemin.eq_ignore_ascii_case(&reste.to_string_lossy())).unwrap();
        assert_eq!(r.niveau(), Niveau::Sur, "{:?}", r.raisons);
        assert_eq!(r.taille, 100);
        let _ = fs::remove_dir_all(&racine);
    }

    #[test]
    fn les_dossiers_du_systeme_sont_interdits() {
        let pf = env("ProgramFiles").unwrap();
        for nom in ["ModifiableWindowsApps", "WindowsPowerShell", "Microsoft Update Health Tools", "Common Files"] {
            let p = pf.join(nom);
            if p.exists() {
                assert!(interdit(&p), "{nom} devrait être interdit");
            }
        }
        assert!(interdit(&windows().join("Temp")));
    }

    #[test]
    fn les_cles_systeme_et_les_racines_ne_sont_jamais_sures() {
        assert!(!suppression_sure(Genre::Cle, "HKLM\\SOFTWARE\\Microsoft", &[]));
        assert!(suppression_sure(Genre::Cle, "HKCU\\Software\\Snap Inc.", &[]));
        let pf = env("ProgramFiles").unwrap();
        assert!(!suppression_sure(Genre::Dossier, &pf.to_string_lossy(), &[]), "Program Files lui-même");
        assert!(!suppression_sure(Genre::Dossier, "C:\\Windows\\System32", &[]));
        assert!(!suppression_sure(Genre::Dossier, &profil().join("Documents").to_string_lossy(), &[]));
        assert!(!suppression_sure(Genre::Raccourci, "C:\\Windows\\notepad.exe", &[]));
    }

    /// Un raccourci n'est supprimable que dans les menus Démarrer ou sur les bureaux (en lecture
    /// seule : on ne fait que demander la règle, rien n'est supprimé).
    #[test]
    fn un_raccourci_hors_des_menus_n_est_jamais_sur() {
        let dossier = std::env::temp_dir().join(format!("nettoyeur-lnk-{}", std::process::id()));
        fs::create_dir_all(&dossier).unwrap();
        let ailleurs = dossier.join("Programme.lnk");
        fs::write(&ailleurs, b"L\0\0\0").unwrap();
        assert!(!suppression_sure(Genre::Raccourci, &ailleurs.to_string_lossy(), &[]));
        fs::remove_dir_all(&dossier).unwrap();
        if let Some(menu) = raccourcis::tous().into_iter().next() {
            assert!(suppression_sure(Genre::Raccourci, &menu.to_string_lossy(), &[]), "{}", menu.display());
        }
    }

    /// Sur cette machine, en lecture seule : les orphelins se calculent sans proposer de dossier
    /// où vit un programme installé.
    #[test]
    fn orphelins_reels_sans_programme_installe() {
        let installes = programmes::lister();
        let dossiers = dossiers_installes(&installes);
        for o in orphelins(&installes) {
            let p = Path::new(&o.chemin);
            assert!(!contient_une_installation(p, &dossiers), "{} contient une installation", o.chemin);
            assert!(o.niveau() != Niveau::Douteux);
        }
    }
}
