//! Vista de impresión: exporta el circuito a SVG vectorial con tema claro.
//!
//! Reutiliza la misma geometría de `draw` (posiciones de orificios, curvas de cable,
//! cuerpo de chip, display y fuente) para que la figura impresa coincida con la app,
//! pero emitiendo SVG en lugar de dibujar con `egui::Painter`.
use crate::draw::{self, View};
use crate::instr::bandas;
use crate::model::*;
use crate::sim::Sim;
use eframe::egui::{pos2, vec2, Pos2, Rect};

/// Opciones de exportación.
#[derive(Clone, Debug, Default)]
pub struct Cfg {
    /// Solo esta placa (1..=3). `None` = todas.
    pub placa: Option<u8>,
    /// Ancho del dibujo en milímetros.
    pub ancho_mm: f32,
    /// Título; si es `None` se usa el nombre del circuito.
    pub titulo: Option<String>,
    /// Dibuja la leyenda de colores al pie.
    pub leyenda: bool,
}

// ---- paleta de impresión (clara) ----
const FONDO: &str = "#ffffff";
const PLACA: &str = "#f3efe4";
const PLACA_BORDE: &str = "#d6cfbd";
const RANURA: &str = "#e3ddcc";
const HOYO: &str = "#b6ae9c";
const RIEL_ROJO: &str = "#c0392b";
const RIEL_AZUL: &str = "#2f5fa8";
const TEXTO: &str = "#1d2328";
const TEXTO_SUAVE: &str = "#6b7280";
const CHIP: &str = "#20242a";
const CHIP_TEXTO: &str = "#f5f6f7";
const PATA: &str = "#8d939a";
const CUERPO_R: &str = "#ead6ac";
const DISPLAY: &str = "#171a1e";
const SEG_ON: &str = "#ff5a37";
const SEG_OFF: &str = "#3a352f";

pub fn hex(c: eframe::egui::Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())
}

fn sombra(c: &str, f: f32) -> String {
    let h = c.trim_start_matches('#');
    if h.len() != 6 {
        return c.to_string();
    }
    let v = u32::from_str_radix(h, 16).unwrap_or(0);
    let (r, g, b) = ((v >> 16) & 255, (v >> 8) & 255, v & 255);
    format!(
        "#{:02x}{:02x}{:02x}",
        (r as f32 * f) as u32,
        (g as f32 * f) as u32,
        (b as f32 * f) as u32
    )
}

/// Acumula las primitivas SVG.
struct Lienzo {
    s: String,
}

