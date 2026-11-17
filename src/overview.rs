//! Dashboard: i riquadri che hai scelto (pin), divisi su tutta la finestra senza scroll.
//! Pin: "vps:<servizio>:<stato|cpu|ram|rete|backup>", "dedi:<servizio>:<stato|traffico>", "avvisi", "scadenze", "fatture".
use crate::i18n::{fill, tr};
use crate::app::{Action, App, Kind, View};
use crate::ui::{self, col, h, muted};
use crate::vps::{last, pts, row};
use eframe::egui::{self, vec2, RichText};
use serde_json::Value;

fn short(s: &str) -> &str {
    s.split('.').next().unwrap_or(s)
}

impl App {
    /// Riquadri predefiniti: per ogni VPS stato, CPU, RAM e rete; per ogni dedicato lo stato; avvisi e scadenze.
    pub fn default_pins(&mut self) -> Vec<String> {
        let (machines, _) = self.machines();
        let mut v = Vec::new();
        for (k, s) in &machines {
            match k {
                Kind::Vps => v.extend(["stato", "cpu", "ram", "rete"].iter().map(|w| format!("vps:{s}:{w}"))),
                Kind::Dedi => v.push(format!("dedi:{s}:stato")),
            }
        }
        v.extend(["avvisi".to_string(), "scadenze".to_string()]);
        v
    }

    /// Tutti i riquadri che si possono mettere in dashboard, con il nome da mostrare.
    fn pin_catalog(&mut self) -> Vec<(String, String)> {
        let (machines, _) = self.machines();
        let mut v = Vec::new();
        for (k, s) in &machines {
            let n = self.display_name(*k, s);
            match k {
                Kind::Vps => {
                    for (w, l) in [("stato", tr("dash.l_stato")), ("cpu", "CPU"), ("ram", "RAM"), ("rete", tr("dash.l_rete")), ("backup", tr("dash.l_backup"))] {
                        v.push((format!("vps:{s}:{w}"), format!("{n} · {l}")));
                    }
                }
                Kind::Dedi => {
                    v.push((format!("dedi:{s}:stato"), format!("{n} · {}", tr("dash.l_stato"))));
                    v.push((format!("dedi:{s}:traffico"), format!("{n} · {}", tr("dash.l_traffico"))));
                }
            }
        }
        for (id, l) in [("avvisi", tr("dash.avvisi")), ("scadenze", tr("dash.in_scadenza")), ("fatture", tr("dash.ultime_fatture"))] {
            v.push((id.into(), l.into()));
        }
        v
    }

    /// Nome da mostrare: quello che hai dato al server su OVH, altrimenti la prima parte del servizio.
    pub fn display_name(&mut self, k: Kind, s: &str) -> String {
        let info = match k {
            Kind::Vps => self.vps_info(s),
            Kind::Dedi => self.dedi_info(s),
        };
        let dn = match (&info, k) {
            (Some(Ok(v)), Kind::Vps) => v["vps"]["displayName"].as_str().map(String::from),
            (Some(Ok(v)), Kind::Dedi) => v["srv"]["iam"]["displayName"].as_str().map(String::from),
            _ => None,
        };
        dn.filter(|d| !d.is_empty() && d != s).unwrap_or_else(|| short(s).to_string())
    }

