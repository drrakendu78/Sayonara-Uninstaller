// SPDX-FileCopyrightText: © 2026 Djamel Lazreg (drrakendu78) <https://djamellazreg.fr>
// SPDX-License-Identifier: LicenseRef-PolyForm-Strict-1.0.0
//
// Repris d'Atreia Hub (scripts/minisign.mjs), à l'identique.
// Vérification d'une signature de mise à jour Tauri (format minisign), sans dépendance : Ed25519 sur
// l'empreinte BLAKE2b-512 du fichier, puis le commentaire de confiance signé avec la signature.
// Sert au script de publication (scripts/publier-maj.mjs) : rien ne part sur le serveur sans être
// vérifié contre la clé publique embarquée dans l'appli (src-tauri/tauri.conf.json).
import crypto from 'node:crypto';
import fs from 'node:fs';

const b64 = (s) => Buffer.from(String(s).trim(), 'base64');
const lines = (txt) => String(txt).replace(/\r/g, '').split('\n').filter((l) => l.length);

/** Clé publique Tauri (texte minisign encodé en base64) → { id, key } (clé Ed25519 brute de 32 octets). */
export function readPublicKey(tauriPub) {
  const txt = b64(tauriPub).toString('utf8');
  const raw = b64(lines(txt)[1]);
  if (raw.length !== 42 || raw.subarray(0, 2).toString('latin1') !== 'Ed') throw new Error('clé publique illisible');
  const der = Buffer.concat([Buffer.from('302a300506032b6570032100', 'hex'), raw.subarray(10)]);
  return { id: raw.subarray(2, 10).toString('hex'), key: crypto.createPublicKey({ key: der, format: 'der', type: 'spki' }) };
}

/**
 * Vérifie `file` (chemin, ou contenu en Buffer) contre sa signature Tauri (`.sig`, texte minisign encodé
 * en base64). Lève une erreur sinon.
 */
export function verify(file, tauriSig, tauriPub) {
  const pub = readPublicKey(tauriPub);
  const [, sigLine, trusted, globalLine] = lines(b64(tauriSig).toString('utf8'));
  const sig = b64(sigLine);
  if (sig.length !== 74 || sig.subarray(0, 2).toString('latin1') !== 'ED') throw new Error('signature illisible (attendu : empreinte BLAKE2b)');
  if (sig.subarray(2, 10).toString('hex') !== pub.id) throw new Error('signature faite avec une autre clé');
  const digest = crypto.createHash('blake2b512').update(Buffer.isBuffer(file) ? file : fs.readFileSync(file)).digest();
  if (!crypto.verify(null, digest, pub.key, sig.subarray(10))) throw new Error('signature fausse : le fichier a changé ou la clé ne correspond pas');
  const comment = String(trusted || '').replace(/^trusted comment: /, '');
  if (!crypto.verify(null, Buffer.concat([sig.subarray(10), Buffer.from(comment, 'utf8')]), pub.key, b64(globalLine))) throw new Error('commentaire de confiance altéré');
  return { keyId: pub.id, comment };
}

// En ligne de commande : node scripts/minisign.mjs <fichier> <fichier.sig> <clé publique (texte ou fichier)>
if (process.argv[1] && process.argv[1].endsWith('minisign.mjs') && process.argv.length >= 5) {
  const [file, sig, pub] = process.argv.slice(2);
  const read = (x) => (fs.existsSync(x) ? fs.readFileSync(x, 'utf8') : x);
  try {
    const r = verify(file, read(sig), read(pub));
    console.log(`signature valide · clé ${r.keyId} · ${r.comment}`);
  } catch (e) {
    console.error('ÉCHEC : ' + e.message);
    process.exit(1);
  }
}
