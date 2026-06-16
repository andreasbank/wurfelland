use std::sync::Arc;
use crate::world::block::BlockType;
use crate::world::item::ItemType;
use crate::world::entity_def::{EntityDef, Behavior};

pub trait HittableEntity {
    fn aabb_min(&self) -> [f32; 3];
    fn aabb_max(&self) -> [f32; 3];
    fn is_dead(&self) -> bool;
}

/// Describes which terrain type an entity is comfortable in.
/// Used by the shared wander/escape helpers below.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Habitat {
    Land,       // avoids water; escapes if it ends up in it (chickens, pigs)
    Water,      // avoids land; escapes if it ends up on it (future: seals)
    Amphibious, // happy in both (future: ducks)
}

impl Habitat {
    pub fn from_name(s: &str) -> Self {
        match s {
            "water"      => Self::Water,
            "amphibious" => Self::Amphibious,
            _            => Self::Land,
        }
    }
}

// ── Shared habitat helpers ─────────────────────────────────────────────────

fn cell_has_water(bx: i32, by: i32, bz: i32, get_block: &impl Fn(i32, i32, i32) -> BlockType) -> bool {
    get_block(bx, by,     bz) == BlockType::Water
    || get_block(bx, by - 1, bz) == BlockType::Water
    || get_block(bx, by + 1, bz) == BlockType::Water
}

/// True when the entity's foot block is the wrong terrain for its habitat.
fn in_wrong_habitat(pos: [f32; 3], habitat: Habitat, get_block: &impl Fn(i32, i32, i32) -> BlockType) -> bool {
    if habitat == Habitat::Amphibious { return false; }
    let bx = pos[0].floor() as i32;
    let by = pos[1].floor() as i32;
    let bz = pos[2].floor() as i32;
    let in_water = get_block(bx, by, bz) == BlockType::Water;
    match habitat {
        Habitat::Land       =>  in_water,
        Habitat::Water      => !in_water,
        Habitat::Amphibious =>  false,
    }
}

/// True when ~2.5 blocks ahead in `yaw_rad` direction is acceptable terrain.
fn direction_suits_habitat(
    pos: [f32; 3], yaw_rad: f32, habitat: Habitat,
    get_block: &impl Fn(i32, i32, i32) -> BlockType,
) -> bool {
    if habitat == Habitat::Amphibious { return true; }
    let tx = pos[0] + yaw_rad.cos() * 2.5;
    let tz = pos[2] + yaw_rad.sin() * 2.5;
    let bx = tx.floor() as i32;
    let bz = tz.floor() as i32;
    let by = pos[1].floor() as i32;
    let ahead_water = cell_has_water(bx, by, bz, get_block);
    match habitat {
        Habitat::Land       => !ahead_water,
        Habitat::Water      =>  ahead_water,
        Habitat::Amphibious =>  true,
    }
}

/// True when there is solid ground within 3 blocks below the projected foot
/// position, i.e. no dangerous drop ahead.
fn no_cliff_ahead(
    pos: [f32; 3], yaw_rad: f32,
    get_block: &impl Fn(i32, i32, i32) -> BlockType,
) -> bool {
    let tx = pos[0] + yaw_rad.cos() * 2.5;
    let tz = pos[2] + yaw_rad.sin() * 2.5;
    let bx = tx.floor() as i32;
    let bz = tz.floor() as i32;
    let foot_y = pos[1].floor() as i32;
    for drop in 0..=3i32 {
        if get_block(bx, foot_y - 1 - drop, bz).is_solid() { return true; }
    }
    false
}

/// Combined habitat + cliff check used when picking wander directions.
/// `find_escape_yaw` deliberately uses `direction_suits_habitat` directly so
/// that habitat-escape is never blocked by cliffs.
fn direction_is_safe(
    pos: [f32; 3], yaw_rad: f32, habitat: Habitat,
    get_block: &impl Fn(i32, i32, i32) -> BlockType,
) -> bool {
    direction_suits_habitat(pos, yaw_rad, habitat, get_block)
        && no_cliff_ahead(pos, yaw_rad, get_block)
}

