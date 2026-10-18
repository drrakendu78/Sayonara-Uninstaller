// SPDX-FileCopyrightText: © 2026 Djamel Lazreg (drrakendu78) <https://djamellazreg.fr>
// SPDX-License-Identifier: LicenseRef-PolyForm-Strict-1.0.0
//
// Publication d'une version de Sayonara Uninstaller sur GitHub : une « release » du dépôt avec l'installeur, sa
// signature et latest.json, que l'appli lit à l'adresse de tauri.conf.json
// (…/releases/latest/download/latest.json). Repris d'Atreia Hub (scripts/publier-maj.mjs).
//
//   node scripts/publier-maj.mjs --build --notes-file scripts/notes-maj/0.2.0.txt   construit (signé) puis publie
//   node scripts/publier-maj.mjs --notes "Ce qui change"                           publie l'installeur déjà construit
//   node scripts/publier-maj.mjs --essai …                                         vérifie tout, ne publie rien
//   --remplacer                                                                    republie une version déjà publiée
//
// Étapes : version lue dans src-tauri/tauri.conf.json (Cargo.toml doit dire la même) ; dépôt propre et
// poussé (la publication pose l'étiquette vX.Y.Z sur ce commit) ; installeur et signature pris dans
// src-tauri/target/release/bundle/nsis ; signature vérifiée contre la clé publique embarquée dans
// l'appli et liée à cette version (requireSignedVersion) ; version en ligne relue (jamais de retour en
// arrière) ; publication en brouillon avec l'installeur et sa signature, latest.json ensuite, puis mise
// en ligne : l'appli ne voit jamais une version dont l'installeur manque ; relecture par HTTPS :
// latest.json servi, installeur retéléchargé, même empreinte, signature revérifiée.
//
// La clé privée reste dans ~/.tauri/sayonara-uninstaller.key (jamais dans le dépôt), sans mot de passe.
// La perdre, c'est ne plus pouvoir mettre à jour les applis installées : la sauvegarder.
// L'installeur est publié sans espace dans son nom (GitHub changerait l'espace en point).
// Publier passe par `gh` (GitHub CLI), connecté au compte propriétaire du dépôt.
import { spawnSync } from 'node:child_process';
import crypto from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { verify } from './minisign.mjs';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const TAURI = path.join(ROOT, 'src-tauri');
const KEY = path.join(os.homedir(), '.tauri', 'sayonara-uninstaller.key');
const PLATFORM = 'windows-x86_64';

const args = process.argv.slice(2);
const flag = (n) => args.includes(n);
const opt = (n) => { const i = args.indexOf(n); return i >= 0 ? args[i + 1] : undefined; };
const die = (m) => { console.error('ÉCHEC : ' + m); process.exit(1); };
const step = (m) => console.log('· ' + m);
const sha256 = (b) => crypto.createHash('sha256').update(b).digest('hex');
const mo = (n) => (n / 1048576).toFixed(1).replace('.', ',') + ' Mo';
const attendre = (ms) => new Promise((r) => setTimeout(r, ms));
/** Compare deux versions x.y.z. */
const cmpVer = (a, b) => {
  const x = String(a).split('.').map((n) => parseInt(n, 10) || 0), y = String(b).split('.').map((n) => parseInt(n, 10) || 0);
  for (let i = 0; i < 3; i++) if ((x[i] || 0) !== (y[i] || 0)) return (x[i] || 0) - (y[i] || 0);
  return 0;
};
const run = (cmd, a) => spawnSync(cmd, a, { cwd: ROOT, encoding: 'utf8' });
const why = (r) => String((r.stderr || '').trim() || (r.error && r.error.message) || `code ${r.status}`);