impl Lienzo {
    fn new() -> Self {
        Self {
            s: String::with_capacity(1 << 20),
        }
    }
    fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, rx: f32, fill: &str) {
        self.s.push_str(&format!(
            r#"<rect x="{x:.2}" y="{y:.2}" width="{w:.2}" height="{h:.2}" rx="{rx:.2}" fill="{fill}"/>"#
        ));
    }
    fn rect_st(&mut self, x: f32, y: f32, w: f32, h: f32, rx: f32, fill: &str, stroke: &str, sw: f32) {
        self.s.push_str(&format!(
            r#"<rect x="{x:.2}" y="{y:.2}" width="{w:.2}" height="{h:.2}" rx="{rx:.2}" fill="{fill}" stroke="{stroke}" stroke-width="{sw:.2}"/>"#
        ));
    }
    fn line(&mut self, a: Pos2, b: Pos2, color: &str, w: f32) {
        self.s.push_str(&format!(
            r#"<line x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}" stroke="{}" stroke-width="{w:.2}" stroke-linecap="round"/>"#,
            a.x, a.y, b.x, b.y, color
        ));
    }
    fn circle(&mut self, c: Pos2, r: f32, fill: &str) {
        self.s.push_str(&format!(
            r#"<circle cx="{:.2}" cy="{:.2}" r="{r:.2}" fill="{}"/>"#,
            c.x, c.y, fill
        ));
    }
    fn circle_st(&mut self, c: Pos2, r: f32, fill: &str, stroke: &str, sw: f32) {
        self.s.push_str(&format!(
            r#"<circle cx="{:.2}" cy="{:.2}" r="{r:.2}" fill="{}" stroke="{stroke}" stroke-width="{sw:.2}"/>"#,
            c.x, c.y, fill
        ));
    }
    fn poly(&mut self, pts: &[Pos2], fill: &str, stroke: &str, sw: f32) {
        let d: Vec<String> = pts.iter().map(|p| format!("{:.2},{:.2}", p.x, p.y)).collect();
        self.s.push_str(&format!(
            r#"<polygon points="{}" fill="{}" stroke="{stroke}" stroke-width="{sw:.2}" stroke-linejoin="round"/>"#,
            d.join(" "),
            fill
        ));
    }
    fn path(&mut self, pts: &[Pos2], color: &str, w: f32, op: f32) {
        if pts.len() < 2 {
            return;
        }
        let mut d = format!("M {:.2} {:.2}", pts[0].x, pts[0].y);
        for p in &pts[1..] {
            d.push_str(&format!(" L {:.2} {:.2}", p.x, p.y));
        }
        self.s.push_str(&format!(
            r#"<path d="{d}" fill="none" stroke="{color}" stroke-width="{w:.2}" stroke-opacity="{op:.2}" stroke-linecap="round" stroke-linejoin="round"/>"#
        ));
    }
    /// Texto con anclaje: 0 = izquierda, 1 = centro, 2 = derecha.
    fn text(&mut self, at: Pos2, txt: &str, size: f32, color: &str, anchor: u8, bold: bool) {
        let a = match anchor {
            1 => "middle",
            2 => "end",
            _ => "start",
        };
        let weight = if bold { " font-weight=\"600\"" } else { "" };
        let t = txt
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        self.s.push_str(&format!(
            r#"<text x="{:.2}" y="{:.2}" font-family="Segoe UI, Arial, sans-serif" font-size="{size:.2}" fill="{color}" text-anchor="{a}"{weight}>{t}</text>"#,
            at.x, at.y
        ));
    }
}

fn quad_pts(c: Pos2, u: eframe::egui::Vec2, hl: f32, hw: f32) -> Vec<Pos2> {
    let n = vec2(-u.y, u.x);
    vec![
        c - u * hl - n * hw,
        c + u * hl - n * hw,
        c + u * hl + n * hw,
        c - u * hl + n * hw,
    ]
}

fn dir(a: Pos2, b: Pos2) -> (eframe::egui::Vec2, f32) {
    let d = b - a;
    let len = d.length().max(0.001);
    (d / len, len)
}

