//! Trazado ordenado de cables: tramos rectos (horizontales y verticales) por los pasillos
//! entre orificios, esquivando el cuerpo de las piezas y separándose de otros cables.
use crate::model::*;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashSet};

const STEP: f32 = 0.5;

/// Posición real de un orificio (con la placa movida).
pub fn pos(c: &Circuito, h: Hole) -> (f32, f32) {
    let (x, y) = hole_xy(h);
    let o = c
        .posiciones
        .get(h.board() as usize - 1)
        .copied()
        .unwrap_or([0.0, 0.0]);
    (x + o[0], y + o[1])
}

struct Grid {
    x0: f32,
    y0: f32,
    w: usize,
    h: usize,
    blocked: Vec<bool>,
    extra: Vec<f32>,
}

impl Grid {
    fn cell(&self, x: f32, y: f32) -> Option<usize> {
        let i = ((x - self.x0) / STEP).round();
        let j = ((y - self.y0) / STEP).round();
        (i >= 0.0 && j >= 0.0 && (i as usize) < self.w && (j as usize) < self.h)
            .then(|| j as usize * self.w + i as usize)
    }
    fn xy(&self, k: usize) -> (f32, f32) {
        (
            self.x0 + (k % self.w) as f32 * STEP,
            self.y0 + (k / self.w) as f32 * STEP,
        )
    }
    fn mark(&mut self, a: (f32, f32), b: (f32, f32), f: impl Fn(&mut Self, usize)) {
        let (mut y, y1) = (a.1.min(b.1), a.1.max(b.1));
        y = self.y0 + ((y - self.y0) / STEP).ceil() * STEP;
        while y <= y1 + 1e-4 {
            let mut x = self.x0 + ((a.0.min(b.0) - self.x0) / STEP).ceil() * STEP;
            while x <= a.0.max(b.0) + 1e-4 {
                if let Some(k) = self.cell(x, y) {
                    f(self, k);
                }
                x += STEP;
            }
            y += STEP;
        }
    }
}

/// Traza con tramos rectos los cables `ids` (en ese orden) y guarda sus puntos de quiebre.
/// Si un cable no encuentra camino libre, queda con su curva automática.
pub fn ordenar(c: &mut Circuito, ids: &[String]) {
    ordenar_con(c, ids, 4.0);
}

