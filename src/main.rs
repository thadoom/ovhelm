#![windows_subsystem = "windows"]
//! Pannello desktop per VPS e server dedicati OVHcloud. Solo API, niente SSH.
mod account;
mod app;
mod dedi;
mod i18n;
mod net;
mod overview;
mod ovh;
mod tray;
mod ui;
mod vps;
mod watch;

use eframe::egui;

fn main() -> eframe::Result {
    if !tray::single_instance() {
        return Ok(());
    }
    i18n::init();
    tray::start();
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(tray::TITLE)
        .with_inner_size([1240.0, 820.0])
        .with_min_inner_size([640.0, 520.0])
        .with_decorations(false) // barra del titolo disegnata dall'app, nello stile della console
        .with_icon(egui::IconData { rgba: tray::icon_rgba(64), width: 64, height: 64 });
    // `--pos X,Y`: apre la finestra lì senza rubare il focus (prove su un secondo monitor)
    let args: Vec<String> = std::env::args().collect();
    if let Some((x, y)) = args.iter().position(|a| a == "--pos").and_then(|i| args.get(i + 1)?.split_once(',')) {
        if let (Ok(x), Ok(y)) = (x.trim().parse::<f32>(), y.trim().parse::<f32>()) {
            viewport = viewport.with_position([x, y]).with_active(false);
        }
    }
    // vsync spento: la UI ridisegna solo su eventi, e con il vsync acceso alcuni driver aspettano
    // il monitor girando a vuoto (misurato: 54% di un core a riposo con il vecchio pallino pulsante)
    let opts = eframe::NativeOptions { viewport, vsync: false, ..Default::default() };
    eframe::run_native(tray::TITLE, opts, Box::new(|cc| Ok(Box::new(app::App::new(cc)))))
}

#[cfg(test)]
mod live {
    /// VPS vera con le chiavi salvate dall'app: stampa gli ultimi valori delle metriche. Solo GET.
    /// `cargo test -- --ignored live_vps --nocapture`
    #[test]
    #[ignore]
    fn live_vps() {
        let c = crate::ovh::Client::connect(crate::app::load_creds().expect("nessuna chiave salvata")).unwrap();
        let list: Vec<String> = serde_json::from_value(c.get("/vps").unwrap()).unwrap();
        let m = crate::vps::fetch_metrics(&c, &list[0], "1h").unwrap();
        for r in m["rows"].as_array().unwrap() {
            println!("{}: ultimo={:?} max={} età={}s err={}", r["label"], crate::vps::last(r), r["max"], r["age"], r["err"]);
        }
        println!("dedicati: {:?}", c.get("/dedicated/server").map(|v| v.to_string()));
        println!("ip: {:?}", c.get("/ip").map(|v| v.to_string()));
        println!("service: {:?}", c.get("/service").map(|v| v.to_string().len()));
    }
}
