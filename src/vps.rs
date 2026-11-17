//! VPS: stato, risorse (Mimir), backup automatico, snapshot, attività, reinstallazione.
use crate::i18n::{fill, tr};
use crate::app::{par_map, Action, App, Data};
use crate::ovh::Client;
use crate::ui::{self, col, h, muted};
use eframe::egui::{self, vec2, RichText};
use serde_json::{json, Value};

/// (durata, passo) in secondi
pub fn range_of(period: &str) -> (i64, i64) {
    match period {
        "1h" => (3600, 30),
        "lastweek" => (7 * 86400, 3600),
        _ => (86400, 300),
    }
}

fn fetch_info(c: &Client, s: &str) -> Result<Value, String> {
    let p = format!("/vps/{s}");
    std::thread::scope(|sc| {
        let vps = sc.spawn(|| c.get(&p));
        let svc = sc.spawn(|| c.get(&format!("{p}/serviceInfos")));
        let ips = sc.spawn(|| c.get(&format!("{p}/ips")));
        let dc = sc.spawn(|| c.get(&format!("{p}/datacenter")).unwrap_or(Value::Null));
        let os = sc.spawn(|| c.get(&format!("{p}/images/current")).unwrap_or(Value::Null));
        Ok(json!({
            "vps": vps.join().unwrap()?,
            "svc": svc.join().unwrap().unwrap_or(Value::Null),
            "ips": ips.join().unwrap().unwrap_or(Value::Null),
            "dc": dc.join().unwrap(),
            "os": os.join().unwrap(),
        }))
    })
}

const KIB_MB: f64 = 1.0 / 1024.0;

/// Token Mimir per VPS, riusato per 10 minuti (app e watcher insieme): chiederlo a ogni grafico
/// moltiplica le chiamate e OVH a un certo punto le rifiuta.
pub fn metrics_token(c: &Client, s: &str, fresh: bool) -> Result<(String, String), String> {
    use std::sync::{Mutex, OnceLock};
    static CACHE: OnceLock<Mutex<std::collections::HashMap<String, (String, String, i64)>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    let now = c.server_now();
    if !fresh {
        if let Some((ep, tok, at)) = cache.lock().unwrap().get(s) {
            if now - at < 600 {
                return Ok((ep.clone(), tok.clone()));
            }
        }
    }
    let t = c.get(&format!("/vps/{s}/metricsToken"))?;
    let (ep, tok) = (ui::sv(&t["endpoint"]), ui::sv(&t["token"]));
    cache.lock().unwrap().insert(s.to_string(), (ep.clone(), tok.clone(), now));
    Ok((ep, tok))
}

