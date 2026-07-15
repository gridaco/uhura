//! `gen-assets` — validate materialized demo assets and render legacy motifs.
//! Reads `fixtures/assets/manifest.toml`. Entries with a `source` are already
//! materialized: their dimensions and SHA-256 are verified without a network
//! request. Legacy entries with `motif` + `seed` remain reproducible locally.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn main() -> ExitCode {
    let root = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("examples/instagram/client"));
    let manifest_path = root.join("fixtures/assets/manifest.toml");
    let out_dir = root.join("fixtures/assets");

    let text = match std::fs::read_to_string(&manifest_path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("gen-assets: {}: {e}", manifest_path.display());
            return ExitCode::from(2);
        }
    };
    let table: toml::Table = match text.parse() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("gen-assets: manifest: {e}");
            return ExitCode::from(2);
        }
    };
    let Some(assets) = table.get("assets").and_then(toml::Value::as_table) else {
        eprintln!("gen-assets: manifest has no [assets] section");
        return ExitCode::from(2);
    };

    let mut preserved = 0usize;
    let mut written = 0usize;
    for (id, entry) in assets {
        let Some(entry) = entry.as_table() else {
            eprintln!("gen-assets: `{id}` is not a table");
            return ExitCode::from(2);
        };
        let get_str = |k: &str| entry.get(k).and_then(toml::Value::as_str);
        let get_int = |k: &str| entry.get(k).and_then(toml::Value::as_integer);
        let (Some(file), Some(alt), Some(size)) =
            (get_str("file"), get_str("alt"), get_int("size"))
        else {
            eprintln!("gen-assets: `{id}` needs file, alt, and size");
            return ExitCode::from(2);
        };
        if alt.trim().is_empty() {
            eprintln!("gen-assets: `{id}`: alt text is required (§8.3)");
            return ExitCode::from(2);
        }
        let size = size.clamp(16, 2048) as u32;

        if get_str("source").is_some() {
            let Some(expected_hash) = get_str("sha256") else {
                eprintln!("gen-assets: sourced asset `{id}` needs sha256");
                return ExitCode::from(2);
            };
            let path = out_dir.join(file);
            let bytes = match std::fs::read(&path) {
                Ok(bytes) => bytes,
                Err(error) => {
                    eprintln!("gen-assets: {}: {error}", path.display());
                    return ExitCode::from(2);
                }
            };
            let actual_hash = uhura_base::sha256_hex(&bytes);
            if actual_hash != expected_hash {
                eprintln!(
                    "gen-assets: `{id}`: sha256 mismatch: expected {expected_hash}, got {actual_hash}"
                );
                return ExitCode::from(2);
            }
            let image = match image::load_from_memory(&bytes) {
                Ok(image) => image,
                Err(error) => {
                    eprintln!("gen-assets: `{id}`: invalid image: {error}");
                    return ExitCode::from(2);
                }
            };
            if image.width() != size || image.height() != size {
                eprintln!(
                    "gen-assets: `{id}`: expected {size}x{size}, got {}x{}",
                    image.width(),
                    image.height()
                );
                return ExitCode::from(2);
            }
            preserved += 1;
            continue;
        }

        let (Some(motif), Some(seed)) = (get_str("motif"), get_int("seed")) else {
            eprintln!("gen-assets: `{id}` needs source + sha256 or motif + seed");
            return ExitCode::from(2);
        };
        let image = render_motif(motif, seed as u64, size);
        let Some(image) = image else {
            eprintln!("gen-assets: `{id}`: unknown motif `{motif}`");
            return ExitCode::from(2);
        };
        if let Err(e) = write_jpeg(&out_dir.join(file), &image) {
            eprintln!("gen-assets: {file}: {e}");
            return ExitCode::from(2);
        }
        written += 1;
    }
    println!(
        "gen-assets: preserved {preserved} sourced assets and wrote {written} generated JPEGs under {}",
        out_dir.display()
    );
    ExitCode::SUCCESS
}

fn write_jpeg(path: &Path, image: &image::RgbImage) -> std::io::Result<()> {
    let mut bytes = Vec::new();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 84);
    encoder
        .encode(
            image.as_raw(),
            image.width(),
            image.height(),
            image::ExtendedColorType::Rgb8,
        )
        .map_err(std::io::Error::other)?;
    std::fs::write(path, bytes)
}

// ── deterministic pseudo-randomness ─────────────────────────────────────

struct XorShift(u64);

impl XorShift {
    fn new(seed: u64) -> Self {
        XorShift(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15).max(1))
    }

    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    /// Uniform in [0, 1).
    fn unit(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }

    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.unit()
    }
}

