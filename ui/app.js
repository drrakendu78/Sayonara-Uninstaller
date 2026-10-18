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
    nav: 'Cache des navigateurs', jeux: 'Cache des jeux', install: 'Téléchargements', gros: 'Téléchargements',
    orphelins: 'Restes de programmes'
  };
  const A_VERIFIER = ['install', 'gros', 'orphelins'];
  const NS = 'http://www.w3.org/2000/svg';

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
  const ECRANS = ['accueil', 'resultats', 'termine', 'programmes', 'desinstallation', 'restes', 'termine-programmes'];
  const PIEDS = { resultats: '#pied', programmes: '#pied-programmes', restes: '#pied-restes' };
  function afficher(nom) {
    for (const id of ECRANS) $('#' + id).hidden = id !== nom;
    for (const [ecran, pied] of Object.entries(PIEDS)) $(pied).hidden = ecran !== nom;
    const titre = $('#' + nom + ' h1');
    if (titre) titre.focus({ preventScroll: true });
  }
  // un petit dessin SVG, sans balisage dans une chaîne (CSP, textContent partout)
  function dessin(largeur, chemins, couleur, epaisseur) {
    const s = document.createElementNS(NS, 'svg');
    s.setAttribute('width', largeur); s.setAttribute('height', largeur); s.setAttribute('viewBox', '0 0 24 24');
    s.setAttribute('fill', 'none'); s.setAttribute('stroke', couleur); s.setAttribute('stroke-width', epaisseur);
    s.setAttribute('stroke-linecap', 'round'); s.setAttribute('stroke-linejoin', 'round'); s.setAttribute('aria-hidden', 'true');
    for (const d of chemins) { const p = document.createElementNS(NS, 'path'); p.setAttribute('d', d); s.append(p); }
    return s;
  }
  // l'icône d'un programme (PNG extrait par le Rust), ou une icône neutre — jamais des lettres
  function icone(source) {
    if (source) { const i = document.createElement('img'); i.alt = ''; i.src = source; return i; }
    const s = dessin(32, ['M4.5 6.5 A2 2 0 0 1 6.5 4.5 H17.5 A2 2 0 0 1 19.5 6.5 V17.5 A2 2 0 0 1 17.5 19.5 H6.5 A2 2 0 0 1 4.5 17.5 Z', 'M4.5 9 H19.5'], '#86868B', 1.6);
    s.classList.add('icone-app');
    return s;
  }
  function element(balise, classe, texte) {
    const e = document.createElement(balise);
    if (classe) e.className = classe;
    if (texte !== undefined) e.textContent = texte;
    return e;
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
      // un reste orphelin dit où il est (« Program Files ») ; un fichier, depuis quand il dort
      const d = document.createElement('span'); d.className = 'meta'; d.textContent = f.lieu || depuis(f.jours);
      const voir = document.createElement('button'); voir.type = 'button'; voir.className = 'afficher'; voir.textContent = 'Afficher';
      voir.setAttribute('aria-label', 'Afficher ' + f.nom + ' dans l’Explorateur');
      voir.addEventListener('click', () => invoke('montrer', { id, index: f.index }).catch(() => {}));
      rang.append(nom, t, d, voir);
      return rang;
    });
    if (lignes.length > MAX) enfants.push(texteSimple('plus', 'et ' + (lignes.length - MAX) + ' autres'));
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

  /* ── 4 · Vos programmes ── */
  let programmes = [];           // { index, nom, editeur, taille, date, icone } — l'index est celui du Rust
  const choisis = new Set();
  const dateCourte = new Intl.DateTimeFormat('fr-FR', { day: 'numeric', month: 'short', year: 'numeric' });
  function installeLe(d) {
    if (!d || d.length !== 8) return '';
    return 'Installé le ' + dateCourte.format(new Date(+d.slice(0, 4), +d.slice(4, 6) - 1, +d.slice(6, 8)));
  }
  const sansAccents = (t) => t.normalize('NFD').replace(/[̀-ͯ]/g, '').toLowerCase();
  const pluriel = (n, un, plusieurs) => n + ' ' + (n > 1 ? plusieurs : un);

  async function ouvrirProgrammes() {
    afficher('programmes');
    choisis.clear();
    programmes = [];
    $('#recherche').value = '';
    $('#liste-programmes').replaceChildren();
    $('#liste-programmes').hidden = true;
    majSelectionProgrammes();
    const vide = $('#vide-programmes');
    vide.hidden = false;
    vide.classList.remove('alerte');
    vide.textContent = 'Lecture de vos programmes…';
    try {
      programmes = await invoke('programmes');
      rendreProgrammes();
    } catch (e) {
      vide.classList.add('alerte');
      vide.textContent = 'La liste n’a pas pu être lue : ' + message(e);
    }
  }

  function rendreProgrammes() {
    const q = sansAccents($('#recherche').value.trim());
    const lignes = programmes.filter((p) => !q || sansAccents(p.nom + ' ' + p.editeur).includes(q)).map((p) => {
      const li = element('li', 'ligne' + (choisis.has(p.index) ? ' cochee' : ''));
      const lab = element('label', 'prog');
      const id = 'p-' + p.index;
      lab.htmlFor = id;
      const nom = element('span', 'nom');
      nom.append(element('span', 'titre', p.nom), element('span', 'desc', p.editeur || ' '));
      const c = element('input');
      c.type = 'checkbox'; c.id = id; c.checked = choisis.has(p.index);
      c.setAttribute('aria-label', 'Désinstaller ' + p.nom);
      c.addEventListener('change', () => {
        if (c.checked) choisis.add(p.index); else choisis.delete(p.index);
        li.classList.toggle('cochee', c.checked);
        majSelectionProgrammes();
      });
      lab.append(icone(p.icone), nom, element('span', 'date', installeLe(p.date)), element('span', 'taille', p.taille ? taille(p.taille) : '—'), c);
      li.append(lab);
      return li;
    });
    const ul = $('#liste-programmes');
    ul.replaceChildren(...lignes);
    ul.hidden = lignes.length === 0;
    const vide = $('#vide-programmes');
    vide.hidden = lignes.length > 0;
    if (!lignes.length) vide.textContent = q ? 'Aucun programme ne correspond à « ' + $('#recherche').value.trim() + ' ».' : 'Aucun programme à proposer.';
  }

  function majSelectionProgrammes() {
    const n = choisis.size;
    const somme = programmes.filter((p) => choisis.has(p.index)).reduce((s, p) => s + (p.taille || 0), 0);
    const sel = $('#selection-programmes');
    sel.classList.remove('alerte');
    if (n === 0) sel.textContent = 'Aucun programme choisi';
    else { sel.replaceChildren(pluriel(n, 'programme', 'programmes') + ' · ', element('b', '', taille(somme))); }
    const b = $('#desinstaller');
    b.disabled = n === 0;
    b.querySelector('.libelle').textContent = n === 0 ? 'Désinstaller' : 'Désinstaller ' + pluriel(n, 'programme', 'programmes');
  }

  $('#vers-programmes').addEventListener('click', ouvrirProgrammes);
  $('#recherche').addEventListener('input', rendreProgrammes);
  $('#retour-programmes').addEventListener('click', () => { afficher('accueil'); disque(); });

  /* ── 5 · Désinstallation : un programme à la fois, avec SON désinstalleur ── */
  const STATUTS = { attente: 'En attente', retire: 'Retiré', annule: 'Pas retiré', echec: 'Pas lancé' };
  let ecouteDesinstallation = null;

  $('#desinstaller').addEventListener('click', async () => {
    const liste = programmes.filter((p) => choisis.has(p.index));
    if (!liste.length) return;
    afficher('desinstallation');
    $('#titre-desinstallation').textContent = 'Désinstallation…';
    $('#texte-desinstallation').textContent = 'Un programme à la fois. S’il vous pose des questions, répondez-lui : on attend.';
    $('#retour-desinstallation').hidden = true;
    const statuts = new Map();
    $('#liste-desinstallation').replaceChildren(...liste.map((p) => {
      const li = element('li', 'ligne');
      const rang = element('div', 'etat');
      const nom = element('span', 'nom');
      nom.append(element('span', 'titre', p.nom), element('span', 'desc', p.editeur || ' '));
      const s = element('span', 'statut attente', STATUTS.attente);
      statuts.set(p.index, s);
      rang.append(icone(p.icone), nom, s);
      li.append(rang);
      return li;
    }));
    let faits = 0;
    const maj = () => {
      $('#barre-desinstallation').style.width = (faits / liste.length * 100).toFixed(1) + '%';
      $('#note-desinstallation').textContent = faits + ' sur ' + liste.length + ' · ensuite, on cherche ce qu’ils ont laissé';
    };
    maj();
    if (ecouteDesinstallation) ecouteDesinstallation();
    ecouteDesinstallation = await T.event.listen('desinstallation', (e) => {
      const { index, etat } = e.payload;
      const s = statuts.get(index);
      if (!s) return;
      s.className = 'statut ' + etat;
      if (etat === 'en_cours') {
        s.replaceChildren(element('span', 'mini-roue'), 'En cours…');
        // un désinstalleur qui ne rend pas la main : « Passer » au bout de quelques secondes
        setTimeout(() => {
          if (!s.classList.contains('en_cours')) return;
          const b = element('button', 'passer', 'Passer');
          b.type = 'button';
          b.addEventListener('click', () => invoke('passer').catch(() => {}));
          s.append(b);
        }, 8000);
        return;
      }
      s.replaceChildren();
      if (etat === 'retire') s.append(dessin(18, ['M5 12.5 L10 17.5 L19 7'], '#1B6E37', 2.6));
      s.append(STATUTS[etat] || etat);
      faits++;
      maj();
    });
    try {
      const etapes = await invoke('desinstaller', { indexes: liste.map((p) => p.index) });
      const retires = etapes.filter((x) => x.etat === 'retire').length;
      if (!retires) {
        $('#titre-desinstallation').textContent = 'Rien n’a été retiré';
        $('#texte-desinstallation').textContent = 'Les désinstallations ont été annulées ou n’ont pas abouti. Rien d’autre n’a été touché.';
        $('#note-desinstallation').textContent = '';
        $('#retour-desinstallation').hidden = false;
        return;
      }
      $('#note-desinstallation').textContent = 'On cherche ce qu’ils ont laissé…';
      montrerRestes(await invoke('chercher_restes'), retires);
    } catch (e) {
      $('#titre-desinstallation').textContent = 'La désinstallation n’a pas abouti';
      $('#texte-desinstallation').textContent = message(e);
      $('#retour-desinstallation').hidden = false;
    }
  });
  $('#retour-desinstallation').addEventListener('click', ouvrirProgrammes);

  /* ── 6 · Les restes ── */
  let restes = { programmes: [], restes: [] };
  let nbRetires = 0;
  const DESSINS = {
    dossier: ['M3.5 6.5 H9.5 L11.5 8.5 H20.5 V18.5 H3.5 Z'],
    cle: ['M4 5 H10 V11 H4 Z', 'M14 5 H20 V11 H14 Z', 'M4 13 H10 V19 H4 Z', 'M14 13 H20 V19 H14 Z'],
    raccourci: ['M6 4 H18 V20 H6 Z', 'M10 14 L14 10 M11 10 H14 V13']
  };
  function genreTexte(x) {
    if (x.genre === 'cle') return 'Réglages du programme · sauvegardés avant';
    if (x.genre === 'raccourci') return 'Raccourci vers un programme retiré';
    return x.nombre ? 'Dossier · ' + espace(entier.format(x.nombre)) + ' ' + (x.nombre > 1 ? 'fichiers' : 'fichier') : 'Dossier vide';
  }

  function montrerRestes(r, retires) {
    restes = r;
    nbRetires = retires;
    if (!r.restes.length) {
      montrerBilanRestes({ programmes: retires, dossiers: 0, cles: 0, raccourcis: 0, octets: 0, admin_refuse: false, sauvegardes: null }, 'rien');
      afficher('termine-programmes');
      return;
    }
    const enfants = [];
    r.programmes.forEach((prog, pi) => {
      const siens = r.restes.filter((x) => x.programme === pi);
      if (!siens.length) return;
      const tete = element('li', 'ligne');
      const g = element('div', 'groupe');
      g.append(icone(prog.icone), element('span', 'titre', prog.nom),
        element('span', 'compte', pluriel(siens.length, 'reste', 'restes') + ' · ' + taille(siens.reduce((s, x) => s + x.taille, 0))));
      tete.append(g);
      enfants.push(tete);
      for (const x of siens) {
        const sur = x.niveau === 'sur';
        const li = element('li', 'ligne' + (sur ? ' cochee' : ''));
        const lab = element('label', 'reste');
        lab.htmlFor = 'r-' + x.index;
        const nom = element('span', 'nom');
        const genre = element('span', 'genre', genreTexte(x));
        if (x.genre !== 'cle') {
          const voir = element('button', 'afficher', 'Afficher');
          voir.type = 'button';
          voir.setAttribute('aria-label', 'Afficher ' + x.chemin + ' dans l’Explorateur');
          voir.addEventListener('click', (e) => { e.preventDefault(); invoke('montrer_reste', { index: x.index }).catch(() => {}); });
          genre.append(' · ', voir);
        }
        const chemin = element('span', 'chemin', (x.genre === 'cle' ? 'Registre · ' : '') + x.chemin);
        chemin.title = x.chemin;
        nom.append(chemin, genre);
        const c = element('input');
        c.type = 'checkbox'; c.id = 'r-' + x.index; c.dataset.index = x.index;
        // « à vérifier » : jamais coché d'office
        c.checked = sur;
        c.setAttribute('aria-label', 'Supprimer ' + x.chemin);
        c.addEventListener('change', () => { li.classList.toggle('cochee', c.checked); majRestes(); });
        lab.append(dessin(22, DESSINS[x.genre] || DESSINS.dossier, '#515154', 1.8), nom,
          element('span', 'pastille ' + (sur ? 'sure' : 'verifier'), sur ? 'Sans risque' : 'À vérifier'),
          element('span', 'taille', x.genre === 'cle' ? '—' : taille(x.taille)), c);
        li.append(lab);
        enfants.push(li);
      }
    });
    $('#liste-restes').replaceChildren(...enfants);
    $('#titre-restes').textContent = (retires > 1 ? 'Ils ont laissé ' : 'Il a laissé ') + taille(r.restes.reduce((s, x) => s + x.taille, 0)) + ' derrière ' + (retires > 1 ? 'eux' : 'lui');
    majRestes();
    afficher('restes');
  }

  function cochesRestes() {
    return [...document.querySelectorAll('#liste-restes input:checked')].map((c) => +c.dataset.index);
  }
  function majRestes() {
    const coches = cochesRestes();
    const somme = restes.restes.filter((x) => coches.includes(x.index)).reduce((s, x) => s + x.taille, 0);
    const sel = $('#selection-restes');
    sel.classList.remove('alerte');
    sel.replaceChildren('Sélection : ', Object.assign(element('b', '', taille(somme)), { id: 'total-restes' }));
    $('#supprimer-restes').disabled = coches.length === 0;
  }

  $('#supprimer-restes').addEventListener('click', async () => {
    const indexes = cochesRestes();
    if (!indexes.length) return;
    const b = $('#supprimer-restes');
    occupe(b, true, 'Suppression…');
    $('#garder-restes').disabled = true;
    const cases = [...document.querySelectorAll('#liste-restes input')];
    cases.forEach((c) => { c.disabled = true; });
    try {
      montrerBilanRestes(await invoke('supprimer_restes', { indexes }));
      afficher('termine-programmes');
    } catch (e) {
      const sel = $('#selection-restes');
      sel.classList.add('alerte');
      sel.textContent = 'La suppression n’a pas abouti : ' + message(e);
    } finally {
      cases.forEach((c) => { c.disabled = false; });
      $('#garder-restes').disabled = false;
      occupe(b, false, 'Supprimer les restes');
    }
  });
  // la sortie de l'écran quand on ne veut rien supprimer (tout décoché, le bouton est éteint)
  $('#garder-restes').addEventListener('click', () => {
    montrerBilanRestes({ programmes: nbRetires, dossiers: 0, cles: 0, raccourcis: 0, octets: 0, admin_refuse: false, sauvegardes: null }, 'garde');
    afficher('termine-programmes');
  });

  /* ── 7 · Programmes retirés ── */
  // mode : 'rien' (aucun reste trouvé), 'garde' (restes gardés), sinon le bilan de la suppression
  function montrerBilanRestes(b, mode) {
    const n = b.programmes || nbRetires;
    const plus = n > 1;
    $('#nb-retires').textContent = pluriel(n, 'programme', 'programmes');
    $('#legende-retires').textContent =
      mode === 'rien' ? (plus ? 'retirés, sans rien laisser derrière eux' : 'retiré, sans rien laisser derrière lui')
      : mode === 'garde' ? (plus ? 'retirés, leurs restes gardés' : 'retiré, ses restes gardés')
      : (plus ? 'retirés, avec ' : 'retiré, avec ') + taille(b.octets) + ' de restes';
    // rien n'a été supprimé : ni bilan à zéro, ni note sur la Corbeille
    $('#bilan-restes').hidden = !!mode;
    $('#note-sauvegarde').hidden = !!mode;
    $('#bilan-dossiers').textContent = b.dossiers;
    $('#bilan-cles').textContent = b.cles;
    $('#bilan-raccourcis').textContent = b.raccourcis;
    $('#note-sauvegarde').textContent = b.cles > 0 && b.sauvegardes
      ? 'La sauvegarde du registre est dans ' + b.sauvegardes + ' : un double-clic la remet.'
      : 'Les dossiers restent dans la Corbeille tant que vous ne la videz pas.';
    $('#note-admin-restes').hidden = !b.admin_refuse;
  }
  $('#fini-programmes').addEventListener('click', () => { afficher('accueil'); disque(); });

  /* ── Mise à jour (maj.rs), reprise d'Atreia Hub : cherchée 2 s après le lancement, toutes les
     30 min, et au retour sur l'appli si la dernière recherche a plus de 5 min. Une pastille dans
     la barre de titre ; l'appli se ferme pour s'installer, donc on le demande une fois, sur la
     pastille même, et jamais pendant une opération. ── */
  const pastille = $('#maj');
  const MAJ = { etat: 'rien', version: '', erreur: '', recu: 0, total: 0, phase: '', a: 0, confirmer: null, attente: false };
  let recherche = false;
  // une analyse, un nettoyage, une désinstallation ou une suppression en cours : on ne ferme pas l'appli dessous
  const occupee = () => !$('#desinstallation').hidden || !!document.querySelector('.principal[aria-busy="true"]');

  function peindreMaj() {
    const montre = MAJ.etat === 'dispo' || MAJ.etat === 'installe';
    pastille.hidden = !montre;
    if (!montre) return;
    const fleche = dessin(14, ['M12 19 V5', 'M6 11 L12 5 L18 11'], '#FFFFFF', 2.4);
    if (MAJ.etat === 'installe') {
      pastille.disabled = true;
      pastille.setAttribute('aria-busy', 'true');
      const p = MAJ.total ? Math.min(100, Math.floor((MAJ.recu / MAJ.total) * 100)) : null;
      pastille.replaceChildren(fleche, MAJ.phase === 'installation' ? 'Installation…' : 'Téléchargement');
      if (MAJ.phase !== 'installation' && p !== null) pastille.append(' ', element('span', 'pourcent', p + ' %'));
      pastille.title = 'Nettoyeur ' + MAJ.version + ' : il se ferme, s’installe puis se relance';
      return;
    }
    pastille.disabled = false;
    pastille.removeAttribute('aria-busy');
    pastille.replaceChildren(fleche, MAJ.attente ? 'Après l’opération en cours'
      : MAJ.confirmer ? 'Installer et relancer ?'
      : MAJ.erreur ? 'Réessayer la mise à jour'
      : 'Mise à jour ' + MAJ.version);
    pastille.title = MAJ.erreur || 'Nettoyeur ' + MAJ.version + ' est disponible : il se ferme, s’installe puis se relance';
  }

  async function chercherMaj() {
    if (recherche || MAJ.etat === 'installe') return;
    recherche = true;
    try {
      const r = (await invoke('maj_verifier')) || {};
      if (r.disponible) Object.assign(MAJ, { etat: 'dispo', version: String(r.version || '') });
      else Object.assign(MAJ, { etat: 'rien', version: '', erreur: '' });
    } catch (e) {
      // hors ligne ou GitHub en panne : rien ne s'affiche, et une mise à jour déjà trouvée reste
    } finally {
      MAJ.a = Date.now();
      recherche = false;
    }
    peindreMaj();
  }

  pastille.addEventListener('click', async () => {
    if (MAJ.etat !== 'dispo') return;
    if (occupee()) {
      MAJ.attente = true;
      peindreMaj();
      setTimeout(() => { MAJ.attente = false; peindreMaj(); }, 2500);
      return;
    }
    if (!MAJ.confirmer) {
      MAJ.confirmer = setTimeout(() => { MAJ.confirmer = null; peindreMaj(); }, 4000);
      peindreMaj();
      return;
    }
    clearTimeout(MAJ.confirmer);
    Object.assign(MAJ, { etat: 'installe', confirmer: null, recu: 0, total: 0, phase: '', erreur: '' });
    peindreMaj();
    try {
      await invoke('maj_installer');
      // réussie, l'installation ferme l'appli avant de revenir ici
    } catch (e) {
      Object.assign(MAJ, { etat: 'dispo', erreur: message(e) });
      peindreMaj();
    }
  });
  T.event.listen('maj-progression', (e) => {
    const p = e.payload || {};
    if (MAJ.etat !== 'installe') return;
    if (p.phase === 'installation') MAJ.phase = 'installation';
    else { MAJ.recu = +p.recu || 0; MAJ.total = +p.total || 0; }
    peindreMaj();
  });
  setTimeout(() => { chercherMaj(); setInterval(chercherMaj, 30 * 60000); }, 2000);
  const auRetour = () => { if (document.visibilityState === 'visible' && MAJ.a && Date.now() - MAJ.a > 5 * 60000) chercherMaj(); };
  document.addEventListener('visibilitychange', auRetour);
  window.addEventListener('focus', auRetour);

  disque();
})();
