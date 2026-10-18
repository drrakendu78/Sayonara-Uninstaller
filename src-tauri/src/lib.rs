// SPDX-FileCopyrightText: © 2026 Djamel Lazreg (drrakendu78) <https://djamellazreg.fr>
// SPDX-License-Identifier: LicenseRef-PolyForm-Strict-1.0.0
//
// Nettoyeur — les commandes appelées par l'interface. Le contrat tient en une règle : le front
// n'envoie QUE des identifiants (catégories) et des NUMÉROS dans les listes gardées ici (fichiers
// d'une catégorie, programmes, restes). Aucun chemin ne vient jamais de l'interface.

mod admin;
mod categories;
mod desinstallation;
mod icones;
mod nettoyage;
mod noms;
mod programmes;
mod raccourcis;
mod registre;
mod restes;
mod systeme;

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use categories::{Fichier, Mesure};
use programmes::Programme;
use restes::{Genre, Reste};

pub use admin::programme_eleve;

/// Ce que la dernière analyse a trouvé et que le nettoyage reprendra tel quel.
#[derive(Default)]
struct Analyse {
    install: Vec<Fichier>,
    gros: Vec<Fichier>,
    orphelins: Vec<Reste>,
    /// octets mesurés dans les dossiers qui demandent l'administrateur, par catégorie
    admin_temp: u64,
    admin_maj: u64,
}

/// Un programme retiré pendant cette session, tel qu'on le connaissait avant.
struct RetireInfo {
    nom: String,
    icone: Option<String>,
    retire: restes::Retire,
}

#[derive(Default)]
struct Etat {
    analyse: Mutex<Option<Analyse>>,
    programmes: Mutex<Vec<Programme>>,
    retires: Mutex<Vec<RetireInfo>>,
    restes: Mutex<Vec<Reste>>,
}

fn verrou<T>(m: &Mutex<T>) -> Result<std::sync::MutexGuard<'_, T>, String> {
    m.lock().map_err(|_| "État indisponible".to_string())
}

#[derive(Serialize)]
struct Disque {
    total: u64,
    libre: u64,
}

#[derive(Serialize, Clone)]
struct Resume {
    id: &'static str,
    taille: u64,
    nombre: u64,
}

#[derive(Serialize)]
struct Ligne {
    index: usize,
    nom: String,
    /// où il se trouve, pour les restes orphelins (« Program Files »)
    lieu: Option<String>,
    taille: u64,
    /// jours depuis la dernière utilisation
    jours: u64,
}

#[derive(Serialize)]
struct Bilan {
    /// place libre sur C: avant et après, et la taille du disque
    avant: u64,
    apres: u64,
    total: u64,
    /// octets envoyés à la Corbeille (les « à vérifier ») : pas encore libérés
    vers_corbeille: u64,
    /// l'utilisateur a refusé l'autorisation administrateur
    admin_refuse: bool,
}

#[tauri::command]
fn infos_disque() -> Result<Disque, String> {
    let e = systeme::espace(&systeme::disque_systeme()).ok_or("Impossible de lire le disque C:")?;
    Ok(Disque { total: e.total, libre: e.libre })
}

#[tauri::command]
async fn analyser(app: AppHandle, etat: State<'_, Etat>) -> Result<Vec<Resume>, String> {
    let (resumes, analyse) = tauri::async_runtime::spawn_blocking(move || {
        analyse_complete(|id| {
            let _ = app.emit("analyse-etape", id);
        })
    })
    .await
    .map_err(|_| "L'analyse s'est arrêtée".to_string())?;
    *verrou(&etat.analyse)? = Some(analyse);
    Ok(resumes)
}