    pub fn overview_ui(&mut self, ui: &mut egui::Ui) {
        let (machines, dedi_err) = self.machines();
        let nv = machines.iter().filter(|m| m.0 == Kind::Vps).count();
        let nd = machines.len() - nv;
        let sub = fill(tr("dash.sottotitolo"), &[("nv", nv.to_string()), ("nd", nd.to_string())]);
        let edit_label = if self.editing { tr("dash.fatto") } else { tr("dash.personalizza") };
        let (refresh, edit) = ui::hero_with(ui, &["Home"], tr("common.dashboard"), None, &sub, edit_label);
        if refresh {
            self.refresh("");
        }
        if edit {
            self.editing = !self.editing;
        }
        if let Some(e) = dedi_err.filter(|e| e.contains("granted")) {
            ui.horizontal(|ui| {
                ui.label(RichText::new(tr("dash.dedicati_senza_permesso")).color(col::WARN));
                if ui.link(tr("dash.crea_un_token_completo")).clicked() {
                    crate::app::open_browser(crate::app::TOKEN_URL);
                }
                let _ = e;
            });
        }

        let pins = self.pins.clone().unwrap_or_else(|| self.default_pins());
        let editing = self.editing;
        let n = pins.len() + editing as usize;
        if n == 0 {
            ui::card(ui, false, |ui| {
                ui.label(h(tr("dash.la_dashboard_e_vuota"), 18.0));
                ui.label(muted(tr("dash.premi_personalizza_e_scegli_cosa")));
            });
            return;
        }
        let catalog = if editing { self.pin_catalog() } else { vec![] };
        let mut remove: Option<String> = None;
        let mut add: Option<String> = None;
        let mut open: Option<View> = None;
        let mut act: Option<(String, Action)> = None;
        ui::tiles(ui, "dash", n, 300.0, 170.0, |ui, i| {
            if i == pins.len() {
                // riquadro per aggiungere
                ui.label(h(tr("dash.aggiungi_un_riquadro"), 16.5));
                ui.add_space(6.0);
                let free: Vec<&(String, String)> = catalog.iter().filter(|(id, _)| !pins.contains(id)).collect();
                if free.is_empty() {
                    ui.label(muted(tr("dash.hai_gia_messo_tutto")));
                    return;
                }
                let sel = free.iter().find(|(id, _)| *id == self.add_pick).map(|x| x.1.clone()).unwrap_or(tr("dash.scegli").into());
                egui::ComboBox::from_id_salt("add-pin").selected_text(sel).width(ui.available_width() - 4.0).show_ui(ui, |ui| {
                    for (id, l) in &free {
                        ui.selectable_value(&mut self.add_pick, id.clone(), l);
                    }
                });
                ui.add_space(6.0);
                if ui.add_enabled(free.iter().any(|(id, _)| *id == self.add_pick), ui::primary(tr("common.aggiungi"))).clicked() {
                    add = Some(self.add_pick.clone());
                }
                return;
            }
            let id = &pins[i];
            if editing {
                let r = ui.max_rect();
                let b = egui::Rect::from_min_size(egui::pos2(r.right() - 22.0, r.top() - 4.0), vec2(26.0, 26.0));
                let resp = ui.interact(b, ui.id().with(("rm", i)), egui::Sense::click()).on_hover_text(tr("common.togli_dalla_dashboard"));
                ui.painter().rect_filled(b, 6, if resp.hovered() { egui::Color32::from_rgb(0xFF, 0xE6, 0xEC) } else { col::BG });
                ui.painter().text(b.center(), egui::Align2::CENTER_CENTER, "✕", egui::FontId::proportional(13.0), col::CRIT);
                if resp.clicked() {
                    remove = Some(id.clone());
                }
            }
            if let Some((v, a)) = self.pin_tile(ui, id) {
                match a {
                    Some(a) => act = Some((v, a)),
                    None => open = Some(parse_view(&v)),
                }
            }
        });
        if let Some(id) = remove.or(add) {
            self.toggle_pin(&id);
            self.add_pick.clear();
        }
        if let Some(v) = open {
            self.go(v);
        }
        if let Some((title, a)) = act {
            self.ask(&title, tr("common.riavvio_immediato_i_servizi_tornano"), tr("common.riavvia"), a);
        }
    }