/// Sweep 8 directions starting opposite of `current_yaw_deg`; return the
/// first yaw (degrees) that suits the habitat, or None if surrounded.
fn find_escape_yaw(
    pos: [f32; 3], current_yaw_deg: f32, habitat: Habitat,
    get_block: &impl Fn(i32, i32, i32) -> BlockType,
) -> Option<f32> {
    for i in 0..8u32 {
        let candidate = (current_yaw_deg + 180.0 + i as f32 * 45.0).rem_euclid(360.0);
        if direction_suits_habitat(pos, candidate.to_radians(), habitat, get_block) {
            return Some(candidate);
        }
    }
    None
}

const GRAVITY: f32 = -20.0;

/// Per-brain AI state. The variant is chosen from `EntityDef::behavior` at
/// construction. Both variants are `Copy` so `update` can pull a working copy
/// out of the entity, mutate it alongside `self`, then store it back — avoiding
/// borrow conflicts between the brain and the shared kinematic fields.
#[derive(Clone, Copy)]
enum Brain {
    Passive(PassiveAi),
    Hostile(HostileAi),
}

/// Wandering/fleeing mob state (chickens, pigs, penguins, cats, cows).
#[derive(Clone, Copy)]
struct PassiveAi {
    scared_timer: f32,
    scare_yaw: f32,
}

/// Skeleton-style combat AI state.
#[derive(Clone, Copy)]
struct HostileAi {
    burn_timer: f32,
    attack_cooldown: f32,
    attack_hit_pending: bool,
    aggro: AggroState,
    target_player: Option<usize>,
    target_lost_timer: f32,
    strafe_dir: f32,
    strafe_timer: f32,
}

/// A mob. Kinematics, health and wander state are shared; species behaviour and
/// all tuning come from `def`, and `brain` selects passive vs. hostile AI.
/// The type aliases below pick a species purely by which `EntityDef` is supplied.
pub struct Entity {
    pub position: [f32; 3],
    pub yaw: f32,
    pub anim_time: f32,
    pub health: f32,
    pub block_light: f32,
    pub net_target_pos: [f32; 3],
    pub net_target_yaw: f32,
    /// Tameable mobs only: true while sitting. Always false for other species.
    pub sitting: bool,
    /// Hostile mobs: the equipped weapon. `BareHands` for passive species.
    pub weapon: WeaponType,
    /// Hostile mobs: remaining swing-animation time. 0 for passive species.
    pub attack_anim: f32,
    pub def: Arc<EntityDef>,
    velocity: [f32; 3],
    on_ground: bool,
    wander_timer: f32,
    target_yaw: f32,
    move_speed: f32,
    knockback: f32,
    brain: Brain,
}

impl Entity {
    pub fn new(x: f32, y: f32, z: f32, def: Arc<EntityDef>) -> Self {
        match def.behavior {
            Behavior::Hostile => Self::new_hostile(x, y, z, def),
            Behavior::Passive => Self::new_passive(x, y, z, def),
        }
    }

    fn new_passive(x: f32, y: f32, z: f32, def: Arc<EntityDef>) -> Self {
        let seed = x * 127.1 + z * 311.7;
        let init_yaw = seed.rem_euclid(360.0);
        let speed = def.speed;
        Entity {
            position: [x, y, z],
            yaw: init_yaw,
            anim_time: seed.rem_euclid(6.28),
            health: def.max_health,
            block_light: 1.0,
            net_target_pos: [x, y, z],
            net_target_yaw: init_yaw,
            sitting: false,
            weapon: WeaponType::BareHands,
            attack_anim: 0.0,
            def,
            velocity: [0.0; 3],
            on_ground: false,
            wander_timer: seed.rem_euclid(5.0),
            target_yaw: init_yaw,
            move_speed: speed,
            knockback: 0.0,
            brain: Brain::Passive(PassiveAi { scared_timer: 0.0, scare_yaw: init_yaw }),
        }
    }

    fn new_hostile(x: f32, y: f32, z: f32, def: Arc<EntityDef>) -> Self {
        let seed = x * 183.7 + z * 431.3;
        let init_yaw = seed.rem_euclid(360.0);
        let speed = def.speed;
        let weapon = match (seed * 73.1 + 17.3).rem_euclid(3.0) as u32 {
            0 => WeaponType::BareHands,
            1 => WeaponType::Axe,
            _ => WeaponType::Sword,
        };
        Entity {
            position: [x, y, z],
            yaw: init_yaw,
            anim_time: seed.rem_euclid(6.28),
            health: def.max_health,
            block_light: 1.0,
            net_target_pos: [x, y, z],
            net_target_yaw: init_yaw,
            sitting: false,
            weapon,
            attack_anim: 0.0,
            def,
            velocity: [0.0; 3],
            on_ground: false,
            wander_timer: seed.rem_euclid(4.0),
            target_yaw: init_yaw,
            move_speed: speed,
            knockback: 0.0,
            brain: Brain::Hostile(HostileAi {
                burn_timer: 0.0,
                attack_cooldown: 0.0,
                attack_hit_pending: false,
                aggro: AggroState::Idle,
                target_player: None,
                target_lost_timer: 0.0,
                strafe_dir: if seed.rem_euclid(2.0) < 1.0 { 1.0 } else { -1.0 },
                strafe_timer: 2.0 + seed.rem_euclid(1.5),
            }),
        }
    }

