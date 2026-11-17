//! Rete e IP: elenco, reverse DNS, firewall di rete (Edge), mitigazione anti-DDoS.
use crate::i18n::{fill, tr};
use crate::app::{par_map, Action, App};
use crate::ovh::{enc, Client};
use crate::ui::{self, col, h, muted};
use eframe::egui::{self, vec2, RichText};
use serde_json::{json, Value};

fn fetch_ips(c: &Client) -> Result<Value, String> {
    let ips: Vec<String> = serde_json::from_value(c.get("/ip")?).unwrap_or_default();
    let ips: Vec<String> = ips.into_iter().take(80).collect();
    Ok(Value::Array(par_map(&ips, 10, |ip| c.get(&format!("/ip/{}", enc(ip))).unwrap_or(json!({ "ip": ip })))))
}

/// Primo indirizzo di un blocco: "1.2.3.4/32" -> "1.2.3.4"
fn first_ip(block: &str) -> String {
    block.split('/').next().unwrap_or(block).to_string()
}

/// lista di id -> dettaglio di ognuno
fn list_detail(c: &Client, base: &str) -> Result<Value, String> {
    let ids: Vec<Value> = serde_json::from_value(c.get(base)?).unwrap_or_default();
    Ok(Value::Array(par_map(&ids, 8, |id| {
        let id = match id {
            Value::String(s) => enc(s),
            v => v.to_string(),
        };
        c.get(&format!("{base}/{id}")).unwrap_or(Value::Null)
    })))
}

impl App {
    pub fn net_ui(&mut self, ui: &mut egui::Ui) {
        if ui::hero(ui, &[tr("common.gestione"), tr("common.rete_e_ip")], tr("common.rete_e_ip"), None, tr("net.reverse_dns_firewall_di_rete")) {
            self.refresh("ip/");
        }
        let Some(list) = self.data("ip/list", 300.0, fetch_ips) else { return ui::loading(ui) };
        let list = match list {
            Ok(l) => l,
            Err(e) => {
                if ui::api_error(ui, &e) {
                    crate::app::open_browser(crate::app::TOKEN_URL);
                }
                return;
            }
        };
        let items: Vec<Value> = list.as_array().cloned().unwrap_or_default();
        if self.f.ip.is_none() {
            self.f.ip = items.first().and_then(|i| i["ip"].as_str()).map(String::from);
        }
        ui::scroll_fill(ui, "net", |ui| self.net_body(ui, items));
    }

    fn net_body(&mut self, ui: &mut egui::Ui, items: Vec<Value>) {
        let wide = ui.available_width() > 900.0;
        let mut pick = None;
        let mut list_ui = |ui: &mut egui::Ui, sel: &Option<String>| {
            ui::card(ui, false, |ui| {
                ui.label(h(format!("{} blocchi", items.len()), 16.0));
                ui.add_space(6.0);
                for it in &items {
                    let ip = ui::sv(&it["ip"]);
                    let on = sel.as_deref() == Some(ip.as_str());
                    let routed = it["routedTo"]["serviceName"].as_str().map(|s| s.split('.').next().unwrap_or(s).to_string()).unwrap_or_default();
                    let text = format!("{ip}\n{} {}", ui::sv(&it["type"]), routed);
                    if ui.add_sized(vec2(ui.available_width(), 44.0), egui::Button::selectable(on, text)).clicked() {
                        pick = Some(ip.clone());
                    }
                }
            });
        };
        let sel = self.f.ip.clone();
        if wide {
            let full = ui.available_width();
            ui.horizontal_top(|ui| {
                let down = egui::Layout::top_down(egui::Align::Min);
                ui.allocate_ui_with_layout(vec2(300.0, 0.0), down, |ui| {
                    ui.set_width(300.0);
                    list_ui(ui, &sel);
                });
                ui.allocate_ui_with_layout(vec2(full - 316.0, 0.0), down, |ui| {
                    ui.set_width(full - 316.0);
                    if let Some(ip) = &sel {
                        self.ip_detail(ui, ip);
                    }
                });
            });
        } else {
            list_ui(ui, &sel);
            ui.add_space(12.0);
            if let Some(ip) = &sel {
                self.ip_detail(ui, ip);
            }
        }
        if let Some(p) = pick {
            self.f.ip = Some(p);
            self.f.rev_ip.clear();
        }
    }

