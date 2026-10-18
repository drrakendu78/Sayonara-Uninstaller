# Nettoyeur

Petite appli grand public (Tauri 2) qui fait de la place sur le disque **C:**, sans jamais toucher
aux photos, aux documents ni aux jeux. Maquette : claude.ai/artifact/TJa7mqybpLY1iUbJiNohjh.

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

## Garde-fous

- L'interface n'envoie que des **identifiants de catégorie** : aucun chemin ne vient du front.
- On ne supprime que dans des dossiers dont le **nom final** est un nom de cache connu
  (`categories::racine_sure`), sur le disque système, jamais derrière un lien ou une jonction.
- Un fichier créé ou modifié dans les **24 dernières heures** reste.
- Les « à vérifier » partent à la **Corbeille** (vidée AVANT, pour qu'ils y restent récupérables) ;
  si l'un est trop gros pour elle, Windows demande avant de le supprimer.
- Windows\Temp et les mises à jour : **une** demande d'autorisation (UAC), seulement s'il y a
  quelque chose à gagner ; le programme relancé ne reçoit que des identifiants.

## Développer

```
cd src-tauri
cargo test                                            # 9 tests, dont jonctions et fichiers récents
cargo test -- --ignored --nocapture analyse_reelle    # l'analyse sur la vraie machine, en lecture seule
cargo tauri dev --no-dev-server
```

`tao` vient de `src-tauri/patches/tao` (cf. `patches/README.md`).
