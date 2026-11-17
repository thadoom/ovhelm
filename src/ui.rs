//! Tema e widget nello stile della console beta di OVHcloud (manager.eu.ovhcloud.com/beta):
//! pagina grigio-azzurra, card bianche con bordo sottile e raggio 14, Source Sans 3, blu #0050D7.
//! Niente animazioni: la UI ridisegna solo quando cambia qualcosa.
use crate::i18n::{fill, tr};
use eframe::egui::{self, pos2, vec2, Color32, CornerRadius, FontId, Pos2, Rect, Response, RichText, Sense, Stroke, Ui};

pub mod col {
    use eframe::egui::Color32;
    const fn hex(c: u32) -> Color32 {
        Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8)
    }
    pub const BG: Color32 = hex(0xF4F5F8); // sfondo pagina
    pub const RAIL: Color32 = Color32::WHITE; // barra laterale (scheda bianca staccata dal bordo)
    pub const RAIL_TEXT: Color32 = hex(0x374F76); // voci di menu
    pub const SURFACE: Color32 = Color32::WHITE;
    pub const SURFACE2: Color32 = hex(0xEDF0F5); // fondo dei controlli segmentati
    pub const LINE: Color32 = hex(0xE6E9F0); // bordi di card, header, barra
    pub const TEXT: Color32 = hex(0x101E35); // etichette e testo forte
    pub const HEAD: Color32 = hex(0x101E35); // titoli
    pub const MUTED: Color32 = hex(0x4D5592); // valori e testo secondario
    pub const OFF: Color32 = hex(0xB3B3B3);
    pub const BRAND: Color32 = hex(0x0050D7);
    pub const BRAND_DARK: Color32 = hex(0x000E9C);
    pub const BRAND_SUBTLE: Color32 = hex(0xE1EBF9); // voce attiva
    pub const ACTIVE_INK: Color32 = hex(0x1A2D4B);
    pub const SKY: Color32 = hex(0x4BB2F6);
    pub const OK: Color32 = hex(0x2B8000);
    pub const WARN: Color32 = hex(0xFFA322);
    pub const CRIT: Color32 = hex(0xBF0020);
    pub const WHITE: Color32 = Color32::WHITE;
}

pub fn theme(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    // Source Sans 3 (OFL, assets/fonts) come la console; Segoe e Segoe UI Symbol dietro per frecce e simboli
    fonts.font_data.insert("ss3".into(), egui::FontData::from_static(include_bytes!("../assets/fonts/SourceSans3-Regular.ttf")).into());
    fonts.font_data.insert("ss3-semi".into(), egui::FontData::from_static(include_bytes!("../assets/fonts/SourceSans3-SemiBold.ttf")).into());
    for (name, file) in [("segoe", "segoeui.ttf"), ("symbols", "seguisym.ttf")] {
        if let Ok(b) = std::fs::read(format!(r"C:\Windows\Fonts\{file}")) {
            fonts.font_data.insert(name.into(), egui::FontData::from_owned(b).into());
        }
    }
    let mut fallback: Vec<String> = ["segoe", "symbols"].iter().filter(|n| fonts.font_data.contains_key(**n)).map(|n| n.to_string()).collect();
    fallback.extend(fonts.families[&egui::FontFamily::Proportional].iter().cloned());
    let mut prop = vec!["ss3".to_string()];
    prop.extend(fallback.iter().cloned());
    let mut semi = vec!["ss3-semi".to_string()];
    semi.extend(fallback);
    fonts.families.insert(egui::FontFamily::Proportional, prop);
    fonts.families.insert(egui::FontFamily::Name("bold".into()), semi);
    ctx.set_fonts(fonts);

    let mut v = egui::Visuals::light();
    v.panel_fill = col::BG;
    v.window_fill = col::SURFACE;
    v.extreme_bg_color = col::SURFACE; // campi di testo
    v.faint_bg_color = col::BG; // righe alterne delle griglie
    v.code_bg_color = col::SURFACE2;
    v.hyperlink_color = col::BRAND;
    v.selection.bg_fill = col::BRAND_SUBTLE;
    v.selection.stroke = Stroke::new(1.0, col::BRAND);
    v.window_corner_radius = CornerRadius::same(14);
    v.window_stroke = Stroke::new(1.0, col::LINE);
    v.window_shadow = egui::Shadow { offset: [0, 8], blur: 24, spread: 0, color: Color32::from_rgba_unmultiplied(16, 30, 53, 40) };
    v.popup_shadow = egui::Shadow { offset: [0, 4], blur: 12, spread: 0, color: Color32::from_rgba_unmultiplied(16, 30, 53, 30) };
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, col::TEXT);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, col::LINE);
    // bottone secondario: bianco, bordo e testo blu
    v.widgets.inactive.weak_bg_fill = col::SURFACE;
    v.widgets.inactive.bg_fill = col::SURFACE;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, col::BRAND);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, col::BRAND);
    v.widgets.hovered.weak_bg_fill = col::BRAND_SUBTLE;
    v.widgets.hovered.bg_fill = col::BRAND_SUBTLE;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, col::BRAND_DARK);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, col::BRAND_DARK);
    v.widgets.active.weak_bg_fill = col::BRAND_SUBTLE;
    v.widgets.active.bg_stroke = Stroke::new(1.0, col::HEAD);
    v.widgets.active.fg_stroke = Stroke::new(1.0, col::HEAD);
    v.widgets.open.weak_bg_fill = col::BRAND_SUBTLE;
    for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.noninteractive, &mut v.widgets.open] {
        w.corner_radius = CornerRadius::same(6);
        w.expansion = 0.0;
    }
    ctx.set_visuals(v);
    ctx.style_mut(|s| {
        s.animation_time = 0.0; // niente transizioni di egui (combo, collassabili)
        s.spacing.button_padding = vec2(16.0, 6.0);
        s.spacing.item_spacing = vec2(10.0, 8.0);
        s.spacing.interact_size.y = 34.0;
        for (ts, size) in [(egui::TextStyle::Body, 14.5), (egui::TextStyle::Button, 14.5), (egui::TextStyle::Heading, 32.0), (egui::TextStyle::Small, 12.5)] {
            if let Some(f) = s.text_styles.get_mut(&ts) {
                f.size = size;
            }
        }
    });
}

