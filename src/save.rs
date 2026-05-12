use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::world::BlockType;

// ── Per-entity snapshot ────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone)]
pub struct EntitySave {
    pub position: [f32; 3],
    pub yaw:      f32,
    pub health:   f32,
}

// ── Per-item-entity snapshot ───────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone)]
pub struct ItemSave {
    pub position: [f32; 3],
    pub item_id:  u8,   // ItemType::tile_index()
}

// ── Inventory slot snapshot ────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone)]
pub struct InventorySlotSave {
    pub index:   usize,
    pub item_id: u8,   // ItemType::tile_index()
    pub count:   u32,
}

// ── Per-block-change record ────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone)]
pub struct BlockChangeSave {
    pub x:        i32,
    pub y:        i32,
    pub z:        i32,
    pub block_id: u8,   // BlockType::to_net_id()
}

// ── Top-level save file ────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone)]
pub struct SaveData {
    pub seed:            u32,
    pub sun_angle:       f32,
    pub player_position: [f32; 3],
    pub player_yaw:      f32,
    pub player_pitch:    f32,

    // Added in v2 — absent in old saves, defaults to empty / zero.
    #[serde(default)]
    pub block_changes: Vec<BlockChangeSave>,
    #[serde(default)]
    pub chickens: Vec<EntitySave>,
    #[serde(default)]
    pub pigs: Vec<EntitySave>,
    #[serde(default)]
    pub items: Vec<ItemSave>,
    #[serde(default)]
    pub inventory: Vec<InventorySlotSave>,
    #[serde(default)]
    pub selected_slot: usize,
    #[serde(default)]
    pub hotbar: Vec<InventorySlotSave>,  // reuses same struct; index = hotbar slot 0–8
}

impl SaveData {
    /// Convert the flat block-change list to the map format World expects.
    pub fn block_changes_as_map(&self) -> HashMap<[i32; 3], BlockType> {
        self.block_changes.iter().map(|bc| {
            ([bc.x, bc.y, bc.z], BlockType::from_net_id(bc.block_id))
        }).collect()
    }
}

// ── File helpers ───────────────────────────────────────────────────────────

fn saves_dir() -> PathBuf {
    PathBuf::from("saves")
}

/// Returns the next sequential save name ("1", "2", "3", …).
pub fn next_save_name() -> String {
    let max = list_saves()
        .iter()
        .filter_map(|s| s.parse::<u32>().ok())
        .max()
        .unwrap_or(0);
    format!("{}", max + 1)
}

pub fn save(name: &str, data: &SaveData) -> Result<(), String> {
    let dir = saves_dir();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{}.json", name));
    let json = serde_json::to_string_pretty(data).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| e.to_string())
}

/// Lists save names sorted newest-first (highest number first).
pub fn list_saves() -> Vec<String> {
    let dir = saves_dir();
    let Ok(entries) = fs::read_dir(&dir) else { return vec![]; };
    let mut names: Vec<String> = entries
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            if p.extension()?.to_str()? == "json" {
                Some(p.file_stem()?.to_str()?.to_string())
            } else {
                None
            }
        })
        .collect();
    names.sort_by(|a, b| match (a.parse::<u32>(), b.parse::<u32>()) {
        (Ok(x), Ok(y)) => x.cmp(&y),
        _ => a.cmp(b),
    });
    names.reverse();
    names
}

pub fn load(name: &str) -> Result<SaveData, String> {
    let path = saves_dir().join(format!("{}.json", name));
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}
