//! Server dedicati: stato, hardware, traffico (MRTG), boot e rescue, attività, reinstallazione, IPMI.
use crate::i18n::{fill, tr};
use crate::app::{par_map, Action, App, Data, Kind};
use crate::ovh::Client;
use crate::ui::{self, col, h, muted};
use crate::vps::{chosen_key, fetch_ssh_keys, ssh_pick_row, tasks_card};
use eframe::egui::{self, vec2, RichText};
use serde_json::{json, Value};

fn fetch_info(c: &Client, s: &str) -> Result<Value, String> {
    let p = format!("/dedicated/server/{s}");
    std::thread::scope(|sc| {
        let srv = sc.spawn(|| c.get(&p));
        let svc = sc.spawn(|| c.get(&format!("{p}/serviceInfos")).unwrap_or(Value::Null));
        let net = sc.spawn(|| c.get(&format!("{p}/specifications/network")).unwrap_or(Value::Null));
        Ok(json!({ "srv": srv.join().unwrap()?, "svc": svc.join().unwrap(), "net": net.join().unwrap() }))
    })
}

/// Traffico della prima scheda pubblica: download e upload (MRTG, valori in bit/s o simili, l'unità arriva dall'API).
fn fetch_traffic(c: &Client, s: &str, period: &str) -> Result<Value, String> {
    let macs: Vec<String> = serde_json::from_value(c.get(&format!("/dedicated/server/{s}/networkInterfaceController?linkType=public"))?).unwrap_or_default();
    let Some(mac) = macs.first() else { return Ok(json!({"rows": []})) };
    let mac = crate::ovh::enc(mac);
    let types = ["traffic:download", "traffic:upload"];
    let res = par_map(&types, 2, |t| c.get(&format!("/dedicated/server/{s}/networkInterfaceController/{mac}/mrtg?period={period}&type={t}")));
    let rows: Vec<Value> = types
        .iter()
        .zip(res)
        .map(|(t, r)| match r {
            Err(e) => json!({"label": t, "err": e}),
            Ok(v) => {
                let a = v.as_array().cloned().unwrap_or_default();
                let unit = a.iter().find_map(|p| p["value"]["unit"].as_str().map(String::from)).unwrap_or_default();
                let used: Vec<Value> = a.iter().map(|p| p["value"]["value"].clone()).collect();
                json!({"label": if *t == "traffic:download" { tr("dedi.download") } else { tr("dedi.upload") }, "unit": unit, "used": used})
            }
        })
        .collect();
    Ok(json!({ "rows": rows }))
}

impl App {
    pub fn dedi_info(&mut self, s: &str) -> Option<Data> {
        let s2 = s.to_string();
        self.data(&format!("dedi/{s}/info"), 60.0, move |c| fetch_info(c, &s2))
    }

    pub fn dedi_ui(&mut self, ui: &mut egui::Ui, s: &str) {
        let info = self.dedi_info(s);
        let (c, st) = self.machine_state(Kind::Dedi, s);
        let title = match &info {
            Some(Ok(v)) => v["srv"]["iam"]["displayName"].as_str().filter(|d| !d.is_empty()).unwrap_or(s).to_string(),
            _ => s.to_string(),
        };
        if ui::hero(ui, &[tr("common.server_crumb"), tr("dedi.server_dedicati"), &title], &title, Some((c, &st)), s) {
            self.refresh(&format!("dedi/{s}/"));
        }
        self.tabs(ui, &[tr("common.panoramica"), tr("dedi.hardware"), tr("dedi.traffico"), tr("dedi.boot_e_rescue"), tr("common.attivita"), tr("common.reinstalla")]);
        ui::scroll_fill(ui, "dedi-tab", |ui| match self.tab {
            0 => self.dedi_state_tab(ui, s, info),
            1 => self.dedi_hw_tab(ui, s),
            2 => self.dedi_traffic_tab(ui, s, info),
            3 => self.dedi_boot_tab(ui, s, info),
            4 => {
                let s2 = s.to_string();
                let t = self.data(&format!("dedi/{s}/tasks"), 30.0, move |c| {
                    let mut ids: Vec<i64> = serde_json::from_value(c.get(&format!("/dedicated/server/{s2}/task"))?).unwrap_or_default();
                    ids.sort_unstable_by(|a, b| b.cmp(a));
                    ids.truncate(25);
                    Ok(Value::Array(par_map(&ids, 8, |id| c.get(&format!("/dedicated/server/{s2}/task/{id}")).unwrap_or(Value::Null))))
                });
                match t {
                    None => ui::loading(ui),
                    Some(Ok(t)) => tasks_card(ui, &t, "function", "startDate"),
                    Some(Err(e)) => {
                        ui::api_error(ui, &e);
                    }
                }
            }
            _ => self.dedi_reinstall_tab(ui, s),
        });
    }

