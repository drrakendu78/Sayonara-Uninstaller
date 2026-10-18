// SPDX-FileCopyrightText: © 2026 Djamel Lazreg (drrakendu78) <https://djamellazreg.fr>
// SPDX-License-Identifier: LicenseRef-PolyForm-Strict-1.0.0
//
// Les programmes installés, lus comme Windows (et Bulk Crap Uninstaller) les liste : les clés
// « Uninstall » de HKLM (64 et 32 bits) et de HKCU. Les composants système (runtimes, pilotes,
// SDK, Edge…) sont MASQUÉS : un ami qui les retire casse son PC.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::noms;
use crate::registre::{self, Ruche};

const UNINSTALL: &str = "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall";
const UNINSTALL_32: &str = "SOFTWARE\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Uninstall";

#[derive(Clone, Debug, Serialize)]
pub struct Programme {
    /// « HKLM\…\Uninstall\{GUID} » : son identité, et le témoin de sa présence
    pub cle: String,
    pub nom: String,
    pub editeur: String,
    pub taille: Option<u64>,
    /// AAAAMMJJ
    pub date: Option<String>,
    #[serde(skip)]
    pub dossier: Option<PathBuf>,
    #[serde(skip)]
    pub desinstaller: String,
    #[serde(skip)]
    pub icone_source: Option<String>,
    #[serde(skip)]
    pub cible: noms::Cible,
    /// le nom tel que déclaré (« Python 3.12.4 ») : deux versions restent deux lignes
    #[serde(skip)]
    pub nom_brut: String,
}

/// Les mots qui trahissent un composant dont Windows ou les jeux ont besoin.
const PROTEGES: [&str; 22] = [
    "redistributable", "runtime", "run time", " sdk", "software development kit", "driver", "pilote", "chipset",
    "visual c++", ".net", "directx", "microsoft edge", "webview2", "update health", "pc health check",
    "management engine", "amd software", "nvidia graphics", "nvidia physx", "realtek", "windows app runtime",
    "microsoft gameinput",
];

pub fn protege(nom: &str) -> bool {
    let n = format!(" {}", nom.to_lowercase());
    PROTEGES.iter().any(|m| n.contains(m)) || n.trim() == "nettoyeur"
}

fn date_valide(d: &str) -> Option<String> {
    let d = d.trim();
    if d.len() != 8 || !d.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let (a, m, j) = (d[..4].parse::<u32>().ok()?, d[4..6].parse::<u32>().ok()?, d[6..].parse::<u32>().ok()?);
    if (1990..=2100).contains(&a) && (1..=12).contains(&m) && (1..=31).contains(&j) { Some(d.to_string()) } else { None }
}

/// Tous les programmes qu'on peut proposer de désinstaller, triés par nom.
pub fn lister() -> Vec<Programme> {
    let mut liste: Vec<Programme> = Vec::new();
    for (ruche, racine) in [(Ruche::Hklm, UNINSTALL), (Ruche::Hklm, UNINSTALL_32), (Ruche::Hkcu, UNINSTALL)] {
        let Some(cle) = registre::ouvrir(ruche, racine) else { continue };
        for sous in cle.sous_cles() {
            let Some(c) = cle.ouvrir_sous(&sous) else { continue };
            let Some(nom) = c.texte("DisplayName") else { continue };
            let Some(desinstaller) = c.texte("UninstallString") else { continue };
            if c.nombre("SystemComponent") == Some(1) || c.nombre("NoRemove") == Some(1) || c.texte("ParentKeyName").is_some() {
                continue;
            }
            let genre = c.texte("ReleaseType").unwrap_or_default().to_lowercase();
            if ["update", "hotfix", "security update", "update rollup", "service pack"].contains(&genre.as_str()) || protege(&nom) {
                continue;
            }
            let editeur = c.texte("Publisher").unwrap_or_default();
            let dossier = c.texte("InstallLocation").map(|d| PathBuf::from(d.trim_matches('"').trim_end_matches('\\'))).filter(|d| d.is_absolute());
            liste.push(Programme {
                cle: format!("{}\\{}\\{}", ruche.prefixe(), racine, sous),
                cible: noms::Cible { nom_court: noms::nom_court(&nom), editeur_court: noms::editeur_court(&editeur) },
                nom: noms::nom_affiche(&nom),
                nom_brut: nom.clone(),
                editeur,
                taille: c.nombre("EstimatedSize").filter(|&k| k > 0).map(|k| k as u64 * 1024),
                date: c.texte("InstallDate").and_then(|d| date_valide(&d)),
                icone_source: c.texte("DisplayIcon"),
                dossier,
                desinstaller,
            });
        }
    }
    liste.sort_by(|a, b| a.nom.to_lowercase().cmp(&b.nom.to_lowercase()).then(a.nom_brut.cmp(&b.nom_brut)));
    // une même application déclarée deux fois (32 et 64 bits) : une seule ligne
    liste.dedup_by(|a, b| a.nom_brut.eq_ignore_ascii_case(&b.nom_brut) && a.editeur == b.editeur);
    // deux versions d'un même programme : le nom complet, pour les distinguer
    let noms: Vec<String> = liste.iter().map(|p| p.nom.to_lowercase()).collect();
    for (i, p) in liste.iter_mut().enumerate() {
        if noms.iter().filter(|n| **n == noms[i]).count() > 1 {
            p.nom = p.nom_brut.clone();
        }
    }
    liste
}