/// Le VPS recenti (VPS-2 2027) rispondono 500 a /vps/{s}/monitoring e 403 a /status:
/// CPU, RAM e rete stanno su Mimir, col token di /vps/{s}/metricsToken.
/// Ogni query deve filtrare `service_name` (LBAC) o Mimir la rifiuta.
pub fn fetch_metrics(c: &Client, s: &str, period: &str) -> Result<Value, String> {
    let (mut ep, mut tok) = metrics_token(c, s, false)?;
    let (span, step) = range_of(period);
    let end = c.server_now() / step * step;
    let start = end - span;
    let sel = format!("{{service_name=\"{s}\"}}");
    // (nome, query, fattore, unità); "RAM totale" serve solo come massimo della RAM
    let qs = [
        ("CPU", format!("rec_libvirtd_domain_cpu_usage{sel}"), 1.0, "%"),
        ("RAM", format!("rec_libvirtd_domain_balloon_rss{sel}"), KIB_MB, "MB"),
        ("RAM totale", format!("rec_libvirtd_domain_balloon_current{sel}"), KIB_MB, "MB"),
        ("Rete in", format!("sum(rate(rec_libvirtd_domain_net_rx_bytes{sel}[5m]))"), 1.0 / 1024.0, "KB/s"),
        ("Rete out", format!("sum(rate(rec_libvirtd_domain_net_tx_bytes{sel}[5m]))"), 1.0 / 1024.0, "KB/s"),
    ];
    let mut res = par_map(&qs, 5, |q| c.prom_range(&ep, &tok, &q.1, start, end, step));
    // token scaduto o revocato: uno nuovo e si riprova una volta
    if res.iter().all(|r| r.as_ref().is_err_and(|e| e.starts_with("401") || e.starts_with("403"))) {
        (ep, tok) = metrics_token(c, s, true)?;
        res = par_map(&qs, 5, |q| c.prom_range(&ep, &tok, &q.1, start, end, step));
    }
    let now = c.server_now();
    let ram_max = res[2].as_ref().ok().and_then(|p| p.last()).map(|p| p.1 * KIB_MB);
    let rows: Vec<Value> = qs
        .iter()
        .zip(&res)
        .filter(|(q, _)| q.0 != "RAM totale")
        .map(|((label, _, k, unit), r)| match r {
            Err(e) => json!({"label": label, "err": e}),
            Ok(pts) => {
                // griglia fissa: dove Mimir non ha il dato resta null e la linea si interrompe
                let mut used = vec![Value::Null; (span / step + 1) as usize];
                for (ts, v) in pts {
                    if let Some(slot) = usize::try_from((ts - start) / step).ok().and_then(|i| used.get_mut(i)) {
                        *slot = json!(v * k);
                    }
                }
                let max = if *label == "RAM" { ram_max } else if *unit == "%" { Some(100.0) } else { None };
                json!({"label": label, "unit": unit, "used": used, "max": max, "age": pts.last().map(|l| now - l.0)})
            }
        })
        .collect();
    Ok(json!({ "step": step, "rows": rows }))
}

/// Card "Dettagli di connessione": IP copiabili.
fn connection(ui: &mut egui::Ui, ips: &[String], copied: &mut bool) {
    ui::card_head(ui, tr("vps.dettagli_di_connessione"), None);
    for ip in ips {
        let label = if ip.contains(':') { "IPv6" } else { "IPv4" };
        *copied |= ui::copy_field(ui, label, ip);
    }
    ui.label(muted(tr("vps.ssh_ssh_utente_ip_con")));
}

pub fn pts(row: &Value) -> Vec<Option<f64>> {
    row["used"].as_array().map(|a| a.iter().map(|x| x.as_f64()).collect()).unwrap_or_default()
}
pub fn last(row: &Value) -> Option<f64> {
    row["used"].as_array()?.iter().rev().find_map(|x| x.as_f64())
}
pub fn row<'a>(m: &'a Value, label: &str) -> Option<&'a Value> {
    m["rows"].as_array()?.iter().find(|r| r["label"] == label)
}

fn fetch_tasks(c: &Client, s: &str) -> Result<Value, String> {
    let mut ids: Vec<i64> = serde_json::from_value(c.get(&format!("/vps/{s}/tasks"))?).unwrap_or_default();
    ids.sort_unstable_by(|a, b| b.cmp(a));
    ids.truncate(25);
    Ok(Value::Array(par_map(&ids, 8, |id| c.get(&format!("/vps/{s}/tasks/{id}")).unwrap_or(Value::Null))))
}

pub fn fetch_ssh_keys(c: &Client) -> Result<Value, String> {
    let names: Vec<String> = serde_json::from_value(c.get("/me/sshKey")?).unwrap_or_default();
    Ok(Value::Array(par_map(&names, 8, |n| c.get(&format!("/me/sshKey/{}", crate::ovh::enc(n))).unwrap_or(json!({"keyName": n})))))
}

impl App {
    pub fn vps_info(&mut self, s: &str) -> Option<Data> {
        let s2 = s.to_string();
        self.data(&format!("vps/{s}/info"), 60.0, move |c| fetch_info(c, &s2))
    }
    pub fn vps_metrics(&mut self, s: &str, period: &'static str) -> Option<Data> {
        let s2 = s.to_string();
        self.data(&format!("vps/{s}/m/{period}"), 60.0, move |c| fetch_metrics(c, &s2, period))
    }

