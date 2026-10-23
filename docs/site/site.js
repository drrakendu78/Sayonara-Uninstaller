// SPDX-FileCopyrightText: © 2026 Djamel Lazreg (drrakendu78) <https://djamellazreg.fr>
// SPDX-License-Identifier: LicenseRef-PolyForm-Strict-1.0.0
//
// La page GitHub de Sayonara Uninstaller : son ouverture (le nom syllabe par syllabe, une fois les polices là), ce qui
// se met au net en défilant, le compteur des 91 Go (le ressort de la vidéo), la fenêtre qui devient le logo, et le
// bouton « Télécharger » qui pointe sur l'installeur de la dernière version publiée.
(function () {
  'use strict';
  var reduit = window.matchMedia && matchMedia('(prefers-reduced-motion: reduce)').matches;

  // L'ouverture : elle attend les polices (sinon le nom se mettrait au net dans une autre police), une seconde au plus.
  var ouvrir = function () { document.documentElement.classList.add('pret'); };
  if (document.fonts && document.fonts.ready) {
    Promise.race([document.fonts.ready, new Promise(function (ok) { setTimeout(ok, 1000); })]).then(ouvrir);
  } else ouvrir();

  // La barre du haut : un filet dès qu'on a défilé.
  var haut = document.querySelector('.haut');
  var filet = function () { haut.classList.toggle('defile', window.scrollY > 8); };
  window.addEventListener('scroll', filet, { passive: true });
  filet();

  // Le compteur : de 0 à 91, avec le ressort de la vidéo (raideur 200, amortissement 42, masse 1).
  function compter(el) {
    var cible = +el.dataset.valeur || 0;
    if (reduit) { el.textContent = String(cible); return; }
    var x = 0, v = 0, avant = performance.now();
    function pas(t) {
      var dt = Math.min(0.032, (t - avant) / 1000);
      avant = t;
      var a = 200 * (1 - x) - 42 * v;
      v += a * dt;
      x += v * dt;
      el.textContent = String(Math.round(Math.min(1, x) * cible));
      if (Math.abs(1 - x) > 0.001 || Math.abs(v) > 0.001) requestAnimationFrame(pas);
      else el.textContent = String(cible);
    }
    el.textContent = '0';
    requestAnimationFrame(pas);
  }

  // Ce qui se met au net en défilant (une fois), le compteur, et la fin.
  var compteur = document.getElementById('compteur');
  var vus = document.querySelectorAll('.revele, .fin');
  if (!('IntersectionObserver' in window)) {
    vus.forEach(function (el) { el.classList.add('vu'); });
  } else {
    var guet = new IntersectionObserver(function (entrees) {
      entrees.forEach(function (e) {
        if (!e.isIntersecting) return;
        e.target.classList.add('vu');
        if (compteur && e.target.contains(compteur)) compter(compteur);
        guet.unobserve(e.target);
      });
    }, { threshold: 0.22, rootMargin: '0px 0px -8% 0px' });
    vus.forEach(function (el) { guet.observe(el); });
  }

  // La dernière version publiée : le bouton télécharge directement son installeur, et on dit laquelle. Sans réponse de
  // GitHub (hors ligne, trop de demandes), les liens restent sur la page des publications.
  fetch('https://api.github.com/repos/drrakendu78/Sayonara-Uninstaller/releases/latest', { headers: { Accept: 'application/vnd.github+json' } })
    .then(function (r) { return r.ok ? r.json() : null; })
    .then(function (p) {
      if (!p || !Array.isArray(p.assets)) return;
      var installeur = p.assets.find(function (a) { return /_x64-setup\.exe$/i.test(a.name || ''); });
      if (!installeur || !/^https:\/\/github\.com\//.test(installeur.browser_download_url || '')) return;
      document.querySelectorAll('a.telecharger').forEach(function (a) { a.href = installeur.browser_download_url; });
      var mo = (installeur.size / 1048576).toLocaleString('fr-FR', { maximumFractionDigits: 1 });
      var quand = p.published_at ? new Date(p.published_at).toLocaleDateString('fr-FR', { day: 'numeric', month: 'long', year: 'numeric' }) : '';
      var version = String(p.tag_name || '').replace(/^v/, '');
      document.getElementById('version').textContent = 'Version ' + version + ' · ' + mo + ' Mo' + (quand ? ' · ' + quand : '');
    })
    .catch(function () {});
})();
