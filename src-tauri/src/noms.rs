// SPDX-FileCopyrightText: © 2026 Djamel Lazreg (drrakendu78) <https://djamellazreg.fr>
// SPDX-FileCopyrightText: © 2017 Marcin Szeniak (Bulk Crap Uninstaller)
// SPDX-License-Identifier: LicenseRef-PolyForm-Strict-1.0.0 AND Apache-2.0
//
// Reconnaître les restes d'un programme à leur NOM — logique portée de Bulk Crap Uninstaller
// (© 2017 Marcin Szeniak, Apache License 2.0 — cf. LICENCES-TIERS/BCU.txt) : StringTools.
// StripStringFromVersionNumber, ApplicationUninstallerEntry.PublisherTrimmed, Sift4.
// SimplestDistance, ConfidenceGenerators et ConfidenceRecords. Modifiée : en Rust, et un
// « niveau » rendu en trois marches pour l'interface (sûr / à vérifier / jamais proposé).

/// « Blender 4.2.1 (x64) » → « Blender » : retire du nom ce qui ressemble à une version.
pub fn nom_court(nom: &str) -> String {
    let mut s = nom.trim().to_string();
    loop {
        let avant = s.len();
        s = s.trim().to_string();
        if s.is_empty() {
            return s;
        }
        if s.ends_with(')') {
            if let Some(p) = s.rfind('(') {
                if p > 4 {
                    s = s[..p].trim_end().to_string();
                }
            }
        }
        s = rogner_fin(&s, &["1", "2", "3", "4", "5", "6", "7", "8", "9", "0", ".", ",", "-", "_"]);
        let bas = s.to_lowercase();
        if bas.ends_with(" v") {
            s.truncate(s.len() - 2);
        }
        s = rogner_fin(&s, &["Application", "Helper", " v", " CE", " version"]);
        s = s.trim_end().to_string();
        if s.len() == avant {
            return s;
        }
    }
}

/// « Discord Inc. » → « Discord » ; « Gudsen Technology Co., Ltd. » garde « Gudsen Technology Co ».
pub fn editeur_court(editeur: &str) -> String {
    rogner_fin(&editeur.replace("(R)", ""), &["corp", "corporation", "limited", "inc", "incorporated", "ltd", "gmbh", "llc", "s.a"])
}

/// Le nom MONTRÉ à l'utilisateur : seulement ce qui est clairement une version (« 2.18.1 »,
/// « version 2.2.5.14 », « v1.2 », « (x64) »). « Insta360 » reste « Insta360 ».
pub fn nom_affiche(nom: &str) -> String {
    let mut mots: Vec<&str> = nom.split_whitespace().collect();
    loop {
        let Some(dernier) = mots.last().copied() else { break };
        let bas = dernier.to_lowercase();
        let version = {
            let v = bas.trim_start_matches('v');
            !v.is_empty() && v.contains('.') && v.chars().all(|c| c.is_ascii_digit() || c == '.')
        };
        let archi = matches!(bas.as_str(), "(x64)" | "(x86)" | "(64-bit)" | "(32-bit)" | "x64" | "x86" | "64-bit" | "32-bit");
        if version || archi || (bas == "version" && mots.len() > 1) || bas == "-" {
            mots.pop();
        } else {
            break;
        }
    }
    let r = mots.join(" ");
    if r.is_empty() { nom.trim().to_string() } else { r }
}

/// Retire de la fin, tant qu'il y en a, l'un des suffixes (sans tenir compte de la casse),
/// ainsi que les virgules, points et espaces qui traînent (ExtendedTrimEndAny). Un suffixe qui
/// commence par une lettre doit être un MOT entier : « Zinc » ne devient pas « Z » (BCU, lui,
/// le rognait).
fn rogner_fin(s: &str, suffixes: &[&str]) -> String {
    let net = |t: &str| t.trim().trim_matches(|c| c == ',' || c == '.' || c == ' ').to_string();
    let mut r = net(s);
    'encore: loop {
        let bas: Vec<char> = r.to_lowercase().chars().collect();
        for suf in suffixes {
            let sb: Vec<char> = suf.to_lowercase().chars().collect();
            if bas.len() < sb.len() || bas[bas.len() - sb.len()..] != sb[..] {
                continue;
            }
            let n = bas.len() - sb.len();
            if sb[0].is_alphabetic() && n > 0 && bas[n - 1].is_alphanumeric() {
                continue;
            }
            if n == 0 {
                return String::new();
            }
            r = net(&r.chars().take(n).collect::<String>());
            continue 'encore;
        }
        return r;
    }
}

