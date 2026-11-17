//! Lingue: un file `lang/<codice>.lang` per lingua, righe `chiave = testo`, compilati dentro l'exe.
//! `{nome}` nel testo è un segnaposto che `fill` sostituisce. Chiave mancante: inglese, poi italiano.
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

pub const LANGS: [(&str, &str); 7] = [
    ("it", "Italiano"),
    ("en", "English"),
    ("fr", "Français"),
    ("de", "Deutsch"),
    ("es", "Español"),
    ("pl", "Polski"),
    ("pt", "Português"),
];
const FILES: [&str; 7] = [
    include_str!("../lang/it.lang"),
    include_str!("../lang/en.lang"),
    include_str!("../lang/fr.lang"),
    include_str!("../lang/de.lang"),
    include_str!("../lang/es.lang"),
    include_str!("../lang/pl.lang"),
    include_str!("../lang/pt.lang"),
];
const EN: usize = 1;
static CUR: AtomicUsize = AtomicUsize::new(EN);
static TABLES: [OnceLock<HashMap<&'static str, &'static str>>; 7] = [const { OnceLock::new() }; 7];

fn table(i: usize) -> &'static HashMap<&'static str, &'static str> {
    TABLES[i].get_or_init(|| {
        FILES[i]
            .lines()
            .filter(|l| !l.starts_with('#'))
            .filter_map(|l| l.trim_end_matches('\r').split_once(" = "))
            .map(|(k, v)| (k.trim(), v))
            .collect()
    })
}

/// Testo della chiave nella lingua scelta.
pub fn tr(key: &'static str) -> &'static str {
    let i = CUR.load(Ordering::Relaxed);
    table(i).get(key).or_else(|| table(EN).get(key)).or_else(|| table(0).get(key)).copied().unwrap_or(key)
}

/// Sostituisce i segnaposto `{nome}`.
pub fn fill(text: &str, args: &[(&str, String)]) -> String {
    let mut s = text.to_string();
    for (k, v) in args {
        s = s.replace(&format!("{{{k}}}"), v);
    }
    s
}

pub fn current() -> usize {
    CUR.load(Ordering::Relaxed)
}

fn saved_path() -> Option<std::path::PathBuf> {
    crate::app::data_dir().map(|d| d.join("lang.txt"))
}

/// Cambia lingua e la ricorda per il prossimo avvio.
pub fn set(i: usize) {
    CUR.store(i.min(LANGS.len() - 1), Ordering::Relaxed);
    if let Some(p) = saved_path() {
        let _ = std::fs::create_dir_all(p.parent().unwrap_or(&p));
        let _ = std::fs::write(p, LANGS[i].0);
    }
}

/// All'avvio: la lingua scelta l'ultima volta, altrimenti quella di Windows, altrimenti inglese.
pub fn init() {
    let saved = saved_path().and_then(|p| std::fs::read_to_string(p).ok());
    let code = saved.unwrap_or_else(|| {
        let mut buf = [0u16; 85];
        let n = unsafe { windows_sys::Win32::Globalization::GetUserDefaultLocaleName(buf.as_mut_ptr(), buf.len() as i32) };
        String::from_utf16_lossy(&buf[..(n.max(1) - 1) as usize])
    });
    let code = code.trim().get(..2).unwrap_or("en").to_lowercase();
    CUR.store(LANGS.iter().position(|l| l.0 == code).unwrap_or(EN), Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_language_has_every_key_and_placeholder() {
        let it = super::table(0);
        for (i, (code, _)) in super::LANGS.iter().enumerate().skip(1) {
            let tbl = super::table(i);
            for (k, v) in it {
                let tr = tbl.get(k).unwrap_or_else(|| panic!("{code}: manca {k}"));
                for ph in v.split('{').skip(1).filter_map(|p| p.split_once('}')).map(|p| p.0) {
                    assert!(tr.contains(&format!("{{{ph}}}")), "{code}: {k} ha perso {{{ph}}}");
                }
            }
        }
    }

    #[test]
    fn fill_replaces_names() {
        assert_eq!(super::fill("tra {d} giorni · {0}", &[("d", "3".into()), ("0", "x".into())]), "tra 3 giorni · x");
    }
}