    fn ip_detail(&mut self, ui: &mut egui::Ui, block: &str) {
        let e = enc(block);
        let r = format!("ip/{block}/");
        // reverse
        let b2 = e.clone();
        let rev = self.data(&format!("ip/{block}/rev"), 300.0, move |c| list_detail(c, &format!("/ip/{b2}/reverse")));
        let mut act: Option<Action> = None;
        ui::card(ui, false, |ui| {
            ui.label(h(tr("net.reverse_dns"), 18.0));
            ui.label(muted(tr("net.il_nome_che_risponde_a")));
            ui.add_space(8.0);
            match &rev {
                None => ui::loading(ui),
                Some(Err(err)) => {
                    ui::api_error(ui, err);
                }
                Some(Ok(v)) => {
                    for x in v.as_array().into_iter().flatten().filter(|x| x.is_object()) {
                        let ipr = ui::sv(&x["ipReverse"]);
                        ui.horizontal(|ui| {
                            ui.label(&ipr);
                            ui.label(muted("→"));
                            ui.label(ui::sv(&x["reverse"]));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.add(ui::ghost(tr("common.elimina"))).clicked() {
                                    act = Some(Action::call("DELETE", format!("/ip/{e}/reverse/{}", enc(&ipr)), None, tr("net.reverse_eliminato"), &r));
                                }
                            });
                        });
                    }
                }
            }
            ui.add_space(8.0);
            if self.f.rev_ip.is_empty() {
                self.f.rev_ip = first_ip(block);
            }
            ui.horizontal_wrapped(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.f.rev_ip).desired_width(170.0));
                ui.add(egui::TextEdit::singleline(&mut self.f.rev_name).hint_text("server.tuodominio.it").desired_width(240.0));
                let ok = !self.f.rev_name.trim().is_empty();
                if ui.add_enabled(ok, ui::primary(tr("net.imposta"))).clicked() {
                    let body = json!({"ipReverse": self.f.rev_ip.trim(), "reverse": self.f.rev_name.trim()});
                    act = Some(Action::call("POST", format!("/ip/{e}/reverse"), Some(body), tr("net.reverse_impostato_ovh_controlla_che"), &r));
                }
            });
        });
        ui.add_space(12.0);

        // firewall
        let b3 = e.clone();
        let fw = self.data(&format!("ip/{block}/fw"), 120.0, move |c| list_detail(c, &format!("/ip/{b3}/firewall")));
        let mut fw_ip: Option<String> = None;
        ui::card(ui, false, |ui| {
            ui.label(h(tr("net.firewall_di_rete"), 18.0));
            ui.label(muted(tr("net.filtra_il_traffico_prima_che")));
            ui.add_space(8.0);
            match &fw {
                None => ui::loading(ui),
                Some(Err(err)) => {
                    ui::api_error(ui, err);
                }
                Some(Ok(v)) => {
                    let list: Vec<&Value> = v.as_array().into_iter().flatten().filter(|x| x.is_object()).collect();
                    if list.is_empty() {
                        ui.label(tr("net.firewall_non_configurato_su_questo"));
                        if ui.add(ui::primary(&fill(tr("net.prepara_il_firewall_per"), &[("0", (first_ip(block)).to_string())]))).clicked() {
                            act = Some(Action::call("POST", format!("/ip/{e}/firewall"), Some(json!({"ipOnFirewall": first_ip(block)})), tr("net.firewall_creato_ancora_spento"), &r));
                        }
                    }
                    for x in list {
                        let ipf = ui::sv(&x["ipOnFirewall"]);
                        let on = x["enabled"] == true;
                        ui.horizontal(|ui| {
                            ui.label(h(&ipf, 15.0));
                            ui::chip(ui, if on { col::OK } else { col::OFF }, if on { tr("ui.attivo") } else { tr("ui.spento") });
                            if x["state"] != "ok" {
                                ui::chip(ui, col::WARN, &ui::sv(&x["state"]));
                            }
                            let t = if on { tr("common.spegni") } else { tr("common.accendi") };
                            if ui.button(t).clicked() {
                                act = Some(Action::call("PUT", format!("/ip/{e}/firewall/{}", enc(&ipf)), Some(json!({"enabled": !on})), tr("net.firewall_aggiornato"), &r));
                            }
                        });
                        fw_ip = Some(ipf);
                    }
                }
            }
        });
        if let Some(ipf) = fw_ip {
            ui.add_space(12.0);
            let (b4, f4) = (e.clone(), enc(&ipf));
            let rules = self.data(&format!("ip/{block}/rules/{ipf}"), 60.0, move |c| list_detail(c, &format!("/ip/{b4}/firewall/{f4}/rule")));
            ui::card(ui, false, |ui| {
                ui.label(h(fill(tr("net.regole_per"), &[("ipf", ipf.to_string())]), 18.0));
                ui.add_space(8.0);
                let mut used = Vec::new();
                match &rules {
                    None => ui::loading(ui),
                    Some(Err(err)) => {
                        ui::api_error(ui, err);
                    }
                    Some(Ok(v)) => {
                        let mut list: Vec<&Value> = v.as_array().into_iter().flatten().filter(|x| x.is_object()).collect();
                        list.sort_by_key(|x| x["sequence"].as_i64().unwrap_or(99));
                        egui::Grid::new("fw-rules").num_columns(4).striped(true).spacing(vec2(16.0, 6.0)).show(ui, |ui| {
                            for x in list {
                                let seq = x["sequence"].as_i64().unwrap_or(-1);
                                used.push(seq);
                                let permit = x["action"] == "permit";
                                ui.label(format!("#{seq}"));
                                ui::chip(ui, if permit { col::OK } else { col::CRIT }, if permit { "permetti" } else { "blocca" });
                                ui.label(ui::sv(&x["rule"]));
                                if ui.add(ui::ghost(tr("common.elimina"))).clicked() {
                                    act = Some(Action::call("DELETE", format!("/ip/{e}/firewall/{}/rule/{seq}", enc(&ipf)), None, tr("net.regola_in_rimozione"), &r));
                                }
                                ui.end_row();
                            }
                        });
                    }
                }
                ui.add_space(10.0);
                let next = (0..20).find(|n| !used.contains(n));
                ui.horizontal_wrapped(|ui| {
                    egui::ComboBox::from_id_salt("fw-act").selected_text(if self.f.fw_action == "permit" { tr("net.permetti") } else { tr("net.blocca") }).show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.f.fw_action, "permit".into(), tr("net.permetti"));
                        ui.selectable_value(&mut self.f.fw_action, "deny".into(), tr("net.blocca"));
                    });
                    egui::ComboBox::from_id_salt("fw-proto").selected_text(self.f.fw_proto.clone()).show_ui(ui, |ui| {
                        for p in ["tcp", "udp", "icmp", "ipv4", "gre", "esp", "ah"] {
                            ui.selectable_value(&mut self.f.fw_proto, p.into(), p);
                        }
                    });
                    ui.add(egui::TextEdit::singleline(&mut self.f.fw_src).hint_text(tr("net.origine_vuoto_tutti")).desired_width(170.0));
                    if self.f.fw_proto == "tcp" || self.f.fw_proto == "udp" {
                        ui.add(egui::TextEdit::singleline(&mut self.f.fw_port).hint_text("porta").desired_width(70.0));
                    }
                    if ui.add_enabled(next.is_some(), ui::primary(tr("common.aggiungi"))).clicked() {
                        let mut body = json!({"sequence": next, "action": self.f.fw_action, "protocol": self.f.fw_proto});
                        let src = self.f.fw_src.trim();
                        body["source"] = json!(if src.is_empty() { "any" } else { src });
                        if let Ok(p) = self.f.fw_port.trim().parse::<u16>() {
                            body["destinationPort"] = json!(p);
                        }
                        act = Some(Action::call("POST", format!("/ip/{e}/firewall/{}/rule", enc(&ipf)), Some(body), tr("net.regola_in_creazione"), &r));
                    }
                });
                if next.is_none() {
                    ui.label(RichText::new(tr("net.max_regole")).color(col::WARN));
                }
            });
        }
        ui.add_space(12.0);

        // anti-DDoS
        let b5 = e.clone();
        let mit = self.data(&format!("ip/{block}/mit"), 60.0, move |c| list_detail(c, &format!("/ip/{b5}/mitigation")));
        ui::card(ui, false, |ui| {
            ui.label(h(tr("net.anti_ddos"), 18.0));
            ui.add_space(8.0);
            match &mit {
                None => ui::loading(ui),
                Some(Err(err)) => {
                    ui::api_error(ui, err);
                }
                Some(Ok(v)) => {
                    let list: Vec<&Value> = v.as_array().into_iter().flatten().filter(|x| x.is_object()).collect();
                    if list.is_empty() {
                        ui.horizontal(|ui| {
                            ui::dot(ui, col::OK);
                            ui.label(tr("net.nessun_attacco_in_corso_la"));
                        });
                    }
                    for x in list {
                        let auto = x["auto"] == true;
                        ui.horizontal(|ui| {
                            ui::dot(ui, col::WARN);
                            ui.label(h(ui::sv(&x["ipOnMitigation"]), 15.0));
                            ui::chip(ui, col::WARN, if auto { tr("net.attacco_rilevato_mitigazione_attiva") } else { tr("net.mitigazione_permanente") });
                        });
                    }
                }
            }
        });
        if let Some(a) = act {
            if let Action::Calls { steps, .. } = &a {
                if steps[0].0 == "DELETE" || steps[0].0 == "PUT" {
                    return self.ask(tr("net.confermi"), tr("net.la_modifica_arriva_sui_router"), tr("common.conferma"), a);
                }
            }
            self.run(a);
        }
    }
}