/// Distance « Sift4 la plus simple » entre deux chaînes (0 = identiques).
pub fn sift4(s1: &str, s2: &str, max_offset: usize) -> usize {
    let a: Vec<char> = s1.chars().collect();
    let b: Vec<char> = s2.chars().collect();
    let (l1, l2) = (a.len(), b.len());
    if l1 == 0 {
        return l2;
    }
    if l2 == 0 {
        return l1;
    }
    let (mut c1, mut c2, mut lcss, mut local) = (0isize, 0isize, 0usize, 0usize);
    while (c1 as usize) < l1 && (c2 as usize) < l2 {
        if a[c1 as usize] == b[c2 as usize] {
            local += 1;
        } else {
            lcss += local;
            local = 0;
            if c1 != c2 {
                let m = c1.max(c2);
                c1 = m;
                c2 = m;
            }
            for i in 0..max_offset as isize {
                if !((c1 + i) < l1 as isize && (c2 + i) < l2 as isize) {
                    break;
                }
                if a[(c1 + i) as usize] == b[c2 as usize] {
                    c1 += i - 1;
                    c2 -= 1;
                    break;
                }
                if a[c1 as usize] == b[(c2 + i) as usize] {
                    c1 -= 1;
                    c2 += i - 1;
                    break;
                }
            }
        }
        c1 += 1;
        c2 += 1;
    }
    lcss += local;
    l1.max(l2) - lcss
}

/// Ce que l'on sait du programme pour reconnaître ses restes.
#[derive(Clone, Debug, Default)]
pub struct Cible {
    pub nom_court: String,
    pub editeur_court: String,
}

/// -1 : rien à voir ; 0 ou 1 : même nom ; plus haut : ressemblance de plus en plus lâche
/// (MatchStringToProductName).
pub fn correspondance(cible: &Cible, element: &str) -> i32 {
    let produit = cible.nom_court.to_lowercase();
    let s = element.replace('_', " ").to_lowercase().trim().to_string();
    let plus_court = produit.chars().count().min(s.chars().count());
    // les noms très courts ne prouvent rien
    if plus_court <= 4 {
        return -1;
    }
    let r = sift4(&produit, &s, 1) as i32;
    if r <= 1 {
        return r;
    }
    // « SnapCamera », « Epic_Games-Launcher » : le même nom sans espaces ni séparateurs (ajout au
    // portage — BCU les manquait, son Sift4 à décalage 1 ne rattrape pas un espace en moins)
    let compact = |t: &str| t.chars().filter(|c| c.is_alphanumeric()).collect::<String>();
    if compact(&produit).chars().count() > 4 && compact(&produit) == compact(&s) {
        return 1;
    }
    // « Snap Inc Snap Camera » : on retire l'éditeur du nom et on réessaie
    let editeur = cible.editeur_court.to_lowercase();
    if editeur.chars().count() > 4 && produit.contains(&editeur) {
        let sans = produit.replace(&editeur, "").trim().to_string();
        if sans.chars().count() <= 4 {
            return -1;
        }
        let r2 = sift4(&sans, &s, 1) as i32;
        if r2 <= 1 {
            return r2;
        }
    }
    if s.contains(&produit) || produit.contains(&s) {
        return 2;
    }
    if (r as usize) < plus_court / 3 {
        return r;
    }
    -1
}

/// « Adobe AIR » ne doit pas faire passer le dossier « Adobe » pour un reste (ItemNameEqualsCompanyName).
pub fn nom_egal_editeur(cible: &Cible, element: &str) -> bool {
    let editeur = cible.editeur_court.to_lowercase();
    !editeur.is_empty() && editeur != cible.nom_court.to_lowercase() && editeur.contains(&element.to_lowercase())
}