pub fn bold(size: f32) -> FontId {
    FontId::new(size, egui::FontFamily::Name("bold".into()))
}
/// Titolo (semibold, blu notte).
pub fn h(text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text).font(bold(size)).color(col::HEAD)
}
pub fn muted(text: impl Into<String>) -> RichText {
    RichText::new(text).color(col::MUTED)
}

/// Bottone principale: pieno blu, testo bianco.
pub fn primary(t: &str) -> egui::Button<'static> {
    egui::Button::new(RichText::new(t.to_string()).color(col::WHITE).font(bold(14.5))).fill(col::BRAND).stroke(Stroke::new(1.0, col::BRAND))
}
pub fn danger(t: &str) -> egui::Button<'static> {
    egui::Button::new(RichText::new(t.to_string()).color(col::CRIT).font(bold(14.5))).fill(col::SURFACE).stroke(Stroke::new(1.0, col::CRIT))
}
pub fn ghost(t: &str) -> egui::Button<'static> {
    egui::Button::new(RichText::new(t.to_string()).color(col::BRAND).font(bold(14.5))).frame(false)
}
/// Link blu con freccia, come "Reinstalla il tuo VPS →".
pub fn link_arrow(ui: &mut Ui, t: &str) -> Response {
    let r = ui.add(egui::Label::new(RichText::new(format!("{t}  →")).color(col::BRAND).font(bold(14.0))).sense(Sense::click()));
    if r.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    r
}

/// Badge a pillola della console: fondo tenue, bordo, testo scuro; i badge di stato hanno il pallino.
pub fn chip(ui: &mut Ui, c: Color32, t: &str) -> Response {
    let rgb = |c: u32| Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8);
    let (bg, border, ink, dot) = match c {
        c if c == col::OK => (rgb(0xEEFAF6), rgb(0x86D4BE), rgb(0x065F46), true),
        c if c == col::WARN => (rgb(0xFFF7CC), rgb(0xFED265), rgb(0x995400), true),
        c if c == col::CRIT => (rgb(0xFFE6EC), rgb(0xEF99AB), rgb(0x800015), true),
        c if c == col::BRAND || c == col::SKY => (col::BRAND_SUBTLE, rgb(0x85D9FD), col::BRAND_DARK, false),
        _ => (rgb(0xE6E6E6), rgb(0xB3B3B3), rgb(0x333333), false),
    };
    // disegnato a mano: un Frame dentro una riga prenderebbe tutta l'altezza della riga
    let g = ui.painter().layout_no_wrap(t.to_string(), bold(13.5), ink);
    let dw = if dot { 13.0 } else { 0.0 };
    let (r, resp) = ui.allocate_exact_size(vec2(g.size().x + 20.0 + dw, 24.0), Sense::hover());
    let p = ui.painter();
    p.rect(r, 12, bg, Stroke::new(1.0, border), egui::StrokeKind::Inside);
    let mut x = r.left() + 10.0;
    if dot {
        p.circle_filled(pos2(x + 3.5, r.center().y), 3.5, ink);
        x += dw;
    }
    p.galley(pos2(x, r.center().y - g.size().y / 2.0), g, ink);
    resp
}