    pub fn is_dead(&self) -> bool { self.health <= 0.0 }

    pub fn aabb_min(&self) -> [f32; 3] {
        let h = self.def.hit_half_width;
        [self.position[0] - h, self.position[1], self.position[2] - h]
    }
    pub fn aabb_max(&self) -> [f32; 3] {
        let h = self.def.hit_half_width;
        [self.position[0] + h, self.position[1] + self.def.height, self.position[2] + h]
    }

    pub fn take_hit(&mut self, push_dir: [f32; 3]) {
        self.health -= 1.0;
        self.velocity[0] = push_dir[0] * self.def.knockback_h;
        self.velocity[1] = self.def.knockback_v;
        self.velocity[2] = push_dir[2] * self.def.knockback_h;
        self.on_ground = false;
        self.knockback = 0.4;
        // `brain` is a Copy local so the variant fields and `self.*` don't alias.
        let mut brain = self.brain;
        match &mut brain {
            Brain::Passive(p) => {
                p.scare_yaw = push_dir[0].atan2(push_dir[2]).to_degrees();
                self.target_yaw = p.scare_yaw;
                p.scared_timer = 5.0;
                self.wander_timer = 0.0;
                self.sitting = false;
            }
            Brain::Hostile(h) => {
                self.target_yaw = push_dir[0].atan2(push_dir[2]).to_degrees() + 180.0;
                self.move_speed = self.def.speed;
                self.wander_timer = 2.0;
                if h.aggro == AggroState::Idle {
                    h.aggro = AggroState::Chasing;
                    h.target_lost_timer = 0.0;
                }
            }
        }
        self.brain = brain;
    }

    /// Tameable mobs toggle sitting on interaction; others ignore it.
    pub fn interact(&mut self) {
        if !self.def.tameable { return; }
        self.sitting = !self.sitting;
        if self.sitting {
            self.velocity = [0.0; 3];
            self.move_speed = 0.0;
        }
    }

    pub fn drops(&self) -> Vec<ItemType> {
        let s = self.anim_time;
        self.def.loot.iter().enumerate().filter_map(|(i, entry)| {
            let r = (s * (127.1 + i as f32 * 73.7)).rem_euclid(1.0);
            if r < entry.chance { Some(entry.item) } else { None }
        }).collect()
    }

    pub fn move_speed_norm(&self) -> f32 {
        if self.move_speed > 0.0 { 1.0 } else { 0.0 }
    }

