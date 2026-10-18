// SPDX-FileCopyrightText: © 2026 Djamel Lazreg (drrakendu78) <https://djamellazreg.fr>
// SPDX-License-Identifier: LicenseRef-PolyForm-Strict-1.0.0
//
// Lancer le désinstalleur d'un programme et attendre qu'il ait VRAIMENT fini. Beaucoup se
// recopient dans Temp puis se relancent (NSIS « Au_.exe », Inno « _iu14D2N.tmp », msiexec) : le
// premier processus se termine aussitôt. On suit donc ses descendants, puis on regarde si la
// clé « Uninstall » du programme a disparu : c'est elle qui dit s'il est parti.

use std::collections::HashSet;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{CloseHandle, FILETIME, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS};
use windows_sys::Win32::System::SystemInformation::GetSystemTimeAsFileTime;
use windows_sys::Win32::System::Threading::{GetProcessTimes, OpenProcess, WaitForSingleObject, PROCESS_QUERY_LIMITED_INFORMATION};
use windows_sys::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

use crate::programmes::{self, Commande, Programme};

/// Au-delà, on rend la main : le désinstalleur attend peut-être une réponse oubliée.
const ATTENTE_MAX: Duration = Duration::from_secs(30 * 60);

/// « Passer » : l'utilisateur ne veut plus attendre ce programme-ci (fenêtre fermée de travers,
/// lien Steam annulé…). Relevé par la boucle d'attente, remis à zéro à chaque programme.
pub static PASSER: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Issue {
    /// la clé du programme a disparu
    Retire,
    /// le désinstalleur s'est fermé, le programme est toujours là (annulé, ou échec)
    Annule,
    /// impossible de lancer la désinstallation
    Echec,
}

fn large(s: &OsStr) -> Vec<u16> {
    s.encode_wide().chain(Some(0)).collect()
}

fn maintenant_ft() -> u64 {
    let mut ft = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
    unsafe { GetSystemTimeAsFileTime(&mut ft) };
    ((ft.dwHighDateTime as u64) << 32) | ft.dwLowDateTime as u64
}

/// Les processus (pid, parent, heure de création), à l'instant.
fn processus() -> Vec<(u32, u32, u64)> {
    let mut v = Vec::new();
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return v;
        }
        let mut e: PROCESSENTRY32W = std::mem::zeroed();
        e.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut ok = Process32FirstW(snap, &mut e);
        while ok != 0 {
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, e.th32ProcessID);
            let mut creation = 0u64;
            if !h.is_null() {
                let (mut c, mut x, mut k, mut u) = (std::mem::zeroed::<FILETIME>(), std::mem::zeroed::<FILETIME>(), std::mem::zeroed::<FILETIME>(), std::mem::zeroed::<FILETIME>());
                if GetProcessTimes(h, &mut c, &mut x, &mut k, &mut u) != 0 {
                    creation = ((c.dwHighDateTime as u64) << 32) | c.dwLowDateTime as u64;
                }
                CloseHandle(h);
            }
            v.push((e.th32ProcessID, e.th32ParentProcessID, creation));
            ok = Process32NextW(snap, &mut e);
        }
        CloseHandle(snap);
    }
    v
}

fn lancer(p: &Programme) -> Option<(Option<HANDLE>, u32)> {
    let (fichier, params) = match programmes::commande(&p.desinstaller)? {
        Commande::Programme { exe, args } => (large(exe.as_os_str()), large(OsStr::new(&args))),
        Commande::Adresse(a) => (large(OsStr::new(&a)), large(OsStr::new(""))),
    };
    let verbe = large(OsStr::new("open"));
    unsafe {
        let mut info: SHELLEXECUTEINFOW = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        info.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC;
        info.lpVerb = verbe.as_ptr();
        info.lpFile = fichier.as_ptr();
        info.lpParameters = params.as_ptr();
        info.nShow = SW_SHOWNORMAL as i32;
        if ShellExecuteExW(&mut info) == 0 {
            return None;
        }
        if info.hProcess.is_null() {
            return Some((None, 0));
        }
        let pid = windows_sys::Win32::System::Threading::GetProcessId(info.hProcess);
        Some((Some(info.hProcess), pid))
    }
}

/// Désinstalle un programme (son propre désinstalleur, avec ses questions) et dit s'il est parti.
pub fn desinstaller(p: &Programme) -> Issue {
    use std::sync::atomic::Ordering;
    PASSER.store(false, Ordering::SeqCst);
    let debut_ft = maintenant_ft();
    let debut = Instant::now();
    let Some((poignee, pid)) = lancer(p) else { return Issue::Echec };
    // la famille du désinstalleur : lui, puis tout processus né APRÈS le lancement d'un parent connu
    let mut famille: HashSet<u32> = HashSet::new();
    if pid != 0 {
        famille.insert(pid);
    }
    let mut calme = 0u32;
    loop {
        std::thread::sleep(Duration::from_millis(400));
        let liste = processus();
        let mut vivants = 0;
        let mut ajoutes = true;
        while ajoutes {
            ajoutes = false;
            for &(id, parent, creation) in &liste {
                if !famille.contains(&id) && famille.contains(&parent) && creation >= debut_ft {
                    famille.insert(id);
                    ajoutes = true;
                }
            }
        }
        for &(id, _, creation) in &liste {
            if famille.contains(&id) && (id != pid || creation >= debut_ft || creation == 0) {
                vivants += 1;
            }
        }
        if let Some(h) = poignee {
            if unsafe { WaitForSingleObject(h, 0) } != 0 {
                vivants += 1; // le premier tourne encore
            }
        }
        let parti = !programmes::encore_installe(p);
        // fini : plus personne de la famille depuis ~2 s (ou la clé a disparu et la famille s'est tue)
        if vivants == 0 {
            calme += 1;
        } else {
            calme = 0;
        }
        if (calme >= 5) || (parti && calme >= 2) || debut.elapsed() > ATTENTE_MAX || PASSER.load(Ordering::SeqCst) {
            break;
        }
        // une adresse (steam://…) : pas de processus à suivre — on attend que la clé disparaisse
        if pid == 0 && !parti && debut.elapsed() < ATTENTE_MAX {
            calme = 0;
        }
    }
    if let Some(h) = poignee {
        unsafe { CloseHandle(h) };
    }
    if programmes::encore_installe(p) { Issue::Annule } else { Issue::Retire }
}