/// Bandiera disegnata a mano nel rettangolo `r` (le emoji bandiera su Windows sono due lettere).
pub fn flag(p: &egui::Painter, r: Rect, code: &str) {
    let p = p.with_clip_rect(r);
    let rgb = |c: u32| Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8);
    let vert = |cols: &[(f32, u32)]| {
        let mut x = r.left();
        for &(w, c) in cols {
            let nx = x + r.width() * w;
            p.rect_filled(Rect::from_min_max(pos2(x, r.top()), pos2(nx, r.bottom())), 0, rgb(c));
            x = nx;
        }
    };
    let horiz = |rows: &[(f32, u32)]| {
        let mut y = r.top();
        for &(h, c) in rows {
            let ny = y + r.height() * h;
            p.rect_filled(Rect::from_min_max(pos2(r.left(), y), pos2(r.right(), ny)), 0, rgb(c));
            y = ny;
        }
    };
    let third = 1.0 / 3.0;
    match code {
        "it" => vert(&[(third, 0x009246), (third, 0xFFFFFF), (third, 0xCE2B37)]),
        "fr" => vert(&[(third, 0x0055A4), (third, 0xFFFFFF), (third, 0xEF4135)]),
        "de" => horiz(&[(third, 0x000000), (third, 0xDD0000), (third, 0xFFCE00)]),
        "es" => horiz(&[(0.25, 0xAA151B), (0.5, 0xF1BF00), (0.25, 0xAA151B)]),
        "pl" => horiz(&[(0.5, 0xFFFFFF), (0.5, 0xDC143C)]),
        "pt" => {
            vert(&[(0.4, 0x006600), (0.6, 0xFF0000)]);
            p.circle_filled(pos2(r.left() + r.width() * 0.4, r.center().y), r.height() * 0.26, rgb(0xFFCC29));
        }
        _ => {
            // Union Jack semplificata
            p.rect_filled(r, 0, rgb(0x012169));
            let (a, b, c, d) = (r.left_top(), r.right_bottom(), r.right_top(), r.left_bottom());
            let w = r.height() * 0.2;
            p.line_segment([a, b], Stroke::new(w, Color32::WHITE));
            p.line_segment([c, d], Stroke::new(w, Color32::WHITE));
            p.line_segment([a, b], Stroke::new(w * 0.4, rgb(0xC8102E)));
            p.line_segment([c, d], Stroke::new(w * 0.4, rgb(0xC8102E)));
            let m = r.center();
            p.rect_filled(Rect::from_center_size(m, vec2(r.width(), r.height() * 0.34)), 0, Color32::WHITE);
            p.rect_filled(Rect::from_center_size(m, vec2(r.height() * 0.34, r.height())), 0, Color32::WHITE);
            p.rect_filled(Rect::from_center_size(m, vec2(r.width(), r.height() * 0.2)), 0, rgb(0xC8102E));
            p.rect_filled(Rect::from_center_size(m, vec2(r.height() * 0.2, r.height())), 0, rgb(0xC8102E));
        }
    }
    p.rect_stroke(r, 2, Stroke::new(1.0, Color32::from_black_alpha(40)), egui::StrokeKind::Inside);
}

/// Pallino di stato.
pub fn dot(ui: &mut Ui, c: Color32) {
    let (r, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
    ui.painter().circle_filled(r.center(), 4.5, c);
}

/// Marchio dell'app: quadrato blu arrotondato con il prompt ">_" bianco.
pub fn mark(p: &egui::Painter, c: Pos2, size: f32) {
    let r = Rect::from_center_size(c, vec2(size, size));
    p.rect_filled(r, size * 0.22, col::BRAND);
    let s = Stroke::new(size * 0.1, col::WHITE);
    let o = r.min;
    p.add(egui::Shape::line(vec![o + vec2(size * 0.24, size * 0.30), o + vec2(size * 0.44, size * 0.50), o + vec2(size * 0.24, size * 0.70)], s));
    p.line_segment([o + vec2(size * 0.52, size * 0.70), o + vec2(size * 0.78, size * 0.70)], s);
}

/// Pannello della console: bianco, bordo sottile, raggio 14. `hover`: cliccabile, bordo blu sotto il mouse.
pub fn card<R>(ui: &mut Ui, hover: bool, add: impl FnOnce(&mut Ui) -> R) -> (Response, R) {
    let out = egui::Frame::new()
        .fill(col::SURFACE)
        .stroke(Stroke::new(1.0, col::LINE))
        .corner_radius(14)
        .inner_margin(egui::Margin { left: 22, right: 22, top: 18, bottom: 20 })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        });
    let resp = if hover { out.response.interact(Sense::click()) } else { out.response };
    if hover && resp.hovered() {
        ui.painter().rect_stroke(resp.rect, 14, Stroke::new(1.0, col::BRAND), egui::StrokeKind::Inside);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    (resp, out.inner)
}