    /// Passive AI: wander, flee when scared, escape wrong habitat.
    pub fn update(&mut self, dt: f32, get_block: impl Fn(i32, i32, i32) -> BlockType) {
        self.anim_time += dt;

        // Sitting (tameable mobs only): apply gravity, skip all AI.
        if self.sitting {
            if !self.on_ground {
                self.velocity[1] += GRAVITY * dt;
                self.velocity[1] = self.velocity[1].max(-50.0);
                let is_solid = |x: i32, y: i32, z: i32| get_block(x, y, z).is_solid();
                let (hw, ht) = (self.def.half_width, self.def.height);
                self.position[1] += self.velocity[1] * dt;
                if aabb_collides(self.position, hw, ht, &is_solid) {
                    if self.velocity[1] < 0.0 { self.position[1] = self.position[1].floor() + 1.0; self.on_ground = true; }
                    else { self.position[1] = (self.position[1] + ht).floor() - ht; }
                    self.velocity[1] = 0.0;
                } else { self.on_ground = false; }
            }
            return;
        }

        // Copy of the passive brain — mutated alongside `self`, stored back below.
        let Brain::Passive(mut p) = self.brain else { return; };

        let habitat = self.def.habitat;
        if in_wrong_habitat(self.position, habitat, &get_block) {
            if let Some(ey) = find_escape_yaw(self.position, self.yaw, habitat, &get_block) {
                self.target_yaw = ey;
            }
            self.move_speed = self.def.speed;
        } else if p.scared_timer > 0.0 {
            p.scared_timer -= dt;
            let jitter = (self.anim_time * 5.7).sin() * 25.0;
            let candidate = p.scare_yaw + jitter;
            if no_cliff_ahead(self.position, candidate.to_radians(), &get_block) {
                self.target_yaw = candidate;
            } else if no_cliff_ahead(self.position, p.scare_yaw.to_radians(), &get_block) {
                self.target_yaw = p.scare_yaw;
            }
            self.move_speed = self.def.speed * self.def.flee_speed_mult;
        } else {
            self.wander_timer -= dt;
            if self.wander_timer <= 0.0 {
                let seed = self.position[0] * 73.1 + self.position[2] * 47.3 + self.anim_time * 37.9;
                let mut chosen = seed.rem_euclid(360.0);
                for attempt in 0..4u32 {
                    let candidate = (seed + attempt as f32 * 97.3).rem_euclid(360.0);
                    if direction_is_safe(self.position, candidate.to_radians(), habitat, &get_block) {
                        chosen = candidate;
                        break;
                    }
                }
                self.target_yaw = chosen;

                let r = (seed * 7.3).rem_euclid(1.0);
                let (ir, wr) = (self.def.idle_range, self.def.walk_range);
                if r < self.def.idle_chance {
                    self.move_speed = 0.0;
                    self.wander_timer = ir.0 + (seed * 0.01).rem_euclid(ir.1 - ir.0);
                } else {
                    self.move_speed = self.def.speed;
                    self.wander_timer = wr.0 + (seed * 0.01).rem_euclid(wr.1 - wr.0);
                }
            }
        }
        self.brain = Brain::Passive(p);

        self.integrate_motion(dt, &get_block);
    }

