use std::mem;
use std::os::raw::c_void;
use serde::Deserialize;

const STRIDE: usize = 6; // [x, y, z, r, g, b]

fn push_vertex(v: &mut Vec<f32>, x: f32, y: f32, z: f32, r: f32, g: f32, b: f32) {
    v.extend_from_slice(&[x, y, z, r, g, b]);
}

fn add_face(v: &mut Vec<f32>, p: [[f32; 3]; 4], shade: f32, r: f32, g: f32, b: f32) {
    for &i in &[0usize, 1, 2, 0, 2, 3] {
        push_vertex(v, p[i][0], p[i][1], p[i][2], r * shade, g * shade, b * shade);
    }
}

fn add_box(v: &mut Vec<f32>,
           x0: f32, y0: f32, z0: f32,
           x1: f32, y1: f32, z1: f32,
           r: f32, g: f32, b: f32) {
    add_face(v, [[x0,y1,z0],[x1,y1,z0],[x1,y1,z1],[x0,y1,z1]], 1.00, r, g, b); // top
    add_face(v, [[x0,y0,z1],[x1,y0,z1],[x1,y0,z0],[x0,y0,z0]], 0.50, r, g, b); // bottom
    add_face(v, [[x0,y0,z1],[x1,y0,z1],[x1,y1,z1],[x0,y1,z1]], 0.80, r, g, b); // front +Z
    add_face(v, [[x1,y0,z0],[x0,y0,z0],[x0,y1,z0],[x1,y1,z0]], 0.80, r, g, b); // back  -Z
    add_face(v, [[x0,y0,z0],[x0,y0,z1],[x0,y1,z1],[x0,y1,z0]], 0.65, r, g, b); // left  -X
    add_face(v, [[x1,y0,z1],[x1,y0,z0],[x1,y1,z0],[x1,y1,z1]], 0.65, r, g, b); // right +X
}

fn parse_hex_color(hex: &str) -> (f32, f32, f32) {
    let hex = hex.trim_start_matches('#');
    let n = u32::from_str_radix(hex, 16).unwrap_or(0xAA_AA_AA);
    let r = ((n >> 16) & 0xFF) as f32 / 255.0;
    let g = ((n >> 8)  & 0xFF) as f32 / 255.0;
    let b =  (n        & 0xFF) as f32 / 255.0;
    (r, g, b)
}

// ── Serde types for Bedrock .geo.json (format_version 1.12.0) ────────────────

#[derive(Deserialize)]
struct GeoFile {
    #[serde(rename = "minecraft:geometry")]
    geometry: Vec<GeoEntry>,
}

#[derive(Deserialize)]
struct GeoEntry {
    description: GeoDescription,
    #[serde(default)]
    bones: Vec<GeoBone>,
}

#[derive(Deserialize)]
struct GeoDescription {
    // Custom field — Blockbench ignores unknown fields in description
    #[serde(default = "default_scale")]
    item_scale: f32,
}

fn default_scale() -> f32 { 1.0 }

#[derive(Deserialize)]
struct GeoBone {
    #[serde(default)]
    name: String,
    #[serde(default)]
    pivot: [f32; 3],
    #[serde(default)]
    cubes: Vec<GeoCube>,
    // Custom field for solid color — Blockbench ignores it
    #[serde(default = "default_color")]
    item_color: String,
}

fn default_color() -> String { "#AAAAAA".to_string() }

// ── Raw bone data for animated models (per-bone, unscaled MC units) ──────────

pub struct GeoCubeData { pub origin: [f32; 3], pub size: [f32; 3] }

pub struct GeoBoneData {
    pub name:  String,
    pub pivot: [f32; 3],
    pub color: (f32, f32, f32),
    pub cubes: Vec<GeoCubeData>,
}

/// Parse a `.geo.json` into its bones (names, pivots, cubes, colours) without
/// flattening or building a VAO. Callers (e.g. the entity renderer) keep the
/// per-bone structure so individual limbs can be animated.
pub fn load_bones(path: &str) -> Result<Vec<GeoBoneData>, String> {
    let json = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let file: GeoFile = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    let entry = file.geometry.into_iter().next().ok_or("no geometry entry")?;
    Ok(entry.bones.into_iter().map(|b| GeoBoneData {
        name:  b.name,
        pivot: b.pivot,
        color: parse_hex_color(&b.item_color),
        cubes: b.cubes.into_iter().map(|c| GeoCubeData { origin: c.origin, size: c.size }).collect(),
    }).collect())
}