/// Etichette e valori uno sotto l'altro, come "Distribuzione / Debian 13" nella console.
pub fn kv(ui: &mut Ui, _id: &str, rows: &[(&str, String)]) {
    for (k, v) in rows {
        field(ui, k, v);
    }
}
pub fn field(ui: &mut Ui, label: &str, value: &str) {
    ui.label(RichText::new(label).font(bold(14.0)).color(col::TEXT));
    ui.add_space(-4.0);
    ui.add(egui::Label::new(RichText::new(value).color(col::MUTED).size(14.0)).selectable(true).wrap());
    ui.add_space(6.0);
}
/// Campo in sola lettura con il bottone copia (IPv4, IPv6 della console). Ritorna true se ha copiato.
pub fn copy_field(ui: &mut Ui, label: &str, value: &str) -> bool {
    ui.label(RichText::new(label).font(bold(14.0)).color(col::TEXT));
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 38.0), Sense::hover());
    let p = ui.painter();
    p.rect(r, 8, col::SURFACE, Stroke::new(1.0, egui::Color32::from_rgb(0xCC, 0xD3, 0xE0)), egui::StrokeKind::Inside);
    p.text(pos2(r.left() + 12.0, r.center().y), egui::Align2::LEFT_CENTER, value, FontId::proportional(14.0), col::MUTED);
    let b = Rect::from_center_size(pos2(r.right() - 20.0, r.center().y), vec2(28.0, 28.0));
    let resp = ui.interact(b, ui.id().with(("copy", label)), Sense::click()).on_hover_text(tr("ui.copia"));
    let c = if resp.hovered() { col::BRAND } else { col::RAIL_TEXT };
    p.rect_stroke(Rect::from_center_size(b.center() + vec2(2.0, 2.0), vec2(10.0, 12.0)), 2, Stroke::new(1.4, c), egui::StrokeKind::Middle);
    p.rect(Rect::from_center_size(b.center() - vec2(2.0, 2.0), vec2(10.0, 12.0)), 2, col::SURFACE, Stroke::new(1.4, c), egui::StrokeKind::Middle);
    ui.add_space(6.0);
    if resp.clicked() {
        ui.ctx().copy_text(value.to_string());
        return true;
    }
    false
}

/// Intestazione di card: titolo a sinistra, link "Aiuto"-style a destra (ritorna il click).
pub fn card_head(ui: &mut Ui, title: &str, link: Option<&str>) -> bool {
    let mut clicked = false;
    ui.horizontal(|ui| {
        ui.label(h(title, 18.0));
        if let Some(l) = link {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let r = ui.add(egui::Label::new(RichText::new(l).color(col::BRAND).font(bold(14.0))).sense(Sense::click()));
                clicked = r.clicked();
            });
        }
    });
    ui.add_space(8.0);
    clicked
}

/// Banner in cima alla pagina: briciole, titolo grande, badge, sottotitolo, onde blu a destra.
/// Ritorna true se si preme "Aggiorna".
pub fn hero(ui: &mut Ui, crumbs: &[&str], title: &str, badge: Option<(Color32, &str)>, sub: &str) -> bool {
    hero_with(ui, crumbs, title, badge, sub, "").0
}

/// Contorno di un rettangolo arrotondato (senso orario), per riempimenti sfumati e maschere.
fn rounded_outline(r: Rect, rad: f32) -> Vec<Pos2> {
    let mut v = Vec::new();
    let corners = [(r.right() - rad, r.top() + rad, -90.0f32), (r.right() - rad, r.bottom() - rad, 0.0), (r.left() + rad, r.bottom() - rad, 90.0), (r.left() + rad, r.top() + rad, 180.0)];
    for (cx, cy, a0) in corners {
        for k in 0..=6 {
            let a = (a0 + 90.0 * k as f32 / 6.0).to_radians();
            v.push(pos2(cx + rad * a.cos(), cy + rad * a.sin()));
        }
    }
    v
}

