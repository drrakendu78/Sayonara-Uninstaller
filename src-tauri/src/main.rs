// SPDX-FileCopyrightText: © 2026 Djamel Lazreg (drrakendu78) <https://djamellazreg.fr>
// SPDX-License-Identifier: LicenseRef-PolyForm-Strict-1.0.0

// Pas de console en release (appli fenêtrée).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Relancé en administrateur par le nettoyage (une seule demande UAC) : on exécute le plan
    // déposé dans le dossier de l'appli — après avoir tout revérifié —, sans fenêtre, puis on sort.
    let args: Vec<String> = std::env::args().collect();
    if args.len() == 3 && args[1] == "--admin" {
        std::process::exit(sayonara_lib::programme_eleve(&args[2]));
    }
    sayonara_lib::run()
}