#[derive(Deserialize)]
struct GeoCube {
    origin: [f32; 3],
    size:   [f32; 3],
}

// ── Public type ───────────────────────────────────────────────────────────────

pub struct GeoModel {
    pub vao:        u32,
    pub vert_count: i32,
    vbo:            u32,
}

impl GeoModel {
    pub fn load(path: &str) -> Result<Self, String> {
        let json = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        Self::from_json(&json)
    }

    pub fn from_json(json: &str) -> Result<Self, String> {
        let file: GeoFile = serde_json::from_str(json).map_err(|e| e.to_string())?;
        let entry = file.geometry.into_iter().next().ok_or("no geometry entry")?;
        let scale = entry.description.item_scale;

        struct RawBox { x0: f32, y0: f32, z0: f32, x1: f32, y1: f32, z1: f32, r: f32, g: f32, b: f32 }
        let mut raw: Vec<RawBox> = Vec::new();

        for bone in &entry.bones {
            let (r, g, b) = parse_hex_color(&bone.item_color);
            for cube in &bone.cubes {
                let x0 = cube.origin[0] * scale;
                let y0 = cube.origin[1] * scale;
                let z0 = cube.origin[2] * scale;
                raw.push(RawBox {
                    x0, y0, z0,
                    x1: x0 + cube.size[0] * scale,
                    y1: y0 + cube.size[1] * scale,
                    z1: z0 + cube.size[2] * scale,
                    r, g, b,
                });
            }
        }

        if raw.is_empty() {
            return Err("no cubes in model".to_string());
        }

        let min_x = raw.iter().map(|b| b.x0).fold(f32::MAX, f32::min);
        let max_x = raw.iter().map(|b| b.x1).fold(f32::MIN, f32::max);
        let min_y = raw.iter().map(|b| b.y0).fold(f32::MAX, f32::min);
        let min_z = raw.iter().map(|b| b.z0).fold(f32::MAX, f32::min);
        let max_z = raw.iter().map(|b| b.z1).fold(f32::MIN, f32::max);

        // Center the model on XZ; set its lowest Y to 0
        let cx = (min_x + max_x) * 0.5;
        let cz = (min_z + max_z) * 0.5;

        let mut verts: Vec<f32> = Vec::new();
        for b in &raw {
            add_box(&mut verts,
                b.x0 - cx, b.y0 - min_y, b.z0 - cz,
                b.x1 - cx, b.y1 - min_y, b.z1 - cz,
                b.r, b.g, b.b);
        }

        let vert_count = (verts.len() / STRIDE) as i32;
        let (vao, vbo) = Self::upload_vao(&verts);
        Ok(GeoModel { vao, vbo, vert_count })
    }

    fn upload_vao(mesh: &[f32]) -> (u32, u32) {
        unsafe {
            let mut vao = 0u32;
            let mut vbo = 0u32;
            gl::GenVertexArrays(1, &mut vao);
            gl::GenBuffers(1, &mut vbo);
            gl::BindVertexArray(vao);
            gl::BindBuffer(gl::ARRAY_BUFFER, vbo);
            gl::BufferData(
                gl::ARRAY_BUFFER,
                (mesh.len() * mem::size_of::<f32>()) as isize,
                mesh.as_ptr() as *const c_void,
                gl::STATIC_DRAW,
            );
            let stride = (STRIDE * mem::size_of::<f32>()) as i32;
            gl::VertexAttribPointer(0, 3, gl::FLOAT, gl::FALSE, stride, std::ptr::null());
            gl::EnableVertexAttribArray(0);
            gl::VertexAttribPointer(1, 3, gl::FLOAT, gl::FALSE, stride,
                (3 * mem::size_of::<f32>()) as *const c_void);
            gl::EnableVertexAttribArray(1);
            gl::BindBuffer(gl::ARRAY_BUFFER, 0);
            gl::BindVertexArray(0);
            (vao, vbo)
        }
    }
}

impl Drop for GeoModel {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteVertexArrays(1, &self.vao);
            gl::DeleteBuffers(1, &self.vbo);
        }
    }
}
