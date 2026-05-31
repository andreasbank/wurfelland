use std::collections::HashMap;
use std::path::{Path, PathBuf};
use serde::Deserialize;
use crate::world::item::ItemType;

#[derive(Deserialize)]
struct GeoFile {
    #[serde(rename = "minecraft:geometry")]
    geometry: Vec<GeoEntry>,
}
#[derive(Deserialize)]
struct GeoEntry {
    description: GeoDesc,
}
#[derive(Deserialize)]
struct GeoDesc {
    #[serde(default)]
    item_id: String,
}

/// Runtime atlas built at game start from `assets/models/*/icon.png`.
/// Each model folder must contain a `*.geo.json` with `"item_id"` in its description.
/// Icons are packed into a 64×64 GPU texture (4×4 grid of 16×16 tiles).
/// If `icon.png` is missing or malformed the broken-icon tile from
/// `assets/textures/blocks_atlas.png` is used instead.
pub struct GeoAtlas {
    pub texture_id: u32,
    uvs: HashMap<ItemType, [f32; 4]>,
}

impl GeoAtlas {
    pub fn build(models_dir: &str) -> Self {
        const ICON: u32 = 16;
        const COLS: u32 = 4;
        const ROWS: u32 = 4;
        const AW:   u32 = COLS * ICON;
        const AH:   u32 = ROWS * ICON;

        let broken  = load_broken_icon();
        let mut pixels = vec![0u8; (AW * AH * 4) as usize];
        let mut uvs    = HashMap::new();
        let mut slot   = 0u32;

        let mut dirs: Vec<_> = match std::fs::read_dir(models_dir) {
            Ok(it) => it.filter_map(|e| e.ok()).filter(|e| e.path().is_dir()).collect(),
            Err(e) => { eprintln!("[geo_atlas] cannot open {models_dir}: {e}"); Vec::new() }
        };
        dirs.sort_by_key(|e| e.file_name());

        for dir in &dirs {
            let dir_path = dir.path();

            let item_id = match find_item_id(&dir_path) {
                Some(id) => id,
                None => continue,
            };
            let item = match ItemType::from_name(&item_id) {
                Some(t) => t,
                None => {
                    eprintln!("[geo_atlas] unknown item_id {item_id:?} in {dir_path:?}");
                    continue;
                }
            };

            if slot >= COLS * ROWS {
                eprintln!("[geo_atlas] atlas full — increase COLS/ROWS to add more geo items");
                break;
            }

            let icon_px = load_icon(&dir_path.join("icon.png"), ICON, &broken);

            let col = slot % COLS;
            let row = slot / COLS;
            let ox  = col * ICON;
            let oy  = row * ICON;
            for py in 0..ICON {
                for px in 0..ICON {
                    let src = (py * ICON + px) as usize * 4;
                    let dst = ((oy + py) * AW + (ox + px)) as usize * 4;
                    pixels[dst..dst+4].copy_from_slice(&icon_px[src..src+4]);
                }
            }

            let u0    =  ox         as f32 / AW as f32;
            let u1    = (ox + ICON) as f32 / AW as f32;
            let v_top =  oy         as f32 / AH as f32;
            let v_bot = (oy + ICON) as f32 / AH as f32;
            uvs.insert(item, [u0, v_bot, u1, v_top]);
            slot += 1;
        }

        let texture_id = unsafe {
            let mut id = 0u32;
            gl::GenTextures(1, &mut id);
            gl::BindTexture(gl::TEXTURE_2D, id);
            gl::TexImage2D(gl::TEXTURE_2D, 0, gl::RGBA as i32,
                AW as i32, AH as i32, 0,
                gl::RGBA, gl::UNSIGNED_BYTE, pixels.as_ptr() as *const _);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as i32);
            gl::BindTexture(gl::TEXTURE_2D, 0);
            id
        };

        GeoAtlas { texture_id, uvs }
    }

    /// Returns `[u0, v_bot, u1, v_top]` for the hotbar tex shader, or `None`.
    pub fn uv_for_item(&self, item: ItemType) -> Option<[f32; 4]> {
        self.uvs.get(&item).copied()
    }
}

fn find_item_id(dir: &Path) -> Option<String> {
    let geo_path: PathBuf = std::fs::read_dir(dir).ok()?.find_map(|e| {
        let e = e.ok()?;
        if e.file_name().to_string_lossy().ends_with(".geo.json") { Some(e.path()) } else { None }
    })?;
    let json = std::fs::read_to_string(&geo_path).ok()?;
    let geo: GeoFile = serde_json::from_str(&json).ok()?;
    let id = geo.geometry.into_iter().next()?.description.item_id;
    if id.is_empty() { None } else { Some(id) }
}

fn load_icon(path: &Path, size: u32, broken: &[u8]) -> Vec<u8> {
    match image::open(path) {
        Ok(img) => {
            let img = img.into_rgba8();
            if img.dimensions() == (size, size) {
                img.into_raw()
            } else {
                eprintln!("[geo_atlas] {path:?} is not {size}×{size} — using broken icon");
                broken.to_vec()
            }
        }
        Err(e) => {
            eprintln!("[geo_atlas] cannot load {path:?}: {e} — using broken icon");
            broken.to_vec()
        }
    }
}

fn load_broken_icon() -> Vec<u8> {
    // Read the missing-icon tile (purple/black checkerboard) from the block atlas.
    const TILE: u32 = 16;
    const TPR:  u32 = 16; // tiles per row in the 256×256 atlas
    let tile = crate::block_atlas_data::MISSING_ICON_TILE as u32;
    let ox = (tile % TPR) * TILE;
    let oy = (tile / TPR) * TILE;

    let path = Path::new("assets/textures/blocks_atlas.png");
    if let Ok(img) = image::open(path) {
        let img = img.into_rgba8();
        if img.width() >= ox + TILE && img.height() >= oy + TILE {
            let mut px = vec![0u8; (TILE * TILE * 4) as usize];
            for y in 0..TILE {
                for x in 0..TILE {
                    let src = img.get_pixel(ox + x, oy + y).0;
                    let idx = (y * TILE + x) as usize * 4;
                    px[idx..idx+4].copy_from_slice(&src);
                }
            }
            return px;
        }
    }
    // Inline fallback if the atlas file is missing: purple/black 2-pixel checkerboard
    let mut px = vec![0u8; 16 * 16 * 4];
    for y in 0..16u32 {
        for x in 0..16u32 {
            let idx = (y * 16 + x) as usize * 4;
            px[idx+3] = 255;
            if (x / 2 + y / 2) % 2 == 0 {
                px[idx]   = 148; // purple
                px[idx+2] = 211;
            }
        }
    }
    px
}

impl Drop for GeoAtlas {
    fn drop(&mut self) {
        unsafe { gl::DeleteTextures(1, &self.texture_id); }
    }
}