/// Exporta el circuito a un SVG.
pub fn exportar(circ: &Circuito, sim: &Sim, cfg: &Cfg) -> String {
    let placa = cfg.placa;
    let tableros = circ.tableros.max(1);
    let (W, H) = if placa.is_some() {
        (1700.0, 640.0)
    } else {
        (1700.0, 760.0)
    };
    // área de dibujo (deja sitio al título y a la leyenda)
    let area = Rect::from_min_size(pos2(40.0, 96.0), vec2(W - 80.0, H - (96.0 + if cfg.leyenda { 96.0 } else { 40.0 })));
    let mut v = match placa {
        Some(b) => {
            let mut v = View::fit(area, 1, circ.columnas, &[[0.0f32, 0.0f32]]);
            let bx = (b.saturating_sub(1)) as f32 * BOARD_OFFSET;
            v.o.x -= bx * v.s;
            v
        }
        None => View::fit(area, tableros, circ.columnas, &circ.posiciones),
    };
    // Las placas movidas a mano conservan su desplazamiento.
    for (i, d) in circ.posiciones.iter().take(16).enumerate() {
        v.off[i] = vec2(d[0], d[1]);
    }
    let s = v.s;

    let mut l = Lienzo::new();

    // ---------- título ----------
    let titulo = cfg
        .titulo
        .clone()
        .unwrap_or_else(|| circ.nombre.clone());
    l.text(pos2(40.0, 44.0), &titulo, 26.0, TEXTO, 0, true);
    let chips = circ.componentes.iter().filter(|c| c.tipo == Tipo::Chip).count();
    let cables = circ.componentes.iter().filter(|c| c.tipo == Tipo::Cable).count();
    let sub = format!(
        "{} · {chips} integrados · {cables} cables · simulación a 5 V · vista de armado en protoboard{}",
        "OpenCircuit",
        match placa {
            Some(b) => format!(" · solo placa {b}"),
            None => String::new(),
        }
    );
    l.text(pos2(40.0, 66.0), &sub, 14.0, TEXTO_SUAVE, 0, false);

    // ---------- placas ----------
    for b in 1..=tableros {
        if let Some(p) = placa {
            if p != b {
                continue;
            }
        }
        let vb = v.board_view(b);
        let bx = (b.saturating_sub(1)) as f32 * BOARD_OFFSET;
        let bw = board_width(vb.columnas);

        // cuerpo
        l.rect(
            vb.p(bx, 0.0).x,
            vb.p(bx, 0.0).y,
            bw * vb.s,
            19.0 * vb.s,
            6.0,
            PLACA,
        );
        // ranura central
        l.rect(
            vb.p(bx, 9.15).x,
            vb.p(bx, 9.15).y,
            bw * vb.s,
            0.7 * vb.s,
            0.0,
            RANURA,
        );
        // rieles
        for (y, signo, c) in [
            (1.0, "+", RIEL_ROJO),
            (2.0, "−", RIEL_AZUL),
            (17.0, "−", RIEL_AZUL),
            (18.0, "+", RIEL_ROJO),
        ] {
            l.line(vb.p(bx + 0.6, y), vb.p(bx + bw - 0.6, y), c, 0.16 * s);
            l.text(vb.p(bx + 0.2, y), signo, 0.9 * s, c, 1, true);
        }
        // letras de fila
        for row in 0..10u8 {
            let (_, y) = hole_xy(Hole::Main { board: b, row, col: 1 });
            l.text(
                vb.p(bx - 0.55, y),
                &((b'a' + row) as char).to_string(),
                0.55 * s,
                TEXTO_SUAVE,
                1,
                false,
            );
        }
        // números de columna (cada 5 y la primera)
        for col in 1..=vb.columnas {
            if col != 1 && col % 5 != 0 {
                continue;
            }
            for y in [3.2, 15.8] {
                l.text(
                    vb.p(bx + col as f32 + 1.0, y),
                    &col.to_string(),
                    0.44 * s,
                    TEXTO_SUAVE,
                    1,
                    false,
                );
            }
        }
        // orificios
        for col in 1..=vb.columnas {
            for row in 0..10u8 {
                let p = vb.h(Hole::Main { board: b, row, col });
                l.circle(p, 0.17 * s, HOYO);
            }
            for (top, plus) in [(true, true), (true, false), (false, false), (false, true)] {
                let p = vb.h(Hole::Rail { board: b, top, plus, col });
                l.circle(p, 0.14 * s, HOYO);
            }
        }
        // etiqueta de la placa
        let c = vb.p(bx + bw / 2.0, -0.9);
        l.text(c, &format!("PLACA {b}"), 0.62 * s, "#8a6d1f", 1, true);
    }

    // ---------- cables ----------
    let mut items: Vec<(&Comp, Vec<Hole>)> = circ
        .componentes
        .iter()
        .filter(|c| !circ.oculta(c))
        .filter_map(|c| c.holes().ok().map(|h| (c, h)))
        .filter(|(_, hs)| match placa {
            Some(p) => hs.first().is_some_and(|h| h.board() == p),
            None => true,
        })
        .collect();
    items.sort_by_key(|(c, _)| match c.tipo {
        Tipo::Fuente => 0,
        Tipo::Cable => 1,
        Tipo::Resistencia | Tipo::Led | Tipo::Interruptor => 2,
        Tipo::Chip | Tipo::Display => 3,
        Tipo::Etiqueta => 4,
    });

    for (c, hs) in &items {
        if c.tipo != Tipo::Cable {
            continue;
        }
        let pts = draw::cable_pts(&v, c, hs);
        let col = hex(draw::color(&c.color_or("rojo")));
        let somb: Vec<Pos2> = pts.iter().map(|q| *q + vec2(0.07 * s, 0.1 * s)).collect();
        l.path(&somb, "#000000", 0.44 * s, 0.12);
        l.path(&pts, &sombra(&col, 0.55), 0.38 * s, 1.0);
        l.path(&pts, &col, 0.26 * s, 1.0);
        for e in [pts[0], *pts.last().unwrap()] {
            l.circle(e, 0.15 * s, &sombra(&col, 0.45));
            l.circle(e, 0.09 * s, &col);
        }
        // etiqueta de red sobre el cable
        if c.nombre.is_some() {
            if let Some(mid) = pts.get(pts.len() / 2) {
                l.text(*mid + vec2(0.0, -0.35 * s), &c.red(), 0.42 * s, TEXTO_SUAVE, 1, false);
            }
        }
    }

    // ---------- componentes ----------
    for (c, hs) in &items {
        if c.tipo == Tipo::Cable {
            continue;
        }
        let vb;
        let vv = if matches!(c.tipo, Tipo::Chip | Tipo::Display) {
            vb = v.board_view(hs[0].board());
            &vb
        } else {
            &v
        };
        let s = vv.s;
        match c.tipo {
            Tipo::Chip => {
                let x0 = vv.h(hs[0]).x - 0.6 * s;
                let x1 = vv.h(hs[6]).x + 0.6 * s;
                let (yt, yb) = (vv.p(0.0, 8.35).y, vv.p(0.0, 10.65).y);
                // patas
                for h in hs.iter() {
                    let hp = vv.h(*h);
                    let above = hp.y < vv.p(0.0, 9.5).y;
                    let yy = if above { yt } else { yb };
                    l.line(pos2(hp.x, hp.y), pos2(hp.x, yy), PATA, 0.16 * s);
                }
                l.rect_st(x0, yt, x1 - x0, yb - yt, 0.14 * s, CHIP, "#0b0d10", 0.03 * s);
                // muesca
                l.circle(pos2(x0, (yt + yb) / 2.0), 0.3 * s, PLACA);
                l.text(
                    pos2((x0 + x1) / 2.0 + 0.2 * s, (yt + yb) / 2.0),
                    &c.modelo.clone().unwrap_or_default(),
                    0.58 * s,
                    CHIP_TEXTO,
                    1,
                    true,
                );
                l.text(
                    pos2(x1 - 0.25 * s, yt + 0.45 * s),
                    &c.id,
                    0.4 * s,
                    "#9aa0a6",
                    2,
                    false,
                );
            }
            Tipo::Resistencia => {
                let (a, b) = (vv.h(hs[0]), vv.h(hs[1]));
                l.line(a, b, PATA, 0.12 * s);
                let (u, len) = dir(a, b);
                let mid = a + (b - a) * 0.5;
                let hl = (len * 0.36).clamp(0.5 * s, 1.15 * s);
                let hw = 0.3 * s;
                l.poly(&quad_pts(mid, u, hl, hw * 0.9), CUERPO_R, "#c9ae7a", 0.03 * s);
                for e in [-1.0f32, 1.0] {
                    l.circle(mid + u * hl * e, hw * 1.1, CUERPO_R);
                }
                for (k, name) in bandas(c.ohmios()).iter().enumerate() {
                    let off = [-0.62f32, -0.3, 0.02, 0.62][k] * hl;
                    let col = hex(draw::color(name));
                    l.poly(
                        &quad_pts(mid + u * off, u, 0.08 * s, hw * if k == 3 { 0.88 } else { 1.02 }),
                        &col,
                        &col,
                        0.0,
                    );
                }
                l.text(
                    mid + vec2(0.0, -0.55 * s),
                    &c.id,
                    0.42 * s,
                    TEXTO,
                    1,
                    false,
                );
            }
            Tipo::Led => {
                let (a, b) = (vv.h(hs[0]), vv.h(hs[1]));
                let d = draw::led_dome(a, b, s);
                l.line(a, d, PATA, 0.12 * s);
                l.line(b, d, PATA, 0.12 * s);
                let col = hex(draw::color(&c.color_or("rojo")));
                let on = sim.led(&c.id).is_some_and(|x| x.on);
                let relleno = if on {
                    col.clone()
                } else {
                    sombra(&col, 0.55)
                };
                l.circle_st(d, 0.62 * s, &relleno, &sombra(&col, 0.35), 0.04 * s);
                if on {
                    l.circle(d, 0.3 * s, "#ffffffcc");
                } else {
                    l.circle(d + vec2(-0.2 * s, -0.2 * s), 0.14 * s, "#ffffffaa");
                }
                l.text(a + vec2(0.0, 0.55 * s), "+", 0.5 * s, "#c0392b", 1, true);
                l.text(d + vec2(0.0, -1.0 * s), &c.id, 0.42 * s, TEXTO, 1, false);
            }
            Tipo::Interruptor => {
                let (a, b) = (vv.h(hs[0]), vv.h(hs[1]));
                l.line(a, b, PATA, 0.12 * s);
                let (u, _) = dir(a, b);
                let mid = a + (b - a) * 0.5;
                l.poly(
                    &quad_pts(mid, u, 0.95 * s, 0.5 * s),
                    "#2f5fa8",
                    "#1b3a6b",
                    0.04 * s,
                );
                l.poly(&quad_pts(mid, u, 0.6 * s, 0.17 * s), "#0f1932", "#0f1932", 0.0);
                let k = if c.cerrado { 0.3 } else { -0.3 };
                l.poly(
                    &quad_pts(mid + u * k * s, u, 0.26 * s, 0.32 * s),
                    "#eeeeee",
                    "#9aa0a6",
                    0.03 * s,
                );
                l.text(
                    mid + vec2(0.0, 0.95 * s),
                    if c.cerrado { "ON" } else { "OFF" },
                    0.5 * s,
                    if c.cerrado { "#1e9e50" } else { "#c0392b" },
                    1,
                    true,
                );
                l.text(mid + vec2(0.0, -0.95 * s), &c.id, 0.42 * s, TEXTO, 1, false);
            }
            Tipo::Display => {
                let (x0, x1, y) = (
                    hs[0..7].iter().map(|h| vv.h(*h).x).fold(f32::MAX, f32::min),
                    hs[0..7].iter().map(|h| vv.h(*h).x).fold(f32::MIN, f32::max),
                    vv.h(hs[0]).y,
                );
                let body = Rect::from_min_max(
                    pos2(x0 - 0.9 * s, y - 2.9 * s),
                    pos2(x1 + 0.9 * s, y - 0.35 * s),
                );
                for h in hs.iter() {
                    l.line(
                        vv.h(*h),
                        pos2(vv.h(*h).x, body.bottom()),
                        PATA,
                        0.12 * s,
                    );
                }
                l.rect_st(
                    body.left(),
                    body.top(),
                    body.width(),
                    body.height(),
                    0.18 * s,
                    DISPLAY,
                    "#000000",
                    0.04 * s,
                );
                let segs = sim
                    .display(&c.id)
                    .map(|d| d.segments)
                    .unwrap_or([false; 7]);
                let (cx, cy) = (body.center().x, body.center().y);
                let w = (body.width() * 0.22).clamp(0.35 * s, 0.72 * s);
                let h = (body.height() * 0.32).clamp(0.55 * s, 1.0 * s);
                let (xl, xr) = (cx - w, cx + w);
                let (yt, ym, yb) = (cy - h, cy, cy + h);
                let seg = |a: Pos2, b: Pos2, lit: bool, l: &mut Lienzo| {
                    l.line(a, b, if lit { SEG_ON } else { SEG_OFF }, 0.13 * s);
                };
                seg(pos2(xl, yt), pos2(xr, yt), segs[0], &mut l);
                seg(pos2(xr, yt), pos2(xr, ym), segs[1], &mut l);
                seg(pos2(xr, ym), pos2(xr, yb), segs[2], &mut l);
                seg(pos2(xl, yb), pos2(xr, yb), segs[3], &mut l);
                seg(pos2(xl, ym), pos2(xl, yb), segs[4], &mut l);
                seg(pos2(xl, yt), pos2(xl, ym), segs[5], &mut l);
                seg(pos2(xl, ym), pos2(xr, ym), segs[6], &mut l);
                l.text(
                    body.left_top() + vec2(0.3 * s, 0.5 * s),
                    &c.id,
                    0.42 * s,
                    "#aeb4ba",
                    0,
                    false,
                );
            }
            Tipo::Etiqueta => {
                let at = vv.h(hs[0]);
                let txt = c.red();
                let col = match c.color.as_deref() {
                    Some(n) => hex(draw::color(n)),
                    None => hex(draw::net_color(&txt)),
                };
                let size = 0.55 * s;
                let w = (txt.len() as f32 * size * 0.58).max(1.2 * s);
                let tag = Rect::from_center_size(at + vec2(0.0, -0.9 * s), vec2(w + 0.3 * s, size * 1.4));
                l.line(at, pos2(at.x, tag.bottom()), &col, 0.06 * s);
                l.rect_st(
                    tag.left(),
                    tag.top(),
                    tag.width(),
                    tag.height(),
                    0.12 * s,
                    &col,
                    &sombra(&col, 0.6),
                    0.03 * s,
                );
                l.text(tag.center(), &txt, size, "#ffffff", 1, true);
            }
            Tipo::Fuente => {
                let b = draw::fuente_box(c, 0);
                let r = Rect::from_min_max(vv.p(b.left(), b.top()), vv.p(b.right(), b.bottom()));
                l.rect_st(
                    r.left(),
                    r.top(),
                    r.width(),
                    r.height(),
                    0.4 * s,
                    "#2b3038",
                    "#11151a",
                    0.05 * s,
                );
                l.text(
                    vv.p(b.left() + 0.4, b.top() + 0.9),
                    "FUENTE",
                    0.55 * s,
                    "#e8eaed",
                    0,
                    true,
                );
                l.text(
                    vv.p(b.left() + 0.4, b.top() + 2.4),
                    &format!("{:.1} V", c.voltios()),
                    0.9 * s,
                    "#49d17a",
                    0,
                    false,
                );
                l.text(
                    vv.p(b.left() + 0.4, b.top() + 3.8),
                    &c.id,
                    0.5 * s,
                    "#aeb4ba",
                    0,
                    false,
                );
                // terminales
                let (tp, tm) = (
                    vv.p(b.right() - 0.7, b.top() + 1.5),
                    vv.p(b.right() - 0.7, b.top() + 3.6),
                );
                l.circle(tp, 0.34 * s, "#c0392b");
                l.text(tp, "+", 0.62 * s, "#ffffff", 1, true);
                l.circle(tm, 0.34 * s, "#1d2328");
                l.text(tm, "−", 0.62 * s, "#ffffff", 1, true);
                if let Some(h0) = hs.first() {
                    l.path(&[tp, vv.h(*h0)], "#c0392b", 0.2 * s, 1.0);
                }
                if let Some(h1) = hs.get(1) {
                    l.path(&[tm, vv.h(*h1)], "#1d2328", 0.2 * s, 1.0);
                }
            }
            Tipo::Cable => {}
        }
    }

    // ---------- leyenda ----------
    if cfg.leyenda {
        let y = H - 58.0;
        l.text(pos2(40.0, y), "Colores de red", 13.0, TEXTO, 0, true);
        let mut nets: Vec<String> = Vec::new();
        for c in &circ.componentes {
            match c.tipo {
                Tipo::Etiqueta => nets.push(c.red()),
                Tipo::Cable => nets.push(c.red()),
                _ => {}
            }
        }
        nets.sort();
        nets.dedup();
        let mut x = 40.0;
        let mut row = 0;
        for n in nets.iter().take(40) {
            let label = n.clone();
            let w = 26.0 + 7.0 * label.len() as f32;
            if x + w > W - 40.0 {
                x = 40.0;
                row += 1;
            }
            let yy = y + 18.0 + row as f32 * 20.0;
            let col = hex(draw::net_color(n));
            l.line(pos2(x, yy - 4.0), pos2(x + 18.0, yy - 4.0), &col, 4.0);
            l.text(pos2(x + 24.0, yy), &label, 11.5, TEXTO_SUAVE, 0, false);
            x += w;
        }
        if nets.is_empty() {
            l.text(
                pos2(40.0, y + 20.0),
                "Sin etiquetas: los colores son los de cada cable.",
                11.5,
                TEXTO_SUAVE,
                0,
                false,
            );
        }
    }

    // ---------- documento ----------
    let mm = cfg.ancho_mm.max(100.0);
    let alto_mm = mm * H / W;
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="{mm:.1}mm" height="{alto_mm:.1}mm" viewBox="0 0 {W:.2} {H:.2}">
<rect x="0" y="0" width="{W:.2}" height="{H:.2}" fill="{FONDO}"/>
{}
</svg>"#,
        l.s
    )
}
