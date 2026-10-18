# Nettoyeur

Petite appli grand public (Tauri 2) qui fait de la place sur le disque **C:**, sans jamais toucher
aux photos, aux documents ni aux jeux, et qui désinstalle les programmes avec ce qu'ils laissent
derrière eux. Maquette : claude.ai/artifact/TJa7mqybpLY1iUbJiNohjh.

## Ce qu'elle nettoie

| Catégorie | Où | Par défaut |
|---|---|---|
| Fichiers temporaires | `%TEMP%`, `C:\Windows\Temp` (admin) | coché |
| Corbeille | la Corbeille de C: | coché |
| Restes des mises à jour | `C:\Windows\SoftwareDistribution\Download` (admin) | coché |
| Cache des navigateurs | Chrome, Edge, Brave, Opera, Firefox : cache seulement (on reste connecté) | coché |
| Cache des jeux | shaders NVIDIA, AMD, Intel, DirectX, Star Citizen | coché |
| Vieux installateurs | Téléchargements, .exe/.msi/.msix non utilisés depuis 3 mois | **à vérifier** |
| Gros fichiers oubliés | Téléchargements, > 1 Go, non utilisés depuis un an | **à vérifier** |
| Restes de programmes désinstallés | dossiers sans programme installé : Program Files (64 et 32 bits), `Programs` de AppData et ProgramData | **à vérifier** |

## Désinstaller des programmes

Le lien « Désinstaller des programmes », sous le bouton de l'accueil :

1. **Vos programmes** — la liste de Windows (clés « Uninstall » de HKLM 64 et 32 bits et de HKCU),
   avec icône, éditeur, date et taille. Les composants système (runtimes, pilotes, SDK, Edge,
   WebView2…) sont masqués.
2. **Désinstallation** — un programme à la fois, avec SON désinstalleur. On attend qu'il ait
   vraiment fini : ses processus enfants, puis la disparition de sa clé « Uninstall ». « Passer »
   apparaît au bout de 8 s ; après 30 min, on n'attend plus.
3. **Les restes** — la logique de Bulk Crap Uninstaller (points de confiance) : dossiers (AppData,
   LocalLow, ProgramData, Program Files), clés `Software\<éditeur>`, raccourcis morts du menu
   Démarrer et du Bureau. « Sans risque » est coché, « à vérifier » jamais ; « Tout garder » ne
   supprime rien.
4. **Bilan** — dossiers et raccourcis à la Corbeille ; chaque clé est exportée en .reg dans
   `Documents\Nettoyeur\Sauvegardes\<date>` AVANT d'être supprimée (un double-clic la remet).

## Garde-fous

- L'interface n'envoie que des **identifiants** (catégories) et des **numéros** dans les listes
  gardées côté Rust : aucun chemin ne vient du front.
- On ne vide que des dossiers dont le **nom final** est un nom de cache connu
  (`categories::racine_sure`), sur le disque système, jamais derrière un lien ou une jonction.
- Un fichier créé ou modifié dans les **24 dernières heures** reste.
- Les « à vérifier » partent à la **Corbeille** (vidée AVANT, pour qu'ils y restent récupérables) ;
  si l'un est trop gros pour elle, Windows demande avant de le supprimer.
- Restes : jamais un dossier qui contient l'installation d'un programme encore là, rien sous
  Windows, aucun dossier « Windows… » ou « Microsoft… », aucune clé `Microsoft`, `Windows`,
  `Classes`, `Clients` ou `Policies` ; Documents, Bureau (hors raccourcis) et Téléchargements ne
  sont jamais fouillés.
- Ce qui demande l'administrateur (Windows\Temp, mises à jour, Program Files, ProgramData, HKLM) :
  **une** demande d'autorisation (UAC), seulement s'il y a quelque chose à faire. Le plan est
  déposé dans `%LOCALAPPDATA%\com.drrakendu78.nettoyeur\plan-admin.json` ; le programme relancé
  ne le lit que si son empreinte SHA-256 est celle reçue en argument, puis revérifie chaque élément.

## Mises à jour

Au lancement, puis toutes les 30 minutes, l'appli lit `latest.json` sur les publications GitHub du dépôt
(`https://github.com/drrakendu78/Nettoyeur/releases/latest/download/latest.json`). Si une version plus récente
existe, une pastille « Mise à jour X » apparaît dans la barre de titre : un clic demande confirmation
(« Installer et relancer ? »), le second télécharge l'installeur, vérifie sa signature avec la clé publique de
`tauri.conf.json` (liée à la version annoncée : `requireSignedVersion`), puis l'installe avec sa barre de
progression et relance le Nettoyeur. Jamais pendant une analyse, un nettoyage ou une désinstallation ; rien en
mode développement. Hors ligne, rien ne s'affiche.

## Publier une version

1. Monter la version dans `src-tauri/Cargo.toml` **et** `src-tauri/tauri.conf.json`.
2. Écrire les notes dans `scripts/notes-maj/<version>.txt`, commiter et pousser.
3. `node scripts/publier-maj.mjs --build --notes-file scripts/notes-maj/<version>.txt` (`--essai` d'abord :
   tout est vérifié, rien n'est publié).

Le script refuse un dépôt non poussé, une signature qui ne correspond pas à la clé de l'appli ou à la version,
un retour en arrière ; il publie en brouillon, ajoute `latest.json` en dernier, met en ligne, puis relit le tout
par HTTPS. La clé privée est dans `~/.tauri/nettoyeur.key`, jamais dans le dépôt : **la sauvegarder**, la perdre
rendrait impossible toute mise à jour des Nettoyeur déjà installés.

## Développer

```
cd src-tauri
cargo test                                            # 43 tests, dont jonctions, clés, raccourcis, plan échangé et mises à jour
cargo test -- --ignored --nocapture analyse_reelle    # l'analyse sur la vraie machine, en lecture seule
NETTOYEUR_PROGRAMME=Blender cargo test -- --ignored --nocapture restes_reels   # ses restes, en lecture seule
cargo tauri dev --no-dev-server
```

`tao` vient de `src-tauri/patches/tao` (cf. `patches/README.md`).

## Licence

Code source visible, sous licence **PolyForm Strict 1.0.0** (`LICENSE`) : lecture, audit et usage non
commercial ; ni redistribution, ni fork, ni œuvre dérivée. Ce n'est pas un logiciel open source.

La détection des restes (`noms.rs`, `restes.rs`) est portée de Bulk Crap Uninstaller
(© 2017 Marcin Szeniak, Apache 2.0) : notice, modifications et licence dans
`LICENCES-TIERS/BCU.txt`.