    /// Hostile (skeleton) AI: sunburn, target acquisition, chase/orbit, melee.
    /// Returns `Some((player_index, damage))` if a swing lands this frame.
    pub fn update_hostile(
        &mut self,
        dt: f32,
        get_block: impl Fn(i32, i32, i32) -> BlockType,
        get_sky_light: impl Fn(i32, i32, i32) -> u8,
        player_positions: &[[f32; 3]],
        sun_angle: f32,
    ) -> Option<(usize, f32)> {
        self.anim_time += dt;
        let Brain::Hostile(mut h) = self.brain else { return None; };

        // Sunburn: direct sky light during daytime deals 1 HP/s.
        let bx = self.position[0].floor() as i32;
        let by = self.position[1].floor() as i32 + 1;
        let bz = self.position[2].floor() as i32;
        let sky = get_sky_light(bx, by, bz);
        let is_daytime = sun_angle.sin() > 0.15;
        if is_daytime && sky >= 12 {
            h.burn_timer -= dt;
            if h.burn_timer <= 0.0 {
                self.health -= 1.0;
                h.burn_timer = 1.0;
            }
        } else {
            h.burn_timer = h.burn_timer.min(0.0);
        }

        // ── Target acquisition: nearest detectable player given aggro state ──
        let best_target: Option<(usize, f32)> = {
            let mut best: Option<(usize, f32)> = None;
            for (i, &ppos) in player_positions.iter().enumerate() {
                let dx = ppos[0] - self.position[0];
                let dz = ppos[2] - self.position[2];
                let dy = ppos[1] - self.position[1];
                let dist2 = dx*dx + dz*dz + dy*dy;
                let detected = match h.aggro {
                    AggroState::Idle =>
                        detect_from_idle(self.position, self.yaw, ppos, &get_block),
                    AggroState::Chasing | AggroState::InCombat =>
                        dist2 < LOSE_TARGET_R2,
                };
                if detected && best.map_or(true, |(_, bd)| dist2 < bd) {
                    best = Some((i, dist2));
                }
            }
            best
        };

        if best_target.is_some() {
            h.target_lost_timer = 0.0;
            h.target_player = best_target.map(|(i, _)| i);
        } else {
            h.target_lost_timer += dt;
        }

        // ── Aggro state transitions ──
        let target_pos = h.target_player
            .and_then(|i| player_positions.get(i))
            .copied();

        if h.target_lost_timer >= LOSE_TARGET_TIME {
            h.aggro = AggroState::Idle;
            h.target_player = None;
        } else if let Some(tpos) = target_pos {
            let dx = tpos[0] - self.position[0];
            let dz = tpos[2] - self.position[2];
            let dy = tpos[1] - self.position[1];
            let dist2_3d = dx*dx + dz*dz + dy*dy;
            match h.aggro {
                AggroState::Idle => {
                    if best_target.is_some() { h.aggro = AggroState::Chasing; }
                }
                AggroState::Chasing => {
                    if dist2_3d < COMBAT_RANGE * COMBAT_RANGE {
                        h.aggro = AggroState::InCombat;
                    }
                }
                AggroState::InCombat => {
                    if dist2_3d > COMBAT_RANGE * COMBAT_RANGE * 2.25 {
                        h.aggro = AggroState::Chasing;
                    }
                }
            }
        }

        // ── Movement AI (skipped during knockback; integrate_motion decays it) ──
        let habitat = self.def.habitat;
        if self.knockback <= 0.0 {
            match h.aggro {
                AggroState::Idle => {
                    if in_wrong_habitat(self.position, habitat, &get_block) {
                        if let Some(ey) = find_escape_yaw(self.position, self.yaw, habitat, &get_block) {
                            self.target_yaw = ey;
                        }
                        self.move_speed = self.def.speed;
                    } else {
                        self.wander_timer -= dt;
                        if self.wander_timer <= 0.0 {
                            let seed = self.position[0] * 137.1 + self.position[2] * 83.7 + self.anim_time * 53.9;
                            let mut chosen = seed.rem_euclid(360.0);
                            for attempt in 0..4u32 {
                                let candidate = (seed + attempt as f32 * 107.3).rem_euclid(360.0);
                                if direction_is_safe(self.position, candidate.to_radians(), habitat, &get_block) {
                                    chosen = candidate;
                                    break;
                                }
                            }
                            self.target_yaw = chosen;
                            let r = (seed * 9.1).rem_euclid(1.0);
                            let (ir, wr) = (self.def.idle_range, self.def.walk_range);
                            if r < self.def.idle_chance {
                                self.move_speed = 0.0;
                                self.wander_timer = ir.0 + (seed * 0.01).rem_euclid(ir.1 - ir.0);
                            } else {
                                self.move_speed = self.def.speed;
                                self.wander_timer = wr.0 + (seed * 0.01).rem_euclid(wr.1 - wr.0);
                            }
                        }
                    }
                }
                AggroState::Chasing => {
                    if let Some(tpos) = target_pos {
                        let dx = tpos[0] - self.position[0];
                        let dz = tpos[2] - self.position[2];
                        self.target_yaw = dx.atan2(dz).to_degrees();
                        self.move_speed = self.def.speed;
                    }
                }
                AggroState::InCombat => {
                    if let Some(tpos) = target_pos {
                        let dx = tpos[0] - self.position[0];
                        let dz = tpos[2] - self.position[2];
                        // Orbit the target — periodically flip strafe direction.
                        h.strafe_timer -= dt;
                        if h.strafe_timer <= 0.0 {
                            h.strafe_dir = -h.strafe_dir;
                            h.strafe_timer = 1.5 + self.anim_time.rem_euclid(1.5);
                        }
                        self.target_yaw = dx.atan2(dz).to_degrees() + h.strafe_dir * 70.0;
                        self.move_speed = self.def.speed * 0.7;
                    }
                }
            }
        }

        self.integrate_motion(dt, &get_block);

        // ── Melee attack ──
        h.attack_cooldown = (h.attack_cooldown - dt).max(0.0);
        self.attack_anim = (self.attack_anim - dt).max(0.0);

        let mut hit = None;
        // Resolve the pending hit at the swing peak (0.25 s into the 0.5 s anim).
        if h.attack_hit_pending && self.attack_anim <= 0.25 {
            h.attack_hit_pending = false;
            if let (Some(pidx), Some(tpos)) = (h.target_player, target_pos) {
                let dx = tpos[0] - self.position[0];
                let dz = tpos[2] - self.position[2];
                let dy = tpos[1] - self.position[1];
                if dx*dx + dz*dz + dy*dy < COMBAT_RANGE * COMBAT_RANGE {
                    let dmg = match self.weapon {
                        WeaponType::BareHands => 1.0,
                        WeaponType::Sword     => 2.0,
                        WeaponType::Axe       => 3.0,
                    };
                    hit = Some((pidx, dmg));
                }
            }
        }

        // Start a new swing when in combat range and cooldown is ready.
        if h.aggro == AggroState::InCombat
            && h.attack_cooldown <= 0.0
            && !h.attack_hit_pending
        {
            if let Some(tpos) = target_pos {
                let dx = tpos[0] - self.position[0];
                let dz = tpos[2] - self.position[2];
                let dy = tpos[1] - self.position[1];
                if dx*dx + dz*dz + dy*dy < COMBAT_RANGE * COMBAT_RANGE {
                    h.attack_cooldown = match self.weapon {
                        WeaponType::Axe => 2.0,
                        _               => 1.5,
                    };
                    self.attack_anim = 0.5;
                    h.attack_hit_pending = true;
                }
            }
        }

        self.brain = Brain::Hostile(h);
        hit
    }

