// SPDX-FileCopyrightText: © 2026 Djamel Lazreg (drrakendu78) <https://djamellazreg.fr>
// SPDX-License-Identifier: LicenseRef-PolyForm-Strict-1.0.0
//
// L'icône d'un programme, pour la liste « Vos programmes » : extraite par Windows (64 px), puis
// rendue en PNG (encodeur minimal, sans compression) et en `data:` URL — l'interface n'a ainsi
// jamais à lire un fichier du disque.

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

use windows_sys::Win32::Graphics::Gdi::{
    CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetObjectW, BITMAP, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS,
};
use windows_sys::Win32::UI::Shell::SHDefExtractIconW;
use windows_sys::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO};

const TAILLE: u32 = 64;

/// « C:\App\app.exe,0 » → l'icône en `data:image/png;base64,…`, ou rien.
pub fn icone(source: &str) -> Option<String> {
    let (fichier, index) = match source.rsplit_once(',') {
        Some((f, i)) if i.trim().parse::<i32>().is_ok() => (f.trim().trim_matches('"'), i.trim().parse::<i32>().unwrap_or(0)),
        _ => (source.trim().trim_matches('"'), 0),
    };
    let chemin = Path::new(fichier);
    let ext = chemin.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    if !chemin.is_absolute() || !matches!(ext.as_str(), "exe" | "ico" | "dll") || !chemin.is_file() {
        return None;
    }
    let large: Vec<u16> = OsStr::new(fichier).encode_wide().chain(Some(0)).collect();
    let mut grande: HICON = std::ptr::null_mut();
    let hr = unsafe { SHDefExtractIconW(large.as_ptr(), index, 0, &mut grande, std::ptr::null_mut(), TAILLE) };
    if hr != 0 || grande.is_null() {
        return None;
    }
    let pixels = unsafe { rgba(grande) };
    unsafe {
        DestroyIcon(grande);
    }
    let (l, h, rgba) = pixels?;
    Some(format!("data:image/png;base64,{}", base64(&png(l, h, &rgba))))
}

/// Les pixels RGBA d'une icône (alpha reconstitué depuis le masque pour les vieilles icônes).
unsafe fn rgba(icone: HICON) -> Option<(u32, u32, Vec<u8>)> {
    let mut info: ICONINFO = std::mem::zeroed();
    if GetIconInfo(icone, &mut info) == 0 {
        return None;
    }
    let resultat = (|| {
        if info.hbmColor.is_null() {
            return None;
        }
        let mut bm: BITMAP = std::mem::zeroed();
        if GetObjectW(info.hbmColor, std::mem::size_of::<BITMAP>() as i32, &mut bm as *mut _ as *mut core::ffi::c_void) == 0 {
            return None;
        }
        let (l, h) = (bm.bmWidth as u32, bm.bmHeight as u32);
        if l == 0 || h == 0 || l > 256 || h > 256 {
            return None;
        }
        let dc = CreateCompatibleDC(std::ptr::null_mut());
        let lire = |bitmap| {
            let mut bi: BITMAPINFO = std::mem::zeroed();
            bi.bmiHeader = BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: l as i32,
                biHeight: -(h as i32), // de haut en bas
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..std::mem::zeroed()
            };
            let mut buf = vec![0u8; (l * h * 4) as usize];
            let n = GetDIBits(dc, bitmap, 0, h, buf.as_mut_ptr() as *mut core::ffi::c_void, &mut bi, DIB_RGB_COLORS);
            if n == 0 { None } else { Some(buf) }
        };
        let couleur = lire(info.hbmColor);
        let masque = if info.hbmMask.is_null() { None } else { lire(info.hbmMask) };
        DeleteDC(dc);
        let mut px = couleur?;
        let sans_alpha = px.chunks_exact(4).all(|p| p[3] == 0);
        for (i, p) in px.chunks_exact_mut(4).enumerate() {
            p.swap(0, 2); // BGRA → RGBA
            if sans_alpha {
                // vieux format : le masque dit ce qui est transparent (blanc) ou opaque (noir)
                let opaque = masque.as_ref().map(|m| m[i * 4] == 0).unwrap_or(true);
                p[3] = if opaque { 255 } else { 0 };
            }
        }
        Some((l, h, px))
    })();
    if !info.hbmColor.is_null() {
        DeleteObject(info.hbmColor);
    }
    if !info.hbmMask.is_null() {
        DeleteObject(info.hbmMask);
    }
    resultat
}

fn crc32(donnees: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &o in donnees {
        crc ^= o as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

/// Un PNG RGBA 8 bits, données « stockées » (deflate sans compression) : simple et sûr.
pub fn png(largeur: u32, hauteur: u32, rgba: &[u8]) -> Vec<u8> {
    let mut brut = Vec::with_capacity(rgba.len() + hauteur as usize);
    for ligne in rgba.chunks_exact((largeur * 4) as usize) {
        brut.push(0); // filtre « aucun »
        brut.extend_from_slice(ligne);
    }
    let mut zlib = vec![0x78, 0x01];
    let blocs: Vec<&[u8]> = brut.chunks(65_535).collect();
    for (i, bloc) in blocs.iter().enumerate() {
        zlib.push(if i + 1 == blocs.len() { 1 } else { 0 });
        let n = bloc.len() as u16;
        zlib.extend_from_slice(&n.to_le_bytes());
        zlib.extend_from_slice(&(!n).to_le_bytes());
        zlib.extend_from_slice(bloc);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &o in &brut {
        a = (a + o as u32) % 65_521;
        b = (b + a) % 65_521;
    }
    zlib.extend_from_slice(&((b << 16) | a).to_be_bytes());

    let mut sortie = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut morceau = |genre: &[u8; 4], donnees: &[u8]| {
        sortie.extend_from_slice(&(donnees.len() as u32).to_be_bytes());
        let mut c = genre.to_vec();
        c.extend_from_slice(donnees);
        sortie.extend_from_slice(&c);
        sortie.extend_from_slice(&crc32(&c).to_be_bytes());
    };
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&largeur.to_be_bytes());
    ihdr.extend_from_slice(&hauteur.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // 8 bits, RGBA
    morceau(b"IHDR", &ihdr);
    morceau(b"IDAT", &zlib);
    morceau(b"IEND", &[]);
    sortie
}

pub fn base64(donnees: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(donnees.len() * 4 / 3 + 4);
    for c in donnees.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        s.push(T[(n >> 18) as usize & 63] as char);
        s.push(T[(n >> 12) as usize & 63] as char);
        s.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        s.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_de_reference() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn crc32_de_reference() {
        assert_eq!(crc32(b"IEND"), 0xAE42_6082);
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn png_valide() {
        let p = png(2, 2, &[255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 0, 0, 0, 0]);
        assert_eq!(&p[..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
        assert_eq!(&p[p.len() - 8..p.len() - 4], b"IEND");
    }

    #[test]
    fn icone_d_un_programme_de_windows() {
        let notepad = std::env::var("SystemRoot").unwrap_or("C:\\Windows".into()) + "\\System32\\notepad.exe";
        let url = icone(&format!("{notepad},0")).expect("icône du Bloc-notes");
        assert!(url.starts_with("data:image/png;base64,iVBORw0KGgo"));
        assert!(icone("C:\\nexiste\\pas.exe").is_none());
        assert!(icone("relatif.exe").is_none());
    }
}