    pub fn vps_backup(&mut self, s: &str) -> Option<Data> {
        let s2 = s.to_string();
        self.data(&format!("vps/{s}/backup"), 300.0, move |c| {
        let conf = c.get(&format!("/vps/{s2}/automatedBackup")).unwrap_or(Value::Null);
        let mut pts: Vec<String> = serde_json::from_value(c.get(&format!("/vps/{s2}/automatedBackup/restorePoints?state=available"))?).unwrap_or_default();
        pts.sort_unstable_by(|a, b| b.cmp(a));
        Ok(json!({"conf": conf, "points": pts}))
        })
    }
    pub fn vps_snap(&mut self, s: &str) -> Option<Data> {
        let s2 = s.to_string();
        self.data(&format!("vps/{s}/snap"), 120.0, move |c| {
        let opts: Vec<String> = serde_json::from_value(c.get(&format!("/vps/{s2}/option"))?).unwrap_or_default();
        let enabled = opts.iter().any(|o| o == "snapshot");
        let snap = if enabled {
            match c.get(&format!("/vps/{s2}/snapshot")) {
                Ok(v) => v,
                Err(e) if e.starts_with("404") => Value::Null,
                Err(e) => return Err(e),
            }
        } else {
            Value::Null
        };
        Ok(json!({"enabled": enabled, "snap": snap}))
        })
    }

    pub fn vps_ui(&mut self, ui: &mut egui::Ui, s: &str) {
        let info = self.vps_info(s);
        let short = s.split('.').next().unwrap_or(s);
        let title = match &info {
            Some(Ok(v)) => v["vps"]["displayName"].as_str().filter(|d| !d.is_empty() && *d != s).unwrap_or(short).to_string(),
            _ => short.to_string(),
        };
        let (c, st) = self.machine_state(crate::app::Kind::Vps, s);
        if ui::hero(ui, &[tr("common.server_crumb"), "VPS", &title], &title, Some((c, &st)), s) {
            self.refresh(&format!("vps/{s}/"));
        }
        self.tabs(ui, &[tr("common.panoramica"), tr("vps.monitoraggio"), tr("vps.backup_automatico"), tr("common.snapshot"), tr("common.attivita"), tr("common.reinstalla")]);
        match self.tab {
            0 => self.vps_state_tab(ui, s, info),
            1 => self.vps_res_tab(ui, s),
            t => ui::scroll_fill(ui, "vps-tab", |ui| match t {
                2 => self.vps_backup_tab(ui, s),
                3 => self.vps_snap_tab(ui, s),
                4 => self.vps_tasks_tab(ui, s),
                _ => self.vps_reinstall_tab(ui, s),
            }),
        }
    }

