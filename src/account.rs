//! Account: tutti i servizi con scadenza e rinnovo, fatture, chiavi SSH.
use crate::i18n::{fill, tr};
use crate::app::{par_map, Action, App};
use crate::ovh::{enc, Client};
use crate::ui::{self, col, h, muted};
use eframe::egui::{self, vec2, RichText};
use serde_json::{json, Value};

pub fn fetch_services(c: &Client) -> Result<Value, String> {
    let ids: Vec<i64> = serde_json::from_value(c.get("/service")?).unwrap_or_default();
    let ids: Vec<i64> = ids.into_iter().take(200).collect();
    let mut v: Vec<Value> = par_map(&ids, 12, |id| c.get(&format!("/service/{id}")).unwrap_or(Value::Null)).into_iter().filter(|x| x.is_object()).collect();
    v.sort_by(|a, b| ui::sv(&a["expirationDate"]).cmp(&ui::sv(&b["expirationDate"])));
    Ok(Value::Array(v))
}

pub fn fetch_bills(c: &Client) -> Result<Value, String> {
    // ultimo anno: la data la calcola il server, qui basta un anno fa in giorni
    let from = c.server_now() - 365 * 86400;
    let mut ids: Vec<String> = serde_json::from_value(c.get(&format!("/me/bill?date.from={}", enc(&iso_date(from))))?).unwrap_or_default();
    ids.sort_unstable_by(|a, b| b.cmp(a));
    ids.truncate(36);
    let mut v: Vec<Value> = par_map(&ids, 10, |id| c.get(&format!("/me/bill/{}", enc(id))).unwrap_or(Value::Null)).into_iter().filter(|x| x.is_object()).collect();
    v.sort_by(|a, b| ui::sv(&b["date"]).cmp(&ui::sv(&a["date"])));
    Ok(Value::Array(v))
}

/// unix -> "AAAA-MM-GG" (days-from-civil al contrario)
pub fn iso_date(unix: i64) -> String {
    let z = unix.div_euclid(86400) + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02}")
}

/// "/vps/{serviceName}" -> "VPS"
fn kind_it(route: &str) -> String {
    let path: Vec<&str> = route.split('/').filter(|p| !p.is_empty() && !p.starts_with('{')).collect();
    match path.as_slice() {
        ["vps"] => "VPS".into(),
        ["dedicated", "server"] => tr("acc.server_dedicato").into(),
        ["ip", ..] => tr("common.ip").into(),
        ["domain", ..] => tr("acc.dominio").into(),
        ["hosting", ..] => tr("acc.hosting").into(),
        ["email", ..] => tr("acc.email").into(),
        p => p.join(" "),
    }
}

pub fn renew_it(r: &Value) -> String {
    let mode = r["mode"].as_str().unwrap_or("");
    let every = match r["interval"].as_str().unwrap_or("") {
        "P1M" => tr("acc.ogni_mese"),
        "P3M" => tr("acc.ogni_3_mesi"),
        "P6M" => tr("acc.ogni_6_mesi"),
        "P1Y" => tr("acc.ogni_anno"),
        "P2Y" => tr("acc.ogni_2_anni"),
        "P3Y" => tr("acc.ogni_3_anni"),
        _ => "",
    };
    match mode {
        "manual" => tr("acc.manuale").into(),
        "deleteAtExpiration" | "deleteAtEndEngagement" => tr("acc.disdetto_a_scadenza").into(),
        "oneShot" => tr("acc.una_tantum").into(),
        "option" => tr("acc.con_il_servizio_principale").into(),
        m if m.starts_with("automatic") => fill(tr("acc.automatico"), &[("every", every.to_string())]).trim().to_string(),
        m => m.to_string(),
    }
}

impl App {
    pub fn account_ui(&mut self, ui: &mut egui::Ui) {
        if ui::hero(ui, &[tr("common.gestione"), tr("common.account")], tr("common.account"), None, tr("acc.scadenze_di_tutti_i_servizi")) {
            self.refresh("svc/");
            self.refresh("bills");
            self.refresh("me/");
        }
        self.tabs(ui, &[tr("acc.servizi"), tr("acc.fatture"), tr("acc.chiavi_ssh")]);
        ui::scroll_fill(ui, "acc-tab", |ui| match self.tab {
            0 => self.services_tab(ui),
            1 => self.bills_tab(ui),
            _ => self.keys_tab(ui),
        });
    }

