// SPDX-FileCopyrightText: © 2026 Djamel Lazreg (drrakendu78) <https://djamellazreg.fr>
// SPDX-License-Identifier: LicenseRef-PolyForm-Strict-1.0.0
//
// Nettoyeur — l'interface. Elle n'envoie au programme QUE des identifiants de catégorie (et un
// numéro dans la liste gardée côté Rust pour « Afficher ») : jamais un chemin. Tout le texte
// passe par textContent.
'use strict';

(() => {
  const T = window.__TAURI__;
  const invoke = (commande, args) => T.core.invoke(commande, args);
  const fenetre = T.window.getCurrentWindow();
  const $ = (s) => document.querySelector(s);

  const NOMS = {
    temp: 'Fichiers temporaires', corbeille: 'Corbeille', maj: 'Restes des mises à jour Windows',
    nav: 'Cache des navigateurs', jeux: 'Cache des jeux', install: 'Téléchargements', gros: 'Téléchargements'
  };
  const A_VERIFIER = ['install', 'gros'];

  /* ── nombres : des Go comme l'Explorateur (1 Go = 1024³ octets), à la française ── */
  const GO = 1024 ** 3, MO = 1024 ** 2, KO = 1024;
  const decimale = new Intl.NumberFormat('fr-FR', { minimumFractionDigits: 1, maximumFractionDigits: 1 });
  const entier = new Intl.NumberFormat('fr-FR', { maximumFractionDigits: 0 });
  // l'espace fine des milliers (U+202F) est presque invisible : une espace insécable normale, comme la maquette
  const espace = (t) => t.replace(/\u202F/g, '\u00A0');
  function taille(octets, sansDecimale) {
    if (octets >= GO) return espace((sansDecimale ? entier : decimale).format(octets / GO)) + ' Go';
    if (octets >= MO) return espace(entier.format(octets / MO)) + ' Mo';
    if (octets > 0) return espace(entier.format(Math.max(1, octets / KO))) + ' Ko';
    return '0 Ko';
  }
  function depuis(jours) {
    if (jours < 60) return 'il y a ' + jours + ' jours';
    if (jours < 730) return 'il y a ' + Math.round(jours / 30) + ' mois';
    return 'il y a ' + Math.floor(jours / 365) + ' ans';
  }
  function message(erreur) {
    return typeof erreur === 'string' ? erreur : (erreur && erreur.message) || 'Une erreur est survenue.';
  }

  /* ── barre de titre maison ── */
  $('#barre').addEventListener('mousedown', (e) => {
    if (e.button !== 0 || e.target.closest('button')) return;
    fenetre.startDragging();
  });
  $('#barre').addEventListener('dblclick', (e) => {
    if (!e.target.closest('button')) fenetre.toggleMaximize();
  });
  $('#reduire').addEventListener('click', () => fenetre.minimize());
  $('#agrandir').addEventListener('click', () => fenetre.toggleMaximize());
  $('#fermer').addEventListener('click', () => fenetre.close());

  /* ── écrans ── */
  function afficher(nom) {
    for (const id of ['accueil', 'resultats', 'termine']) $('#' + id).hidden = id !== nom;
    $('#pied').hidden = nom !== 'resultats';
    const titre = $('#' + nom + ' h1');
    if (titre) titre.focus({ preventScroll: true });
  }
  function occupe(bouton, oui, libelle) {
    bouton.disabled = oui;
    if (oui) bouton.setAttribute('aria-busy', 'true'); else bouton.removeAttribute('aria-busy');
    if (libelle) bouton.querySelector('.libelle').textContent = libelle;
  }

  /* ── 1 · Accueil : le disque C: ── */
  async function disque() {
    try {
      const d = await invoke('infos_disque');
      const utilise = d.total - d.libre;
      $('#utilise').textContent = taille(utilise, true);
      $('#sur').textContent = 'utilisés sur ' + taille(d.total, true);
      $('#anneau').setAttribute('aria-label', 'Disque C : ' + taille(utilise, true) + ' utilisés sur ' + taille(d.total, true));
      const arc = d.total > 0 ? (utilise / d.total) * 2 * Math.PI * 100 : 0;
      // une image plus tard : l'anneau se remplit au lieu d'apparaître plein
      requestAnimationFrame(() => $('#arc').setAttribute('stroke-dasharray', arc.toFixed(1) + ' 629'));
    } catch (e) {
      $('#utilise').textContent = '—';
      $('#sur').textContent = 'disque illisible';
    }
  }

  const noteAccueil = $('#note-accueil');
  const NOTE_ACCUEIL = noteAccueil.textContent;
  let etapes = null;

  $('#analyser').addEventListener('click', async () => {
    const b = $('#analyser');
    occupe(b, true, 'Analyse en cours…');
    noteAccueil.classList.remove('alerte');
    noteAccueil.textContent = 'On regarde ce qui peut partir…';
    try {
      if (!etapes) {
        etapes = await T.event.listen('analyse-etape', (e) => {
          if (NOMS[e.payload]) noteAccueil.textContent = 'Analyse : ' + NOMS[e.payload] + '…';
        });
      }
      const resumes = await invoke('analyser');
      montrerResultats(resumes);
      afficher('resultats');
      noteAccueil.textContent = NOTE_ACCUEIL;
    } catch (e) {
      noteAccueil.classList.add('alerte');
      noteAccueil.textContent = 'L’analyse n’a pas abouti : ' + message(e);
    } finally {
      occupe(b, false, 'Analyser mon PC');
    }
  });

  /* ── 2 · Résultats ── */
  const trouve = {};       // id → { taille, nombre }
  const listes = {};       // id → lignes reçues (les « à vérifier »)

  function ligne(id) { return document.querySelector('.ligne[data-id="' + id + '"]'); }
  function case_(id) { return $('#c-' + id); }

  function montrerResultats(resumes) {
    for (const r of resumes) {
      trouve[r.id] = r;
      delete listes[r.id];
      const l = ligne(r.id);
      if (!l) continue;
      const vide = r.taille === 0;
      l.querySelector('.taille').textContent = vide ? '—' : taille(r.taille);
      l.querySelector('.rang').classList.toggle('vide', vide);
      const c = case_(r.id);
      c.disabled = vide;
      // « à vérifier » : jamais coché d'office
      c.checked = !vide && !A_VERIFIER.includes(r.id);
      const detail = l.querySelector('.detail');
      if (detail) { detail.hidden = true; detail.replaceChildren(); }
    }
    total();
  }

  function selection() {
    return Object.keys(trouve).filter((id) => case_(id) && case_(id).checked);
  }

  function total() {
    const somme = selection().reduce((s, id) => s + trouve[id].taille, 0);
    const texte = taille(somme);
    const rien = Object.values(trouve).every((r) => r.taille === 0);
    $('#titre-resultats').firstChild.textContent = rien ? 'Votre PC est déjà propre' : 'On a trouvé ';
    $('#total-titre').hidden = rien;
    $('#titre-resultats').lastChild.textContent = rien ? '' : ' à récupérer';
    $('#sous-titre').textContent = rien ? 'Rien à récupérer pour l’instant. Revenez dans quelques semaines.' : 'Les éléments « à vérifier » ne sont jamais cochés d’office.';
    $('#total-titre').textContent = texte;
    $('#total-pied').textContent = texte;
    $('#total-bouton').textContent = texte;
    $('#nettoyer').disabled = somme === 0;
    const sel = $('#selection');
    sel.classList.remove('alerte');
    if (!sel.querySelector('#total-pied')) { sel.textContent = 'Sélection : '; const b = document.createElement('b'); b.id = 'total-pied'; b.textContent = texte; sel.append(b); }
  }

  // « la liste s'ouvre avant » : cocher une catégorie « à vérifier » montre ses fichiers sur place
  async function ouvrirListe(id) {
    const detail = ligne(id).querySelector('.detail');
    detail.hidden = false;
    if (!listes[id]) {
      try { listes[id] = await invoke('lister', { id }); }
      catch (e) { listes[id] = null; detail.replaceChildren(texteSimple('vide-liste', message(e))); return; }
    }
    const lignes = listes[id];
    const MAX = 50;
    const enfants = lignes.slice(0, MAX).map((f) => {
      const rang = document.createElement('div'); rang.className = 'fichier';
      const nom = document.createElement('span'); nom.className = 'nom-fichier'; nom.textContent = f.nom; nom.title = f.nom;
      const t = document.createElement('span'); t.className = 'meta'; t.textContent = taille(f.taille);
      const d = document.createElement('span'); d.className = 'meta'; d.textContent = depuis(f.jours);
      const voir = document.createElement('button'); voir.type = 'button'; voir.className = 'afficher'; voir.textContent = 'Afficher';
      voir.setAttribute('aria-label', 'Afficher ' + f.nom + ' dans l’Explorateur');
      voir.addEventListener('click', () => invoke('montrer', { id, index: f.index }).catch(() => {}));
      rang.append(nom, t, d, voir);
      return rang;
    });
    if (lignes.length > MAX) enfants.push(texteSimple('plus', 'et ' + (lignes.length - MAX) + ' autres fichiers'));
    detail.replaceChildren(...enfants);
  }
  function texteSimple(classe, texte) { const p = document.createElement('div'); p.className = classe; p.textContent = texte; return p; }

  for (const id of Object.keys(NOMS)) {
    const c = case_(id);
    if (!c) continue;
    c.addEventListener('change', () => {
      if (A_VERIFIER.includes(id)) {
        if (c.checked) ouvrirListe(id);
        else ligne(id).querySelector('.detail').hidden = true;
      }
      total();
    });
  }

  $('#retour').addEventListener('click', () => { afficher('accueil'); disque(); });

  $('#nettoyer').addEventListener('click', async () => {
    const ids = selection();
    if (!ids.length) return;
    const b = $('#nettoyer');
    occupe(b, true, 'Nettoyage en cours…');
    $('#retour').disabled = true;
    document.querySelectorAll('.rang input').forEach((c) => { c.dataset.avant = c.disabled ? '1' : ''; c.disabled = true; });
    try {
      const bilan = await invoke('nettoyer', { ids });
      montrerBilan(bilan);
      afficher('termine');
    } catch (e) {
      const sel = $('#selection');
      sel.classList.add('alerte');
      sel.textContent = 'Le nettoyage n’a pas abouti : ' + message(e);
    } finally {
      document.querySelectorAll('.rang input').forEach((c) => { c.disabled = c.dataset.avant === '1'; });
      $('#retour').disabled = false;
      occupe(b, false, '');
      b.querySelector('.libelle').replaceChildren('Nettoyer ', Object.assign(document.createElement('span'), { id: 'total-bouton', textContent: $('#total-pied') ? $('#total-pied').textContent : '' }));
    }
  });

  /* ── 3 · Terminé ── */
  function montrerBilan(b) {
    const libere = Math.max(0, b.apres - b.avant);
    $('#libere').textContent = taille(libere);
    $('#avant').textContent = taille(b.avant, true);
    $('#apres').textContent = taille(b.apres, true);
    const pct = (x) => (b.total > 0 ? (x / b.total) * 100 : 0);
    $('#jauge-avant').style.width = pct(b.avant).toFixed(2) + '%';
    $('#jauge-gain').style.width = (libere > 0 ? Math.max(0.6, pct(libere)) : 0).toFixed(2) + '%';
    $('#note-admin').hidden = !b.admin_refuse;
  }
  $('#fini').addEventListener('click', () => { afficher('accueil'); disque(); });

  disque();
})();
