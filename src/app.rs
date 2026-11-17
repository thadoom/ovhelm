//! Stato dell'app, store dei dati, guscio (rail, intestazione, transizioni), conferme e login.
use crate::i18n::{fill, tr};
use crate::ovh::{Client, Creds};
use crate::ui::{self, col, h, muted};
use crate::{tray, watch};
use eframe::egui::{self, pos2, vec2, Align2, FontId, Id, Rect, RichText, Sense, Stroke};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

/// Token su tutte le API: lo crei una volta e vale anche per le sezioni che arriveranno.
/// Vuol dire accesso completo all'account (ordini e disdette comprese): l'app non le chiama, ma le chiavi
/// vanno trattate come la password. Il README ha l'elenco ristretto per chi preferisce limitarlo.
pub const TOKEN_URL: &str = "https://eu.api.ovh.com/createToken/?GET=/*&POST=/*&PUT=/*&DELETE=/*";

// ---------- credenziali: solo Credential Manager di Windows ----------
fn kr() -> keyring::Result<keyring::Entry> {
    keyring::Entry::new("Ovhelm", "ovh-api")
}
pub fn load_creds() -> Option<Creds> {
    if let Some(c) = kr().ok().and_then(|e| e.get_password().ok()).and_then(|s| serde_json::from_str(&s).ok()) {
        return Some(c);
    }
    // prima del cambio di nome le chiavi stavano sotto "ovh-panel": le sposta
    let old = keyring::Entry::new("ovh-panel", "ovh-api").ok()?;
    let c: Creds = serde_json::from_str(&old.get_password().ok()?).ok()?;
    if save_creds(&c).is_ok() {
        let _ = old.delete_credential();
    }
    Some(c)
}
fn save_creds(c: &Creds) -> Result<(), String> {
    let s = serde_json::to_string(c).map_err(|e| e.to_string())?;
    kr().and_then(|e| e.set_password(&s)).map_err(|e| e.to_string())
}
fn forget_creds() {
    if let Ok(e) = kr() {
        let _ = e.delete_credential();
    }
}

/// %APPDATA%\Ovhelm: pin e lingua (niente segreti). La cartella del vecchio nome si sposta qui.
pub fn data_dir() -> Option<std::path::PathBuf> {
    let base = std::path::PathBuf::from(std::env::var_os("APPDATA")?);
    let dir = base.join("Ovhelm");
    let old = base.join("ovh-panel");
    if !dir.exists() && old.exists() {
        let _ = std::fs::rename(&old, &dir);
    }
    Some(dir)
}
fn pins_path() -> Option<std::path::PathBuf> {
    data_dir().map(|d| d.join("pins.json"))
}
fn load_pins() -> Option<Vec<String>> {
    serde_json::from_str(&std::fs::read_to_string(pins_path()?).ok()?).ok()
}

pub fn open_browser(url: &str) {
    if url.starts_with("https://") {
        let _ = std::process::Command::new("rundll32").args(["url.dll,FileProtocolHandler", url]).spawn();
    }
}

/// map in parallelo, al massimo `n` richieste insieme
pub fn par_map<T: Sync, R: Send>(items: &[T], n: usize, f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let mut out = Vec::with_capacity(items.len());
    for chunk in items.chunks(n.max(1)) {
        std::thread::scope(|sc| {
            let hs: Vec<_> = chunk.iter().map(|x| sc.spawn(|| f(x))).collect();
            out.extend(hs.into_iter().map(|h| h.join().unwrap()));
        });
    }
    out
}

pub type Data = Result<Arc<Value>, String>;
struct Slot {
    val: Option<Data>,
    at: f64,
    loading: bool,
}

pub enum Msg {
    Connected(Result<Client, String>, bool, Creds),
    Data(String, Result<Value, String>),
    Done(Result<String, String>, String),
    Open(Result<String, String>),
    Alert(watch::Alert),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Vps,
    Dedi,
}
#[derive(Clone, PartialEq)]
pub enum View {
    Overview,
    Machine(Kind, String),
    Network,
    Account,
}