    fn services_tab(&mut self, ui: &mut egui::Ui) {
        let Some(v) = self.data("svc/all", 900.0, fetch_services) else { return ui::loading(ui) };
        let v = match v {
            Ok(v) => v,
            Err(e) => {
                if ui::api_error(ui, &e) {
                    crate::app::open_browser(crate::app::TOKEN_URL);
                }
                return;
            }
        };
        let now = self.client.as_ref().map(|c| c.server_now()).unwrap_or(0);
        ui.add(egui::TextEdit::singleline(&mut self.f.svc_filter).hint_text(tr("acc.filtra_per_nome_o_prodotto")).desired_width(320.0));
        ui.add_space(8.0);
        let q = self.f.svc_filter.to_lowercase();
        ui::card(ui, false, |ui| {
            let narrow = ui.available_width() < 700.0;
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
            egui::Grid::new("svc").num_columns(if narrow { 2 } else { 4 }).striped(true).spacing(vec2(18.0, 8.0)).show(ui, |ui| {
                for s in v.as_array().into_iter().flatten() {
                    let name = s["resource"]["displayName"].as_str().or(s["resource"]["name"].as_str()).unwrap_or("?").to_string();
                    let prod = s["plan"]["product"]["name"].as_str().map(String::from).unwrap_or_else(|| kind_it(s["route"]["path"].as_str().unwrap_or("")));
                    if !q.is_empty() && !name.to_lowercase().contains(&q) && !prod.to_lowercase().contains(&q) {
                        continue;
                    }
                    let exp = s["expirationDate"].as_str().unwrap_or("");
                    let days = ui::days_until(exp, now);
                    let c = match days {
                        Some(d) if d < 0 => col::CRIT,
                        Some(d) if d <= 14 => col::WARN,
                        _ => col::TEXT,
                    };
                    ui.vertical(|ui| {
                        ui.label(&name);
                        ui.label(muted(&prod).size(12.5));
                    });
                    ui.vertical(|ui| {
                        ui.label(RichText::new(ui::it_date(exp)).color(c));
                        if let Some(d) = days {
                            ui.label(muted(if d < 0 { fill(tr("acc.scaduto_da_giorni"), &[("0", (-d).to_string())]) } else { fill(tr("acc.tra_giorni"), &[("d", d.to_string())]) }).size(12.5));
                        }
                    });
                    if !narrow {
                        ui.label(renew_it(&s["renew"]));
                        let st = s["state"].as_str().unwrap_or("?");
                        ui::chip(ui, if st == "ok" { col::OK } else { col::CRIT }, if st == "ok" { "ok" } else { st });
                    }
                    ui.end_row();
                }
            });
        });
    }

    fn bills_tab(&mut self, ui: &mut egui::Ui) {
        let Some(v) = self.data("bills", 900.0, fetch_bills) else { return ui::loading(ui) };
        let v = match v {
            Ok(v) => v,
            Err(e) => {
                if ui::api_error(ui, &e) {
                    crate::app::open_browser(crate::app::TOKEN_URL);
                }
                return;
            }
        };
        let mut open = None;
        ui::card(ui, false, |ui| {
            ui.label(h(tr("acc.fatture_dell_ultimo_anno"), 18.0));
            ui.add_space(8.0);
            if v.as_array().is_none_or(|a| a.is_empty()) {
                ui.label(muted(tr("acc.nessuna_fattura")));
            }
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
            egui::Grid::new("bills").num_columns(4).striped(true).spacing(vec2(18.0, 8.0)).show(ui, |ui| {
                for b in v.as_array().into_iter().flatten() {
                    ui.label(ui::it_date(&ui::sv(&b["date"])));
                    ui.label(muted(ui::sv(&b["billId"])));
                    ui.label(h(ui::sv(&b["priceWithTax"]["text"]), 15.0));
                    if let Some(u) = b["pdfUrl"].as_str() {
                        if ui.link("PDF").clicked() {
                            open = Some(u.to_string());
                        }
                    } else {
                        ui.label("");
                    }
                    ui.end_row();
                }
            });
        });
        if let Some(u) = open {
            crate::app::open_browser(&u);
        }
    }

    fn keys_tab(&mut self, ui: &mut egui::Ui) {
        let keys = self.data("me/ssh", 300.0, crate::vps::fetch_ssh_keys);
        let mut act = None;
        ui::card(ui, false, |ui| {
            ui.label(h(tr("acc.chiavi_ssh_dell_account"), 18.0));
            ui.label(muted(tr("acc.le_usi_quando_reinstalli_una")));
            ui.add_space(8.0);
            match &keys {
                None => ui::loading(ui),
                Some(Err(e)) => {
                    if ui::api_error(ui, e) {
                        crate::app::open_browser(crate::app::TOKEN_URL);
                    }
                }
                Some(Ok(v)) => {
                    for k in v.as_array().into_iter().flatten() {
                        let name = ui::sv(&k["keyName"]);
                        let key = ui::sv(&k["key"]);
                        let tail: String = key.chars().rev().take(24).collect::<Vec<_>>().into_iter().rev().collect();
                        ui.horizontal(|ui| {
                            ui.label(h(&name, 15.0));
                            ui.label(muted(format!("{}…{tail}", key.split(' ').next().unwrap_or(""))));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.add(ui::ghost(tr("common.elimina"))).clicked() {
                                    act = Some(Action::call("DELETE", format!("/me/sshKey/{}", enc(&name)), None, tr("acc.chiave_eliminata"), "me/ssh"));
                                }
                            });
                        });
                    }
                }
            }
            ui.add_space(10.0);
            ui.add(egui::TextEdit::singleline(&mut self.f.key_name).hint_text(tr("acc.nome_es_portatile")).desired_width(260.0));
            ui.add(egui::TextEdit::multiline(&mut self.f.key_text).hint_text("ssh-ed25519 AAAA... utente@pc").desired_rows(2).desired_width(f32::INFINITY));
            let ok = !self.f.key_name.trim().is_empty() && self.f.key_text.trim().starts_with("ssh-");
            if ui.add_enabled(ok, ui::primary(tr("acc.aggiungi_chiave"))).clicked() {
                let body = json!({"keyName": self.f.key_name.trim(), "key": self.f.key_text.trim()});
                act = Some(Action::call("POST", "/me/sshKey".into(), Some(body), tr("acc.chiave_aggiunta"), "me/ssh"));
                self.f.key_name.clear();
                self.f.key_text.clear();
            }
        });
        if let Some(a) = act {
            self.run(a);
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn iso_date_known() {
        assert_eq!(super::iso_date(1794873600), "2026-11-17");
        assert_eq!(super::iso_date(0), "1970-01-01");
        assert_eq!(super::iso_date(951782400), "2000-02-29");
    }
}