    fn dedi_state_tab(&mut self, ui: &mut egui::Ui, s: &str, info: Option<Data>) {
        let Some(info) = info else { return ui::loading(ui) };
        let info = match info {
            Ok(i) => i,
            Err(e) => {
                if ui::api_error(ui, &e) {
                    crate::app::open_browser(crate::app::TOKEN_URL);
                }
                return;
            }
        };
        let v = &info["srv"];
        let svc = &info["svc"];
        let exp = svc["expiration"].as_str().unwrap_or("");
        let days = ui::days_until(exp, self.now_unix());
        let rows = [
            (tr("common.ip"), ui::sv(&v["ip"])),
            (tr("dedi.reverse"), ui::sv(&v["reverse"])),
            (tr("dedi.gamma"), ui::sv(&v["commercialRange"])),
            (tr("dedi.datacenter"), format!("{} · rack {}", ui::sv(&v["datacenter"]).to_uppercase(), ui::sv(&v["rack"]))),
            (tr("common.sistema"), ui::sv(&v["os"])),
            (tr("dedi.alimentazione"), ui::state_it(v["powerState"].as_str().unwrap_or("?")).to_string()),
            (tr("dedi.banda"), ui::uv(&info["net"]["bandwidth"]["OvhToInternet"])),
            (tr("dedi.scadenza"), format!("{}{}", ui::it_date(exp), days.map(|d| format!("  {}", fill(tr("common.tra_giorni"), &[("d", d.to_string())]))).unwrap_or_default())),
        ];
        let monitoring = v["monitoring"].as_bool().unwrap_or(false);
        let mut act: Option<u8> = None;
        ui::card(ui, false, |ui| {
            ui.label(h(tr("dedi.dettagli"), 18.0));
            ui.add_space(8.0);
            ui::kv(ui, "dedi-kv", &rows);
            let t = &info["net"]["traffic"];
            if let (Some(used), Some(size)) = (t["outputQuotaUsed"]["value"].as_f64(), t["outputQuotaSize"]["value"].as_f64()) {
                ui.add_space(10.0);
                ui.label(muted(fill(tr("common.traffico_in_uscita_su"), &[("0", (ui::uv(&t["outputQuotaUsed"])).to_string()), ("1", (ui::uv(&t["outputQuotaSize"])).to_string())])));
                ui::bar(ui, (used / size.max(1e-9)) as f32, if t["isThrottled"] == true { col::CRIT } else { col::BRAND });
            }
        });
        ui.add_space(12.0);
        ui::card(ui, false, |ui| {
            ui.label(h(tr("dedi.azioni"), 18.0));
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                if ui.add(ui::primary(tr("common.riavvia"))).clicked() {
                    act = Some(1);
                }
                if ui.button(tr("dedi.console_ipmi")).on_hover_text(tr("dedi.kvm_html5_nel_browser_sessione")).clicked() {
                    act = Some(2);
                }
                if ui.button(tr("dedi.rescue")).clicked() {
                    act = Some(3);
                }
                let label = if monitoring { tr("dedi.monitoraggio_ovh_attivo") } else { tr("dedi.monitoraggio_ovh_spento") };
                if ui.button(label).on_hover_text(tr("dedi.ovh_fa_ping_al_server")).clicked() {
                    act = Some(4);
                }
            });
        });
        let r = format!("dedi/{s}/");
        match act {
            Some(1) => self.ask(
                tr("dedi.riavviare_il_server"),
                tr("dedi.riavvio_forzato_hard_reboot_come"),
                tr("common.riavvia"),
                Action::call("POST", format!("/dedicated/server/{s}/reboot"), None, tr("common.riavvio_inviato"), &r),
            ),
            Some(2) => {
                let s2 = s.to_string();
                self.toast(tr("dedi.chiedo_la_sessione_ipmi_a"), false);
                self.open_url_job(move |c| {
                    let p = format!("/dedicated/server/{s2}/features/ipmi/access");
                    c.post(&p, Some(json!({"type": "kvmipHtml5URL", "ttl": 15})))?;
                    for _ in 0..30 {
                        std::thread::sleep(std::time::Duration::from_secs(2));
                        if let Ok(v) = c.get(&format!("{p}?type=kvmipHtml5URL")) {
                            if let Some(u) = v["value"].as_str() {
                                return Ok(u.to_string());
                            }
                        }
                    }
                    Err(tr("dedi.ovh_non_ha_preparato_la").into())
                });
            }
            Some(3) => self.set_tab(3),
            Some(4) => self.ask(
                if monitoring { tr("dedi.spegnere_il_monitoraggio_ovh") } else { tr("dedi.accendere_il_monitoraggio_ovh") },
                tr("dedi.con_il_monitoraggio_attivo_ovh"),
                tr("common.conferma"),
                Action::call("PUT", format!("/dedicated/server/{s}"), Some(json!({"monitoring": !monitoring})), tr("dedi.fatto"), &r),
            ),
            _ => {}
        }
    }

    fn dedi_hw_tab(&mut self, ui: &mut egui::Ui, s: &str) {
        let s2 = s.to_string();
        let Some(hw) = self.data(&format!("dedi/{s}/hw"), 3600.0, move |c| c.get(&format!("/dedicated/server/{s2}/specifications/hardware"))) else {
            return ui::loading(ui);
        };
        let hw = match hw {
            Ok(v) => v,
            Err(e) => {
                ui::api_error(ui, &e);
                return;
            }
        };
        let cpu = format!(
            "{} · {} CPU × {} core / {} thread",
            ui::sv(&hw["processorName"]),
            ui::sv(&hw["numberOfProcessors"]),
            ui::sv(&hw["coresPerProcessor"]),
            ui::sv(&hw["threadsPerProcessor"])
        );
        ui::card(ui, false, |ui| {
            ui.label(h(ui::sv(&hw["description"]), 18.0));
            ui.add_space(8.0);
            ui::kv(
                ui,
                "hw-kv",
                &[
                    (tr("dedi.processore"), cpu),
                    ("RAM", ui::uv(&hw["memorySize"])),
                    (tr("dedi.scheda_madre"), ui::sv(&hw["motherboard"])),
                    (tr("dedi.formato"), ui::sv(&hw["formFactor"])),
                    (tr("dedi.avvio"), ui::sv(&hw["bootMode"])),
                    (tr("dedi.raid_predefinito"), format!("{} {}", ui::sv(&hw["defaultHardwareRaidType"]), ui::uv(&hw["defaultHardwareRaidSize"]))),
                ],
            );
        });
        ui.add_space(12.0);
        ui::card(ui, false, |ui| {
            ui.label(h(tr("dedi.dischi"), 18.0));
            ui.add_space(8.0);
            for g in hw["diskGroups"].as_array().into_iter().flatten() {
                ui.label(format!("{} × {} {} · {}", ui::sv(&g["numberOfDisks"]), ui::uv(&g["diskSize"]), ui::sv(&g["diskType"]), ui::sv(&g["description"])));
                if let Some(rc) = g["raidController"].as_str() {
                    ui.label(muted(fill(tr("dedi.controller_raid"), &[("rc", rc.to_string())])));
                }
                ui.add_space(4.0);
            }
        });
    }

    fn dedi_traffic_tab(&mut self, ui: &mut egui::Ui, s: &str, info: Option<Data>) {
        if let Some(Ok(i)) = &info {
            let n = &i["net"];
            ui::card(ui, false, |ui| {
                ui.label(h(tr("common.rete"), 18.0));
                ui.add_space(8.0);
                ui::kv(
                    ui,
                    "net-kv",
                    &[
                        (tr("dedi.verso_internet"), ui::uv(&n["bandwidth"]["OvhToInternet"])),
                        (tr("dedi.da_internet"), ui::uv(&n["bandwidth"]["InternetToOvh"])),
                        (tr("dedi.interna_ovh"), ui::uv(&n["bandwidth"]["OvhToOvh"])),
                        (tr("dedi.porta"), ui::uv(&n["connection"])),
                        ("vRack", ui::uv(&n["vrack"]["bandwidth"])),
                        ("IPv4", format!("{} (gw {})", ui::sv(&n["routing"]["ipv4"]["ip"]), ui::sv(&n["routing"]["ipv4"]["gateway"]))),
                        ("IPv6", format!("{} (gw {})", ui::sv(&n["routing"]["ipv6"]["ip"]), ui::sv(&n["routing"]["ipv6"]["gateway"]))),
                    ],
                );
            });
            ui.add_space(12.0);
        }
        let mut pick = None;
        let period = match self.period {
            "1h" => "hourly",
            "lastweek" => "weekly",
            _ => "daily",
        };
        ui.horizontal(|ui| {
            for (p, l) in [("1h", tr("common.1_ora")), ("lastday", tr("common.24_ore")), ("lastweek", tr("common.7_giorni"))] {
                if ui.selectable_label(self.period == p, l).clicked() {
                    pick = Some(p);
                }
            }
        });
        if let Some(p) = pick {
            self.period = p;
        }
        let s2 = s.to_string();
        let Some(t) = self.data(&format!("dedi/{s}/traffic/{period}"), 120.0, move |c| fetch_traffic(c, &s2, period)) else {
            return ui::loading(ui);
        };
        let t = match t {
            Ok(t) => t,
            Err(e) => {
                ui::api_error(ui, &e);
                return;
            }
        };
        for (i, r) in t["rows"].as_array().into_iter().flatten().enumerate() {
            ui::card(ui, false, |ui| {
                ui.label(h(ui::sv(&r["label"]), 16.0));
                if let Some(e) = r["err"].as_str() {
                    ui.label(RichText::new(e).color(col::CRIT));
                    return;
                }
                let p = crate::vps::pts(r);
                if let Some(l) = crate::vps::last(r) {
                    ui.label(h(format!("{} {}", ui::fmt_num(l), ui::sv(&r["unit"])), 22.0));
                }
                ui::spark(ui, &p, None, if i == 0 { col::BRAND } else { col::SKY }, 90.0);
            });
            ui.add_space(12.0);
        }
    }

    fn dedi_boot_tab(&mut self, ui: &mut egui::Ui, s: &str, info: Option<Data>) {
        let cur = info.as_ref().and_then(|i| i.as_ref().ok()).and_then(|i| i["srv"]["bootId"].as_i64());
        let s2 = s.to_string();
        let Some(boots) = self.data(&format!("dedi/{s}/boots"), 3600.0, move |c| {
            let ids: Vec<i64> = serde_json::from_value(c.get(&format!("/dedicated/server/{s2}/boot"))?).unwrap_or_default();
            Ok(Value::Array(par_map(&ids, 8, |id| c.get(&format!("/dedicated/server/{s2}/boot/{id}")).unwrap_or(json!({"bootId": id})))))
        }) else {
            return ui::loading(ui);
        };
        let boots = match boots {
            Ok(b) => b,
            Err(e) => {
                ui::api_error(ui, &e);
                return;
            }
        };
        let mut go = None;
        ui::card(ui, false, |ui| {
            ui.label(h(tr("dedi.avvio"), 18.0));
            ui.label(muted(tr("dedi.rescue_un_sistema_minimale_che")));
            ui.add_space(10.0);
            for b in boots.as_array().into_iter().flatten() {
                let id = b["bootId"].as_i64().unwrap_or(-1);
                let kind = ui::sv(&b["bootType"]);
                ui.horizontal(|ui| {
                    let sel = self.f.boot.unwrap_or(cur.unwrap_or(-1)) == id;
                    if ui.radio(sel, "").clicked() {
                        self.f.boot = Some(id);
                    }
                    ui.label(ui::sv(&b["description"]));
                    ui::chip(ui, if kind == "rescue" { col::WARN } else { col::OFF }, &kind);
                    if Some(id) == cur {
                        ui::chip(ui, col::OK, tr("ui.attuale"));
                    }
                });
            }
            ui.add_space(12.0);
            let target = self.f.boot.filter(|b| Some(*b) != cur);
            ui.horizontal(|ui| {
                if ui.add_enabled(target.is_some(), ui::primary(tr("dedi.imposta_e_riavvia"))).clicked() {
                    go = target.map(|t| (t, true));
                }
                if ui.add_enabled(target.is_some(), egui::Button::new(tr("dedi.imposta_senza_riavviare"))).clicked() {
                    go = target.map(|t| (t, false));
                }
            });
        });
        if let Some((id, reboot)) = go {
            let mut steps = vec![("PUT", format!("/dedicated/server/{s}"), Some(json!({ "bootId": id })))];
            if reboot {
                steps.push(("POST", format!("/dedicated/server/{s}/reboot"), None));
            }
            let a = Action::Calls { steps, ok: if reboot { tr("dedi.boot_cambiato_riavvio_inviato").into() } else { tr("dedi.boot_cambiato").into() }, refresh: format!("dedi/{s}/") };
            self.ask(
                tr("dedi.cambiare_il_boot"),
                if reboot { tr("dedi.il_server_si_riavvia_subito") } else { tr("dedi.vale_dal_prossimo_riavvio") },
                tr("common.conferma"),
                a,
            );
            self.f.boot = None;
        }
    }

    fn dedi_reinstall_tab(&mut self, ui: &mut egui::Ui, s: &str) {
        let s2 = s.to_string();
        let tpl = self.data(&format!("dedi/{s}/templates"), 3600.0, move |c| c.get(&format!("/dedicated/server/{s2}/install/compatibleTemplates")));
        let s3 = s.to_string();
        let status = self.data(&format!("dedi/{s}/install"), 15.0, move |c| c.get(&format!("/dedicated/server/{s3}/install/status")));
        let keys = self.data("me/ssh", 300.0, fetch_ssh_keys);
        if let Some(Ok(st)) = &status {
            ui::card(ui, false, |ui| {
                ui.label(h(tr("dedi.installazione_in_corso"), 18.0));
                for step in st["progress"].as_array().into_iter().flatten() {
                    let ss = step["status"].as_str().unwrap_or("?");
                    ui.horizontal(|ui| {
                        ui::chip(ui, ui::state_color(ss), ui::state_it(ss));
                        ui.label(ui::sv(&step["comment"]));
                    });
                }
            });
            ui.add_space(12.0);
        }
        let mut go = false;
        ui::card(ui, false, |ui| {
            ui.label(h(tr("common.reinstalla_il_sistema"), 18.0));
            ui.label(RichText::new(tr("dedi.cancella_tutti_i_dischi")).color(col::CRIT));
            ui.add_space(10.0);
            let list: Vec<String> = match &tpl {
                None => return ui::loading(ui),
                Some(Err(e)) => {
                    ui::api_error(ui, e);
                    return;
                }
                Some(Ok(v)) => v["ovh"].as_array().into_iter().flatten().filter_map(|x| x.as_str().map(String::from)).collect(),
            };
            egui::Grid::new("dedi-re").num_columns(2).spacing(vec2(20.0, 10.0)).show(ui, |ui| {
                ui.label(muted(tr("common.sistema")));
                let sel = if self.f.os.is_empty() { tr("common.scegli_il_sistema").to_string() } else { self.f.os.clone() };
                egui::ComboBox::from_id_salt("dedi-os").selected_text(sel).width(320.0).show_ui(ui, |ui| {
                    for o in &list {
                        ui.selectable_value(&mut self.f.os, o.clone(), o);
                    }
                });
                ui.end_row();
                ui.label(muted(tr("dedi.hostname")));
                ui.add(egui::TextEdit::singleline(&mut self.f.host).hint_text("facoltativo").desired_width(320.0));
                ui.end_row();
                ssh_pick_row(ui, &mut self.f.ssh_pick, &mut self.f.ssh_paste, &keys);
                ui.end_row();
            });
            ui.add_space(12.0);
            if ui.add_enabled(!self.f.os.is_empty(), ui::danger(tr("common.reinstalla_2"))).clicked() {
                go = true;
            }
        });
        if go {
            let mut cust = json!({});
            if !self.f.host.trim().is_empty() {
                cust["hostname"] = json!(self.f.host.trim());
            }
            if let Some(k) = chosen_key(&self.f.ssh_pick, &self.f.ssh_paste, &keys) {
                cust["sshKey"] = json!(k);
            }
            let body = json!({ "operatingSystem": self.f.os, "customizations": cust });
            self.ask_typed(
                tr("dedi.reinstallare_il_server"),
                tr("dedi.tutti_i_dischi_vengono_cancellati"),
                tr("common.reinstalla"),
                s,
                Action::call("POST", format!("/dedicated/server/{s}/reinstall"), Some(body), tr("common.reinstallazione_avviata"), &format!("dedi/{s}/")),
            );
        }
    }
}
