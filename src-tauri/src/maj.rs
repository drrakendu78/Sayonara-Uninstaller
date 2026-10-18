// SPDX-FileCopyrightText: © 2026 Djamel Lazreg (drrakendu78) <https://djamellazreg.fr>
// SPDX-License-Identifier: LicenseRef-PolyForm-Strict-1.0.0
//
// Mises à jour du Nettoyeur depuis les publications GitHub de son dépôt : `latest.json` et
// installeur signé, publiés par scripts/publier-maj.mjs, lus à l'adresse de tauri.conf.json —
// l'interface ne donne jamais d'adresse. Repris d'Atreia Hub (update.rs).
//
// Le module de Tauri vérifie la signature (minisign) de l'installeur avec la clé publique de
// tauri.conf.json avant de le lancer, et `requireSignedVersion` refuse une signature faite pour une
// autre version que celle annoncée : un `latest.json` trafiqué ne peut ni faire installer un fichier
// étranger ni ramener une ancienne version. Les commandes du module ne sont pas ouvertes à
// l'interface (aucune permission « updater » dans capabilities) : seulement les deux ci-dessous.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, WebviewWindow};
use tauri_plugin_updater::{Error, Updater, UpdaterExt};

/// Une seule installation à la fois (double clic).
static EN_COURS: AtomicBool = AtomicBool::new(false);

fn module(app: &AppHandle) -> Result<Updater, String> {
    app.updater_builder().timeout(Duration::from_secs(20)).build().map_err(|e| e.to_string())
}

/// Une erreur lisible pour les cas qu'on rencontre vraiment.
fn lisible(e: Error) -> String {
    match e {
        Error::ReleaseNotFound => "Aucune version publiée.".into(),
        Error::Reqwest(_) | Error::Network(_) => "Serveur des mises à jour injoignable (réseau ?).".into(),
        Error::Minisign(_) | Error::Base64(_) | Error::SignatureUtf8(_) | Error::SignedVersionMismatch { .. } | Error::MissingSignedVersion => {
            "Signature de l'installeur invalide : mise à jour refusée.".into()
        }
        Error::TargetNotFound(_) | Error::TargetsNotFound(_) => "Pas d'installeur Windows dans la dernière version.".into(),
        Error::Serialization(_) | Error::Semver(_) => "Fichier de version illisible.".into(),
        e => e.to_string(),
    }
}

/// La dernière version publiée. En mode développement, rien ne part : chaque relance interrogerait
/// GitHub, et l'installeur remplacerait l'appli installée, pas celle-ci.
#[tauri::command]
pub async fn maj_verifier(app: AppHandle) -> Result<Value, String> {
    let actuelle = app.package_info().version.to_string();
    if cfg!(debug_assertions) {
        return Ok(json!({ "actuelle": actuelle, "disponible": false, "dev": true }));
    }
    match module(&app)?.check().await {
        Ok(Some(u)) => Ok(json!({
            "actuelle": actuelle,
            "disponible": true,
            "version": u.version,
            "notes": u.body.clone().unwrap_or_default(),
        })),
        // rien de publié (GitHub répond 404) : pas de mise à jour, pas une panne
        Ok(None) | Err(Error::ReleaseNotFound) => Ok(json!({ "actuelle": actuelle, "disponible": false })),
        Err(e) => Err(lisible(e)),
    }
}

/// Télécharge la dernière version, vérifie sa signature et lance son installeur (barre de
/// progression), qui relance ensuite le Nettoyeur : l'appli se ferme dès l'installeur lancé. Depuis
/// la fenêtre principale seulement, et jamais en mode développement.
#[tauri::command]
pub async fn maj_installer(app: AppHandle, window: WebviewWindow) -> Result<(), String> {
    if window.label() != "main" {
        return Err("Fenêtre inconnue.".into());
    }
    if cfg!(debug_assertions) {
        return Err("Mode développement : la mise à jour s'installe depuis l'appli installée.".into());
    }
    if EN_COURS.swap(true, Ordering::SeqCst) {
        return Err("Mise à jour déjà en cours.".into());
    }
    let r = installer(&app).await;
    // on n'arrive ici qu'en cas d'échec : réussie, l'installation ferme l'appli
    EN_COURS.store(false, Ordering::SeqCst);
    r
}

async fn installer(app: &AppHandle) -> Result<(), String> {
    let maj = module(app)?.check().await.map_err(lisible)?.ok_or_else(|| "Aucune mise à jour à installer.".to_string())?;
    let version = maj.version.clone();
    let (mut recu, mut dernier) = (0u64, Instant::now());
    let _ = app.emit("maj-progression", json!({ "version": version, "recu": 0, "total": null }));
    let octets = maj
        .download(
            |n, total| {
                recu += n as u64;
                if dernier.elapsed() >= Duration::from_millis(120) || Some(recu) == total {
                    dernier = Instant::now();
                    let _ = app.emit("maj-progression", json!({ "version": version, "recu": recu, "total": total }));
                }
            },
            || {
                let _ = app.emit("maj-progression", json!({ "version": version, "phase": "installation" }));
            },
        )
        .await
        .map_err(lisible)?;
    maj.install(octets).map_err(lisible)
}

#[cfg(test)]
mod tests {
    use tauri_plugin_updater::RemoteRelease;

    /// Le latest.json que publie scripts/publier-maj.mjs, lu par le module de Tauri lui-même :
    /// version, date, adresse et signature de la plateforme Windows (clé `windows-x86_64`).
    #[test]
    fn latest_json_lisible_par_le_module() {
        let url = "https://github.com/drrakendu78/Nettoyeur/releases/download/v0.2.0/Nettoyeur_0.2.0_x64-setup.exe";
        let v = serde_json::json!({
            "version": "0.2.0",
            "notes": "Ligne 1\nLigne 2",
            "pub_date": "2026-10-18T21:00:00Z",
            "platforms": { "windows-x86_64": { "signature": "c2ln", "url": url } }
        });
        let r: RemoteRelease = serde_json::from_value(v).expect("format refusé par le module");
        assert_eq!(r.version.to_string(), "0.2.0");
        assert!(r.pub_date.is_some());
        assert_eq!(r.notes.as_deref(), Some("Ligne 1\nLigne 2"));
        assert_eq!(r.download_url("windows-x86_64").unwrap().as_str(), url);
        assert_eq!(r.signature("windows-x86_64").unwrap(), "c2ln");
    }

    /// La configuration qui rend les mises à jour sûres ne doit pas disparaître en route : adresse
    /// des publications du dépôt en HTTPS, clé publique, signature liée à la version, artefacts signés.
    #[test]
    fn configuration_des_mises_a_jour() {
        let conf: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let maj = &conf["plugins"]["updater"];
        assert_eq!(maj["endpoints"], serde_json::json!(["https://github.com/drrakendu78/Nettoyeur/releases/latest/download/latest.json"]));
        assert_eq!(maj["requireSignedVersion"], true);
        assert_eq!(maj["windows"]["installMode"], "passive");
        let cle = maj["pubkey"].as_str().unwrap();
        assert!(cle.len() > 100 && !cle.contains("PRIVATE") && !cle.contains("secret"));
        assert_eq!(conf["bundle"]["createUpdaterArtifacts"], true);
        // aucune permission du module n'est donnée à l'interface : elle passe par nos commandes
        let capacites = include_str!("../capabilities/default.json");
        assert!(!capacites.contains("updater"), "l'interface ne doit pas piloter le module directement");
    }
}