// ── Version et notes ──
const conf = JSON.parse(fs.readFileSync(path.join(TAURI, 'tauri.conf.json'), 'utf8'));
const version = String(conf.version || '');
if (!/^\d+\.\d+\.\d+$/.test(version)) die(`version illisible dans tauri.conf.json : « ${version} »`);
const cargoVer = (fs.readFileSync(path.join(TAURI, 'Cargo.toml'), 'utf8').match(/^version\s*=\s*"([^"]+)"/m) || [])[1];
if (cargoVer !== version) die(`Cargo.toml dit ${cargoVer}, tauri.conf.json dit ${version} : les aligner`);
const up = (conf.plugins || {}).updater || {};
const endpoint = Array.isArray(up.endpoints) ? up.endpoints[0] : '';
const depot = (/^https:\/\/github\.com\/([\w.-]+\/[\w.-]+)\/releases\/latest\/download\/latest\.json$/.exec(endpoint) || [])[1];
if (!up.pubkey || !depot) die('tauri.conf.json : plugins.updater incomplet (pubkey, adresse https://github.com/…/releases/latest/download/latest.json)');
if (!up.requireSignedVersion) die('tauri.conf.json : requireSignedVersion doit rester à true');
const tag = 'v' + version;
const exeName = `${conf.productName}_${version}_x64-setup.exe`;
const nomPublie = exeName.replace(/\s+/g, '-');
const url = `https://github.com/${depot}/releases/download/${tag}/${nomPublie}`;
let notes = opt('--notes');
if (opt('--notes-file')) notes = fs.readFileSync(path.resolve(opt('--notes-file')), 'utf8');
notes = String(notes || '').replace(/\r/g, '').trim();
if (!notes) die('notes de version manquantes : --notes "…" ou --notes-file fichier');

// ── Dépôt propre et poussé : l'étiquette de la publication va sur ce commit ──
let r = run('git', ['status', '--porcelain', '--untracked-files=no']);
if (r.status !== 0) die('git : ' + why(r));
if (r.stdout.trim()) die('des modifications ne sont pas commitées : commiter avant de publier');
const head = run('git', ['rev-parse', 'HEAD']).stdout.trim();
const branche = run('git', ['rev-parse', '--abbrev-ref', 'HEAD']).stdout.trim();
r = run('git', ['ls-remote', 'origin', `refs/heads/${branche}`]);
if (r.status !== 0) die('dépôt GitHub injoignable : ' + why(r));
if (!r.stdout.startsWith(head)) die(`le commit ${head.slice(0, 7)} n'est pas poussé sur origin/${branche} : pousser avant de publier`);
r = run('gh', ['auth', 'status']);
if (r.status !== 0) die('gh (GitHub CLI) n’est pas connecté : gh auth login');
step(`dépôt ${depot} : ${branche} propre et poussé (${head.slice(0, 7)})`);

// ── Construction (facultative) ──
if (flag('--build')) {
  if (!fs.existsSync(KEY)) die('clé privée absente : ' + KEY);
  step(`construction signée de la version ${version} (cargo tauri build)…`);
  const b = spawnSync('cargo', ['tauri', 'build'], {
    cwd: TAURI,
    stdio: 'inherit',
    env: { ...process.env, TAURI_SIGNING_PRIVATE_KEY: fs.readFileSync(KEY, 'utf8').trim(), TAURI_SIGNING_PRIVATE_KEY_PASSWORD: '' },
  });
  if (b.status !== 0) die('la construction a échoué');
}

// ── Installeur et signature ──
const nsis = path.join(TAURI, 'target', 'release', 'bundle', 'nsis');
const exe = path.join(nsis, exeName);
if (!fs.existsSync(exe) || !fs.existsSync(exe + '.sig')) die(`installeur ou signature absents pour ${version} : construire avec --build`);
const sig = fs.readFileSync(exe + '.sig', 'utf8').trim();
const bytes = fs.readFileSync(exe);
let checked;
try { checked = verify(bytes, sig, up.pubkey); } catch (e) { die('signature : ' + e.message); }
const signedFor = (checked.comment.split('\t').find((f) => f.startsWith('version:')) || '').slice('version:'.length);
if (signedFor !== version) die(`signature liée à « ${signedFor || 'aucune version'} », pas à ${version} : l'appli la refuserait`);
const sum = sha256(bytes);
step(`${exeName} · ${mo(bytes.length)} · signature valide (clé ${checked.keyId}, version ${signedFor})`);