/// Come `hero`, con un secondo bottone accanto ad "Aggiorna" (vuoto = niente). Ritorna (aggiorna, extra).
pub fn hero_with(ui: &mut Ui, crumbs: &[&str], title: &str, badge: Option<(Color32, &str)>, sub: &str, extra: &str) -> (bool, bool) {
    let w = ui.available_width();
    let (r, _) = ui.allocate_exact_size(vec2(w, if sub.is_empty() { 96.0 } else { 116.0 }), Sense::hover());
    let p = ui.painter_at(r);
    let rad = 14.0;
    // fondo sfumato già arrotondato: ventaglio dal centro con il colore che dipende dalla x
    let shade = |q: Pos2| -> Color32 {
        let t = ((q.x - r.left()) / r.width()).clamp(0.0, 1.0);
        let lerp = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t * t) as u8;
        Color32::from_rgb(lerp(0xFF, 0xE4), lerp(0xFF, 0xEE), lerp(0xFF, 0xFD))
    };
    let outline = rounded_outline(r, rad);
    let mut m = egui::Mesh::default();
    m.colored_vertex(r.center(), shade(r.center()));
    for q in &outline {
        m.colored_vertex(*q, shade(*q));
    }
    let n = outline.len() as u32;
    for i in 0..n {
        m.add_triangle(0, 1 + i, 1 + (i + 1) % n);
    }
    p.add(egui::Shape::mesh(m));
    // onde: fasci di linee sottili che si incrociano e scorrono piano, come nel banner della console.
    // Si muovono solo con la finestra in primo piano e con le animazioni di Windows accese, a 30 fps.
    let live = ui.input(|i| i.focused) && crate::tray::animations_on();
    let phase = if live {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));
        ui.input(|i| i.time) as f32 * 0.35
    } else {
        0.0
    };
    // come nella console le onde passano anche sotto il titolo: sfumano verso sinistra per lasciarlo leggibile
    let (x0, n) = (r.left(), 64usize);
    for band in 0..2 {
        for i in 0..22 {
            let k = i as f32 / 21.0;
            let pts: Vec<Pos2> = (0..=n)
                .map(|j| {
                    let t = j as f32 / n as f32;
                    let x = x0 + (r.right() - x0) * t;
                    let ph = if band == 0 { 2.6 } else { 3.8 };
                    let dir = if band == 0 { 1.0 } else { -0.7 };
                    let y = r.top() + r.height() * (0.10 + 0.8 * k)
                        + (t * ph + k * 1.6 + band as f32 + phase * dir).sin() * r.height() * (0.10 + 0.32 * t)
                        + (t * 6.0 + phase * 1.3 + k).sin() * r.height() * 0.03
                        - t * t * r.height() * 0.35 * (band as f32 * 2.0 - 1.0);
                    pos2(x, y)
                })
                .collect();
            let a = 0.10 + 0.28 * k * (1.0 - band as f32 * 0.4);
            // a pezzi con alpha crescente verso destra (una linea egui ha un solo colore)
            for c in 0..8 {
                let (from, to) = (c * n / 8, ((c + 1) * n / 8).min(n));
                let t = (c as f32 + 0.5) / 8.0;
                let fade = 0.12 + 0.88 * t * t;
                p.add(egui::Shape::line(pts[from..=to].to_vec(), Stroke::new(1.0, col::BRAND.gamma_multiply(a * fade))));
            }
        }
    }
    // le onde sono dritte: negli angoli si coprono col colore della pagina, poi il bordo arrotondato
    let mut mask = egui::Mesh::default();
    for (corner, cx, cy, a0) in [
        (r.left_top(), r.left() + rad, r.top() + rad, 180.0f32),
        (r.right_top(), r.right() - rad, r.top() + rad, 270.0),
        (r.right_bottom(), r.right() - rad, r.bottom() - rad, 0.0),
        (r.left_bottom(), r.left() + rad, r.bottom() - rad, 90.0),
    ] {
        let base = mask.vertices.len() as u32;
        mask.colored_vertex(corner, col::BG);
        for k in 0..=8 {
            let a = (a0 + 90.0 * k as f32 / 8.0).to_radians();
            mask.colored_vertex(pos2(cx + (rad + 0.5) * a.cos(), cy + (rad + 0.5) * a.sin()), col::BG);
        }
        for k in 0..8u32 {
            mask.add_triangle(base, base + 1 + k, base + 2 + k);
        }
    }
    p.add(egui::Shape::mesh(mask));
    p.rect_stroke(r, rad, Stroke::new(1.0, col::LINE), egui::StrokeKind::Inside);
    // testo
    let mut y = r.top() + 14.0;
    let crumb = crumbs.join("  /  ");
    p.text(pos2(r.left() + 22.0, y), egui::Align2::LEFT_TOP, &crumb, bold(12.5), col::BRAND);
    y += 22.0;
    let g = p.layout_no_wrap(title.to_string(), bold(32.0), col::HEAD);
    let tw = g.size().x;
    p.galley(pos2(r.left() + 22.0, y), g, col::HEAD);
    if !sub.is_empty() {
        p.text(pos2(r.left() + 22.0, y + 44.0), egui::Align2::LEFT_TOP, sub, FontId::proportional(14.0), col::MUTED);
    }
    if let Some((c, t)) = badge {
        let rect = Rect::from_min_size(pos2(r.left() + 22.0 + tw + 14.0, y + 11.0), vec2(200.0, 26.0));
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(egui::Layout::left_to_right(egui::Align::Center)));
        chip(&mut child, c, t);
    }
    let br = Rect::from_min_size(pos2(r.right() - 330.0, r.top() + 14.0), vec2(314.0, 32.0));
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(br).layout(egui::Layout::right_to_left(egui::Align::Center)));
    let clicked = child.add(egui::Button::new(RichText::new(format!("↻  {}", crate::i18n::tr("ui.aggiorna"))).font(bold(13.5))).fill(col::WHITE)).clicked();
    let ex = !extra.is_empty() && child.add(egui::Button::new(RichText::new(extra).font(bold(13.5))).fill(col::WHITE)).clicked();
    ui.add_space(12.0);
    (clicked, ex)
}