    /// Disegna un riquadro. Ritorna Some((vista, None)) per "apri", Some((titolo, Some(azione))) per un'azione.
    fn pin_tile(&mut self, ui: &mut egui::Ui, id: &str) -> Option<(String, Option<Action>)> {
        let parts: Vec<&str> = id.split(':').collect();
        let mut out = None;
        match parts.as_slice() {
            ["vps", s, what] => {
                let s = s.to_string();
                let name = self.display_name(Kind::Vps, &s);
                match *what {
                    "stato" => {
                        let info = self.vps_info(&s);
                        let (c, st) = self.machine_state(Kind::Vps, &s);
                        ui.horizontal(|ui| {
                            if ui.add(egui::Label::new(h(&name, 16.5)).sense(egui::Sense::click())).on_hover_text(tr("dash.apri")).clicked() {
                                out = Some((format!("vps:{s}"), None));
                            }
                            ui::chip(ui, c, &st);
                        });
                        if let Some(Ok(v)) = &info {
                            ui.label(muted(fill(tr("dash.vps"), &[("0", (ui::sv(&v["vps"]["model"]["offer"])).to_string()), ("1", (v["dc"]["longName"].as_str().unwrap_or("")).to_string())])).size(13.0));
                            ui.add_space(4.0);
                            for ip in v["ips"].as_array().into_iter().flatten().take(2) {
                                ui.label(RichText::new(ui::sv(ip)).color(col::HEAD).size(14.0));
                            }
                            ui.label(muted(ui::sv(&v["os"]["name"])).size(13.0));
                        }
                        ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                            ui.horizontal(|ui| {
                                if ui.add(ui::primary(tr("common.riavvia"))).clicked() {
                                    out = Some((
                                        fill(tr("dash.riavviare"), &[("name", name.to_string())]),
                                        Some(Action::call("POST", format!("/vps/{s}/reboot"), None, tr("common.riavvio_inviato"), &format!("vps/{s}/"))),
                                    ));
                                }
                                if ui::link_arrow(ui, tr("dash.apri")).clicked() {
                                    out = Some((format!("vps:{s}"), None));
                                }
                            });
                        });
                    }
                    "cpu" | "ram" | "rete" => {
                        let m = self.vps_metrics(&s, "1h");
                        let label = match *what {
                            "cpu" => "CPU",
                            "ram" => "RAM",
                            _ => tr("common.rete"),
                        };
                        ui::tile_head(ui, label, &format!("{name} · {}", tr("dash.ultima_ora")), None);
                        match &m {
                            Some(Ok(m)) => metric_body(ui, m, what),
                            Some(Err(e)) => {
                                ui.label(RichText::new(e).color(col::CRIT).size(13.0));
                            }
                            None => ui::loading(ui),
                        }
                    }
                    "backup" => {
                        let b = self.vps_backup(&s);
                        let sn = self.vps_snap(&s);
                        ui::tile_head(ui, tr("common.backup"), &name, None);
                        if let Some(Ok(b)) = &b {
                            let lastp = b["points"].as_array().and_then(|a| a.first()).and_then(|x| x.as_str()).map(ui::it_date);
                            ui::field(ui, tr("dash.ultimo_backup_automatico"), &lastp.unwrap_or("nessuno".into()));
                            ui::field(ui, tr("dash.punti_di_ripristino"), &b["points"].as_array().map(|a| a.len()).unwrap_or(0).to_string());
                        }
                        if let Some(Ok(d)) = &sn {
                            let t = if d["enabled"] != true { tr("dash.opzione_non_attiva").to_string() } else if d["snap"].is_object() { ui::it_date(&ui::sv(&d["snap"]["creationDate"])) } else { "nessuno".into() };
                            ui::field(ui, tr("common.snapshot"), &t);
                        }
                    }
                    _ => {
                        ui.label(muted(fill(tr("dash.riquadro_sconosciuto"), &[("id", id.to_string())])));
                    }
                }
            }
            ["dedi", s, what] => {
                let s = s.to_string();
                let name = self.display_name(Kind::Dedi, &s);
                let info = self.dedi_info(&s);
                let (c, st) = self.machine_state(Kind::Dedi, &s);
                ui.horizontal(|ui| {
                    if ui.add(egui::Label::new(h(&name, 16.5)).sense(egui::Sense::click())).clicked() {
                        out = Some((format!("dedi:{s}"), None));
                    }
                    ui::chip(ui, c, &st);
                });
                if let Some(Ok(v)) = &info {
                    let srv = &v["srv"];
                    if *what == "traffico" {
                        let t = &v["net"]["traffic"];
                        if let (Some(u), Some(z)) = (t["outputQuotaUsed"]["value"].as_f64(), t["outputQuotaSize"]["value"].as_f64()) {
                            ui.label(muted(fill(tr("common.traffico_in_uscita_su"), &[("0", (ui::uv(&t["outputQuotaUsed"])).to_string()), ("1", (ui::uv(&t["outputQuotaSize"])).to_string())])));
                            ui::bar(ui, (u / z.max(1e-9)) as f32, col::BRAND);
                        } else {
                            ui.label(muted(fill(tr("dash.banda"), &[("0", (ui::uv(&v["net"]["bandwidth"]["OvhToInternet"])).to_string())])));
                        }
                    } else {
                        ui.label(muted(format!("{} · {}", ui::sv(&srv["commercialRange"]), ui::sv(&srv["datacenter"]).to_uppercase())).size(13.0));
                        ui.label(RichText::new(ui::sv(&srv["ip"])).color(col::HEAD));
                        ui.label(muted(ui::sv(&srv["os"])).size(13.0));
                    }
                }
            }
            ["avvisi"] => {
                ui::tile_head(ui, tr("dash.avvisi"), "", None);
                if self.alerts.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        ui::dot(ui, col::OK);
                        ui.label(muted(tr("dash.tutto_tranquillo_se_un_server")));
                    });
                }
                for a in self.alerts.iter().take(8) {
                    ui.horizontal_wrapped(|ui| {
                        ui::dot(ui, if a.bad { col::CRIT } else { col::OK });
                        ui.label(muted(&a.when));
                        ui.label(RichText::new(short(&a.machine)).strong());
                        ui.label(&a.text);
                    });
                }
            }
            ["scadenze"] => {
                ui::tile_head(ui, tr("dash.in_scadenza"), tr("dash.prossimi_45_giorni"), None);
                let svc = self.data("svc/all", 900.0, crate::account::fetch_services);
                let now = self.client.as_ref().map(|c| c.server_now()).unwrap_or(0);
                match &svc {
                    None => ui::loading(ui),
                    Some(Err(_)) => {
                        ui.label(muted(tr("dash.serve_il_permesso_su_service")));
                    }
                    Some(Ok(v)) => {
                        let soon: Vec<(i64, &Value)> = v
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(|s| Some((ui::days_until(s["expirationDate"].as_str()?, now)?, s)))
                            .filter(|(d, _)| *d <= 45)
                            .collect();
                        if soon.is_empty() {
                            ui.label(muted(tr("dash.niente_in_scadenza")));
                        }
                        for (d, s) in soon.into_iter().take(8) {
                            let name = s["resource"]["displayName"].as_str().or(s["resource"]["name"].as_str()).unwrap_or("?");
                            let auto = s["renew"]["mode"].as_str().is_some_and(|m| m.starts_with("automatic"));
                            ui.horizontal_wrapped(|ui| {
                                ui::dot(ui, if d <= 7 && !auto { col::CRIT } else if auto { col::OK } else { col::WARN });
                                ui.label(name);
                                ui.label(muted(fill(tr("dash.tra_giorni"), &[("d", d.to_string()), ("0", (crate::account::renew_it(&s["renew"])).to_string())])));
                            });
                        }
                    }
                }
            }
            ["fatture"] => {
                ui::tile_head(ui, tr("dash.ultime_fatture"), "", None);
                let b = self.data("bills", 900.0, crate::account::fetch_bills);
                match &b {
                    None => ui::loading(ui),
                    Some(Err(e)) => {
                        ui.label(RichText::new(e).color(col::CRIT).size(13.0));
                    }
                    Some(Ok(v)) => {
                        for x in v.as_array().into_iter().flatten().take(6) {
                            ui.horizontal(|ui| {
                                ui.label(muted(ui::it_date(&ui::sv(&x["date"]))));
                                ui.label(RichText::new(ui::sv(&x["priceWithTax"]["text"])).color(col::HEAD).strong());
                                if let Some(u) = x["pdfUrl"].as_str() {
                                    if ui.link("PDF").clicked() {
                                        crate::app::open_browser(u);
                                    }
                                }
                            });
                        }
                    }
                }
            }
            _ => {
                ui.label(muted(fill(tr("dash.riquadro_sconosciuto"), &[("id", id.to_string())])));
            }
        }
        out
    }
}

