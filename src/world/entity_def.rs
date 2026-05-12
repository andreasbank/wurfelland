use std::collections::HashMap;
use std::sync::Arc;
use serde::Deserialize;
use crate::world::entity::Habitat;
use crate::world::item::ItemType;

// ── JSON schema types (mirroring the file layout) ─────────────────────────

#[derive(Deserialize)]
struct EntityFile {
    #[allow(dead_code)]
    format_version: String,
    identifier: String,
    components: Components,
}

#[derive(Deserialize)]
struct Components {
    habitat: String,
    health: HealthComp,
    physics: PhysicsComp,
    movement: MovementComp,
    wander: WanderComp,
    loot: Vec<LootEntryRaw>,
}

#[derive(Deserialize)]
struct HealthComp   { max: f32 }

#[derive(Deserialize)]
struct PhysicsComp  { half_width: f32, height: f32, jump_speed: f32, hit_half_width: f32 }

#[derive(Deserialize)]
struct MovementComp { speed: f32 }

#[derive(Deserialize)]
struct WanderComp {
    idle_chance: f32,
    idle_min_secs: f32,
    idle_max_secs: f32,
    walk_min_secs: f32,
    walk_max_secs: f32,
}

#[derive(Deserialize)]
struct LootEntryRaw { item: String, chance: f32 }

// ── Resolved loot entry ────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct LootEntry {
    pub item: ItemType,
    pub chance: f32,
}

// ── The processed definition shared across all instances of one entity type ─

#[derive(Debug)]
pub struct EntityDef {
    pub identifier: String,
    pub habitat: Habitat,
    pub max_health: f32,
    pub half_width: f32,
    pub height: f32,
    pub jump_speed: f32,
    pub hit_half_width: f32,
    pub speed: f32,
    pub idle_chance: f32,
    pub idle_range: (f32, f32),
    pub walk_range: (f32, f32),
    pub loot: Vec<LootEntry>,
}

impl EntityDef {
    fn from_file(file: EntityFile) -> Self {
        let c = file.components;
        let loot = c.loot.iter().filter_map(|e| {
            ItemType::from_name(&e.item).map(|item| LootEntry { item, chance: e.chance })
        }).collect();

        EntityDef {
            identifier:    file.identifier,
            habitat:       Habitat::from_name(&c.habitat),
            max_health:    c.health.max,
            half_width:    c.physics.half_width,
            height:        c.physics.height,
            jump_speed:    c.physics.jump_speed,
            hit_half_width: c.physics.hit_half_width,
            speed:         c.movement.speed,
            idle_chance:   c.wander.idle_chance,
            idle_range:    (c.wander.idle_min_secs, c.wander.idle_max_secs),
            walk_range:    (c.wander.walk_min_secs, c.wander.walk_max_secs),
            loot,
        }
    }
}

// ── Registry ───────────────────────────────────────────────────────────────

pub struct EntityRegistry {
    defs: HashMap<String, Arc<EntityDef>>,
}

impl EntityRegistry {
    /// Load every `*.json` file found in `dir`.
    pub fn load(dir: &str) -> Self {
        let mut defs = HashMap::new();
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(err) => {
                eprintln!("[EntityRegistry] Cannot open '{}': {}", dir, err);
                return EntityRegistry { defs };
            }
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") { continue; }
            let text = match std::fs::read_to_string(&path) {
                Ok(t) => t,
                Err(err) => { eprintln!("[EntityRegistry] Read error {:?}: {}", path, err); continue; }
            };
            match serde_json::from_str::<EntityFile>(&text) {
                Ok(file) => {
                    let def = Arc::new(EntityDef::from_file(file));
                    println!("[EntityRegistry] Loaded '{}'", def.identifier);
                    defs.insert(def.identifier.clone(), def);
                }
                Err(err) => eprintln!("[EntityRegistry] Parse error {:?}: {}", path, err),
            }
        }
        EntityRegistry { defs }
    }

    pub fn get(&self, identifier: &str) -> Option<Arc<EntityDef>> {
        self.defs.get(identifier).cloned()
    }
}