/// Riquadri che si dividono tutto lo spazio rimasto, senza scroll di pagina: colonne da almeno `min_w`,
/// righe alte quanto serve per riempire l'altezza (mai sotto `min_h`; solo allora compare lo scroll).
pub fn tiles(ui: &mut Ui, id: &str, n: usize, min_w: f32, min_h: f32, mut f: impl FnMut(&mut Ui, usize)) {
    if n == 0 {
        return;
    }
    let gap = 12.0;
    let avail = ui.available_rect_before_wrap();
    let w = avail.width();
    let cols = (((w + gap) / (min_w + gap)).floor() as usize).clamp(1, n);
    let rows = n.div_ceil(cols);
    let fit = (avail.height() - 8.0 - gap * (rows as f32 - 1.0)) / rows as f32;
    let th = fit.max(min_h).floor();
    let tw = (w - gap * (cols as f32 - 1.0)) / cols as f32;
    egui::ScrollArea::vertical().id_salt(id).auto_shrink([false, false]).show(ui, |ui| {
        let top = ui.cursor().min;
        ui.allocate_exact_size(vec2(w, rows as f32 * th + gap * (rows as f32 - 1.0)), Sense::hover());
        for i in 0..n {
            let (row, c) = (i / cols, i % cols);
            let rect = Rect::from_min_size(top + vec2(c as f32 * (tw + gap), row as f32 * (th + gap)), vec2(tw, th));
            ui.painter().rect(rect, 14, col::SURFACE, Stroke::new(1.0, col::LINE), egui::StrokeKind::Inside);
            let inner = rect.shrink2(vec2(20.0, 16.0));
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner).layout(egui::Layout::top_down(egui::Align::Min)).id_salt((id, i)));
            child.set_clip_rect(rect.shrink(1.0).intersect(ui.clip_rect()));
            f(&mut child, i);
        }
    });
}

/// Contenuto che può essere lungo (liste): scorre dentro lo spazio rimasto, la pagina resta ferma.
pub fn scroll_fill(ui: &mut Ui, id: &str, f: impl FnOnce(&mut Ui)) {
    egui::ScrollArea::vertical().id_salt(id).auto_shrink([false, false]).show(ui, |ui| {
        f(ui);
        ui.add_space(12.0);
    });
}

/// Testata di un riquadro: titolo, sottotitolo e, se `pin` c'è, la puntina per la dashboard (true = cliccata).
pub fn tile_head(ui: &mut Ui, title: &str, sub: &str, pin: Option<bool>) -> bool {
    let mut clicked = false;
    ui.horizontal(|ui| {
        ui.label(h(title, 16.5));
        if !sub.is_empty() {
            ui.label(muted(sub).size(13.0));
        }
        if let Some(on) = pin {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let (r, resp) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::click());
                let c = if on { col::BRAND } else if resp.hovered() { col::BRAND_DARK } else { col::OFF };
                if on || resp.hovered() {
                    ui.painter().rect_filled(r, 6, col::BRAND_SUBTLE);
                }
                ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, "📌", FontId::proportional(13.0), c);
                let tip = if on { tr("common.togli_dalla_dashboard") } else { tr("ui.metti_nella_dashboard") };
                clicked = resp.on_hover_text(tip).clicked();
            });
        }
    });
    ui.add_space(4.0);
    clicked
}