    /// Shared kinematics: rotate toward `target_yaw`, apply knockback/walk
    /// velocity and gravity, then resolve collisions one axis at a time.
    fn integrate_motion(&mut self, dt: f32, get_block: &impl Fn(i32, i32, i32) -> BlockType) {
        let diff = angle_diff(self.target_yaw, self.yaw);
        let turn_step = self.def.turn_speed * dt;
        if diff.abs() <= turn_step { self.yaw = self.target_yaw; }
        else { self.yaw += diff.signum() * turn_step; }

        if self.knockback > 0.0 { self.knockback -= dt; }
        else {
            let yr = self.yaw.to_radians();
            self.velocity[0] = yr.cos() * self.move_speed;
            self.velocity[2] = yr.sin() * self.move_speed;
        }

        if !self.on_ground {
            self.velocity[1] += GRAVITY * dt;
            self.velocity[1] = self.velocity[1].max(-50.0);
        }

        let is_solid = |x: i32, y: i32, z: i32| get_block(x, y, z).is_solid();
        let (hw, ht, js) = (self.def.half_width, self.def.height, self.def.jump_speed);

        self.position[0] += self.velocity[0] * dt;
        if aabb_collides(self.position, hw, ht, &is_solid) {
            self.position[0] -= self.velocity[0] * dt;
            self.velocity[0] = 0.0;
            if self.on_ground { self.velocity[1] = js; self.on_ground = false; }
        }

        self.position[1] += self.velocity[1] * dt;
        if aabb_collides(self.position, hw, ht, &is_solid) {
            if self.velocity[1] < 0.0 { self.position[1] = self.position[1].floor() + 1.0; self.on_ground = true; }
            else { self.position[1] = (self.position[1] + ht).floor() - ht; }
            self.velocity[1] = 0.0;
        } else { self.on_ground = false; }

        self.position[2] += self.velocity[2] * dt;
        if aabb_collides(self.position, hw, ht, &is_solid) {
            self.position[2] -= self.velocity[2] * dt;
            self.velocity[2] = 0.0;
            if self.on_ground { self.velocity[1] = js; self.on_ground = false; }
        }
    }
}

impl HittableEntity for Entity {
    fn aabb_min(&self) -> [f32; 3] { self.aabb_min() }
    fn aabb_max(&self) -> [f32; 3] { self.aabb_max() }
    fn is_dead(&self) -> bool { self.is_dead() }
}

// Phase 1: every passive mob shares one implementation. The species is selected
// entirely by the EntityDef passed to `Entity::new` (loaded from assets/entities).
pub type Chicken = Entity;
pub type Pig     = Entity;
pub type Penguin = Entity;
pub type Cat     = Entity;
pub type Cow     = Entity;

// ─── Skeleton ─────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum WeaponType { BareHands, Axe, Sword }

#[derive(Clone, Copy, PartialEq, Debug)]
enum AggroState { Idle, Chasing, InCombat }

const COMBAT_RANGE: f32 = 2.0;   // Metres: enter/attack range
const PASSIVE_DETECT_R2: f32 = 36.0;   // 6² — always noticed
const ACTIVE_DETECT_R2:  f32 = 196.0;  // 14² — frontal cone + LOS
const LOSE_TARGET_R2:    f32 = 576.0;  // 24² — give up chase
const LOSE_TARGET_TIME:  f32 = 4.0;    // seconds without target → Idle

