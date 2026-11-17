//! Controllo in background: ogni minuto guarda tutte le macchine e avvisa quando qualcosa cambia.
//! VPS: stato + età dell'ultima metrica (niente dati da 7 minuti = spenta o piantata).
//! Dedicati: `state` e `powerState`.
use crate::i18n::{fill, tr};
use crate::app::Msg;
use crate::ovh::Client;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone)]
pub struct Alert {
    pub when: String,
    pub machine: String,
    pub text: String,
    pub bad: bool,
}

pub fn local_hm() -> String {
    let mut t: windows_sys::Win32::Foundation::SYSTEMTIME = unsafe { std::mem::zeroed() };
    unsafe { windows_sys::Win32::System::SystemInformation::GetLocalTime(&mut t) };
    format!("{:02}:{:02}", t.wHour, t.wMinute)
}

fn short(s: &str) -> &str {
    s.split('.').next().unwrap_or(s)
}

fn check_vps(c: &Client, s: &str) -> (bool, String) {
    let st = match c.get(&format!("/vps/{s}")) {
        Ok(v) => v["state"].as_str().unwrap_or("?").to_string(),
        Err(e) => return (true, fill(tr("watch.non_leggibile"), &[("e", e.to_string())])), // errore API: non è la macchina, non avvisare
    };
    if st != "running" {
        return (false, fill(tr("watch.stato"), &[("0", (crate::ui::state_it(&st)).to_string())]));
    }
    let Ok((ep, tok)) = crate::vps::metrics_token(c, s, false) else { return (true, "ok".into()) };
    let now = c.server_now();
    let q = format!("rec_libvirtd_domain_cpu_usage{{service_name=\"{s}\"}}");
    match c.prom_range(&ep, &tok, &q, now - 900, now, 60) {
        Ok(pts) => match pts.last() {
            Some(l) if now - l.0 <= 420 => (true, "ok".into()),
            Some(l) => (false, fill(tr("watch.accesa_ma_nessuna_metrica_da_2"), &[("0", ((now - l.0) / 60).to_string())])),
            None => (false, tr("watch.accesa_ma_nessuna_metrica_da").into()),
        },
        Err(_) => (true, "ok".into()),
    }
}

fn check_dedi(c: &Client, s: &str) -> (bool, String) {
    match c.get(&format!("/dedicated/server/{s}")) {
        Ok(v) => {
            let st = v["state"].as_str().unwrap_or("?");
            let pw = v["powerState"].as_str().unwrap_or("poweron");
            if st != "ok" {
                (false, fill(tr("watch.stato"), &[("0", (crate::ui::state_it(st)).to_string())]))
            } else if pw != "poweron" {
                (false, tr("ui.spento").into())
            } else {
                (true, "ok".into())
            }
        }
        Err(e) => (true, fill(tr("watch.non_leggibile"), &[("e", e.to_string())])),
    }
}

fn list(c: &Client, path: &str) -> Vec<String> {
    c.get(path).ok().and_then(|v| serde_json::from_value::<Vec<String>>(v).ok()).unwrap_or_default()
}

/// Gira finché `epoch` resta `mine` (scollegare l'account lo cambia).
pub fn spawn(c: Client, tx: Sender<Msg>, ctx: eframe::egui::Context, epoch: Arc<AtomicU64>, mine: u64) {
    std::thread::spawn(move || {
        let mut prev: HashMap<String, bool> = HashMap::new();
        loop {
            if epoch.load(Ordering::Relaxed) != mine {
                return;
            }
            let mut cur: Vec<(String, bool, String)> = Vec::new();
            let vps = list(&c, "/vps");
            let dedi = list(&c, "/dedicated/server");
            std::thread::scope(|sc| {
                let hs: Vec<_> = vps
                    .iter()
                    .map(|s| (s.clone(), sc.spawn(|| check_vps(&c, s))))
                    .chain(dedi.iter().map(|s| (s.clone(), sc.spawn(|| check_dedi(&c, s)))))
                    .collect();
                for (s, h) in hs {
                    let (ok, text) = h.join().unwrap_or((true, String::new()));
                    cur.push((s, ok, text));
                }
            });
            for (s, ok, text) in cur {
                let was = prev.insert(s.clone(), ok);
                // all'avvio avvisa solo se è già giù; dopo, a ogni cambio
                let changed = match was {
                    None => !ok,
                    Some(w) => w != ok,
                };
                if changed {
                    let text = if ok { tr("watch.di_nuovo_ok").to_string() } else { text };
                    crate::tray::notify(short(&s), &text, !ok);
                    let _ = tx.send(Msg::Alert(Alert { when: local_hm(), machine: s, text, bad: !ok }));
                    ctx.request_repaint();
                }
            }
            for _ in 0..60 {
                std::thread::sleep(Duration::from_secs(1));
                if epoch.load(Ordering::Relaxed) != mine {
                    return;
                }
            }
        }
    });
}