/// Valore grande e grafico che riempie il riquadro (CPU, RAM o rete in+out).
pub fn metric_body(ui: &mut egui::Ui, m: &Value, what: &str) {
    match what {
        "rete" => {
            let (i, o) = (row(m, "Rete in"), row(m, "Rete out"));
            let fmt = |r: Option<&Value>| r.and_then(last).map(|v| if v >= 1024.0 { format!("{:.1} MB/s", v / 1024.0) } else { format!("{v:.0} KB/s") }).unwrap_or("-".into());
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("↓ {}", fmt(i))).font(ui::bold(20.0)).color(col::BRAND));
                ui.add_space(8.0);
                ui.label(RichText::new(format!("↑ {}", fmt(o))).font(ui::bold(20.0)).color(col::SKY));
            });
            let (pi, po) = (i.map(pts).unwrap_or_default(), o.map(pts).unwrap_or_default());
            ui::spark_multi(ui, &[(&pi, col::BRAND), (&po, col::SKY)], None, f32::INFINITY);
        }
        "ram" => {
            let r = row(m, "RAM");
            let (v, mx) = (r.and_then(last), r.and_then(|r| r["max"].as_f64()));
            let txt = match (v, mx) {
                (Some(v), Some(mx)) => format!("{:.1} / {:.1} GB", v / 1024.0, mx / 1024.0),
                (Some(v), None) => format!("{:.1} GB", v / 1024.0),
                _ => "-".into(),
            };
            ui.label(RichText::new(txt).font(ui::bold(22.0)).color(col::HEAD));
            ui::spark(ui, &r.map(pts).unwrap_or_default(), mx, col::SKY, f32::INFINITY);
        }
        _ => {
            let r = row(m, "CPU");
            ui.label(RichText::new(r.and_then(last).map(|v| format!("{v:.0}%")).unwrap_or("-".into())).font(ui::bold(22.0)).color(col::HEAD));
            ui::spark(ui, &r.map(pts).unwrap_or_default(), Some(100.0), col::BRAND, f32::INFINITY);
        }
    }
}

/// "vps:<servizio>" / "dedi:<servizio>" -> vista
fn parse_view(v: &str) -> View {
    match v.split_once(':') {
        Some(("vps", s)) => View::Machine(Kind::Vps, s.to_string()),
        Some(("dedi", s)) => View::Machine(Kind::Dedi, s.to_string()),
        _ => View::Overview,
    }
}