/// Les points de confiance de BCU (ConfidenceRecords), avec leur raison.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Raison {
    EditeurReconnu,     // +4  le dossier parent porte le nom de l'éditeur
    TousSousDossiers,   // +4  rien d'autre ne restera dedans
    DossierEncoreUtilise, // -7 un autre programme y est installé
    LienExplicite,      // +4  le programme y renvoie lui-même (dossier d'installation, raccourci)
    NomEgalEditeur,     // -2  « Adobe » pour « Adobe AIR »
    NomApproche,        // -2  ressemblance seulement
    NomIdentique,       // +2
    NomDouteux,         // -3  « Data », « Config », « Settings »…
    DossierVide,        // +4
    Executables,        // -4  des .exe dedans : un programme vit peut-être là
    Fichiers,           //  0
    BeaucoupDeFichiers, // -2
    NomEncoreUtilise,   // -4  un programme installé porte ce nom
    SansSousDossier,    // +2
    EditeurEncoreUtilise, // -4
    NomVoisin,          // -2  « AppX Extended » quand on retire « AppX »
    DansDossierConnu,   // -1  directement dans un dossier de Windows (AppData, Program Files…)
    Profondeur(i32),    // 2 - 2 × niveau
}

impl Raison {
    pub fn points(self) -> i32 {
        match self {
            Raison::EditeurReconnu | Raison::TousSousDossiers | Raison::LienExplicite | Raison::DossierVide => 4,
            Raison::DossierEncoreUtilise => -7,
            Raison::NomEgalEditeur | Raison::NomApproche | Raison::BeaucoupDeFichiers | Raison::NomVoisin => -2,
            Raison::NomIdentique | Raison::SansSousDossier => 2,
            Raison::NomDouteux => -3,
            Raison::Executables | Raison::NomEncoreUtilise | Raison::EditeurEncoreUtilise => -4,
            Raison::Fichiers => 0,
            Raison::DansDossierConnu => -1,
            Raison::Profondeur(n) => n,
        }
    }
}

/// Les trois marches montrées à l'utilisateur.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Niveau {
    /// « Sans risque » : coché d'office (BCU : Good et VeryGood, total ≥ 2)
    Sur,
    /// « À vérifier » : jamais coché d'office (BCU : Questionable, total 0 ou 1)
    AVerifier,
    /// jamais proposé (BCU : Bad, total < 0)
    Douteux,
}

pub fn niveau(raisons: &[Raison]) -> Niveau {
    if raisons.is_empty() {
        return Niveau::Douteux;
    }
    let total: i32 = raisons.iter().map(|r| r.points()).sum();
    if total < 0 {
        Niveau::Douteux
    } else if total < 2 {
        Niveau::AVerifier
    } else {
        Niveau::Sur
    }
}

/// Les raisons qui naissent du NOM d'un élément trouvé à `niveau` sous un dossier fouillé
/// (GenerateConfidence) ; vide si le nom n'a rien à voir avec le programme.
pub fn raisons_du_nom(cible: &Cible, element: &str, nom_parent: &str, profondeur: i32, parent_est_dossier_connu: bool) -> Vec<Raison> {
    let m = correspondance(cible, element);
    if m < 0 {
        return Vec::new();
    }
    let mut r = vec![if m < 2 { Raison::NomIdentique } else { Raison::NomApproche }, Raison::Profondeur(2 - profondeur.abs() * 2)];
    if nom_egal_editeur(cible, element) {
        r.push(Raison::NomEgalEditeur);
    }
    if profondeur > 0 {
        let parent = nom_parent.replace('_', " ").to_lowercase();
        if !parent.is_empty() && cible.editeur_court.to_lowercase().contains(&parent) {
            r.push(Raison::EditeurReconnu);
        }
    }
    if parent_est_dossier_connu {
        r.push(Raison::DansDossierConnu);
    }
    r
}

/// Les noms de dossier qui ne disent rien du programme.
pub const NOMS_DOUTEUX: [&str; 6] = ["install", "settings", "config", "configuration", "users", "data"];

/// Les dossiers qu'on n'ouvre jamais (DirectoryNameBlacklist de BCU, plus quelques-uns).
pub const NOMS_INTERDITS: [&str; 30] = [
    "microsoft", "microsoft games", "temp", "programs", "common", "common files", "clients", "downloads",
    "desktop", "internet explorer", "windows", "windows nt", "windows photo viewer", "windows mail",
    "windows defender", "windows media player", "uninstall information", "reference assemblies",
    "installshield installation information", "installer", "winsxs", "windowsapps", "directx", "directxredist",
    "packages", "package cache", "nvidia corporation", "microsoft.net", "dotnet", "windows kits",
];

#[cfg(test)]
mod tests {
    use super::*;

    fn cible(nom: &str, editeur: &str) -> Cible {
        Cible { nom_court: nom_court(nom), editeur_court: editeur_court(editeur) }
    }

