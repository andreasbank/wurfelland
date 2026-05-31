//! Generates assets for the game:
//!   • `assets/textures/blocks_atlas.png` — 256×256 block face atlas (edit this in an image editor)
//!   • `assets/textures/blocks_atlas.png` (broken-icon fallback) — already included above
//!   • `assets/models/*/icon.png` — 16×16 isometric icons for each geo model
//! Run once after creating or editing geo.json files, or to regenerate the initial block atlas:
//!     cargo run --bin gen_icons

#[path = "../block_atlas_data.rs"]
mod block_atlas_data;

use serde::Deserialize;
use std::path::PathBuf;

// ── Geo JSON types (mirrors geo_model.rs) ────────────────────────────────────

#[derive(Deserialize)]
struct GeoFile {
    #[serde(rename = "minecraft:geometry")]
    geometry: Vec<GeoEntry>,
}
#[derive(Deserialize)]
struct GeoEntry {
    description: GeoDesc,
    #[serde(default)]
    bones: Vec<GeoBone>,
}
#[derive(Deserialize)]
struct GeoDesc {
    #[serde(default = "one")]
    item_scale: f32,
}
fn one() -> f32 { 1.0 }
#[derive(Deserialize)]
struct GeoBone {
    #[serde(default)]
    cubes: Vec<GeoCube>,
    #[serde(default = "grey")]
    item_color: String,
}
fn grey() -> String { "#AAAAAA".to_string() }
#[derive(Deserialize)]
struct GeoCube {
    origin: [f32; 3],
    size:   [f32; 3],
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn hex_rgb(s: &str) -> [f32; 3] {
    let s = s.trim_start_matches('#');
    let n = u32::from_str_radix(s, 16).unwrap_or(0xAA_AA_AA);
    [((n >> 16) & 0xFF) as f32 / 255.0,
     ((n >>  8) & 0xFF) as f32 / 255.0,
     ( n        & 0xFF) as f32 / 255.0]
}

// ── Box type shared between main and renderer ─────────────────────────────────

struct B { x0:f32,y0:f32,z0:f32, x1:f32,y1:f32,z1:f32, r:f32,g:f32,b:f32 }

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() {
    gen_block_atlas();

    let models_dir = PathBuf::from("assets/models");
    let mut dirs: Vec<_> = std::fs::read_dir(&models_dir)
        .expect("assets/models not found — run from the project root")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .collect();
    dirs.sort_by_key(|e| e.file_name());

    for entry in dirs {
        let dir = entry.path();

        // Find the first *.geo.json in this subdirectory
        let Some(geo_path) = std::fs::read_dir(&dir).ok().and_then(|mut it| {
            it.find(|e| e.as_ref()
                .map_or(false, |e| e.file_name().to_string_lossy().ends_with(".geo.json")))
                .and_then(|e| e.ok().map(|e| e.path()))
        }) else { continue };

        let json = match std::fs::read_to_string(&geo_path) {
            Ok(s)  => s,
            Err(e) => { eprintln!("skip {geo_path:?}: {e}"); continue }
        };
        let geo: GeoFile = match serde_json::from_str(&json) {
            Ok(f)  => f,
            Err(e) => { eprintln!("bad json {geo_path:?}: {e}"); continue }
        };
        let entry = match geo.geometry.into_iter().next() {
            Some(e) => e,
            None    => continue,
        };
        let scale = entry.description.item_scale;

        let mut boxes: Vec<B> = Vec::new();
        for bone in entry.bones {
            let [r, g, b] = hex_rgb(&bone.item_color);
            for cube in bone.cubes {
                let (ox, oy, oz) = (cube.origin[0]*scale, cube.origin[1]*scale, cube.origin[2]*scale);
                boxes.push(B {
                    x0: ox,  y0: oy,  z0: oz,
                    x1: ox + cube.size[0]*scale,
                    y1: oy + cube.size[1]*scale,
                    z1: oz + cube.size[2]*scale,
                    r, g, b,
                });
            }
        }
        if boxes.is_empty() { continue; }

        // Center on XZ, set min-Y to 0 (mirrors geo_model.rs)
        let min_x = boxes.iter().map(|b| b.x0).fold(f32::MAX, f32::min);
        let max_x = boxes.iter().map(|b| b.x1).fold(f32::MIN, f32::max);
        let min_y = boxes.iter().map(|b| b.y0).fold(f32::MAX, f32::min);
        let min_z = boxes.iter().map(|b| b.z0).fold(f32::MAX, f32::min);
        let max_z = boxes.iter().map(|b| b.z1).fold(f32::MIN, f32::max);
        let cx = (min_x + max_x) * 0.5;
        let cz = (min_z + max_z) * 0.5;
        for b in &mut boxes {
            b.x0 -= cx; b.x1 -= cx;
            b.y0 -= min_y; b.y1 -= min_y;
            b.z0 -= cz; b.z1 -= cz;
        }

        let pixels = render_icon(&boxes);
        let img    = image::RgbaImage::from_raw(16, 16, pixels).unwrap();
        let out    = dir.join("icon.png");
        img.save(&out).unwrap_or_else(|e| panic!("failed to save {out:?}: {e}"));
        println!("wrote {}", out.display());
    }
}

// ── Block atlas ───────────────────────────────────────────────────────────────

fn gen_block_atlas() {
    let dir = PathBuf::from("assets/textures");
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("blocks_atlas.png");
    let pixels = block_atlas_data::build_block_atlas_pixels();
    let img = image::RgbaImage::from_raw(256, 256, pixels).unwrap();
    img.save(&out).unwrap_or_else(|e| panic!("failed to save {out:?}: {e}"));
    println!("wrote {}", out.display());
}

// ── Software rasteriser ───────────────────────────────────────────────────────

fn render_icon(boxes: &[B]) -> Vec<u8> {
    // Camera: looks along +Z after world rotation.
    // Ry(yaw) then Rx(pitch) applied to world points.
    let yaw   = (-25.0_f32).to_radians(); // negative → right face visible
    let pitch =  20.0_f32.to_radians();   // positive → top face visible
    let (sy, cy) = yaw.sin_cos();
    let (sp, cp) = pitch.sin_cos();

    let rot = |x: f32, y: f32, z: f32| -> [f32; 3] {
        // Ry(yaw)
        let (x1, y1, z1) = (x*cy + z*sy,  y,  -x*sy + z*cy);
        // Rx(pitch)
        [x1,  y1*cp - z1*sp,  y1*sp + z1*cp]
    };

    struct Face { pts: [[f32; 2]; 4], depth: f32, color: [u8; 3] }
    let mut faces: Vec<Face> = Vec::new();

    for b in boxes {
        let (x0,y0,z0,x1,y1,z1) = (b.x0,b.y0,b.z0,b.x1,b.y1,b.z1);
        // (outward normal, shading factor, 4 CCW corners viewed from outside)
        let defs: &[([f32;3], f32, [[f32;3];4])] = &[
            ([0., 1., 0.], 1.00, [[x0,y1,z0],[x1,y1,z0],[x1,y1,z1],[x0,y1,z1]]), // top
            ([0.,-1., 0.], 0.50, [[x0,y0,z1],[x1,y0,z1],[x1,y0,z0],[x0,y0,z0]]), // bottom
            ([0., 0., 1.], 0.80, [[x0,y0,z1],[x1,y0,z1],[x1,y1,z1],[x0,y1,z1]]), // front +Z
            ([0., 0.,-1.], 0.80, [[x1,y0,z0],[x0,y0,z0],[x0,y1,z0],[x1,y1,z0]]), // back  -Z
            ([-1.,0., 0.], 0.65, [[x0,y0,z0],[x0,y0,z1],[x0,y1,z1],[x0,y1,z0]]), // left  -X
            ([1., 0., 0.], 0.65, [[x1,y0,z1],[x1,y0,z0],[x1,y1,z0],[x1,y1,z1]]), // right +X
        ];
        for (normal, shade, corners) in defs {
            let rn = rot(normal[0], normal[1], normal[2]);
            if rn[2] <= 0.0 { continue; } // back-facing

            let mut pts = [[0.0f32; 2]; 4];
            let mut depth_sum = 0.0f32;
            for (i, &c) in corners.iter().enumerate() {
                let r = rot(c[0], c[1], c[2]);
                pts[i] = [r[0], r[1]];
                depth_sum += r[2];
            }
            faces.push(Face {
                pts,
                depth: depth_sum / 4.0,
                color: [
                    (b.r * shade * 255.0).clamp(0., 255.) as u8,
                    (b.g * shade * 255.0).clamp(0., 255.) as u8,
                    (b.b * shade * 255.0).clamp(0., 255.) as u8,
                ],
            });
        }
    }

    // Painter's algorithm: ascending depth = far-to-near
    faces.sort_by(|a, b| a.depth.partial_cmp(&b.depth).unwrap());

    // 2D bounding box → scale to fit [PAD, 16-PAD]²
    let (mut mn_x, mut mx_x, mut mn_y, mut mx_y) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
    for f in &faces {
        for &[x, y] in &f.pts {
            mn_x = mn_x.min(x); mx_x = mx_x.max(x);
            mn_y = mn_y.min(y); mx_y = mx_y.max(y);
        }
    }
    let range  = (mx_x - mn_x).max(mx_y - mn_y).max(1e-6);
    let c_sx   = (mn_x + mx_x) * 0.5;
    let c_sy   = (mn_y + mx_y) * 0.5;
    const PAD: f32 = 1.5;
    let s = (16.0 - PAD * 2.0) / range;
    // Project to screen: X right, Y up→down (flip)
    let to_px = |sx: f32, sy: f32| -> [f32; 2] {
        [(sx - c_sx) * s + 8.0,
         -(sy - c_sy) * s + 8.0]
    };

    let mut canvas = vec![0u8; 16 * 16 * 4];
    for face in &faces {
        let spx: [[f32; 2]; 4] = face.pts.map(|[x, y]| to_px(x, y));

        let bx0 = spx.iter().map(|p| p[0]).fold(f32::MAX, f32::min).floor().max(0.0) as u32;
        let bx1 = spx.iter().map(|p| p[0]).fold(f32::MIN, f32::max).ceil().min(15.0) as u32;
        let by0 = spx.iter().map(|p| p[1]).fold(f32::MAX, f32::min).floor().max(0.0) as u32;
        let by1 = spx.iter().map(|p| p[1]).fold(f32::MIN, f32::max).ceil().min(15.0) as u32;

        // Detect winding (CCW → area > 0, CW → area < 0) for edge test sign
        let area: f32 = (0..4).map(|i| {
            let a = spx[i]; let b = spx[(i+1)%4];
            a[0]*b[1] - b[0]*a[1]
        }).sum::<f32>();
        let sign = if area >= 0.0 { 1.0f32 } else { -1.0 };

        for py in by0..=by1 {
            for px in bx0..=bx1 {
                let p = [px as f32 + 0.5, py as f32 + 0.5];
                let inside = (0..4).all(|i| {
                    let a = spx[i]; let b = spx[(i+1)%4];
                    ((b[0]-a[0])*(p[1]-a[1]) - (b[1]-a[1])*(p[0]-a[0])) * sign >= -0.01
                });
                if inside {
                    let idx = (py as usize * 16 + px as usize) * 4;
                    canvas[idx..idx+3].copy_from_slice(&face.color);
                    canvas[idx+3] = 255;
                }
            }
        }
    }
    canvas
}