/// Una o più chiamate API in fila (es. imposta boot rescue, poi riavvia).
#[derive(Clone)]
pub enum Action {
    Calls { steps: Vec<(&'static str, String, Option<Value>)>, ok: String, refresh: String },
    Forget,
}
impl Action {
    pub fn call(method: &'static str, path: String, body: Option<Value>, ok: &str, refresh: &str) -> Self {
        Action::Calls { steps: vec![(method, path, body)], ok: ok.into(), refresh: refresh.into() }
    }
}

pub struct Confirm {
    pub action: Action,
    pub title: String,
    pub body: String,
    pub yes: String,
    /// passaggi prima di eseguire (2 = "Continua" e poi l'ultima conferma)
    pub steps: u8,
    pub step: u8,
    /// testo da digitare all'ultimo passaggio
    pub typed: Option<String>,
    pub input: String,
}

/// Campi dei form delle pagine.
#[derive(Default)]
pub struct Forms {
    pub snap_desc: String,
    pub img: String,
    pub ssh_pick: String,
    pub ssh_paste: String,
    pub host: String,
    pub no_pw: bool,
    pub os: String,
    pub boot: Option<i64>,
    pub ip: Option<String>,
    pub rev_ip: String,
    pub rev_name: String,
    pub fw_action: String,
    pub fw_proto: String,
    pub fw_src: String,
    pub fw_port: String,
    pub key_name: String,
    pub key_text: String,
    pub svc_filter: String,
}

pub struct App {
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    pub ctx: egui::Context,
    pub client: Option<Client>,
    form: Creds,
    connecting: bool,
    login_err: Option<String>,
    store: HashMap<String, Slot>,
    pub view: View,
    pub tab: usize,
    pub confirm: Option<Confirm>,
    note: Option<(String, bool, f64)>,
    pub alerts: Vec<watch::Alert>,
    pub period: &'static str,
    pub f: Forms,
    epoch: Arc<AtomicU64>,
    hwnd_set: bool,
    hid_once: bool,
    pub now: f64,
    pub compact: bool,
    last_frame: std::time::Instant,
    user_open: bool,
    lang_open: bool,
    /// riquadri scelti per la dashboard; None = quelli predefiniti
    pub pins: Option<Vec<String>>,
    pub editing: bool,
    pub add_pick: String,
    /// --demo: dati inventati, nessuna chiamata alle API
    pub demo: bool,
}

impl App {
    pub fn new(cc: &eframe::CreationContext) -> Self {
        ui::theme(&cc.egui_ctx);
        let (tx, rx) = channel();
        let mut app = App {
            tx,
            rx,
            ctx: cc.egui_ctx.clone(),
            client: None,
            form: Creds { app_key: String::new(), app_secret: String::new(), consumer_key: String::new() },
            connecting: false,
            login_err: None,
            store: HashMap::new(),
            view: View::Overview,
            tab: 0,
            confirm: None,
            note: None,
            alerts: Vec::new(),
            period: "lastday",
            f: Forms { fw_action: "deny".into(), fw_proto: "tcp".into(), ..Default::default() },
            epoch: Arc::new(AtomicU64::new(0)),
            hwnd_set: false,
            hid_once: false,
            now: 0.0,
            compact: false,
            last_frame: std::time::Instant::now(),
            user_open: false,
            lang_open: false,
            pins: load_pins(),
            editing: false,
            add_pick: String::new(),
            demo: false,
        };
        // --demo [--page dashboard|vps|monitor|network|account|bills|dedicated] [--lang xx]
        let args: Vec<String> = std::env::args().collect();
        let arg = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
        if args.iter().any(|a| a == "--demo") {
            app.demo = true;
            app.alerts = crate::demo::alerts();
            if let Some(l) = arg("--lang") {
                crate::i18n::force(&l);
            }
            let vps = crate::demo::VPS[0].to_string();
            (app.view, app.tab) = match arg("--page").as_deref() {
                Some("vps") => (View::Machine(Kind::Vps, vps), 0),
                Some("monitor") => (View::Machine(Kind::Vps, vps), 1),
                Some("network") => (View::Network, 0),
                Some("account") => (View::Account, 0),
                Some("bills") => (View::Account, 1),
                Some("dedicated") => (View::Machine(Kind::Dedi, crate::demo::DEDI.into()), 0),
                _ => (View::Overview, 0),
            };
            return app;
        }
        if let Some(c) = load_creds() {
            app.form = c.clone();
            app.connect(c, false);
        }
        app
    }

    fn connect(&mut self, creds: Creds, save: bool) {
        self.connecting = true;
        self.login_err = None;
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            // /vps come prova: ogni token valido per questa app lo può leggere
            let r = Client::connect(creds.clone()).and_then(|c| c.get("/vps").map(|_| c));
            let _ = tx.send(Msg::Connected(r, save, creds));
            ctx.request_repaint();
        });
    }

    /// Dato `key`: lo carica in background la prima volta e lo ricarica quando ha più di `ttl`
    /// secondi (0 = mai), continuando a mostrare il valore vecchio intanto.
    pub fn data(&mut self, key: &str, ttl: f64, f: impl FnOnce(&Client) -> Result<Value, String> + Send + 'static) -> Option<Data> {
        if self.demo {
            if !self.store.contains_key(key) {
                let v = crate::demo::get(key, self.now_unix()).map(|v| Ok(Arc::new(v)));
                self.store.insert(key.to_string(), Slot { val: v, at: self.now, loading: false });
            }
            return self.store.get(key).and_then(|s| s.val.clone());
        }
        let now = self.now;
        let need = match self.store.get(key) {
            None => true,
            Some(s) => !s.loading && (s.val.is_none() || (ttl > 0.0 && now - s.at > ttl) || s.at < -1e17),
        };
        if need {
            if let Some(c) = self.client.clone() {
                let slot = self.store.entry(key.to_string()).or_insert(Slot { val: None, at: now, loading: true });
                slot.loading = true;
                let (tx, ctx, k) = (self.tx.clone(), self.ctx.clone(), key.to_string());
                std::thread::spawn(move || {
                    let _ = tx.send(Msg::Data(k, f(&c)));
                    ctx.request_repaint();
                });
            }
        }
        if ttl > 0.0 {
            self.ctx.request_repaint_after(std::time::Duration::from_secs_f64(ttl.max(1.0)));
        }
        self.store.get(key).and_then(|s| s.val.clone())
    }
    /// Ricarica tutto quello che inizia con `prefix` (il vecchio resta visibile finché arriva il nuovo).
    pub fn refresh(&mut self, prefix: &str) {
        for (k, s) in self.store.iter_mut() {
            if k.starts_with(prefix) {
                s.at = -1e18;
            }
        }
    }