/// L'analyse elle-même : elle ne fait que LIRE (tailles, dates). `etape` annonce chaque catégorie.
fn analyse_complete(etape: impl Fn(&'static str)) -> (Vec<Resume>, Analyse) {
    let maintenant = SystemTime::now();
    let mut analyse = Analyse::default();
    let mut resumes = Vec::new();
    for id in categories::IDS {
        etape(id);
        let m = match id {
            "corbeille" => {
                let (taille, nombre) = systeme::corbeille(&systeme::disque_systeme());
                Mesure { taille, nombre }
            }
            "install" | "gros" => {
                if analyse.install.is_empty() && analyse.gros.is_empty() {
                    if let Some(dl) = systeme::telechargements() {
                        let (i, g) = categories::telechargements(&dl, maintenant);
                        analyse.install = i;
                        analyse.gros = g;
                    }
                }
                let liste = if id == "install" { &analyse.install } else { &analyse.gros };
                Mesure { taille: liste.iter().map(|f| f.taille).sum(), nombre: liste.len() as u64 }
            }
            "orphelins" => {
                analyse.orphelins = restes::orphelins(&programmes::lister());
                Mesure { taille: analyse.orphelins.iter().map(|r| r.taille).sum(), nombre: analyse.orphelins.len() as u64 }
            }
            _ => {
                let mut total = Mesure::default();
                for r in categories::racines(id) {
                    let m = categories::mesurer(&r.chemin, maintenant);
                    if r.admin {
                        if id == "temp" { analyse.admin_temp += m.taille } else { analyse.admin_maj += m.taille }
                    }
                    total += m;
                }
                total
            }
        };
        resumes.push(Resume { id, taille: m.taille, nombre: m.nombre });
    }
    (resumes, analyse)
}

/// La liste d'une catégorie « à vérifier », pour la montrer AVANT de nettoyer.
#[tauri::command]
fn lister(id: String, etat: State<'_, Etat>) -> Result<Vec<Ligne>, String> {
    let garde = verrou(&etat.analyse)?;
    let analyse = garde.as_ref().ok_or("Lancez d'abord l'analyse")?;
    let maintenant = SystemTime::now();
    let fichiers = match id.as_str() {
        "install" => &analyse.install,
        "gros" => &analyse.gros,
        "orphelins" => {
            return Ok(analyse
                .orphelins
                .iter()
                .enumerate()
                .map(|(index, r)| {
                    let p = Path::new(&r.chemin);
                    let parent = Reste { chemin: p.parent().map(|x| x.to_string_lossy().to_string()).unwrap_or_default(), ..r.clone() };
                    Ligne { index, nom: p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(), lieu: Some(restes::affichage(&parent)), taille: r.taille, jours: 0 }
                })
                .collect());
        }
        _ => return Err("Catégorie inconnue".into()),
    };
    Ok(fichiers
        .iter()
        .enumerate()
        .map(|(index, f)| Ligne {
            index,
            nom: f.chemin.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
            lieu: None,
            taille: f.taille,
            jours: maintenant.duration_since(f.derniere).map(|d| d.as_secs() / 86_400).unwrap_or(0),
        })
        .collect())
}

/// Montre un élément « à vérifier » dans l'Explorateur — pris dans la liste de l'analyse, jamais
/// un chemin reçu.
#[tauri::command]
fn montrer(id: String, index: usize, etat: State<'_, Etat>) -> Result<(), String> {
    let garde = verrou(&etat.analyse)?;
    let analyse = garde.as_ref().ok_or("Lancez d'abord l'analyse")?;
    let chemin: PathBuf = match id.as_str() {
        "install" => analyse.install.get(index).map(|f| f.chemin.clone()),
        "gros" => analyse.gros.get(index).map(|f| f.chemin.clone()),
        "orphelins" => analyse.orphelins.get(index).map(|r| PathBuf::from(&r.chemin)),
        _ => None,
    }
    .ok_or("Élément introuvable")?;
    if !chemin.exists() || !systeme::montrer_dans_explorateur(&chemin) {
        return Err("Élément introuvable".into());
    }
    Ok(())
}

/// Les « à vérifier » encore valables au moment de nettoyer : même dossier, même règle, toujours
/// là, pas devenus des liens. Un fichier rouvert entre-temps n'est plus « oublié ».
fn encore_valables(liste: &[Fichier], id: &str) -> Vec<(PathBuf, u64)> {
    let maintenant = SystemTime::now();
    let dossier = systeme::telechargements();
    liste
        .iter()
        .filter(|f| f.chemin.parent() == dossier.as_deref())
        .filter_map(|f| {
            let meta = std::fs::symlink_metadata(&f.chemin).ok()?;
            (categories::categorie_du_telechargement(&meta, &f.chemin, maintenant) == Some(id)).then(|| (f.chemin.clone(), meta.len()))
        })
        .collect()
}

#[tauri::command]
async fn nettoyer(ids: Vec<String>, etat: State<'_, Etat>) -> Result<Bilan, String> {
    if ids.is_empty() || ids.iter().any(|i| !categories::connu(i)) {
        return Err("Catégorie inconnue".into());
    }
    // ce que l'analyse a trouvé : le nettoyage ne reprend QUE ça (et le revérifie)
    let analyse = verrou(&etat.analyse)?.take().ok_or("Lancez d'abord l'analyse")?;
    let veut = |id: &str| ids.iter().any(|i| i == id);
    let (temp, corbeille, maj, nav, jeux, install, gros, orphelins) =
        (veut("temp"), veut("corbeille"), veut("maj"), veut("nav"), veut("jeux"), veut("install"), veut("gros"), veut("orphelins"));

    tauri::async_runtime::spawn_blocking(move || {
        let c = systeme::disque_systeme();
        let avant = systeme::espace(&c).ok_or("Impossible de lire le disque C:")?;
        let maintenant = SystemTime::now();

        // 1. la Corbeille D'ABORD : les « à vérifier » y entrent ensuite et y restent récupérables
        if corbeille {
            systeme::vider_corbeille(&c);
        }
        // 2. ce qui se fait sans autorisation
        for (id, oui) in [("temp", temp), ("nav", nav), ("jeux", jeux)] {
            if oui {
                for r in categories::racines(id).into_iter().filter(|r| !r.admin) {
                    nettoyage::vider(&r.chemin, maintenant);
                }
            }
        }
        // 3. les « à vérifier » de Téléchargements et les orphelins accessibles : à la Corbeille
        let mut a_envoyer: Vec<(PathBuf, u64)> = Vec::new();
        if install {
            a_envoyer.extend(encore_valables(&analyse.install, "install"));
        }
        if gros {
            a_envoyer.extend(encore_valables(&analyse.gros, "gros"));
        }
        let mut plan = admin::Plan::default();
        if orphelins {
            let installes = programmes::lister();
            for r in analyse.orphelins.iter().filter(|r| restes::suppression_sure(Genre::Dossier, &r.chemin, &installes)) {
                if r.admin() { plan.dossiers.push(r.chemin.clone()) } else { a_envoyer.push((PathBuf::from(&r.chemin), r.taille)) }
            }
        }
        if !a_envoyer.is_empty() {
            let chemins: Vec<PathBuf> = a_envoyer.iter().map(|(p, _)| p.clone()).collect();
            systeme::vers_corbeille(&chemins);
        }
        // 4. Windows\Temp, les mises à jour, les orphelins de Program Files : UNE demande
        //    d'autorisation, et seulement s'il y a quelque chose à y gagner
        if temp && analyse.admin_temp > 0 {
            plan.categories.push("temp".into());
        }
        if maj && analyse.admin_maj > 0 {
            plan.categories.push("maj".into());
        }
        let admin_dossiers: Vec<(PathBuf, u64)> = analyse.orphelins.iter().filter(|r| plan.dossiers.contains(&r.chemin)).map(|r| (PathBuf::from(&r.chemin), r.taille)).collect();
        let mut admin_refuse = false;
        if !plan.categories.is_empty() || !plan.dossiers.is_empty() {
            admin_refuse = matches!(admin::executer(&plan), systeme::Eleve::Refuse);
        }
        let vers_corbeille = a_envoyer.iter().chain(admin_dossiers.iter()).filter(|(p, _)| !p.exists()).map(|(_, t)| t).sum();

        let apres = systeme::espace(&c).ok_or("Impossible de lire le disque C:")?;
        Ok(Bilan { avant: avant.libre, apres: apres.libre, total: apres.total, vers_corbeille, admin_refuse })
    })
    .await
    .map_err(|_| "Le nettoyage s'est arrêté".to_string())?
}

/* ── Programmes : désinstaller, puis retirer ce qu'ils ont laissé ── */

#[derive(Serialize)]
struct ProgrammeUi {
    index: usize,
    nom: String,
    editeur: String,
    taille: Option<u64>,
    date: Option<String>,
    icone: Option<String>,
}

fn icone_de(p: &Programme) -> Option<String> {
    p.icone_source.as_deref().and_then(icones::icone).or_else(|| match programmes::commande(&p.desinstaller) {
        Some(programmes::Commande::Programme { exe, .. }) => icones::icone(&exe.to_string_lossy()),
        _ => None,
    })
}

#[tauri::command]
async fn programmes(etat: State<'_, Etat>) -> Result<Vec<ProgrammeUi>, String> {
    let (liste, ui) = tauri::async_runtime::spawn_blocking(|| {
        let liste = programmes::lister();
        let ui: Vec<ProgrammeUi> = liste
            .iter()
            .enumerate()
            .map(|(index, p)| ProgrammeUi { index, nom: p.nom.clone(), editeur: p.editeur.clone(), taille: p.taille, date: p.date.clone(), icone: icone_de(p) })
            .collect();
        (liste, ui)
    })
    .await
    .map_err(|_| "La lecture des programmes s'est arrêtée".to_string())?;
    *verrou(&etat.programmes)? = liste;
    Ok(ui)
}

#[derive(Serialize, Clone)]
struct EtapeDesinstallation {
    index: usize,
    etat: desinstallation::Issue,
}

#[tauri::command]
async fn desinstaller(app: AppHandle, indexes: Vec<usize>, etat: State<'_, Etat>) -> Result<Vec<EtapeDesinstallation>, String> {
    let choisis: Vec<(usize, Programme)> = {
        let liste = verrou(&etat.programmes)?;
        indexes.iter().map(|&i| liste.get(i).cloned().map(|p| (i, p)).ok_or("Programme inconnu")).collect::<Result<_, _>>()?
    };
    if choisis.is_empty() {
        return Err("Aucun programme choisi".into());
    }
    let (etapes, retires) = tauri::async_runtime::spawn_blocking(move || {
        let mut etapes = Vec::new();
        let mut retires = Vec::new();
        for (i, p) in choisis {
            // ce qu'on sait de lui AVANT qu'il parte : c'est ce qui permettra de reconnaître ses restes
            let avant = RetireInfo { nom: p.nom.clone(), icone: icone_de(&p), retire: restes::Retire { cible: p.cible.clone(), dossier: programmes::dossier(&p) } };
            let _ = app.emit("desinstallation", serde_json::json!({ "index": i, "etat": "en_cours" }));
            let issue = desinstallation::desinstaller(&p);
            let _ = app.emit("desinstallation", EtapeDesinstallation { index: i, etat: issue });
            if issue == desinstallation::Issue::Retire {
                retires.push(avant);
            }
            etapes.push(EtapeDesinstallation { index: i, etat: issue });
        }
        (etapes, retires)
    })
    .await
    .map_err(|_| "La désinstallation s'est arrêtée".to_string())?;
    *verrou(&etat.retires)? = retires;
    Ok(etapes)
}

/// « Passer » : ne plus attendre le désinstalleur en cours.
#[tauri::command]
fn passer() {
    desinstallation::PASSER.store(true, std::sync::atomic::Ordering::SeqCst);
}

#[derive(Serialize)]
struct ResteUi {
    index: usize,
    /// le programme (dans `programmes` de la réponse)
    programme: usize,
    genre: Genre,
    chemin: String,
    taille: u64,
    nombre: u64,
    niveau: noms::Niveau,
}

#[derive(Serialize)]
struct ProgrammeRetire {
    nom: String,
    icone: Option<String>,
}

#[derive(Serialize)]
struct Restes {
    programmes: Vec<ProgrammeRetire>,
    restes: Vec<ResteUi>,
}

#[tauri::command]
async fn chercher_restes(etat: State<'_, Etat>) -> Result<Restes, String> {
    let (cibles, programmes_ui): (Vec<restes::Retire>, Vec<ProgrammeRetire>) = {
        let r = verrou(&etat.retires)?;
        (
            r.iter().map(|x| restes::Retire { cible: x.retire.cible.clone(), dossier: x.retire.dossier.clone() }).collect(),
            r.iter().map(|x| ProgrammeRetire { nom: x.nom.clone(), icone: x.icone.clone() }).collect(),
        )
    };
    let trouves = tauri::async_runtime::spawn_blocking(move || restes::chercher(&cibles, &programmes::lister()))
        .await
        .map_err(|_| "La recherche s'est arrêtée".to_string())?;
    let ui = trouves
        .iter()
        .enumerate()
        .map(|(index, r)| ResteUi { index, programme: r.programme.unwrap_or(0), genre: r.genre, chemin: restes::affichage(r), taille: r.taille, nombre: r.nombre, niveau: r.niveau() })
        .collect();
    *verrou(&etat.restes)? = trouves;
    Ok(Restes { programmes: programmes_ui, restes: ui })
}

#[tauri::command]
fn montrer_reste(index: usize, etat: State<'_, Etat>) -> Result<(), String> {
    let liste = verrou(&etat.restes)?;
    let r = liste.get(index).ok_or("Élément introuvable")?;
    let p = Path::new(&r.chemin);
    if r.genre == Genre::Cle || !p.exists() || !systeme::montrer_dans_explorateur(p) {
        return Err("Élément introuvable".into());
    }
    Ok(())
}

#[derive(Serialize)]
struct BilanRestes {
    programmes: usize,
    dossiers: usize,
    cles: usize,
    raccourcis: usize,
    octets: u64,
    admin_refuse: bool,
    sauvegardes: Option<String>,
}

#[tauri::command]
async fn supprimer_restes(indexes: Vec<usize>, etat: State<'_, Etat>) -> Result<BilanRestes, String> {
    let choisis: Vec<Reste> = {
        let liste = verrou(&etat.restes)?;
        indexes.iter().map(|&i| liste.get(i).cloned().ok_or("Élément inconnu")).collect::<Result<_, _>>()?
    };
    let nb_programmes = verrou(&etat.retires)?.len();
    tauri::async_runtime::spawn_blocking(move || {
        let installes = programmes::lister();
        let surs: Vec<Reste> = choisis.into_iter().filter(|r| restes::suppression_sure(r.genre, &r.chemin, &installes)).collect();
        let mut plan = admin::Plan::default();
        let mut corbeille: Vec<PathBuf> = Vec::new();
        let mut sauvegardes: Option<PathBuf> = None;
        for r in &surs {
            match (r.genre, r.admin()) {
                (Genre::Cle, true) => plan.cles.push(r.chemin.clone()),
                (Genre::Cle, false) => {
                    if sauvegardes.is_none() {
                        sauvegardes = admin::dossier_sauvegardes();
                    }
                    if let Some(d) = &sauvegardes {
                        admin::retirer_cle(&r.chemin, d);
                    }
                }
                (Genre::Raccourci, true) => plan.raccourcis.push(r.chemin.clone()),
                (Genre::Dossier, true) => plan.dossiers.push(r.chemin.clone()),
                (_, false) => corbeille.push(PathBuf::from(&r.chemin)),
            }
        }
        if !corbeille.is_empty() {
            systeme::vers_corbeille(&corbeille);
        }
        let mut admin_refuse = false;
        if !plan.dossiers.is_empty() || !plan.raccourcis.is_empty() || !plan.cles.is_empty() {
            admin_refuse = matches!(admin::executer(&plan), systeme::Eleve::Refuse);
        }
        // le bilan se lit sur le disque et dans le registre, pas sur nos intentions
        let parti = |r: &Reste| match r.genre {
            Genre::Cle => registre::decouper(&r.chemin).map(|(ru, ch)| !registre::existe(ru, &ch)).unwrap_or(false),
            _ => !Path::new(&r.chemin).exists(),
        };
        let compte = |g: Genre| surs.iter().filter(|r| r.genre == g && parti(r)).count();
        let octets = surs.iter().filter(|r| r.genre != Genre::Cle && parti(r)).map(|r| r.taille).sum();
        if !plan.cles.is_empty() && sauvegardes.is_none() {
            sauvegardes = systeme::documents().map(|d| d.join("Nettoyeur").join("Sauvegardes"));
        }
        Ok(BilanRestes {
            programmes: nb_programmes,
            dossiers: compte(Genre::Dossier),
            cles: compte(Genre::Cle),
            raccourcis: compte(Genre::Raccourci),
            octets,
            admin_refuse,
            sauvegardes: sauvegardes.map(|d| restes::affichage(&Reste { genre: Genre::Dossier, chemin: d.to_string_lossy().to_string(), raisons: vec![], taille: 0, nombre: 0, programme: None })),
        })
    })
    .await
    .map_err(|_| "La suppression s'est arrêtée".to_string())?
}

/// Barre de titre maison : réduire, agrandir, fermer passent par l'API de fenêtre de Tauri
/// (capacités dans capabilities/default.json).
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(Etat::default())
        .invoke_handler(tauri::generate_handler![
            infos_disque,
            analyser,
            lister,
            montrer,
            nettoyer,
            programmes,
            desinstaller,
            passer,
            chercher_restes,
            montrer_reste,
            supprimer_restes
        ])
        .run(tauri::generate_context!())
        .expect("Nettoyeur n'a pas pu démarrer");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sur la VRAIE machine, en LECTURE SEULE (rien n'est supprimé) :
    /// `cargo test -- --ignored --nocapture analyse_reelle`
    #[test]
    #[ignore]
    fn analyse_reelle() {
        let debut = std::time::Instant::now();
        let (resumes, analyse) = analyse_complete(|_| {});
        for r in &resumes {
            println!("{:<10} {:>9.2} Go {:>9} éléments", r.id, r.taille as f64 / (1u64 << 30) as f64, r.nombre);
        }
        println!(
            "admin : Windows\\Temp {} o, mises à jour {} o ; installateurs {}, gros {}, orphelins {} ; en {:?}",
            analyse.admin_temp, analyse.admin_maj, analyse.install.len(), analyse.gros.len(), analyse.orphelins.len(), debut.elapsed()
        );
        for o in analyse.orphelins.iter().take(15) {
            println!("   orphelin {:?} {:>8} Ko  {}", o.niveau(), o.taille / 1024, o.chemin);
        }
        let d = infos_disque().unwrap();
        println!("disque C: {} Go libres sur {} Go", d.libre >> 30, d.total >> 30);
    }

    /// Sur la VRAIE machine, en LECTURE SEULE : les restes qu'on trouverait à un programme
    /// INSTALLÉ si on le retirait (ses dossiers sont encore là). `NETTOYEUR_PROGRAMME=Blender`.
    #[test]
    #[ignore]
    fn restes_reels_d_un_programme_installe() {
        let nom = std::env::var("NETTOYEUR_PROGRAMME").unwrap_or_else(|_| "Blender".into());
        let tous = programmes::lister();
        let p = tous.iter().find(|p| p.nom.to_lowercase().contains(&nom.to_lowercase())).expect("programme introuvable").clone();
        let autres: Vec<Programme> = tous.into_iter().filter(|x| x.cle != p.cle).collect();
        let debut = std::time::Instant::now();
        let r = restes::chercher(&[restes::Retire { cible: p.cible.clone(), dossier: programmes::dossier(&p) }], &autres);
        println!("{} ({:?}, {:?}) : {} restes en {:?}", p.nom, p.cible, programmes::dossier(&p), r.len(), debut.elapsed());
        for x in &r {
            println!("   {:?} {:?} {:>8} Ko  {}  {:?}", x.genre, x.niveau(), x.taille / 1024, x.chemin, x.raisons);
        }
    }
}