    fn vps_state_tab(&mut self, ui: &mut egui::Ui, s: &str, info: Option<Data>) {
        let Some(info) = info else { return ui::loading(ui) };
        let info = match info {
            Ok(i) => i,
            Err(e) => {
                ui::api_error(ui, &e);
                return;
            }
        };
        let v = info["vps"].clone();
        let st = v["state"].as_str().unwrap_or("?").to_string();
        let m = v["model"].clone();
        let offer = ui::sv(if m["offer"].is_string() { &m["offer"] } else { &m["name"] });
        let vcore = m["vcore"].as_i64().or(v["vcore"].as_i64());
        let mem = m["memory"].as_i64().or(v["memoryLimit"].as_i64());
        let dc = if info["dc"]["longName"].is_string() { ui::sv(&info["dc"]["longName"]) } else { ui::sv(&v["zone"]) };
        let ips: Vec<String> = info["ips"].as_array().into_iter().flatten().filter_map(|x| x.as_str().map(String::from)).collect();
        let os = ui::sv(&info["os"]["name"]);
        let svc = info["svc"].clone();
        let metrics = self.vps_metrics(s, "1h");
        let backup = self.vps_backup(s);
        let snap = self.vps_snap(s);
        let now = self.client.as_ref().map(|c| c.server_now()).unwrap_or(0);

        let mut act: Option<&str> = None;
        let mut copied = false;
        ui::tiles(ui, "vps-pano", 6, 290.0, 210.0, |ui, i| match i {
            0 => {
                ui::tile_head(ui, tr("vps.informazioni_generali"), "", None);
                ui::field(ui, tr("common.sistema"), &os);
                ui::field(ui, tr("vps.region"), &dc);
                ui.horizontal_wrapped(|ui| {
                    if ui.add(ui::primary(tr("common.riavvia"))).clicked() {
                        act = Some("reboot");
                    }
                    if ui.add_enabled(st != "running", egui::Button::new(tr("common.accendi"))).clicked() {
                        act = Some("start");
                    }
                    if ui.add_enabled(st != "stopped", ui::danger(tr("common.spegni"))).clicked() {
                        act = Some("stop");
                    }
                });
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    if ui::link_arrow(ui, tr("vps.console_kvm")).on_hover_text(tr("vps.per_le_emergenze_si_apre")).clicked() {
                        act = Some("kvm");
                    }
                    if ui::link_arrow(ui, tr("common.reinstalla")).clicked() {
                        act = Some("tab5");
                    }
                });
            }
            1 => connection(ui, &ips, &mut copied),
            2 => {
                ui::tile_head(ui, tr("vps.la_mia_configurazione"), "", None);
                ui::field(ui, tr("vps.modello"), &offer);
                ui::field(ui, tr("vps.processore_e_memoria"), &format!("{} vCore · {} GB", vcore.unwrap_or(0), mem.unwrap_or(0) / 1024));
                ui::field(ui, tr("vps.disco_principale"), &m["disk"].as_i64().map(|g| format!("{g} GB")).unwrap_or("-".into()));
            }
            3 => {
                if ui::tile_head(ui, tr("vps.utilizzo_adesso"), "", None) {}
                let (cpu, ram, ram_t) = match &metrics {
                    Some(Ok(m)) => {
                        let cpu = row(m, "CPU").and_then(last);
                        let rr = row(m, "RAM");
                        let ram = rr.and_then(last);
                        let max = rr.and_then(|r| r["max"].as_f64());
                        (cpu, ram.zip(max).map(|(a, b)| (a / b) as f32), ram.map(|r| format!("{:.1} GB", r / 1024.0)))
                    }
                    _ => (None, None, None),
                };
                let size = (ui.available_height() - 22.0).clamp(70.0, 120.0);
                ui.horizontal(|ui| {
                    ui::gauge(ui, "CPU", cpu.map(|c| c as f32 / 100.0), &cpu.map(|c| format!("{c:.0}%")).unwrap_or("-".into()), col::BRAND, size);
                    ui::gauge(ui, "RAM", ram, &ram_t.unwrap_or("-".into()), col::SKY, size);
                });
                if let Some(Err(e)) = &metrics {
                    ui.label(muted(fill(tr("vps.metriche_non_disponibili"), &[("e", e.to_string())])));
                }
            }
            4 => {
                ui::tile_head(ui, tr("common.backup"), "", None);
                let snap_txt = match &snap {
                    Some(Ok(d)) if d["enabled"] != true => tr("vps.opzione_non_attiva").to_string(),
                    Some(Ok(d)) if d["snap"].is_object() => fill(tr("vps.presente_del"), &[("0", (ui::it_date(&ui::sv(&d["snap"]["creationDate"]))).to_string())]),
                    Some(Ok(_)) => tr("vps.nessuno_snapshot").into(),
                    Some(Err(e)) => e.clone(),
                    None => "...".into(),
                };
                let bk = match &backup {
                    Some(Ok(b)) => {
                        let lastp = b["points"].as_array().and_then(|a| a.first()).and_then(|x| x.as_str()).map(ui::it_date);
                        let on = b["conf"]["state"] == "enabled";
                        format!("{}{}", if on { tr("vps.attivo") } else { tr("vps.non_attivo") }, lastp.map(|l| format!(", {}", fill(tr("vps.ultimo_il"), &[("l", l.to_string())]))).unwrap_or_default())
                    }
                    Some(Err(e)) => e.clone(),
                    None => "...".into(),
                };
                ui::field(ui, tr("vps.backup_automatico"), &bk);
                ui::field(ui, tr("common.snapshot"), &snap_txt);
                if ui::link_arrow(ui, tr("vps.visualizza_i_backup")).clicked() {
                    act = Some("tab2");
                }
            }
            _ => {
                ui::tile_head(ui, tr("vps.abbonamento"), "", None);
                let auto = svc["renew"]["automatic"] == true;
                ui::chip(ui, if auto { col::OK } else { col::WARN }, if auto { tr("vps.rinnovo_automatico") } else { tr("vps.rinnovo_manuale") });
                ui.add_space(8.0);
                let exp = svc["expiration"].as_str().unwrap_or("");
                let days = ui::days_until(exp, now).map(|d| format!(" {}", fill(tr("common.tra_giorni"), &[("d", d.to_string())]))).unwrap_or_default();
                ui::field(ui, tr("vps.prossima_scadenza"), &format!("{}{days}", ui::it_date(exp)));
                if let Some(e) = svc["engagedUpTo"].as_str() {
                    ui::field(ui, tr("vps.impegno_fino_al"), &ui::it_date(e));
                }
            }
        });
        if copied {
            self.toast(tr("vps.copiato_negli_appunti"), false);
        }
        let r = format!("vps/{s}/");
        match act {
            Some("reboot") => self.ask(tr("vps.riavviare_la_vps"), tr("common.riavvio_immediato_i_servizi_tornano"), tr("common.riavvia"), Action::call("POST", format!("/vps/{s}/reboot"), None, tr("common.riavvio_inviato"), &r)),
            Some("start") => self.ask(tr("vps.accendere_la_vps"), tr("vps.la_vps_parte_e_i"), tr("common.accendi"), Action::call("POST", format!("/vps/{s}/start"), None, tr("vps.accensione_inviata"), &r)),
            Some("stop") => self.ask(
                tr("vps.spegnere_la_vps"),
                tr("vps.tutto_quello_che_gira_sul"),
                tr("common.spegni"),
                Action::call("POST", format!("/vps/{s}/stop"), None, tr("vps.spegnimento_inviato"), &r),
            ),
            Some("kvm") => {
                let s2 = s.to_string();
                self.open_url_job(move |c| c.post(&format!("/vps/{s2}/getConsoleUrl"), None).map(|v| ui::sv(&v)));
            }
            Some(t) if t.starts_with("tab") => self.set_tab(t[3..].parse().unwrap_or(0)),
            _ => {}
        }
    }

    /// Monitoraggio: CPU, RAM e rete in tre riquadri che riempiono la finestra; la puntina li porta in dashboard.
    fn vps_res_tab(&mut self, ui: &mut egui::Ui, s: &str) {
        let mut pick = None;
        ui.horizontal(|ui| {
            for (p, l) in [("1h", tr("common.1_ora")), ("lastday", tr("common.24_ore")), ("lastweek", tr("common.7_giorni"))] {
                if ui.selectable_label(self.period == p, l).clicked() {
                    pick = Some(p);
                }
            }
            ui.label(muted(tr("vps.un_buco_nella_linea_nessun")));
        });
        if let Some(p) = pick {
            self.period = p;
        }
        ui.add_space(8.0);
        let period = self.period;
        let Some(m) = self.vps_metrics(s, period) else { return ui::loading(ui) };
        let m = match m {
            Ok(m) => m,
            Err(e) => {
                ui::api_error(ui, &e);
                return;
            }
        };
        let step = m["step"].as_i64().unwrap_or(300);
        let what = ["cpu", "ram", "rete"];
        let pinned: Vec<bool> = what.iter().map(|w| self.is_pinned(&format!("vps:{s}:{w}"))).collect();
        let mut toggle = None;
        ui::tiles(ui, "vps-mon", 3, 270.0, 200.0, |ui, i| {
            let (title, rowname) = [("CPU", "CPU"), ("RAM", "RAM"), (tr("common.rete"), "Rete in")][i];
            let r = row(&m, rowname);
            let peak = r.map(pts).unwrap_or_default().iter().flatten().copied().fold(f64::MIN, f64::max);
            let sub = match i {
                0 if peak > f64::MIN => fill(tr("vps.picco"), &[("peak", format!("{peak:.0}"))]),
                1 if peak > f64::MIN => fill(tr("vps.picco_gb"), &[("0", format!("{:.1}", peak / 1024.0))]),
                _ => String::new(),
            };
            if ui::tile_head(ui, title, &sub, Some(pinned[i])) {
                toggle = Some(what[i]);
            }
            if let Some(a) = r.and_then(|r| r["age"].as_i64()).filter(|a| *a > step + 360) {
                ui.label(RichText::new(fill(tr("vps.nessun_dato_da_min_vps"), &[("0", (a / 60).to_string())])).color(col::CRIT));
            }
            if let Some(e) = r.and_then(|r| r["err"].as_str()) {
                ui.label(RichText::new(fill(tr("vps.non_disponibile"), &[("e", e.to_string())])).color(col::CRIT));
                return;
            }
            crate::overview::metric_body(ui, &m, what[i]);
        });
        if let Some(w) = toggle {
            self.toggle_pin(&format!("vps:{s}:{w}"));
        }
    }

    fn vps_backup_tab(&mut self, ui: &mut egui::Ui, s: &str) {
        let b = self.vps_backup(s);
        let Some(b) = b else { return ui::loading(ui) };
        let b = match b {
            Ok(b) => b,
            Err(e) => {
                ui::api_error(ui, &fill(tr("vps.backup_automatico_non_disponibile"), &[("e", e.to_string())]));
                return;
            }
        };
        let mut act = None;
        ui::card(ui, false, |ui| {
            let c = &b["conf"];
            ui.horizontal(|ui| {
                ui.label(h(tr("vps.backup_automatico"), 18.0));
                if c.is_object() {
                    let st = ui::sv(&c["state"]);
                    ui::chip(ui, if st == "enabled" { col::OK } else { col::WARN }, if st == "enabled" { tr("ui.attivo") } else { &st });
                }
            });
            if let Some(t) = c["schedule"].as_str() {
                ui.label(muted(fill(tr("vps.ogni_giorno_alle_ora_del"), &[("0", (&t[..t.len().min(5)]).to_string())])));
            }
            ui.add_space(10.0);
            let points: Vec<String> = b["points"].as_array().into_iter().flatten().filter_map(|x| x.as_str().map(String::from)).collect();
            if points.is_empty() {
                ui.label(tr("vps.nessun_punto_di_ripristino_disponibile"));
            }
            for p in points {
                ui.horizontal(|ui| {
                    ui.label(ui::it_date(&p));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(ui::danger(tr("common.ripristina"))).clicked() {
                            act = Some(p.clone());
                        }
                    });
                });
                ui.separator();
            }
        });
        if let Some(p) = act {
            let body = fill(tr("vps.il_disco_torna_com_era"), &[("0", (ui::it_date(&p)).to_string())]);
            let a = Action::call(
                "POST",
                format!("/vps/{s}/automatedBackup/restore"),
                Some(json!({"restorePoint": p, "type": "full"})),
                tr("vps.ripristino_avviato_la_vps_si"),
                &format!("vps/{s}/"),
            );
            self.ask_typed(tr("vps.ripristinare_il_backup"), &body, tr("common.ripristina"), s, a);
        }
    }

    fn vps_snap_tab(&mut self, ui: &mut egui::Ui, s: &str) {
        let d = self.vps_snap(s);
        let Some(d) = d else { return ui::loading(ui) };
        let d = match d {
            Ok(d) => d,
            Err(e) => {
                ui::api_error(ui, &e);
                return;
            }
        };
        let mut act = None;
        ui::card(ui, false, |ui| {
            ui.label(h(tr("common.snapshot"), 18.0));
            ui.add_space(8.0);
            if d["enabled"] != true {
                ui.label(tr("vps.l_opzione_snapshot_non_e"));
                ui.label(muted(tr("vps.si_attiva_dal_pannello_ovhcloud")));
                return;
            }
            let v = &d["snap"];
            if v.is_object() {
                ui::kv(ui, "snap-kv", &[(tr("vps.creato_il"), ui::it_date(&ui::sv(&v["creationDate"]))), (tr("vps.descrizione"), ui::sv(&v["description"]))]);
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui.add(ui::danger(tr("common.ripristina"))).clicked() {
                        act = Some(1);
                    }
                    if ui.button(tr("common.elimina")).clicked() {
                        act = Some(2);
                    }
                });
            } else {
                ui.label(tr("vps.nessuno_snapshot_2"));
                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.f.snap_desc).hint_text(tr("vps.descrizione_facoltativa")).desired_width(280.0));
                    if ui.add(ui::primary(tr("vps.crea_snapshot"))).clicked() {
                        act = Some(3);
                    }
                });
            }
        });
        let r = format!("vps/{s}/");
        match act {
            Some(1) => self.ask_typed(
                tr("vps.tornare_allo_snapshot"),
                tr("vps.il_disco_torna_allo_snapshot"),
                tr("common.ripristina"),
                s,
                Action::call("POST", format!("/vps/{s}/snapshot/revert"), None, tr("vps.ripristino_snapshot_avviato"), &r),
            ),
            Some(2) => self.ask(tr("vps.eliminare_lo_snapshot"), tr("vps.non_si_recupera"), tr("common.elimina"), Action::call("DELETE", format!("/vps/{s}/snapshot"), None, tr("vps.snapshot_in_eliminazione"), &r)),
            Some(3) => {
                let body = json!({ "description": self.f.snap_desc.trim() });
                self.ask(
                    tr("vps.creare_lo_snapshot"),
                    tr("vps.copia_del_disco_adesso_ne"),
                    tr("vps.crea"),
                    Action::call("POST", format!("/vps/{s}/createSnapshot"), Some(body), tr("vps.snapshot_in_creazione"), &r),
                )
            }
            _ => {}
        }
    }

    fn vps_tasks_tab(&mut self, ui: &mut egui::Ui, s: &str) {
        let s2 = s.to_string();
        let Some(t) = self.data(&format!("vps/{s}/tasks"), 30.0, move |c| fetch_tasks(c, &s2)) else { return ui::loading(ui) };
        match t {
            Ok(t) => tasks_card(ui, &t, "type", "date"),
            Err(e) => {
                ui::api_error(ui, &e);
            }
        }
    }

    fn vps_reinstall_tab(&mut self, ui: &mut egui::Ui, s: &str) {
        let s2 = s.to_string();
        let imgs = self.data(&format!("vps/{s}/images"), 3600.0, move |c| {
            let ids: Vec<String> = serde_json::from_value(c.get(&format!("/vps/{s2}/images/available"))?).unwrap_or_default();
            let mut v = par_map(&ids, 8, |id| c.get(&format!("/vps/{s2}/images/available/{id}")).unwrap_or(json!({"id": id, "name": id})));
            v.sort_by(|a, b| ui::sv(&a["name"]).cmp(&ui::sv(&b["name"])));
            Ok(Value::Array(v))
        });
        let keys = self.data("me/ssh", 300.0, fetch_ssh_keys);
        let mut go = false;
        ui::card(ui, false, |ui| {
            ui.label(h(tr("common.reinstalla_il_sistema"), 18.0));
            ui.label(RichText::new(tr("vps.cancella_disco")).color(col::CRIT));
            ui.add_space(10.0);
            let imgs = match imgs {
                None => return ui::loading(ui),
                Some(Err(e)) => {
                    ui::api_error(ui, &e);
                    return;
                }
                Some(Ok(v)) => v,
            };
            let list: Vec<(String, String)> = imgs.as_array().into_iter().flatten().map(|v| (ui::sv(&v["id"]), ui::sv(&v["name"]))).collect();
            let cur = list.iter().find(|x| x.0 == self.f.img).map(|x| x.1.clone()).unwrap_or(tr("common.scegli_il_sistema").into());
            egui::Grid::new("vps-re").num_columns(2).spacing(vec2(20.0, 10.0)).show(ui, |ui| {
                ui.label(muted(tr("common.sistema")));
                egui::ComboBox::from_id_salt("vps-img").selected_text(cur).width(320.0).show_ui(ui, |ui| {
                    for (id, name) in &list {
                        ui.selectable_value(&mut self.f.img, id.clone(), name);
                    }
                });
                ui.end_row();
                ssh_pick_row(ui, &mut self.f.ssh_pick, &mut self.f.ssh_paste, &keys);
                ui.end_row();
                ui.label("");
                ui.checkbox(&mut self.f.no_pw, tr("vps.non_mandare_la_password_di"));
                ui.end_row();
            });
            ui.add_space(12.0);
            if ui.add_enabled(!self.f.img.is_empty(), ui::danger(tr("common.reinstalla_2"))).clicked() {
                go = true;
            }
        });
        if go {
            let key = chosen_key(&self.f.ssh_pick, &self.f.ssh_paste, &keys);
            let mut body = json!({ "imageId": self.f.img });
            if let Some(k) = &key {
                body["publicSshKey"] = json!(k);
                body["doNotSendPassword"] = json!(self.f.no_pw);
            }
            self.ask_typed(
                tr("vps.reinstallare_la_vps"),
                tr("vps.il_disco_viene_cancellato_e"),
                tr("common.reinstalla"),
                s,
                Action::call("POST", format!("/vps/{s}/rebuild"), Some(body), tr("common.reinstallazione_avviata"), &format!("vps/{s}/")),
            );
        }
    }
}