    #[test]
    fn noms_courts_comme_bcu() {
        assert_eq!(nom_court("Blender 4.2.1"), "Blender");
        assert_eq!(nom_court("RSI Launcher 2.18.1"), "RSI Launcher");
        assert_eq!(nom_court("Snap Camera 1.20.0"), "Snap Camera");
        assert_eq!(nom_court("Insta360 Link Controller version 2.2.5.14"), "Insta360 Link Controller");
        assert_eq!(nom_court("7-Zip 24.08 (x64)"), "7-Zip");
        assert_eq!(nom_court("Discord"), "Discord");
        assert_eq!(nom_court("Mobalytics 1.956.260"), "Mobalytics");
    }

    #[test]
    fn editeurs_courts_comme_bcu() {
        assert_eq!(editeur_court("Discord Inc."), "Discord");
        assert_eq!(editeur_court("Microsoft Corporation"), "Microsoft");
        assert_eq!(editeur_court("Snap Inc."), "Snap");
        assert_eq!(editeur_court("Blender Foundation"), "Blender Foundation");
    }

    #[test]
    fn sift4_reference() {
        assert_eq!(sift4("discord", "discord", 1), 0);
        assert_eq!(sift4("", "abc", 1), 3);
        assert_eq!(sift4("discord", "discorf", 1), 1);
        // à décalage 1 (celui de BCU), un espace en moins n'est pas rattrapé : voir `correspondance`
        assert!(sift4("snap camera", "snapcamera", 1) > 1);
        assert!(sift4("blender", "photoshop", 1) > 4);
    }

    #[test]
    fn la_correspondance_reconnait_les_restes_et_rejette_le_reste() {
        let snap = cible("Snap Camera 1.20.0", "Snap Inc.");
        assert_eq!(correspondance(&snap, "Snap Camera"), 0);
        assert!(correspondance(&snap, "Snap_Camera") <= 1);
        assert_eq!(correspondance(&snap, "SnapCamera"), 1, "sans espace : même nom");
        let epic = cible("Epic Games Launcher", "Epic Games, Inc.");
        assert_eq!(correspondance(&epic, "EpicGamesLauncher"), 1);
        assert_eq!(correspondance(&snap, "Snap Camera Data"), 2, "contient le nom : ressemblance");
        assert_eq!(correspondance(&snap, "Spotify"), -1);
        assert_eq!(correspondance(&snap, "Snap"), -1, "trop court pour conclure");
        // 5 lettres : au-dessus du seuil de 4, le nom suffit
        let gloam = cible("Gloam", "David Torcivia");
        assert_eq!(correspondance(&gloam, "Gloam"), 0);
        assert_eq!(correspondance(&gloam, "GloamApp"), 2);
    }

    #[test]
    fn noms_affiches_sans_casser_les_noms() {
        assert_eq!(nom_affiche("Insta360 Link Controller version 2.2.5.14"), "Insta360 Link Controller");
        assert_eq!(nom_affiche("RSI Launcher 2.18.1"), "RSI Launcher");
        assert_eq!(nom_affiche("Insta360"), "Insta360");
        assert_eq!(nom_affiche("Microsoft Windows Desktop Runtime 10.0.13 (x64)"), "Microsoft Windows Desktop Runtime");
        assert_eq!(nom_affiche("Fallout 4"), "Fallout 4");
        assert_eq!(nom_affiche("glab v1.123.0"), "glab");
    }

    #[test]
    fn un_suffixe_d_editeur_doit_etre_un_mot() {
        assert_eq!(editeur_court("Zinc"), "Zinc");
        assert_eq!(editeur_court("Acme, Inc."), "Acme");
    }

    #[test]
    fn niveaux_comme_bcu() {
        assert_eq!(niveau(&[]), Niveau::Douteux);
        assert_eq!(niveau(&[Raison::NomIdentique, Raison::Profondeur(2)]), Niveau::Sur);
        assert_eq!(niveau(&[Raison::NomApproche, Raison::Profondeur(0)]), Niveau::Douteux);
        assert_eq!(niveau(&[Raison::NomApproche, Raison::Profondeur(2)]), Niveau::AVerifier);
        assert_eq!(niveau(&[Raison::NomIdentique, Raison::Profondeur(2), Raison::DossierEncoreUtilise]), Niveau::Douteux);
    }
}