/// Le programme est-il toujours déclaré ? (sa clé Uninstall existe encore)
pub fn encore_installe(p: &Programme) -> bool {
    match registre::decouper(&p.cle) {
        Some((ruche, chemin)) => registre::existe(ruche, &chemin),
        None => false,
    }
}

/// Le dossier où vit le programme : InstallLocation, sinon celui de son icône, sinon celui de son
/// désinstalleur — jamais un dossier de Windows (msiexec, rundll32…), du Temp ou du cache des
/// installateurs.
pub fn dossier(p: &Programme) -> Option<PathBuf> {
    if let Some(d) = &p.dossier {
        return Some(d.clone());
    }
    let windows = std::env::var_os("SystemRoot").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("C:\\Windows"));
    let acceptable = |d: &Path| {
        let bas = d.to_string_lossy().to_lowercase();
        d.is_absolute() && d.components().count() > 2 && !bas.starts_with(&windows.to_string_lossy().to_lowercase())
            && !bas.contains("\\temp") && !bas.contains("package cache") && !bas.contains("\\installer")
    };
    if let Some(icone) = &p.icone_source {
        let fichier = icone.rsplit_once(',').map(|(f, _)| f).unwrap_or(icone).trim().trim_matches('"');
        if let Some(parent) = Path::new(fichier).parent().filter(|d| acceptable(d)) {
            return Some(parent.to_path_buf());
        }
    }
    let Commande::Programme { exe, .. } = commande(&p.desinstaller)? else { return None };
    exe.parent().filter(|d| acceptable(d)).map(Path::to_path_buf)
}

#[derive(Debug, PartialEq, Eq)]
pub enum Commande {
    /// un programme et ses arguments
    Programme { exe: PathBuf, args: String },
    /// une adresse à ouvrir (steam://uninstall/123)
    Adresse(String),
}

fn guid_msi(s: &str) -> Option<String> {
    let debut = s.find('{')?;
    let fin = s[debut..].find('}')? + debut;
    let g = &s[debut..=fin];
    (g.len() == 38 && g[1..37].chars().all(|c| c.is_ascii_hexdigit() || c == '-')).then(|| g.to_uppercase())
}