/// Cheap value noise on a lattice (bilinear, seeded) — grain and blobs.
fn noise(seed: u64, x: f32, y: f32) -> f32 {
    fn hash(seed: u64, ix: i64, iy: i64) -> f32 {
        let mut h = seed ^ (ix as u64).wrapping_mul(0x517c_c1b7_2722_0a95);
        h ^= (iy as u64).wrapping_mul(0x2545_f491_4f6c_dd1d);
        h ^= h >> 33;
        h = h.wrapping_mul(0xff51_afd7_ed55_8ccd);
        h ^= h >> 33;
        (h >> 40) as f32 / (1u64 << 24) as f32
    }
    let (ix, iy) = (x.floor() as i64, y.floor() as i64);
    let (fx, fy) = (x - x.floor(), y - y.floor());
    let (sx, sy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let top = hash(seed, ix, iy) * (1.0 - sx) + hash(seed, ix + 1, iy) * sx;
    let bottom = hash(seed, ix, iy + 1) * (1.0 - sx) + hash(seed, ix + 1, iy + 1) * sx;
    top * (1.0 - sy) + bottom * sy
}

// ── color helpers ───────────────────────────────────────────────────────

#[derive(Clone, Copy)]
struct Rgb(f32, f32, f32);

impl Rgb {
    fn mix(self, other: Rgb, t: f32) -> Rgb {
        let t = t.clamp(0.0, 1.0);
        Rgb(
            self.0 + (other.0 - self.0) * t,
            self.1 + (other.1 - self.1) * t,
            self.2 + (other.2 - self.2) * t,
        )
    }

    fn scale(self, k: f32) -> Rgb {
        Rgb(self.0 * k, self.1 * k, self.2 * k)
    }

    fn pixel(self) -> image::Rgb<u8> {
        image::Rgb([
            (self.0.clamp(0.0, 1.0) * 255.0) as u8,
            (self.1.clamp(0.0, 1.0) * 255.0) as u8,
            (self.2.clamp(0.0, 1.0) * 255.0) as u8,
        ])
    }
}

fn hsl(h: f32, s: f32, l: f32) -> Rgb {
    let h = h.rem_euclid(360.0) / 60.0;
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - (h.rem_euclid(2.0) - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    Rgb(r + m, g + m, b + m)
}

// ── motifs ──────────────────────────────────────────────────────────────

fn render_motif(motif: &str, seed: u64, size: u32) -> Option<image::RgbImage> {
    let render: fn(&mut XorShift, u64, f32, f32) -> Rgb = match motif {
        "duotone" => duotone,
        "glaze" => glaze,
        "wave" => wave,
        "aurora" => aurora,
        "crumb" => crumb,
        "field" => field,
        _ => return None,
    };
    // Each pixel re-derives the motif constants from a fresh RNG seeded
    // identically, so a pixel is a pure function of (seed, u, v).
    let image = image::RgbImage::from_fn(size, size, |px, py| {
        let u = px as f32 / size as f32;
        let v = py as f32 / size as f32;
        let mut rng = XorShift::new(seed);
        render(&mut rng, seed, u, v).pixel()
    });
    Some(image)
}

/// Avatars: a two-tone diagonal split with an offset disc — a distinctive,
/// initial-free mark per seed.
fn duotone(rng: &mut XorShift, seed: u64, u: f32, v: f32) -> Rgb {
    let hue = rng.range(0.0, 360.0);
    let a = hsl(hue, 0.45, 0.42);
    let b = hsl(hue + rng.range(120.0, 200.0), 0.40, 0.68);
    let angle = rng.range(-0.6, 0.6);
    let split = u + angle * (v - 0.5) - 0.5;
    let base = if split < 0.0 { a } else { b };
    let (cx, cy, r) = (
        rng.range(0.30, 0.70),
        rng.range(0.30, 0.70),
        rng.range(0.18, 0.30),
    );
    let d = ((u - cx).powi(2) + (v - cy).powi(2)).sqrt();
    if d < r {
        if split < 0.0 { b } else { a }
    } else {
        let grain = noise(seed, u * 24.0, v * 24.0);
        base.scale(0.96 + grain * 0.08)
    }
}

/// Glaze test tiles: a grid with per-tile hue jitter and kiln speckle.
fn glaze(rng: &mut XorShift, seed: u64, u: f32, v: f32) -> Rgb {
    let hue = rng.range(0.0, 40.0) - 10.0; // copper/terracotta band
    let tiles = 4.0;
    let (tu, tv) = (u * tiles, v * tiles);
    let (col, row) = (tu.floor(), tv.floor());
    let (fu, fv) = (tu - col, tv - row);
    let tile_jitter = noise(seed ^ 0x51ce, col * 7.0 + 0.5, row * 7.0 + 0.5);
    let l = 0.34 + tile_jitter * 0.28;
    let mut color = hsl(hue + tile_jitter * 18.0, 0.52, l);
    // grout lines
    let edge = fu.min(1.0 - fu).min(fv).min(1.0 - fv);
    if edge < 0.045 {
        color = hsl(30.0, 0.12, 0.82); // maple bench grout
    } else {
        let speckle = noise(seed ^ 0xdead, u * 180.0, v * 180.0);
        if speckle > 0.82 {
            color = color.scale(0.55);
        }
        let sheen = noise(seed ^ 0xbeef, u * 6.0, v * 6.0);
        color = color.scale(0.92 + sheen * 0.16);
    }
    color
}

/// Sea and dusk: layered sine swells with a bright horizon band.
fn wave(rng: &mut XorShift, seed: u64, u: f32, v: f32) -> Rgb {
    let hue = rng.range(185.0, 225.0);
    let horizon = rng.range(0.28, 0.42);
    let sky = hsl(hue + 15.0, 0.35, 0.82);
    let sun = hsl(35.0, 0.75, 0.72);
    let sea_light = hsl(hue, 0.50, 0.46);
    let sea_dark = hsl(hue + 8.0, 0.55, 0.22);
    if v < horizon {
        let glow = 1.0 - (v / horizon);
        return sky.mix(sun, (glow * 0.5).powi(2));
    }
    let depth = (v - horizon) / (1.0 - horizon);
    let swell = ((u * 9.0 + depth * 4.0 + seed as f32 % 7.0).sin() * 0.5 + 0.5)
        * ((u * 23.0 - depth * 9.0).sin() * 0.5 + 0.5);
    let foam = noise(seed, u * 60.0, v * 60.0);
    let mut color = sea_dark.mix(sea_light, swell * (1.0 - depth * 0.6));
    if foam > 0.86 && swell > 0.55 {
        color = color.mix(Rgb(0.95, 0.97, 0.98), 0.7);
    }
    color
}

/// Night sky: dark gradient, green curtain ribbons, star pricks.
fn aurora(rng: &mut XorShift, seed: u64, u: f32, v: f32) -> Rgb {
    let night = Rgb(0.03, 0.05, 0.10).mix(Rgb(0.07, 0.09, 0.16), v);
    let drift = rng.range(0.0, std::f32::consts::TAU);
    let mut glow = 0.0f32;
    for band in 0..3 {
        let b = band as f32;
        let center = 0.25 + 0.18 * b + 0.12 * ((u * (2.0 + b) * 3.0 + drift).sin());
        let d = (v - center).abs();
        glow += (0.045 / (d + 0.02)).min(1.4) * (0.5 + 0.5 * ((u * 9.0 + b * 2.0).sin()));
    }
    let curtain = hsl(140.0, 0.85, 0.45)
        .scale(glow.min(1.6) * 0.55)
        .mix(hsl(280.0, 0.5, 0.4).scale(glow.min(1.0) * 0.2), 0.25);
    let star = noise(seed, u * 240.0, v * 240.0);
    let mut color = Rgb(
        night.0 + curtain.0,
        night.1 + curtain.1,
        night.2 + curtain.2,
    );
    if star > 0.985 && v < 0.7 {
        color = color.mix(Rgb(1.0, 1.0, 0.95), 0.8);
    }
    // fjord silhouette
    let ridge = 0.82 + 0.06 * ((u * 5.0 + drift).sin());
    if v > ridge {
        color = Rgb(0.01, 0.02, 0.04);
    }
    color
}

/// Bread and clay: a warm base with thresholded blob holes.
fn crumb(rng: &mut XorShift, seed: u64, u: f32, v: f32) -> Rgb {
    let hue = rng.range(28.0, 44.0);
    let base = hsl(hue, 0.42, 0.72);
    let crust = hsl(hue - 6.0, 0.55, 0.38);
    let cell =
        noise(seed, u * 14.0, v * 14.0) * 0.65 + noise(seed ^ 0x77, u * 40.0, v * 40.0) * 0.35;
    let mut color = if cell < 0.36 {
        base.scale(0.55 + cell * 0.5) // open crumb holes
    } else {
        base.scale(0.9 + noise(seed ^ 0x99, u * 90.0, v * 90.0) * 0.18)
    };
    // crust vignette
    let edge = (u - 0.5).abs().max((v - 0.5).abs()) * 2.0;
    if edge > 0.82 {
        color = color.mix(crust, ((edge - 0.82) / 0.18).powi(2));
    }
    color
}

/// Soft editorial blocks: 3–4 muted color fields with grain.
fn field(rng: &mut XorShift, seed: u64, u: f32, v: f32) -> Rgb {
    let hue = rng.range(0.0, 360.0);
    let vertical = rng.unit() > 0.5;
    let t = if vertical { u } else { v };
    let bands = 3 + (seed % 2) as usize;
    let idx = ((t * bands as f32) as usize).min(bands - 1);
    let l = 0.38 + 0.14 * idx as f32;
    let s = 0.22 + 0.06 * ((idx * 7) % 3) as f32;
    let color = hsl(hue + idx as f32 * 24.0, s, l);
    let grain = noise(seed, u * 120.0, v * 120.0);
    let soft = noise(seed ^ 0x42, u * 3.0, v * 3.0);
    color.scale(0.93 + grain * 0.07 + soft * 0.06)
}
