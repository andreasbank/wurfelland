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
    /// Tameable mobs (e.g. cats) toggle a sitting state when interacted with.
    #[serde(default)]
    tameable: bool,
    /// "passive" (wander/flee) or "hostile" (skeleton-style combat AI).
    #[serde(default)]
    behavior: Behavior,
    /// How this mob detects targets (hearing/sight ranges + field of view).
    #[serde(default)]
    vision: VisionComp,
}

fn default_hearing_radius() -> f32 { 8.0 }
fn default_vision_radius() -> f32 { 16.0 }
fn default_vision_angle() -> f32 { 160.0 }

#[derive(Deserialize)]
struct VisionComp {
    /// Blocks within which the mob always notices a non-sneaking target.
    #[serde(default = "default_hearing_radius")]
    hearing_radius: f32,
    /// Blocks within which the mob notices a target inside its field of view.
    #[serde(default = "default_vision_radius")]
    vision_radius: f32,
    /// Total field-of-view cone (degrees) used beyond the hearing radius.
    #[serde(default = "default_vision_angle")]
    vision_angle: f32,
}

impl Default for VisionComp {
    fn default() -> Self {
        VisionComp {
            hearing_radius: default_hearing_radius(),
            vision_radius:  default_vision_radius(),
            vision_angle:   default_vision_angle(),
        }
    }
}

/// Selects which AI drives a mob at runtime. Defaults to passive.
#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum Behavior {
    #[default]
    Passive,
    Hostile,
}

#[derive(Deserialize)]
struct HealthComp   { max: f32 }

fn default_render_scale() -> f32 { 1.0 }
fn default_turn_speed() -> f32 { 90.0 }
fn default_knockback_h() -> f32 { 5.0 }
fn default_knockback_v() -> f32 { 4.5 }
fn default_flee_mult() -> f32 { 1.5 }

#[derive(Deserialize)]
struct PhysicsComp {
    half_width: f32, height: f32, jump_speed: f32, hit_half_width: f32,
    #[serde(default = "default_render_scale")]
    render_scale: f32,
}

#[derive(Deserialize)]
struct MovementComp {
    speed: f32,
    /// Degrees/second the mob rotates toward its target heading.
    #[serde(default = "default_turn_speed")]
    turn_speed: f32,
    /// Horizontal velocity multiplier applied to the push direction when hit.
    #[serde(default = "default_knockback_h")]
    knockback_h: f32,
    /// Upward velocity imparted when hit.
    #[serde(default = "default_knockback_v")]
    knockback_v: f32,
    /// Speed multiplier while fleeing after being hit.
    #[serde(default = "default_flee_mult")]
    flee_speed_mult: f32,
}

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
    pub render_scale: f32,
    pub speed: f32,
    pub turn_speed: f32,
    pub knockback_h: f32,
    pub knockback_v: f32,
    pub flee_speed_mult: f32,
    pub tameable: bool,
    pub behavior: Behavior,
    pub hearing_radius: f32,
    pub vision_radius: f32,
    pub vision_angle: f32,
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
            render_scale:  c.physics.render_scale,
            speed:         c.movement.speed,
            turn_speed:    c.movement.turn_speed,
            knockback_h:   c.movement.knockback_h,
            knockback_v:   c.movement.knockback_v,
            flee_speed_mult: c.movement.flee_speed_mult,
            tameable:      c.tameable,
            behavior:      c.behavior,
            hearing_radius: c.vision.hearing_radius,
            vision_radius:  c.vision.vision_radius,
            vision_angle:   c.vision.vision_angle,
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