    /// Lavoro in background che finisce in un messaggio.
    pub fn job(&self, f: impl FnOnce(&Client) -> Msg + Send + 'static) {
        let Some(c) = self.client.clone() else { return };
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            let _ = tx.send(f(&c));
            ctx.request_repaint();
        });
    }
    pub fn open_url_job(&self, f: impl FnOnce(&Client) -> Result<String, String> + Send + 'static) {
        self.job(move |c| Msg::Open(f(c)));
        self.ctx.request_repaint();
    }
    /// Ora unix: quella del server OVH se collegati, altrimenti quella del PC (demo).
    pub fn now_unix(&self) -> i64 {
        self.client.as_ref().map(|c| c.server_now()).unwrap_or_else(|| {
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
        })
    }

    pub fn is_pinned(&mut self, id: &str) -> bool {
        match &self.pins {
            Some(p) => p.iter().any(|x| x == id),
            None => self.default_pins().iter().any(|x| x == id),
        }
    }
    /// Mette o toglie un riquadro dalla dashboard e salva subito.
    pub fn toggle_pin(&mut self, id: &str) {
        let mut v = self.pins.clone().unwrap_or_else(|| self.default_pins());
        if let Some(i) = v.iter().position(|x| x == id) {
            v.remove(i);
        } else {
            v.push(id.to_string());
        }
        self.pins = Some(v);
        if let (Some(path), Some(v)) = (pins_path(), &self.pins) {
            let _ = std::fs::create_dir_all(path.parent().unwrap_or(&path));
            let _ = std::fs::write(&path, serde_json::to_string(v).unwrap_or_default());
        }
    }

    pub fn toast(&mut self, text: impl Into<String>, err: bool) {
        self.note = Some((text.into(), err, self.now));
    }

    pub fn go(&mut self, v: View) {
        if v == self.view {
            return;
        }
        self.view = v;
        self.tab = 0;
        self.f.ip = None;
    }
    pub fn set_tab(&mut self, t: usize) {
        self.tab = t;
    }

    pub fn ask(&mut self, title: &str, body: &str, yes: &str, action: Action) {
        self.confirm = Some(Confirm { action, title: title.into(), body: body.into(), yes: yes.into(), steps: 1, step: 0, typed: None, input: String::new() });
    }
    /// Due passaggi e il nome da digitare: per quello che cancella dati.
    pub fn ask_typed(&mut self, title: &str, body: &str, yes: &str, typed: &str, action: Action) {
        self.confirm = Some(Confirm {
            action,
            title: title.into(),
            body: body.into(),
            yes: yes.into(),
            steps: 2,
            step: 0,
            typed: Some(typed.into()),
            input: String::new(),
        });
    }

    pub fn run(&mut self, a: Action) {
        match a {
            Action::Calls { steps, ok, refresh } => {
                let r2 = refresh.clone();
                self.job(move |c| {
                    for (m, p, b) in steps {
                        if let Err(e) = c.call(m, &p, b) {
                            return Msg::Done(Err(e), r2);
                        }
                    }
                    Msg::Done(Ok(ok), r2)
                });
            }
            Action::Forget => {
                forget_creds();
                self.epoch.fetch_add(1, Ordering::Relaxed);
                self.client = None;
                self.store.clear();
                self.alerts.clear();
                self.form = Creds { app_key: String::new(), app_secret: String::new(), consumer_key: String::new() };
                self.view = View::Overview;
                self.note = None;
            }
        }
    }

    fn handle(&mut self, m: Msg) {
        match m {
            Msg::Connected(r, save, creds) => {
                self.connecting = false;
                match r {
                    Ok(c) => {
                        if save {
                            if let Err(e) = save_creds(&creds) {
                                self.toast(fill(tr("app.collegato_ma_non_riesco_a"), &[("e", e.to_string())]), true);
                            }
                        }
                        let mine = self.epoch.fetch_add(1, Ordering::Relaxed) + 1;
                        watch::spawn(c.clone(), self.tx.clone(), self.ctx.clone(), self.epoch.clone(), mine);
                        self.client = Some(c);
                    }
                    Err(e) => self.login_err = Some(e),
                }
            }
            Msg::Data(k, r) => {
                let now = self.now;
                let s = self.store.entry(k).or_insert(Slot { val: None, at: now, loading: false });
                s.val = Some(r.map(Arc::new));
                s.at = now;
                s.loading = false;
            }
            Msg::Open(r) => match r {
                Ok(u) => {
                    open_browser(&u);
                    self.toast(tr("app.aperto_nel_browser"), false);
                }
                Err(e) => self.toast(fill(tr("app.non_riesco_ad_aprirlo"), &[("e", e.to_string())]), true),
            },
            Msg::Done(r, refresh) => {
                match r {
                    Ok(s) => self.toast(s, false),
                    Err(e) => self.toast(fill(tr("app.errore"), &[("e", e.to_string())]), true),
                }
                self.refresh(&refresh);
            }
            Msg::Alert(a) => {
                self.alerts.insert(0, a);
                self.alerts.truncate(50);
            }
        }
    }

    // ---------- macchine ----------
    pub fn machines(&mut self) -> (Vec<(Kind, String)>, Option<String>) {
        let mut out = Vec::new();
        let mut dedi_err = None;
        if let Some(Ok(v)) = self.data("list/vps", 300.0, |c| c.get("/vps")) {
            out.extend(v.as_array().into_iter().flatten().filter_map(|s| s.as_str()).map(|s| (Kind::Vps, s.to_string())));
        }
        match self.data("list/dedi", 300.0, |c| c.get("/dedicated/server")) {
            Some(Ok(v)) => out.extend(v.as_array().into_iter().flatten().filter_map(|s| s.as_str()).map(|s| (Kind::Dedi, s.to_string()))),
            Some(Err(e)) => dedi_err = Some(e),
            None => {}
        }
        (out, dedi_err)
    }
    /// stato sintetico per pallini e card: (colore, testo)
    pub fn machine_state(&mut self, k: Kind, s: &str) -> (egui::Color32, String) {
        let info = match k {
            Kind::Vps => self.vps_info(s),
            Kind::Dedi => self.dedi_info(s),
        };
        match info {
            Some(Ok(v)) => {
                let st = match k {
                    Kind::Vps => v["vps"]["state"].as_str().unwrap_or("?").to_string(),
                    Kind::Dedi => {
                        let st = v["srv"]["state"].as_str().unwrap_or("?");
                        if st == "ok" { v["srv"]["powerState"].as_str().unwrap_or("poweron").to_string() } else { st.to_string() }
                    }
                };
                (ui::state_color(&st), ui::state_it(&st).to_string())
            }
            Some(Err(_)) => (col::OFF, "?".into()),
            None => (col::OFF, "...".into()),
        }
    }

    // ---------- guscio ----------
    /// Barra in alto: marchio, nome, badge, avviso, e i bottoni della finestra (riduci, ingrandisci, chiudi).
    /// Sostituisce la barra di Windows: si trascina da qui, doppio clic ingrandisce.
    fn header(&mut self, ui: &mut egui::Ui) {
        let full = ui.max_rect();
        let ctx = ui.ctx().clone();
        let maxed = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
        let drag = ui.interact(full, Id::new("titlebar"), Sense::click_and_drag());
        if drag.drag_started_by(egui::PointerButton::Primary) {
            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        if drag.double_clicked() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maxed));
        }
        // bottoni finestra: 46 x 48 come quelli di Windows, disegnati a mano
        let bw = 46.0;
        let mut x = full.right();
        for kind in [2u8, 1, 0] {
            x -= bw;
            let r = Rect::from_min_size(pos2(x, full.top()), vec2(bw, full.height() - 1.0));
            let resp = ui.interact(r, Id::new(("winbtn", kind)), Sense::click());
            let p = ui.painter();
            let hot = resp.hovered();
            if hot {
                p.rect_filled(r, 0, if kind == 2 { egui::Color32::from_rgb(0xC4, 0x2B, 0x1C) } else { col::BG });
            }
            let c = if hot && kind == 2 { col::WHITE } else { col::HEAD };
            let s = Stroke::new(1.0, c);
            let m = r.center();
            match kind {
                0 => {
                    p.line_segment([m + vec2(-5.0, 0.0), m + vec2(5.0, 0.0)], s);
                }
                1 if maxed => {
                    p.rect_stroke(Rect::from_center_size(m + vec2(-1.5, 1.5), vec2(8.0, 8.0)), 1, s, egui::StrokeKind::Middle);
                    p.line_segment([m + vec2(-3.5, -4.5), m + vec2(4.5, -4.5)], s);
                    p.line_segment([m + vec2(4.5, -4.5), m + vec2(4.5, 3.5)], s);
                }
                1 => {
                    p.rect_stroke(Rect::from_center_size(m, vec2(10.0, 10.0)), 1, s, egui::StrokeKind::Middle);
                }
                _ => {
                    p.line_segment([m + vec2(-5.0, -5.0), m + vec2(5.0, 5.0)], s);
                    p.line_segment([m + vec2(-5.0, 5.0), m + vec2(5.0, -5.0)], s);
                }
            }
            let tip = ["Riduci a icona", if maxed { tr("common.ripristina") } else { tr("app.ingrandisci") }, tr("app.chiudi_resta_nella_tray")][kind as usize];
            if resp.on_hover_text(tip).clicked() {
                ctx.send_viewport_cmd(match kind {
                    0 => egui::ViewportCommand::Minimized(true),
                    1 => egui::ViewportCommand::Maximized(!maxed),
                    _ => egui::ViewportCommand::Close,
                });
            }
        }
        let content = Rect::from_min_max(full.min, pos2(x, full.bottom()));
        let mut ui = ui.new_child(egui::UiBuilder::new().max_rect(content).layout(egui::Layout::left_to_right(egui::Align::Center)));
        let ui = &mut ui;
        ui.add_space(14.0);
        let (r, _) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::hover());
        ui::mark(ui.painter(), r.center(), 26.0);
        ui.add_space(4.0);
        ui.label(RichText::new(tray::TITLE).font(ui::bold(19.0)).color(col::HEAD));
        let g = ui.painter().layout_no_wrap(">PANEL_".into(), FontId::monospace(10.5), col::WHITE);
        let (r, _) = ui.allocate_exact_size(vec2(g.size().x + 14.0, 18.0), Sense::hover());
        ui.painter().rect_filled(r, 9, col::BRAND);
        ui.painter().galley(pos2(r.left() + 7.0, r.center().y - g.size().y / 2.0), g, col::WHITE);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(8.0);
            self.lang_button(ui);
            if !self.compact {
                ui.add_space(12.0);
                ui.label(RichText::new(tr("app.disclaimer")).size(12.5).color(col::MUTED));
            }
        });
    }

    /// Bandiera e codice della lingua; clic = elenco delle lingue con le bandiere.
    fn lang_button(&mut self, ui: &mut egui::Ui) {
        let cur = crate::i18n::current();
        let (code, _) = crate::i18n::LANGS[cur];
        let (r, resp) = ui.allocate_exact_size(vec2(68.0, 32.0), Sense::click());
        let p = ui.painter();
        if resp.hovered() || self.lang_open {
            p.rect_filled(r, 6, col::BG);
        }
        ui::flag(p, Rect::from_min_size(pos2(r.left() + 8.0, r.center().y - 7.0), vec2(21.0, 14.0)), code);
        p.text(pos2(r.left() + 36.0, r.center().y), Align2::LEFT_CENTER, code.to_uppercase(), ui::bold(13.0), col::HEAD);
        p.text(pos2(r.right() - 9.0, r.center().y), Align2::CENTER_CENTER, "▾", FontId::proportional(11.0), col::MUTED);
        if resp.on_hover_text(tr("app.lingua")).clicked() {
            self.lang_open = !self.lang_open;
        }
        if !self.lang_open {
            return;
        }
        let ctx = ui.ctx().clone();
        let area = egui::Area::new(Id::new("lang-menu")).order(egui::Order::Foreground).fixed_pos(pos2(r.right() - 190.0, r.bottom() + 4.0)).show(&ctx, |ui| {
            egui::Frame::new()
                .fill(col::SURFACE)
                .stroke(Stroke::new(1.0, col::LINE))
                .corner_radius(10)
                .inner_margin(6)
                .shadow(egui::Shadow { offset: [0, 4], blur: 16, spread: 0, color: egui::Color32::from_rgba_unmultiplied(16, 30, 53, 40) })
                .show(ui, |ui| {
                    for (i, (c, name)) in crate::i18n::LANGS.iter().enumerate() {
                        let (rr, rp) = ui.allocate_exact_size(vec2(178.0, 34.0), Sense::click());
                        let p = ui.painter();
                        if i == cur {
                            p.rect_filled(rr, 6, col::BRAND_SUBTLE);
                        } else if rp.hovered() {
                            p.rect_filled(rr, 6, col::BG);
                        }
                        ui::flag(p, Rect::from_min_size(pos2(rr.left() + 10.0, rr.center().y - 7.0), vec2(21.0, 14.0)), c);
                        p.text(pos2(rr.left() + 42.0, rr.center().y), Align2::LEFT_CENTER, *name, if i == cur { ui::bold(14.0) } else { FontId::proportional(14.0) }, col::HEAD);
                        if rp.clicked() {
                            crate::i18n::set(i);
                            self.lang_open = false;
                        }
                    }
                });
        });
        // clic fuori dal menu: si chiude
        let outside = ctx.input(|i| i.pointer.any_click() && i.pointer.interact_pos().is_some_and(|p| !area.response.rect.contains(p) && !r.contains(p)));
        if outside {
            self.lang_open = false;
        }
    }

    /// Bordi della finestra senza cornice: cursore giusto e ridimensionamento trascinando.
    fn resize_edges(&self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().maximized.unwrap_or(false)) {
            return;
        }
        let Some(p) = ctx.input(|i| i.pointer.hover_pos()) else { return };
        let r = ctx.content_rect();
        let e = 5.0;
        let (l, rt, t, b) = (p.x - r.left() < e, r.right() - p.x < e, p.y - r.top() < e, r.bottom() - p.y < e);
        use egui::{CursorIcon as C, ResizeDirection as D};
        let dir = match (l, rt, t, b) {
            (true, _, true, _) => Some((D::NorthWest, C::ResizeNorthWest)),
            (_, true, true, _) => Some((D::NorthEast, C::ResizeNorthEast)),
            (true, _, _, true) => Some((D::SouthWest, C::ResizeSouthWest)),
            (_, true, _, true) => Some((D::SouthEast, C::ResizeSouthEast)),
            (true, ..) => Some((D::West, C::ResizeWest)),
            (_, true, ..) => Some((D::East, C::ResizeEast)),
            (_, _, true, _) => Some((D::North, C::ResizeNorth)),
            (.., true) => Some((D::South, C::ResizeSouth)),
            _ => None,
        };
        if let Some((d, c)) = dir {
            ctx.set_cursor_icon(c);
            if ctx.input(|i| i.pointer.primary_pressed()) {
                ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(d));
            }
        }
    }

    /// Barra laterale: scheda bianca staccata dal bordo, voce attiva su fondo azzurro.
    fn rail(&mut self, ui: &mut egui::Ui) {
        let compact = self.compact;
        let w = ui.available_width();
        ui.add_space(10.0);
        let (machines, _) = self.machines();
        let mut items: Vec<(View, &str, String, Option<egui::Color32>)> = vec![(View::Overview, "⌂", tr("common.dashboard").into(), None)];
        let heads_at = items.len();
        for (k, s) in &machines {
            let (c, _) = self.machine_state(*k, s);
            let short = self.display_name(*k, s);
            items.push((View::Machine(*k, s.clone()), if *k == Kind::Vps { "☁" } else { "🖥" }, short, Some(c)));
        }
        let tail_at = items.len();
        items.push((View::Network, "🌐", tr("common.rete_e_ip").into(), None));
        items.push((View::Account, "💳", tr("common.account").into(), None));

        let mut clicked = None;
        for (i, (v, icon, label, st)) in items.iter().enumerate() {
            if i == heads_at && !machines.is_empty() {
                section(ui, compact, tr("app.server"));
            }
            if i == tail_at {
                section(ui, compact, tr("app.gestione"));
            }
            let (r, resp) = ui.allocate_exact_size(vec2(w, 38.0), Sense::click());
            let r = r.shrink2(vec2(8.0, 1.0));
            let on = *v == self.view;
            let p = ui.painter();
            if on {
                p.rect_filled(r, 8, col::BRAND_SUBTLE);
            } else if resp.hovered() {
                p.rect_filled(r, 8, col::BG);
            }
            let (c, font) = if on { (col::ACTIVE_INK, ui::bold(14.0)) } else { (col::RAIL_TEXT, FontId::proportional(14.0)) };
            let ix = if compact { r.center().x } else { r.left() + 18.0 };
            p.text(pos2(ix, r.center().y), Align2::CENTER_CENTER, *icon, FontId::proportional(15.0), c);
            if !compact {
                p.text(pos2(r.left() + 38.0, r.center().y), Align2::LEFT_CENTER, label, font, c);
            }
            if let Some(dc) = st {
                let dp = if compact { pos2(r.right() - 9.0, r.top() + 9.0) } else { pos2(r.right() - 14.0, r.center().y) };
                p.circle_filled(dp, 4.0, *dc);
            }
            if resp.clicked() {
                clicked = Some(v.clone());
            }
            if compact {
                resp.on_hover_text(label.as_str());
            }
        }
        if let Some(v) = clicked {
            self.go(v);
        }
        self.user_block(ui);
    }

    /// In fondo alla barra laterale: con quale account sei collegato. Clic = menu con "Scollega".
    fn user_block(&mut self, ui: &mut egui::Ui) {
        let me = self.data("me", 3600.0, |c| c.get("/me"));
        let (name, nic) = match &me {
            Some(Ok(v)) => (
                format!("{} {}", ui::sv(&v["firstname"]), ui::sv(&v["name"])).trim().to_string(),
                ui::sv(&v["nichandle"]),
            ),
            _ => (tr("app.account_ovhcloud").to_string(), String::new()),
        };
        let initials: String = if nic.is_empty() { String::new() } else { name.split_whitespace().filter_map(|w| w.chars().next()).take(2).collect::<String>().to_uppercase() };
        let compact = self.compact;
        ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
            ui.add_space(6.0);
            let w = ui.available_width();
            let (r, resp) = ui.allocate_exact_size(vec2(w, 52.0), Sense::click());
            let r = r.shrink2(vec2(8.0, 2.0));
            let p = ui.painter();
            if resp.hovered() || self.user_open {
                p.rect_filled(r, 8, col::BG);
            }
            let c = if compact { r.center() } else { pos2(r.left() + 24.0, r.center().y) };
            p.circle_filled(c, 16.0, egui::Color32::from_rgb(0xE6, 0xE6, 0xE6));
            p.text(c, Align2::CENTER_CENTER, if initials.is_empty() { "?" } else { &initials }, ui::bold(12.0), col::HEAD);
            if !compact {
                let x = r.left() + 50.0;
                p.text(pos2(x, r.center().y - (if nic.is_empty() { 0.0 } else { 8.0 })), Align2::LEFT_CENTER, &name, ui::bold(14.0), col::HEAD);
                if !nic.is_empty() {
                    p.text(pos2(x, r.center().y + 9.0), Align2::LEFT_CENTER, &nic, FontId::proportional(12.0), col::MUTED);
                }
                p.text(pos2(r.right() - 12.0, r.center().y), Align2::CENTER_CENTER, if self.user_open { "▾" } else { "▴" }, FontId::proportional(12.0), col::MUTED);
            }
            let resp = if compact { resp.on_hover_text(format!("{name} {nic}")) } else { resp };
            if resp.clicked() {
                self.user_open = !self.user_open;
            }
            p.line_segment([pos2(r.left(), r.top() - 4.0), pos2(r.right(), r.top() - 4.0)], Stroke::new(1.0, col::LINE));
            if self.user_open {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.add_space(14.0);
                    if ui.add(ui::ghost(if compact { "⏏" } else { tr("app.scollega_account") })).on_hover_text(tr("app.cancella_le_chiavi_da_questo")).clicked() {
                        self.user_open = false;
                        self.ask(tr("app.scollegare_l_account"), tr("app.le_chiavi_vengono_cancellate_dal"), tr("app.scollega"), Action::Forget);
                    }
                });
                if !compact && matches!(me, Some(Err(_))) {
                    ui.horizontal(|ui| {
                        ui.add_space(14.0);
                        if ui.link(RichText::new(tr("app.mostra_nome")).size(12.0)).clicked() {
                            open_browser(TOKEN_URL);
                        }
                    });
                }
            }
        });
    }

    /// Controllo segmentato della console: fondo grigio, scheda attiva bianca con bordo.
    pub fn tabs(&mut self, ui: &mut egui::Ui, names: &[&str]) {
        let mut pick = None;
        egui::ScrollArea::horizontal().id_salt("tabs").show(ui, |ui| {
            egui::Frame::new().fill(col::SURFACE2).corner_radius(8).inner_margin(2).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    for (i, n) in names.iter().enumerate() {
                        let on = i == self.tab;
                        let font = if on { ui::bold(14.0) } else { FontId::proportional(14.0) };
                        let galley = ui.painter().layout_no_wrap(n.to_string(), font.clone(), col::TEXT);
                        let (r, resp) = ui.allocate_exact_size(vec2(galley.size().x + 32.0, 34.0), Sense::click());
                        let p = ui.painter();
                        if on {
                            p.rect(r, 6, col::SURFACE, Stroke::new(1.0, col::LINE), egui::StrokeKind::Inside);
                        } else if resp.hovered() {
                            p.rect_filled(r, 6, egui::Color32::from_white_alpha(140));
                        }
                        p.text(r.center(), Align2::CENTER_CENTER, *n, font, if on { col::HEAD } else { col::RAIL_TEXT });
                        if resp.clicked() {
                            pick = Some(i);
                        }
                    }
                });
            });
        });
        if let Some(i) = pick {
            self.set_tab(i);
        }
        ui.add_space(14.0);
    }

    fn page(&mut self, ui: &mut egui::Ui) {
        match self.view.clone() {
            View::Overview => self.overview_ui(ui),
            View::Machine(Kind::Vps, s) => self.vps_ui(ui, &s),
            View::Machine(Kind::Dedi, s) => self.dedi_ui(ui, &s),
            View::Network => self.net_ui(ui),
            View::Account => self.account_ui(ui),
        }
    }

    fn confirm_ui(&mut self, ctx: &egui::Context) {
        let Some(cf) = &mut self.confirm else { return };
        let (mut close, mut go) = (false, false);
        let resp = egui::Modal::new(Id::new("confirm")).show(ctx, |ui| {
            ui.set_width(420.0);
            let last = cf.step + 1 >= cf.steps;
            ui.label(h(if cf.step > 0 { tr("app.ultima_conferma") } else { cf.title.as_str() }, 20.0));
            ui.add_space(6.0);
            ui.label(&cf.body);
            let need = cf.typed.as_deref().filter(|_| last);
            if let Some(t) = need {
                ui.add_space(10.0);
                ui.label(RichText::new(fill(tr("app.scrivi_per_confermare"), &[("t", t.to_string())])).color(col::WARN));
                ui.add(egui::TextEdit::singleline(&mut cf.input).desired_width(f32::INFINITY));
            }
            ui.add_space(16.0);
            let ok = need.is_none_or(|t| cf.input.trim() == t);
            ui.horizontal(|ui| {
                if ui.button(tr("app.annulla")).clicked() {
                    close = true;
                }
                let label = if last { cf.yes.clone() } else { tr("app.continua").into() };
                if ui.add_enabled(ok, ui::danger(&label)).clicked() {
                    if last {
                        go = true;
                    } else {
                        cf.step += 1;
                    }
                }
            });
        });
        if go {
            let a = cf.action.clone();
            self.confirm = None;
            self.run(a);
        } else if close || resp.should_close() {
            self.confirm = None;
        }
    }

    fn login_ui(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().frame(egui::Frame::new().fill(col::BG)).show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space((ui.available_height() * 0.5 - 270.0).max(16.0));
                ui.set_max_width(460.0);
                let (r, _) = ui.allocate_exact_size(vec2(56.0, 56.0), Sense::hover());
                ui::mark(ui.painter(), r.center(), 52.0);
                ui.add_space(14.0);
                ui.label(h(tr("app.collega_l_account_ovhcloud"), 26.0));
                ui.add_space(4.0);
                ui.label(muted(tr("app.tre_chiavi_api_restano_nel")));
                ui.label(RichText::new(tr("app.login_disclaimer")).size(12.5).color(col::MUTED));
                ui.add_space(18.0);
                ui::card(ui, false, |ui| {
                    let busy = self.connecting;
                    ui.add_enabled_ui(!busy, |ui| {
                        for (label, v, secret) in [
                            (tr("app.application_key"), &mut self.form.app_key, false),
                            (tr("app.application_secret"), &mut self.form.app_secret, true),
                            (tr("app.consumer_key"), &mut self.form.consumer_key, true),
                        ] {
                            ui.label(muted(label));
                            ui.add(egui::TextEdit::singleline(v).password(secret).desired_width(f32::INFINITY));
                            ui.add_space(6.0);
                        }
                    });
                    ui.add_space(8.0);
                    let f = &self.form;
                    let filled = [&f.app_key, &f.app_secret, &f.consumer_key].iter().all(|s| !s.trim().is_empty());
                    let label = if busy { tr("app.collego") } else { tr("app.salva_e_collega") };
                    if ui.add_enabled(filled && !busy, ui::primary(label).min_size(vec2(ui.available_width(), 36.0))).clicked() {
                        let c = Creds { app_key: f.app_key.trim().into(), app_secret: f.app_secret.trim().into(), consumer_key: f.consumer_key.trim().into() };
                        self.connect(c, true);
                    }
                    if let Some(e) = &self.login_err {
                        ui.add_space(8.0);
                        ui.label(RichText::new(e).color(col::CRIT));
                    }
                });
                ui.add_space(14.0);
                if ui.link(tr("app.non_hai_le_chiavi_creale")).clicked() {
                    open_browser(TOKEN_URL);
                }
            });
        });
    }
}

