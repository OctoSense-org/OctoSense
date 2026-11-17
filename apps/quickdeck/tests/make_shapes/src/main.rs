//! Draws every preset shape of the deck engine as a small line picture, for
//! Quick Deck's shape gallery: `make-shapes <out dir>`. What it writes ships, in
//! apps/quickdeck/bundle/shapes/; the bundle never runs this.
use deckcraft_geom::preset;
use kurbo::{BezPath, PathEl, Point};
use std::fmt::Write as _;
use std::{fs, path::PathBuf};

const VIEW: f64 = 24.0;
const ART: f64 = 20.0;

fn num(v: f64) -> String {
    let s = format!("{:.2}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" || s.is_empty() { "0".to_string() } else { s.to_string() }
}

fn point(out: &mut String, p: Point, dx: f64, dy: f64, k: f64) {
    let _ = write!(out, "{} {} ", num(p.x * k + dx), num(p.y * k + dy));
}

fn d_of(path: &BezPath, dx: f64, dy: f64, k: f64) -> String {
    let mut d = String::new();
    for el in path.elements() {
        match *el {
            PathEl::MoveTo(p) => { d.push('M'); point(&mut d, p, dx, dy, k); }
            PathEl::LineTo(p) => { d.push('L'); point(&mut d, p, dx, dy, k); }
            PathEl::QuadTo(a, p) => { d.push('Q'); point(&mut d, a, dx, dy, k); point(&mut d, p, dx, dy, k); }
            PathEl::CurveTo(a, b, p) => { d.push('C'); point(&mut d, a, dx, dy, k); point(&mut d, b, dx, dy, k); point(&mut d, p, dx, dy, k); }
            PathEl::ClosePath => d.push('Z'),
        }
    }
    d.trim_end().to_string()
}

fn main() {
    let out = PathBuf::from(std::env::args().nth(1).expect("an out dir"));
    fs::create_dir_all(&out).unwrap();
    let mut made = 0;
    let mut missing = vec![];
    for info in preset::CATALOG {
        let Some(geo) = preset::build(info.name, ART, ART, &[]) else {
            missing.push(info.name);
            continue;
        };
        let stroked: Vec<_> = geo.paths.iter().filter(|p| p.stroke).collect();
        let paths: Vec<_> = if stroked.is_empty() { geo.paths.iter().collect() } else { stroked };
        // A shape can reach past its box (callouts' tails): fit what it draws.
        let mut bounds: Option<kurbo::Rect> = None;
        for p in &paths {
            let b = kurbo::Shape::bounding_box(&p.path);
            bounds = Some(match bounds { Some(all) => all.union(b), None => b });
        }
        let b = bounds.unwrap_or(kurbo::Rect::new(0.0, 0.0, ART, ART));
        let k = (ART / b.width().max(0.01)).min(ART / b.height().max(0.01)).min(1.0);
        let dx = (VIEW - b.width() * k) / 2.0 - b.x0 * k;
        let dy = (VIEW - b.height() * k) / 2.0 - b.y0 * k;
        let mut svg = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {VIEW} {VIEW}\" fill=\"none\" stroke=\"#000\" stroke-width=\"1.3\" stroke-linecap=\"round\" stroke-linejoin=\"round\">"
        );
        for p in &paths {
            let _ = write!(svg, "<path d=\"{}\"/>", d_of(&p.path, dx, dy, k));
        }
        svg.push_str("</svg>\n");
        fs::write(out.join(format!("{}.svg", info.name)), svg).unwrap();
        made += 1;
    }
    println!("made {made} pictures; no geometry for {missing:?}");
}