/// Anello 270° con il valore al centro.
pub fn gauge(ui: &mut Ui, label: &str, frac: Option<f32>, text: &str, c: Color32, size: f32) {
    let (r, _) = ui.allocate_exact_size(vec2(size, size + 18.0), Sense::hover());
    let center = pos2(r.center().x, r.top() + size / 2.0);
    let rad = size / 2.0 - 6.0;
    let arc = |to: f32| -> Vec<Pos2> {
        let a0 = 135f32.to_radians();
        let n = (48.0 * to).ceil().max(1.0) as usize;
        (0..=n).map(|i| a0 + 270f32.to_radians() * to * i as f32 / n as f32).map(|a| pos2(center.x + rad * a.cos(), center.y + rad * a.sin())).collect()
    };
    let p = ui.painter();
    p.add(egui::Shape::line(arc(1.0), Stroke::new(8.0, col::SURFACE2)));
    if let Some(f) = frac.filter(|f| *f > 0.002) {
        p.add(egui::Shape::line(arc(f.min(1.0)), Stroke::new(8.0, c)));
    }
    p.text(center, egui::Align2::CENTER_CENTER, text, bold(size * 0.2), if frac.is_some() { col::HEAD } else { col::OFF });
    p.text(pos2(center.x, r.bottom() - 6.0), egui::Align2::CENTER_CENTER, label, bold(12.5), col::MUTED);
}

/// Linea con area tenue sotto; None = buco (nessun dato), la linea si interrompe.
pub fn spark(ui: &mut Ui, pts: &[Option<f64>], cap: Option<f64>, c: Color32, height: f32) {
    spark_multi(ui, &[(pts, c)], cap, height);
}

/// Una o più serie (es. rete in e out). `height` = f32::INFINITY riempie lo spazio rimasto.
pub fn spark_multi(ui: &mut Ui, series: &[(&[Option<f64>], Color32)], cap: Option<f64>, height: f32) {
    let height = if height.is_finite() { height } else { (ui.available_height() - 2.0).max(40.0) };
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    let p = ui.painter();
    p.rect(r, 8, col::SURFACE, Stroke::new(1.0, col::LINE), egui::StrokeKind::Inside);
    let inner = r.shrink2(vec2(10.0, 10.0));
    for k in 1..4 {
        let y = inner.top() + inner.height() * k as f32 / 4.0;
        p.line_segment([pos2(inner.left(), y), pos2(inner.right(), y)], Stroke::new(1.0, col::BG));
    }
    let Some(pts) = series.first().map(|s| s.0) else { return };
    if pts.len() < 2 {
        return;
    }
    let max = cap.unwrap_or(0.0).max(series.iter().flat_map(|s| s.0.iter().flatten()).copied().fold(f64::MIN, f64::max)).max(1e-9);
    let n = (pts.len() - 1) as f32;
    let at = |i: usize, v: f64| pos2(inner.left() + inner.width() * i as f32 / n, inner.bottom() - inner.height() * (v / max) as f32);
    for &(pts, c) in series {
    let mut seg: Vec<Pos2> = Vec::new();
    let flush = |seg: &mut Vec<Pos2>| {
        if seg.len() >= 2 {
            let mut m = egui::Mesh::default();
            for w in seg.windows(2) {
                let i = m.vertices.len() as u32;
                m.colored_vertex(w[0], c.gamma_multiply(0.16));
                m.colored_vertex(w[1], c.gamma_multiply(0.16));
                m.colored_vertex(pos2(w[1].x, inner.bottom()), c.gamma_multiply(0.01));
                m.colored_vertex(pos2(w[0].x, inner.bottom()), c.gamma_multiply(0.01));
                m.add_triangle(i, i + 1, i + 2);
                m.add_triangle(i, i + 2, i + 3);
            }
            p.add(egui::Shape::mesh(m));
            p.add(egui::Shape::line(std::mem::take(seg), Stroke::new(2.0, c)));
        } else {
            seg.clear();
        }
    };
    for (i, v) in pts.iter().enumerate() {
        match v {
            Some(v) => seg.push(at(i, *v)),
            None => flush(&mut seg),
        }
    }
    flush(&mut seg);
    }
    let (pts, c) = series[0];
    // valore sotto il mouse
    if let Some(h) = resp.hover_pos() {
        let i = (((h.x - inner.left()) / inner.width()) * n).round().clamp(0.0, n) as usize;
        let x = inner.left() + inner.width() * i as f32 / n;
        p.line_segment([pos2(x, inner.top()), pos2(x, inner.bottom())], Stroke::new(1.0, col::OFF));
        if let Some(v) = pts[i] {
            p.circle_filled(at(i, v), 4.0, c);
            p.text(pos2(x + 6.0, inner.top()), egui::Align2::LEFT_TOP, fmt_num((v * 10.0).round() / 10.0), bold(12.0), col::HEAD);
        }
    }
}

/// Barra di riempimento (quote, dischi).
pub fn bar(ui: &mut Ui, frac: f32, c: Color32) {
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 8.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(r, 4, col::SURFACE2);
    p.rect_filled(Rect::from_min_size(r.min, vec2(r.width() * frac.clamp(0.0, 1.0), r.height())), 4, c);
}