fn section(ui: &mut egui::Ui, compact: bool, t: &str) {
    ui.add_space(10.0);
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 18.0), Sense::hover());
    if compact {
        ui.painter().line_segment([pos2(r.center().x - 12.0, r.center().y), pos2(r.center().x + 12.0, r.center().y)], Stroke::new(1.0, col::LINE));
    } else {
        ui.painter().text(pos2(r.left() + 22.0, r.center().y), Align2::LEFT_CENTER, t, ui::bold(11.5), col::MUTED);
    }
    ui.add_space(4.0);
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // al massimo 60 fps: dorme il tempo che manca invece di aspettare il vsync girando a vuoto
        let min = std::time::Duration::from_micros(16_600);
        if let Some(wait) = min.checked_sub(self.last_frame.elapsed()) {
            std::thread::sleep(wait);
        }
        self.last_frame = std::time::Instant::now();
        self.now = ctx.input(|i| i.time);
        if !self.hwnd_set {
            use raw_window_handle::{HasWindowHandle, RawWindowHandle};
            if let Ok(h) = frame.window_handle() {
                if let RawWindowHandle::Win32(w) = h.as_raw() {
                    tray::set_main(w.hwnd.get());
                    self.hwnd_set = true;
                }
            }
        }
        // la X chiude nella tray: i controlli continuano e le notifiche arrivano
        if ctx.input(|i| i.viewport().close_requested()) && tray::ready() && self.client.is_some() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            tray::hide_main();
            if !self.hid_once {
                self.hid_once = true;
                tray::notify(tray::TITLE, tr("app.resto_nella_tray_e_ti"), false);
            }
        }
        while let Ok(m) = self.rx.try_recv() {
            self.handle(m);
        }
        let width = ctx.content_rect().width();
        self.compact = width < 980.0;
        egui::TopBottomPanel::top("header")
            .exact_height(48.0)
            .show_separator_line(false)
            .frame(egui::Frame::new().fill(col::SURFACE))
            .show(ctx, |ui| {
                let r = ui.max_rect();
                ui.painter().line_segment([r.left_bottom(), r.right_bottom()], Stroke::new(1.0, col::LINE));
                self.header(ui);
            });
        self.resize_edges(ctx);
        if self.client.is_none() && !self.demo {
            return self.login_ui(ctx);
        }
        egui::SidePanel::left("rail")
            .exact_width(if self.compact { 72.0 } else { 280.0 })
            .resizable(false)
            .show_separator_line(false)
            .frame(egui::Frame::new().fill(col::BG).inner_margin(egui::Margin { left: 8, right: 0, top: 8, bottom: 8 }))
            .show(ctx, |ui| {
                egui::Frame::new().fill(col::RAIL).corner_radius(14).inner_margin(egui::Margin::symmetric(0, 6)).show(ui, |ui| {
                    ui.set_min_height(ui.available_height());
                    ui.set_width(ui.available_width());
                    self.rail(ui);
                });
            });

        let pad = if width < 700.0 { 8 } else { 10 };
        egui::CentralPanel::default().frame(egui::Frame::new().fill(col::BG).inner_margin(egui::Margin { left: pad, right: pad, top: 8, bottom: 8 })).show(ctx, |ui| {
            self.page(ui);
        });

        // avviso in basso a destra, sparisce da solo dopo 7 secondi
        if let Some((t, err, t0)) = self.note.clone() {
            let left = 7.0 - (self.now - t0);
            if left <= 0.0 {
                self.note = None;
            } else {
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(left));
                egui::Area::new(Id::new("toast")).anchor(Align2::RIGHT_BOTTOM, vec2(-20.0, -20.0)).show(ctx, |ui| {
                    egui::Frame::new()
                        .fill(if err { egui::Color32::from_rgb(0xFF, 0xE6, 0xEC) } else { col::BRAND_SUBTLE })
                        .stroke(Stroke::new(1.0, if err { col::CRIT } else { col::BRAND }))
                        .corner_radius(4)
                        .inner_margin(egui::Margin::symmetric(16, 10))
                        .shadow(egui::Shadow { offset: [0, 2], blur: 8, spread: 0, color: egui::Color32::from_black_alpha(30) })
                        .show(ui, |ui| {
                            ui.set_max_width(420.0);
                            ui.label(RichText::new(t).color(if err { col::CRIT } else { col::BRAND_DARK }));
                        });
                });
            }
        }
        self.confirm_ui(ctx);
    }
}