// ── Ce qui est en ligne ──
const res = await fetch(endpoint, { cache: 'no-store' }).catch((e) => die('GitHub injoignable : ' + e.message));
let live = null;
if (res.status !== 404) {
  if (!res.ok) die(`${endpoint} : HTTP ${res.status}`);
  live = await res.json().catch(() => die('latest.json en ligne illisible'));
}
step(live ? `en ligne : ${live.version}` : 'en ligne : rien de publié');
if (live && cmpVer(live.version, version) > 0) die(`GitHub annonce déjà ${live.version}, plus récente que ${version}`);
if (live && cmpVer(live.version, version) === 0 && !flag('--remplacer')) die(`${version} est déjà publiée (--remplacer pour la republier)`);
// une publication vX.Y.Z existe-t-elle déjà, brouillon compris (un essai interrompu) ?
const existe = run('gh', ['release', 'view', tag, '--repo', depot, '--json', 'tagName']).status === 0;
if (existe && !flag('--remplacer')) die(`la publication ${tag} existe déjà sur GitHub (--remplacer pour la republier)`);

const manifest = {
  version,
  notes,
  pub_date: new Date().toISOString().replace(/\.\d{3}Z$/, 'Z'),
  platforms: { [PLATFORM]: { signature: sig, url } },
};
if (flag('--essai')) {
  console.log('\nEssai : rien n’a été publié. latest.json prévu :\n' + JSON.stringify(manifest, null, 2));
  process.exit(0);
}

// ── Publication : brouillon avec l'installeur et sa signature, latest.json, puis mise en ligne ──
const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'sayonara-maj-'));
try {
  const fichierNotes = path.join(tmp, 'notes.md');
  fs.writeFileSync(fichierNotes, notes + '\n');
  const latest = path.join(tmp, 'latest.json');
  fs.writeFileSync(latest, JSON.stringify(manifest, null, 2) + '\n');
  // les mêmes octets, sous le nom publié
  const exePublie = path.join(tmp, nomPublie);
  fs.copyFileSync(exe, exePublie);
  fs.copyFileSync(exe + '.sig', exePublie + '.sig');
  r = existe
    ? run('gh', ['release', 'upload', tag, exePublie, exePublie + '.sig', '--repo', depot, '--clobber'])
    : run('gh', ['release', 'create', tag, exePublie, exePublie + '.sig', '--repo', depot, '--draft', '--target', head,
      '--title', `${conf.productName} ${version}`, '--notes-file', fichierNotes]);
  if (r.status !== 0) die('envoi de l’installeur : ' + why(r));
  step(`${tag} : installeur et signature envoyés${existe ? ' (remplacés)' : ' (brouillon)'}`);
  r = run('gh', ['release', 'upload', tag, latest, '--repo', depot, '--clobber']);
  if (r.status !== 0) die('envoi de latest.json : ' + why(r));
  r = run('gh', ['release', 'edit', tag, '--repo', depot, '--draft=false', '--latest']);
  if (r.status !== 0) die('mise en ligne : ' + why(r));
  step(`${tag} en ligne`);
} finally {
  fs.rmSync(tmp, { recursive: true, force: true });
}

// ── Relecture par HTTPS, comme l'appli la fera (GitHub met parfois plus de 2 min à suivre « latest » : 0.1.1) ──
let served = null;
for (let i = 0; i < 60 && !served; i++) {
  const s = await fetch(endpoint, { cache: 'no-store' }).then((x) => (x.ok ? x.json() : null)).catch(() => null);
  if (s && s.version === version) served = s;
  else await attendre(5000);
}
if (!served) die(`latest.json servi n'annonce pas ${version} au bout de 5 min (la publication, elle, est en ligne)`);
const p = (served.platforms || {})[PLATFORM] || {};
if (p.signature !== sig || p.url !== url) die('latest.json servi différent de celui envoyé');
const dl = await fetch(p.url, { cache: 'no-store' });
if (!dl.ok) die(`${p.url} : HTTP ${dl.status}`);
const got = Buffer.from(await dl.arrayBuffer());
if (sha256(got) !== sum) die('installeur servi différent de l’installeur local');
try { verify(got, p.signature, up.pubkey); } catch (e) { die('signature de l’installeur servi : ' + e.message); }
step(`relu par HTTPS : ${p.url} · ${mo(got.length)} · empreinte et signature conformes`);
console.log(`\n${conf.productName} ${version} est publiée : les applis installées la proposent à leur prochaine recherche (au lancement, puis toutes les 30 min).`);
