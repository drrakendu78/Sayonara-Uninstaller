// SPDX-FileCopyrightText: © 2026 Djamel Lazreg (drrakendu78) <https://djamellazreg.fr>
// SPDX-License-Identifier: LicenseRef-PolyForm-Strict-1.0.0

// Pas de console en release (appli fenêtrée).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Relancé en administrateur par le nettoyage (une seule demande UAC) : on vide les dossiers
    // « admin » des catégories reçues, sans fenêtre, puis on sort. Seuls des identifiants passent.
    let args: Vec<String> = std::env::args().collect();
    if args.len() == 3 && args[1] == "--nettoyer-admin" {
        std::process::exit(nettoyeur_lib::nettoyer_admin(&args[2]));
    }
    nettoyeur_lib::run()
}