/// Lit la commande de désinstallation telle que Windows la stocke : entre guillemets ou non,
/// msiexec (toujours ramené à « /X{GUID} », la désinstallation avec son interface), adresse.
pub fn commande(brut: &str) -> Option<Commande> {
    let s = brut.trim();
    if s.is_empty() {
        return None;
    }
    let bas = s.to_lowercase();
    if bas.starts_with("steam://") || bas.starts_with("com.epicgames.launcher://") || bas.starts_with("uplay://") {
        return Some(Commande::Adresse(s.to_string()));
    }
    let systeme = std::env::var_os("SystemRoot").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("C:\\Windows")).join("System32");
    if bas.contains("msiexec") {
        let guid = guid_msi(s)?;
        return Some(Commande::Programme { exe: systeme.join("msiexec.exe"), args: format!("/X{guid}") });
    }
    if let Some(reste) = s.strip_prefix('"') {
        let fin = reste.find('"')?;
        let exe = PathBuf::from(&reste[..fin]);
        return Some(Commande::Programme { args: reste[fin + 1..].trim().to_string(), exe: resoudre(&exe, &systeme) });
    }
    // sans guillemets : le plus court préfixe qui finit par .exe et existe
    let mots: Vec<&str> = s.split(' ').collect();
    for i in 1..=mots.len() {
        let candidat = mots[..i].join(" ");
        if candidat.to_lowercase().ends_with(".exe") {
            let exe = resoudre(Path::new(&candidat), &systeme);
            if exe.is_file() {
                return Some(Commande::Programme { exe, args: mots[i..].join(" ") });
            }
        }
    }
    let exe = resoudre(Path::new(s), &systeme);
    exe.is_file().then(|| Commande::Programme { exe, args: String::new() })
}

/// « rundll32.exe » sans chemin → System32 ; sinon tel quel.
fn resoudre(exe: &Path, systeme: &Path) -> PathBuf {
    if exe.is_absolute() { exe.to_path_buf() } else { systeme.join(exe) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_composants_systeme_sont_masques() {
        for n in ["Microsoft Visual C++ 2015-2022 Redistributable (x64)", "Microsoft Windows Desktop Runtime 10.0.13 (x64)",
                  "Windows Software Development Kit - Windows 10.0.26100", "AMD Chipset Software", "Microsoft Edge",
                  "Microsoft Edge WebView2 Runtime", "AMD Software", "Vulkan Run Time Libraries"] {
            assert!(protege(n), "{n} devrait être masqué");
        }
        for n in ["Discord", "Blender", "Snap Camera", "RSI Launcher", "Deezer", "Docker Desktop"] {
            assert!(!protege(n), "{n} devrait être proposé");
        }
    }

    #[test]
    fn lecture_des_commandes() {
        let sys = std::env::var("SystemRoot").unwrap_or("C:\\Windows".into());
        assert_eq!(
            commande("MsiExec.exe /I{6A5E9B1C-1111-2222-3333-444455556666}"),
            Some(Commande::Programme { exe: PathBuf::from(format!("{sys}\\System32\\msiexec.exe")), args: "/X{6A5E9B1C-1111-2222-3333-444455556666}".into() })
        );
        assert_eq!(commande("steam://uninstall/440"), Some(Commande::Adresse("steam://uninstall/440".into())));
        assert_eq!(
            commande("\"C:\\Program Files\\Snap Camera\\unins000.exe\" /SILENT"),
            Some(Commande::Programme { exe: PathBuf::from("C:\\Program Files\\Snap Camera\\unins000.exe"), args: "/SILENT".into() })
        );
        // sans guillemets, avec des espaces : le préfixe qui existe vraiment
        let notepad = format!("{sys}\\System32\\notepad.exe");
        assert_eq!(commande(&format!("{notepad} /x")), Some(Commande::Programme { exe: PathBuf::from(&notepad), args: "/x".into() }));
        assert_eq!(commande("MsiExec.exe /X"), None, "msiexec sans GUID : rien");
        assert_eq!(commande(""), None);
    }

    #[test]
    fn dates_valides_seulement() {
        assert_eq!(date_valide("20261003"), Some("20261003".into()));
        assert_eq!(date_valide("20265008"), None, "mois 50 (vu chez Discord)");
        assert_eq!(date_valide("Tue Sep 8 2026"), None);
    }

    #[test]
    fn la_liste_reelle_se_lit() {
        let l = lister();
        assert!(!l.is_empty());
        assert!(l.iter().all(|p| !protege(&p.nom) && !p.cle.is_empty()));
    }
}
