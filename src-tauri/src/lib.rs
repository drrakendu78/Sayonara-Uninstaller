// SPDX-FileCopyrightText: © 2026 Djamel Lazreg (drrakendu78) <https://djamellazreg.fr>
// SPDX-License-Identifier: LicenseRef-PolyForm-Strict-1.0.0
//
// Nettoyeur — les commandes appelées par l'interface. Le contrat tient en une règle : le front
// n'envoie QUE des identifiants de catégorie (et, pour montrer un fichier, un numéro dans la
// liste que l'analyse a gardée ici). Aucun chemin ne vient jamais de l'interface.

mod categories;
mod nettoyage;
mod systeme;

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::SystemTime;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use categories::{Fichier, Mesure};

pub use nettoyage::nettoyer_admin;

/// Ce que la dernière analyse a trouvé et que le nettoyage reprendra tel quel.
#[derive(Default)]
struct Analyse {
    install: Vec<Fichier>,
    gros: Vec<Fichier>,
    /// octets mesurés dans les dossiers qui demandent l'administrateur, par catégorie
    admin_temp: u64,
    admin_maj: u64,
}

#[derive(Default)]
struct Etat {
    analyse: Mutex<Option<Analyse>>,
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
    Ok(Disque {
        total: e.total,
        libre: e.libre,
    })
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
    *etat.analyse.lock().map_err(|_| "État indisponible")? = Some(analyse);
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
                let liste = if id == "install" {
                    &analyse.install
                } else {
                    &analyse.gros
                };
                Mesure {
                    taille: liste.iter().map(|f| f.taille).sum(),
                    nombre: liste.len() as u64,
                }
            }
            _ => {
                let mut total = Mesure::default();
                for r in categories::racines(id) {
                    let m = categories::mesurer(&r.chemin, maintenant);
                    if r.admin {
                        if id == "temp" {
                            analyse.admin_temp += m.taille
                        } else {
                            analyse.admin_maj += m.taille
                        }
                    }
                    total += m;
                }
                total
            }
        };
        resumes.push(Resume {
            id,
            taille: m.taille,
            nombre: m.nombre,
        });
    }
    (resumes, analyse)
}

/// La liste d'une catégorie « à vérifier », pour la montrer AVANT de nettoyer.
#[tauri::command]
fn lister(id: String, etat: State<'_, Etat>) -> Result<Vec<Ligne>, String> {
    let garde = etat.analyse.lock().map_err(|_| "État indisponible")?;
    let analyse = garde.as_ref().ok_or("Lancez d'abord l'analyse")?;
    let liste = match id.as_str() {
        "install" => &analyse.install,
        "gros" => &analyse.gros,
        _ => return Err("Catégorie inconnue".into()),
    };
    let maintenant = SystemTime::now();
    Ok(liste
        .iter()
        .enumerate()
        .map(|(index, f)| Ligne {
            index,
            nom: f
                .chemin
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default(),
            taille: f.taille,
            jours: maintenant
                .duration_since(f.derniere)
                .map(|d| d.as_secs() / 86_400)
                .unwrap_or(0),
        })
        .collect())
}

/// Montre un fichier « à vérifier » dans l'Explorateur — pris dans la liste de l'analyse, jamais
/// un chemin reçu.
#[tauri::command]
fn montrer(id: String, index: usize, etat: State<'_, Etat>) -> Result<(), String> {
    let garde = etat.analyse.lock().map_err(|_| "État indisponible")?;
    let analyse = garde.as_ref().ok_or("Lancez d'abord l'analyse")?;
    let liste = match id.as_str() {
        "install" => &analyse.install,
        "gros" => &analyse.gros,
        _ => return Err("Catégorie inconnue".into()),
    };
    let f = liste.get(index).ok_or("Fichier introuvable")?;
    if !f.chemin.is_file() || !systeme::montrer_dans_explorateur(&f.chemin) {
        return Err("Fichier introuvable".into());
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
            (categories::categorie_du_telechargement(&meta, &f.chemin, maintenant) == Some(id))
                .then(|| (f.chemin.clone(), meta.len()))
        })
        .collect()
}

#[tauri::command]
async fn nettoyer(ids: Vec<String>, etat: State<'_, Etat>) -> Result<Bilan, String> {
    if ids.is_empty() || ids.iter().any(|i| !categories::connu(i)) {
        return Err("Catégorie inconnue".into());
    }
    // ce que l'analyse a trouvé : le nettoyage ne reprend QUE ça (et le revérifie)
    let analyse = etat
        .analyse
        .lock()
        .map_err(|_| "État indisponible")?
        .take()
        .ok_or("Lancez d'abord l'analyse")?;
    let veut = |id: &str| ids.iter().any(|i| i == id);
    let (temp, corbeille, maj, nav, jeux, install, gros) = (
        veut("temp"),
        veut("corbeille"),
        veut("maj"),
        veut("nav"),
        veut("jeux"),
        veut("install"),
        veut("gros"),
    );

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
        // 3. Windows\Temp et les restes des mises à jour : UNE demande d'autorisation, et
        //    seulement s'il y a quelque chose à y gagner
        let mut admin: Vec<&str> = Vec::new();
        if temp && analyse.admin_temp > 0 {
            admin.push("temp");
        }
        if maj && analyse.admin_maj > 0 {
            admin.push("maj");
        }
        let mut admin_refuse = false;
        if !admin.is_empty() {
            admin_refuse = matches!(
                systeme::relancer_eleve(&format!("--nettoyer-admin {}", admin.join(","))),
                systeme::Eleve::Refuse
            );
        }
        // 4. les « à vérifier » : à la Corbeille, jamais supprimés pour de bon
        let mut a_envoyer: Vec<(PathBuf, u64)> = Vec::new();
        if install {
            a_envoyer.extend(encore_valables(&analyse.install, "install"));
        }
        if gros {
            a_envoyer.extend(encore_valables(&analyse.gros, "gros"));
        }
        let mut vers_corbeille = 0u64;
        if !a_envoyer.is_empty() {
            let chemins: Vec<PathBuf> = a_envoyer.iter().map(|(p, _)| p.clone()).collect();
            systeme::vers_corbeille(&chemins);
            vers_corbeille = a_envoyer
                .iter()
                .filter(|(p, _)| !p.exists())
                .map(|(_, t)| t)
                .sum();
        }

        let apres = systeme::espace(&c).ok_or("Impossible de lire le disque C:")?;
        Ok(Bilan {
            avant: avant.libre,
            apres: apres.libre,
            total: apres.total,
            vers_corbeille,
            admin_refuse,
        })
    })
    .await
    .map_err(|_| "Le nettoyage s'est arrêté".to_string())?
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
            nettoyer
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
            println!(
                "{:<10} {:>9.2} Go {:>9} fichiers",
                r.id,
                r.taille as f64 / (1u64 << 30) as f64,
                r.nombre
            );
        }
        println!(
            "admin : Windows\\Temp {} o, mises à jour {} o ; installateurs {}, gros {} ; en {:?}",
            analyse.admin_temp,
            analyse.admin_maj,
            analyse.install.len(),
            analyse.gros.len(),
            debut.elapsed()
        );
        let d = infos_disque().unwrap();
        println!(
            "disque C: {} Go libres sur {} Go",
            d.libre >> 30,
            d.total >> 30
        );
    }
}
