// SPDX-FileCopyrightText: © 2026 Djamel Lazreg (drrakendu78) <https://djamellazreg.fr>
// SPDX-License-Identifier: LicenseRef-PolyForm-Strict-1.0.0

fn main() {
    // tao DOIT venir de `patches/tao` (runner de la boucle d'événements en `Arc`, cf.
    // Cargo.toml et patches/README.md). Une montée de Tauri qui exige un autre tao fait
    // sauter le `[patch]` SANS erreur de Cargo — et les crashs aléatoires reviendraient en
    // silence. Dans `Cargo.lock`, un paquet tiré d'un registre porte une ligne `source`.
    {
        let lock = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.lock");
        println!("cargo:rerun-if-changed={}", lock.display());
        if let Ok(contenu) = std::fs::read_to_string(&lock) {
            for paquet in contenu.split("[[package]]") {
                if paquet.contains("\nname = \"tao\"") && paquet.contains("\nsource = ") {
                    panic!(
                        "tao n'est plus la copie patchée de src-tauri/patches/tao : le [patch] \
                         a sauté (montée de Tauri ?). Cf. src-tauri/patches/README.md."
                    );
                }
            }
        }
    }
    tauri_build::build()
}