/// `sep` es cuánto penaliza reutilizar un pasillo: 2 compacto, 4 normal, 8 amplio.
/// Devuelve cuántos cables encontraron ruta.
pub fn ordenar_con(c: &mut Circuito, ids: &[String], sep: f32) -> usize {
    let mut g = {
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for b in 1..=c.tableros.max(1) {
            let o = c
                .posiciones
                .get(b as usize - 1)
                .copied()
                .unwrap_or([0.0, 0.0]);
            let bx = (b - 1) as f32 * BOARD_OFFSET + o[0];
            x0 = x0.min(bx - 1.0);
            x1 = x1.max(bx + board_width(c.columnas) + 1.0);
            y0 = y0.min(o[1] - 1.0);
            y1 = y1.max(o[1] + 20.0);
        }
        let (w, h) = (
            ((x1 - x0) / STEP) as usize + 1,
            ((y1 - y0) / STEP) as usize + 1,
        );
        Grid {
            x0,
            y0,
            w,
            h,
            blocked: vec![false; w * h],
            extra: vec![0.0; w * h],
        }
    };
    let rutear: HashSet<&String> = ids.iter().collect();
    let block = |g: &mut Grid, k: usize| g.blocked[k] = true;
    let wire_cost = |g: &mut Grid, k: usize| g.extra[k] += sep;
    for x in &c.componentes {
        let Ok(hs) = x.holes() else { continue };
        let ps: Vec<(f32, f32)> = hs.iter().map(|h| pos(c, *h)).collect();
        for p in &ps {
            g.mark(*p, *p, block); // nunca pasar por encima de una pata o punta ocupada
        }
        let (mx0, my0, mx1, my1) = ps
            .iter()
            .fold((f32::MAX, f32::MAX, f32::MIN, f32::MIN), |a, p| {
                (a.0.min(p.0), a.1.min(p.1), a.2.max(p.0), a.3.max(p.1))
            });
        match x.tipo {
            Tipo::Chip => g.mark((mx0 - 0.6, my0 - 1.15), (mx1 + 0.6, my1 + 1.15), block),
            Tipo::Display => g.mark((mx0 - 0.9, my0 - 2.9), (mx1 + 0.9, my1 - 0.3), block),
            Tipo::Resistencia | Tipo::Interruptor => {
                g.mark((mx0 - 0.45, my0 - 0.45), (mx1 + 0.45, my1 + 0.45), block)
            }
            Tipo::Led => g.mark((mx0 - 0.45, my0 - 1.2), (mx1 + 0.45, my1 + 0.45), block),
            Tipo::Cable if !rutear.contains(&x.id) && !x.puntos.is_empty() => {
                let pts: Vec<(f32, f32)> = std::iter::once(ps[0])
                    .chain(x.puntos.iter().map(|q| (q[0], q[1])))
                    .chain(std::iter::once(ps[1]))
                    .collect();
                for w in pts.windows(2) {
                    g.mark(w[0], w[1], wire_cost);
                }
            }
            _ => {}
        }
    }
    let mut ok = 0;
    for id in ids {
        let Some(i) = c
            .componentes
            .iter()
            .position(|x| &x.id == id && x.tipo == Tipo::Cable)
        else {
            continue;
        };
        let Ok(hs) = c.componentes[i].holes() else {
            continue;
        };
        let (a, b) = (pos(c, hs[0]), pos(c, hs[1]));
        let path = match (g.cell(a.0, a.1), g.cell(b.0, b.1)) {
            (Some(s), Some(t)) => astar(&g, s, t),
            _ => None,
        };
        let Some(path) = path else {
            c.componentes[i].puntos.clear();
            continue;
        };
        ok += 1;
        for k in &path {
            g.extra[*k] += sep; // los siguientes cables buscan otro pasillo
        }
        let mut puntos = Vec::new();
        for w in path.windows(3) {
            let d1 = w[1] as i64 - w[0] as i64;
            let d2 = w[2] as i64 - w[1] as i64;
            if d1 != d2 {
                let (x, y) = g.xy(w[1]);
                puntos.push([x, y]);
            }
        }
        if puntos.is_empty() {
            // tramo único: un punto intermedio lo marca como recto (sin puntos se dibuja curvo)
            puntos.push(if a.0 != b.0 && a.1 != b.1 {
                [b.0, a.1]
            } else {
                [(a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0]
            });
        }
        c.componentes[i].puntos = puntos;
    }
    ok
}

fn astar(g: &Grid, s: usize, t: usize) -> Option<Vec<usize>> {
    let n = g.w * g.h;
    let (tx, ty) = ((t % g.w) as i64, (t / g.w) as i64);
    let near = |k: usize, e: usize| {
        ((k % g.w) as i64 - (e % g.w) as i64).abs() + ((k / g.w) as i64 - (e / g.w) as i64).abs()
            <= 1
    };
    // estado = celda * 4 + dirección de llegada
    let mut best = vec![f32::MAX; n * 4];
    let mut prev = vec![usize::MAX; n * 4];
    let mut heap = BinaryHeap::new();
    let key = |f: f32| Reverse((f * 100.0) as i64);
    for d in 0..4 {
        best[s * 4 + d] = 0.0;
        heap.push((key(0.0), s * 4 + d));
    }
    const D: [(i64, i64); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];
    while let Some((_, st)) = heap.pop() {
        let (k, d) = (st / 4, st % 4);
        if k == t {
            let mut out = vec![k];
            let mut cur = st;
            while prev[cur] != usize::MAX {
                cur = prev[cur];
                out.push(cur / 4);
            }
            out.reverse();
            return Some(out);
        }
        let (x, y) = ((k % g.w) as i64, (k / g.w) as i64);
        for (nd, (dx, dy)) in D.iter().enumerate() {
            let (nx, ny) = (x + dx, y + dy);
            if nx < 0 || ny < 0 || nx >= g.w as i64 || ny >= g.h as i64 {
                continue;
            }
            let nk = ny as usize * g.w + nx as usize;
            if g.blocked[nk] && nk != t && !near(nk, s) && !near(nk, t) {
                continue;
            }
            let (px, py) = g.xy(nk);
            let over_hole = px.fract() == 0.0 && py.fract() == 0.0 && nk != t;
            let cost = best[st]
                + 1.0
                + if nd != d && k != s { 3.0 } else { 0.0 }
                + if over_hole { 0.8 } else { 0.0 }
                + g.extra[nk];
            let ns = nk * 4 + nd;
            if cost < best[ns] {
                best[ns] = cost;
                prev[ns] = st;
                let h = ((nx - tx).abs() + (ny - ty).abs()) as f32;
                heap.push((key(cost + h), ns));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cable_recto_esquiva_resistencia() {
        let mut c = ejemplos()[0].clone(); // R1 de c5 a c9
        let mut w = Comp::nuevo("W9", Tipo::Cable, &["c3", "c12"]);
        w.color = Some("azul".into());
        c.componentes.push(w);
        ordenar(&mut c, &["W9".into()]);
        let p = &c.find("W9").unwrap().puntos;
        assert!(p.len() >= 2, "debe rodear la resistencia: {p:?}");
        // todos los tramos son horizontales o verticales
        let (a, b) = (
            pos(&c, Hole::parse("c3").unwrap()),
            pos(&c, Hole::parse("c12").unwrap()),
        );
        let pts: Vec<(f32, f32)> = std::iter::once(a)
            .chain(p.iter().map(|q| (q[0], q[1])))
            .chain(std::iter::once(b))
            .collect();
        assert!(
            pts.windows(2).all(|w| w[0].0 == w[1].0 || w[0].1 == w[1].1),
            "{pts:?}"
        );
    }
}