/// DDA line-of-sight: returns false if any solid block sits between the two points.
fn has_line_of_sight(
    from: [f32; 3], to: [f32; 3],
    get_block: &impl Fn(i32, i32, i32) -> BlockType,
) -> bool {
    let eye_y = 1.6_f32;
    let (fx, fy, fz) = (from[0], from[1] + eye_y, from[2]);
    let (dx, dy, dz) = (to[0] - fx, to[1] + eye_y - fy, to[2] - fz);
    let dist = (dx*dx + dy*dy + dz*dz).sqrt();
    if dist < 0.001 { return true; }
    let steps = (dist / 0.4).ceil() as i32;
    let inv = 1.0 / steps as f32;
    for i in 1..steps {
        let t = i as f32 * inv;
        if get_block(
            (fx + dx * t).floor() as i32,
            (fy + dy * t).floor() as i32,
            (fz + dz * t).floor() as i32,
        ).is_solid() { return false; }
    }
    true
}

/// Returns true when a player at `player_pos` should be noticed by an Idle skeleton.
fn detect_from_idle(
    skel_pos: [f32; 3], skel_yaw: f32,
    player_pos: [f32; 3],
    get_block: &impl Fn(i32, i32, i32) -> BlockType,
) -> bool {
    let dx = player_pos[0] - skel_pos[0];
    let dz = player_pos[2] - skel_pos[2];
    let dist2_xz = dx * dx + dz * dz;
    if dist2_xz < PASSIVE_DETECT_R2 { return true; }
    if dist2_xz < ACTIVE_DETECT_R2 {
        let angle_to = dx.atan2(dz).to_degrees();
        if angle_diff(angle_to, skel_yaw).abs() < 60.0 {
            return has_line_of_sight(skel_pos, player_pos, get_block);
        }
    }
    false
}

// The skeleton is a hostile Entity; see Brain::Hostile / update_hostile.
pub type Skeleton = Entity;

/// Slab-method ray vs AABB intersection. Returns entry distance if hit within max_dist.
pub fn ray_aabb_intersect(
    ro: [f32; 3], rd: [f32; 3],
    min: [f32; 3], max: [f32; 3],
    max_dist: f32,
) -> Option<f32> {
    let mut tmin = 0.0f32;
    let mut tmax = max_dist;
    for i in 0..3 {
        if rd[i].abs() < 1e-6 {
            if ro[i] < min[i] || ro[i] > max[i] { return None; }
        } else {
            let inv = 1.0 / rd[i];
            let t1 = (min[i] - ro[i]) * inv;
            let t2 = (max[i] - ro[i]) * inv;
            let (t1, t2) = if t1 < t2 { (t1, t2) } else { (t2, t1) };
            tmin = tmin.max(t1);
            tmax = tmax.min(t2);
            if tmin > tmax { return None; }
        }
    }
    if tmax >= 0.0 { Some(tmin.max(0.0)) } else { None }
}

/// Returns index and entry distance of the nearest living entity the ray hits.
pub fn nearest_entity_hit<E: HittableEntity>(
    entities: &[E],
    ro: [f32; 3], rd: [f32; 3],
    max_dist: f32,
) -> Option<(usize, f32)> {
    let mut best: Option<(usize, f32)> = None;
    for (i, e) in entities.iter().enumerate() {
        if e.is_dead() { continue; }
        if let Some(t) = ray_aabb_intersect(ro, rd, e.aabb_min(), e.aabb_max(), max_dist) {
            if best.map_or(true, |(_, bt)| t < bt) {
                best = Some((i, t));
            }
        }
    }
    best
}

pub fn angle_diff(target: f32, current: f32) -> f32 {
    let mut d = target - current;
    while d > 180.0 { d -= 360.0; }
    while d < -180.0 { d += 360.0; }
    d
}

fn aabb_collides(
    pos: [f32; 3],
    half_w: f32,
    height: f32,
    is_solid: &impl Fn(i32, i32, i32) -> bool,
) -> bool {
    let min_x = (pos[0] - half_w).floor() as i32;
    let max_x = (pos[0] + half_w - 0.001).floor() as i32;
    let min_y = pos[1].floor() as i32;
    let max_y = (pos[1] + height - 0.001).floor() as i32;
    let min_z = (pos[2] - half_w).floor() as i32;
    let max_z = (pos[2] + half_w - 0.001).floor() as i32;
    for x in min_x..=max_x {
        for y in min_y..=max_y {
            for z in min_z..=max_z {
                if is_solid(x, y, z) { return true; }
            }
        }
    }
    false
}