pub fn loading(ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.spinner();
        ui.label(muted(tr("ui.carico")));
    });
}

/// Errore API. Un 403 "not been granted" vuol dire token senza permesso: lo diciamo chiaro.
pub fn api_error(ui: &mut Ui, e: &str) -> bool {
    if e.starts_with("403") && e.contains("granted") {
        ui.label(RichText::new(tr("ui.token_senza_permesso")).color(col::WARN));
        ui.label(muted(tr("ui.crea_un_token_nuovo_con")));
        ui.label(muted(fill(tr("ui.ovh"), &[("e", e.to_string())])).size(12.0));
        return ui.add(primary(tr("ui.crea_token_completo"))).clicked();
    }
    ui.label(RichText::new(e).color(col::CRIT));
    false
}

pub fn state_color(s: &str) -> Color32 {
    match s {
        "running" | "ok" | "poweron" | "done" | "up" => col::OK,
        "stopped" | "poweroff" | "error" | "hacked" | "hackedBlocked" | "ovhError" | "customerError" | "cancelled" | "down" => col::CRIT,
        _ => col::WARN,
    }
}
pub fn state_it(s: &str) -> &str {
    match s {
        "running" => tr("ui.accesa"),
        "stopped" => tr("ui.spenta"),
        "rebooting" => tr("ui.in_riavvio"),
        "stopping" => tr("ui.si_spegne"),
        "backuping" => tr("ui.backup_in_corso"),
        "installing" => tr("ui.installazione"),
        "maintenance" => tr("ui.manutenzione"),
        "rescued" => tr("ui.rescue"),
        "upgrading" => tr("ui.upgrade"),
        "poweron" => tr("ui.accesa"),
        "poweroff" => tr("ui.spenta"),
        "ok" => tr("ui.ok"),
        "error" => tr("ui.errore"),
        "hacked" | "hackedBlocked" => tr("ui.bloccata_per_abuso"),
        "done" => tr("ui.fatto"),
        "doing" => tr("ui.in_corso"),
        "todo" | "init" => tr("ui.in_coda"),
        "cancelled" => tr("ui.annullato"),
        "customerError" | "ovhError" => tr("ui.errore"),
        s => s,
    }
}

/// "2027-11-17" o "2026-11-16T22:15:31+01:00" -> "17/11/2027" / "16/11/2026 22:15"
pub fn it_date(s: &str) -> String {
    let b = s.as_bytes();
    if b.len() < 10 || b[4] != b'-' || b[7] != b'-' {
        return s.to_string();
    }
    let d = format!("{}/{}/{}", &s[8..10], &s[5..7], &s[0..4]);
    if b.len() >= 16 && b[10] == b'T' {
        format!("{d} {}", &s[11..16])
    } else {
        d
    }
}

/// Giorni da oggi a una data "AAAA-MM-GG..." (negativi = passata). Civil-from-days, niente crate di date.
pub fn days_until(s: &str, now_unix: i64) -> Option<i64> {
    let y: i64 = s.get(0..4)?.parse().ok()?;
    let m: i64 = s.get(5..7)?.parse().ok()?;
    let d: i64 = s.get(8..10)?.parse().ok()?;
    let (y2, m2) = if m <= 2 { (y - 1, m + 9) } else { (y, m - 3) };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let doy = (153 * m2 + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    Some(days - now_unix.div_euclid(86400))
}

pub fn sv(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => "-".into(),
        v => v.to_string(),
    }
}

/// {"unit":"GB","value":32} -> "32 GB"
pub fn uv(v: &serde_json::Value) -> String {
    match (v["value"].as_f64(), v["unit"].as_str()) {
        (Some(x), Some(u)) => format!("{} {u}", fmt_num(x)),
        (Some(x), None) => fmt_num(x),
        _ => "-".into(),
    }
}
pub fn fmt_num(x: f64) -> String {
    if x.fract().abs() < 1e-9 {
        format!("{x:.0}")
    } else {
        format!("{x:.1}")
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn days_until_known_dates() {
        // 2026-11-17 00:00 UTC = 1794873600
        assert_eq!(super::days_until("2026-11-17", 1794873600), Some(0));
        assert_eq!(super::days_until("2027-11-17", 1794873600), Some(365));
        assert_eq!(super::days_until("2026-11-10T10:00:00+01:00", 1794873600 + 3600), Some(-7));
    }

    #[test]
    fn it_date_formats() {
        assert_eq!(super::it_date("2027-11-17"), "17/11/2027");
        assert_eq!(super::it_date("2026-11-16T22:15:31+01:00"), "16/11/2026 22:15");
        assert_eq!(super::it_date("boh"), "boh");
    }
}
