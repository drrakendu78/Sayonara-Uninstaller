# Crates patchées

> Copie de `Stelliverse/src-tauri/patches` (même `tao/`, même texte). Le banc `banc-tao/` et
> l'alignement `webview2-com` / `windows-core` cités plus bas sont ceux de la prod ; cet outil
> n'utilise pas WebView2 directement.

## `tao/` — tao 0.37.1, runner de la boucle d'événements en `Arc` (Windows)

**Symptôme.** L'appli se ferme d'un coup, au bout de quelques minutes ou de plusieurs
heures, sans rien dans le journal de diagnostic (la session suivante note seulement
`previous_session_interrupted_crash_or_forced_exit`). L'Observateur d'événements
Windows (journal Application, ID 1000) montre `stelliverse.exe` avec l'un de ces codes :

- `0xc000001d` (instruction illégale) dans `stelliverse.exe` : un `Rc`/`Arc::clone`
  tombe sur un compteur absurde et Rust avorte volontairement ;
- `0xc0000374` (corruption du tas) dans `ntdll.dll` : Windows détecte la mémoire
  libérée puis réutilisée.

**Cause.** Tauri 2 range le `EventLoopWindowTarget` de tao dans son contexte
(`tauri_runtime_wry::Context`) et clone ce contexte à chaque clone d'`AppHandle`, de
`Window` ou de `Webview`, depuis n'importe quel thread (IPC comprise). Sous Windows,
ce target porte le runner derrière un `Rc`, dont le compteur n'est pas atomique : deux
threads qui clonent ou lâchent en même temps perdent une mise à jour, et le runner est
libéré alors qu'il sert encore. Plus l'appli sonde en tâche de fond, plus c'est fréquent.

Bug amont : [tauri-apps/tauri#15408](https://github.com/tauri-apps/tauri/issues/15408)
(PR #15411 non fusionnée). Toujours présent dans tao 0.37.1 et Tauri 2.12.4 au
14/10/2026 : monter de version ne corrige rien. tao a refusé deux fois ce même
passage en `Arc` (tao#1290, tao#1334) : pour ses mainteneurs, la vraie correction est
côté Tauri (#15411 : ne garder le target que sur le thread principal).

**Le patch.** Deux lignes : `EventLoopRunnerShared` passe de `Rc` à `Arc`
(`src/platform_impl/windows/event_loop/runner.rs`) et sa construction suit
(`src/platform_impl/windows/event_loop.rs`), marquées `STELLIVERSE`. Le runner reste
utilisé sur le seul thread principal ; seul son comptage de références devient
atomique. Le reste est identique à tao 0.37.1 publié (contrôle :
`diff -r --strip-trailing-cr` contre `~/.cargo/registry/src/*/tao-0.37.1` — git
convertit les fins de ligne au checkout).

**Preuve** (`banc-tao/`, 4 threads + le principal qui clonent et lâchent le target) :
- tao 0.35.3 (05/10/2026) : d'origine → 6 crashs sur 6, en 2 à 3 s (`0xc000001d`,
  `0xc0000374`, `0xc0000005`) ; patché → 7 passages sur 7 sans crash (≈ 9 milliards
  de clones/lâchers) ;
- tao 0.37.1 (14/10/2026, montée vers Tauri 2.12.4) : d'origine → 3 crashs sur 3
  (`0xc0000005`, `0xc0000374`, `0xc000001d`) ; patché → « OK », 721 millions de
  clones/lâchers en 20 s.

**Avec lui, depuis Tauri 2.12 :** `webview2-com` 0.39 et `windows-core` 0.62 dans
`Cargo.toml`, alignés sur wry 0.57 (keyguard.rs et diagnostics/system.rs passent leurs
types aux interfaces WebView2 de wry ; un décalage de version ne compile pas).

**Garde-fou.** `build.rs` refuse de compiler si `Cargo.lock` tire de nouveau tao d'un
registre — c'est ce qui arrive, sans erreur de Cargo, quand une montée de Tauri exige
une autre version de tao.

**Quand on monte Tauri.**

1. tauri#15408 corrigé en amont : supprimer `patches/`, le `[patch]` de `Cargo.toml`
   et le garde-fou de `build.rs`.
2. Sinon : recopier la nouvelle version de tao ici, réappliquer les deux lignes,
   mettre la version à jour dans `banc-tao/Cargo.toml`, puis
   `cargo run --release -- 20` dans `banc-tao/` doit afficher « OK ».