/// Lista di task (VPS: type/date; dedicati: function/startDate).
pub fn tasks_card(ui: &mut egui::Ui, t: &Value, kind: &str, date: &str) {
    ui::card(ui, false, |ui| {
        ui.label(h(tr("vps.ultime_operazioni"), 18.0));
        ui.add_space(8.0);
        let list: Vec<&Value> = t.as_array().into_iter().flatten().filter(|x| x.is_object()).collect();
        if list.is_empty() {
            ui.label(muted(tr("vps.nessuna_operazione_recente")));
        }
        egui::Grid::new("tasks").num_columns(4).striped(true).spacing(vec2(18.0, 8.0)).show(ui, |ui| {
            for x in list {
                let st = x["state"].as_str().or(x["status"].as_str()).unwrap_or("?");
                ui.label(ui::it_date(&ui::sv(&x[date])));
                ui.label(ui::sv(&x[kind]));
                ui::chip(ui, ui::state_color(st), ui::state_it(st));
                ui.label(muted(x["progress"].as_i64().map(|p| format!("{p}%")).unwrap_or_else(|| x["comment"].as_str().unwrap_or("").to_string())));
                ui.end_row();
            }
        });
    });
}

/// Riga del form: chiave SSH salvata nell'account oppure incollata.
pub fn ssh_pick_row(ui: &mut egui::Ui, pick: &mut String, paste: &mut String, keys: &Option<Data>) {
    ui.label(muted(tr("vps.chiave_ssh")));
    ui.vertical(|ui| {
        let names: Vec<String> = match keys {
            Some(Ok(k)) => k.as_array().into_iter().flatten().map(|x| ui::sv(&x["keyName"])).collect(),
            _ => vec![],
        };
        let sel = if pick.is_empty() { tr("vps.nessuna_password_per_email").to_string() } else { pick.clone() };
        egui::ComboBox::from_id_salt("ssh-pick").selected_text(sel).width(320.0).show_ui(ui, |ui| {
            ui.selectable_value(pick, String::new(), tr("vps.nessuna_password_per_email"));
            for n in &names {
                ui.selectable_value(pick, n.clone(), n);
            }
            ui.selectable_value(pick, "__paste".into(), tr("vps.incolla_una_chiave"));
        });
        if pick == "__paste" {
            ui.add(egui::TextEdit::multiline(paste).hint_text("ssh-ed25519 AAAA...").desired_rows(2).desired_width(420.0));
        }
    });
}
pub fn chosen_key(pick: &str, paste: &str, keys: &Option<Data>) -> Option<String> {
    if pick == "__paste" {
        return Some(paste.trim().to_string()).filter(|k| !k.is_empty());
    }
    let Some(Ok(k)) = keys else { return None };
    k.as_array()?.iter().find(|x| x["keyName"] == pick).and_then(|x| x["key"].as_str()).map(String::from)
}
