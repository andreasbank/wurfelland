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
    /// Where to walk while `Alert` — the last spot a noise (or lost target) came from.
    alert_pos: [f32; 3],
    /// Time left investigating before giving up and returning to `Idle`.
    alert_timer: f32,
    /// Seconds the current chase target has been out of sight (grace before Alert).
    lost_timer: f32,
}

/// A mob. Kinematics, health and wander state are shared; species behaviour and
/// all tuning come from `def`, and `brain` selects passive vs. hostile AI. The
/// species is chosen entirely by which `EntityDef` is supplied to `new`.
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
    /// Footstep loudness this frame (0 = silent). High while walking on the
    /// ground; drives footstep sound and is what enemies can "hear".
    pub noise: f32,
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
            noise: 0.0,
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
            noise: 0.0,
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
                alert_pos: [x, y, z],
                alert_timer: 0.0,
                lost_timer: 0.0,
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
                p.scare_yaw = push_dir[2].atan2(push_dir[0]).to_degrees();
                self.target_yaw = p.scare_yaw;
                p.scared_timer = 5.0;
                self.wander_timer = 0.0;
                self.sitting = false;
            }
            Brain::Hostile(h) => {
                self.target_yaw = push_dir[2].atan2(push_dir[0]).to_degrees() + 180.0;
                self.move_speed = self.def.speed;
                self.wander_timer = 2.0;
                // Already chasing? Stay locked on. Otherwise close on whoever hit us
                // (opposite the knockback push) and aggro for real once we see them.
                if h.aggro == AggroState::Idle || h.aggro == AggroState::Alert {
                    let pmag = (push_dir[0]*push_dir[0] + push_dir[2]*push_dir[2]).sqrt().max(1e-3);
                    h.alert_pos = [
                        self.position[0] - push_dir[0] / pmag * 6.0,
                        self.position[1],
                        self.position[2] - push_dir[2] / pmag * 6.0,
                    ];
                    h.alert_timer = ALERT_TIME;
                    h.aggro = AggroState::Alert;
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
        targets: &[Target],
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
            // Out of sunlight: keep the remaining countdown instead of forcing an
            // instant burn tick the moment we step back into the light.
            h.burn_timer = h.burn_timer.max(0.0);
        }

        // ── Sensing: hearing only points the mob at a noise; sight starts/holds aggro.
        // Sight always needs the target inside the facing cone — an alerted mob turns
        // to face the spot it's investigating, so it sees a target that's actually there.
        let mut nearest_seen:  Option<(usize, f32)> = None;
        let mut nearest_heard: Option<([f32; 3], f32)> = None;
        for (i, t) in targets.iter().enumerate() {
            let dx = t.pos[0] - self.position[0];
            let dz = t.pos[2] - self.position[2];
            let dist2_xz = dx*dx + dz*dz;
            if seen_target(self.position, self.yaw, t,
                           self.def.vision_radius, self.def.vision_angle, true, &get_block)
                && nearest_seen.map_or(true, |(_, d)| dist2_xz < d) {
                nearest_seen = Some((i, dist2_xz));
            }
            if heard_target(self.position, t, self.def.hearing_radius)
                && nearest_heard.map_or(true, |(_, d)| dist2_xz < d) {
                nearest_heard = Some((t.pos, dist2_xz));
            }
        }

        // Is the current chase target still in sight and in range? (sneaking shrinks
        // the radius.) Sustains Chasing / InCombat.
        let cur = h.target_player.and_then(|i| targets.get(i).map(|t| (i, t)));
        let chase_held = cur.map_or(false, |(_, t)| {
            let dx = t.pos[0] - self.position[0];
            let dz = t.pos[2] - self.position[2];
            let mut chase_r = self.def.vision_radius * CHASE_RANGE_MULT;
            if t.sneaking { chase_r *= SNEAK_CHASE_FACTOR; }
            dx*dx + dz*dz < chase_r * chase_r
                && has_line_of_sight(self.position, t.pos, &get_block)
        });

        // ── Aggro state transitions ──
        match h.aggro {
            AggroState::Idle => {
                if let Some((i, _)) = nearest_seen {
                    h.aggro = AggroState::Chasing;
                    h.target_player = Some(i);
                    h.lost_timer = 0.0;
                } else if let Some((pos, _)) = nearest_heard {
                    h.aggro = AggroState::Alert;
                    h.alert_pos = pos;
                    h.alert_timer = ALERT_TIME;
                }
            }
            AggroState::Alert => {
                if let Some((i, _)) = nearest_seen {
                    h.aggro = AggroState::Chasing;     // line of sight → real aggro
                    h.target_player = Some(i);
                    h.lost_timer = 0.0;
                } else {
                    if let Some((pos, _)) = nearest_heard {
                        h.alert_pos = pos;             // a fresh noise updates the goal
                        h.alert_timer = ALERT_TIME;
                    }
                    h.alert_timer -= dt;
                    let dx = h.alert_pos[0] - self.position[0];
                    let dz = h.alert_pos[2] - self.position[2];
                    if h.alert_timer <= 0.0 || dx*dx + dz*dz < ALERT_REACH * ALERT_REACH {
                        h.aggro = AggroState::Idle;    // reached the spot / gave up
                    }
                }
            }
            AggroState::Chasing | AggroState::InCombat => {
                if let (true, Some((_, t))) = (chase_held, cur) {
                    h.lost_timer = 0.0;
                    let dx = t.pos[0] - self.position[0];
                    let dz = t.pos[2] - self.position[2];
                    let dy = t.pos[1] - self.position[1];
                    let dist2_3d = dx*dx + dz*dz + dy*dy;
                    if h.aggro == AggroState::Chasing {
                        if dist2_3d < COMBAT_RANGE * COMBAT_RANGE {
                            h.aggro = AggroState::InCombat;
                        }
                    } else if dist2_3d > COMBAT_RANGE * COMBAT_RANGE * 2.25 {
                        h.aggro = AggroState::Chasing;
                    }
                } else {
                    // Lost sight: after a short grace, investigate the last known spot
                    // instead of giving up outright.
                    h.lost_timer += dt;
                    if h.lost_timer >= CHASE_LOSE_GRACE {
                        h.alert_pos = cur.map_or(self.position, |(_, t)| t.pos);
                        h.alert_timer = ALERT_TIME;
                        h.aggro = AggroState::Alert;
                        h.target_player = None;
                    }
                }
            }
        }

        // Current chase target position, after any transition above (for movement & melee).
        let target_pos = h.target_player.and_then(|i| targets.get(i)).map(|t| t.pos);

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
                AggroState::Alert => {
                    // Head toward the last noise/last-seen spot, looking for the target.
                    let dx = h.alert_pos[0] - self.position[0];
                    let dz = h.alert_pos[2] - self.position[2];
                    self.target_yaw = dz.atan2(dx).to_degrees();
                    self.move_speed = self.def.speed;
                }
                AggroState::Chasing => {
                    if let Some(tpos) = target_pos {
                        let dx = tpos[0] - self.position[0];
                        let dz = tpos[2] - self.position[2];
                        self.target_yaw = dz.atan2(dx).to_degrees();
                        self.move_speed = self.def.speed;
                    }
                }
                AggroState::InCombat => {
                    if let Some(tpos) = target_pos {
                        let dx = tpos[0] - self.position[0];
                        let dz = tpos[2] - self.position[2];
                        // Face the target head-on and press in until at striking
                        // distance, then hold ground and swing (no circling).
                        self.target_yaw = dz.atan2(dx).to_degrees();
                        let dist_xz = (dx*dx + dz*dz).sqrt();
                        self.move_speed = if dist_xz > COMBAT_RANGE * 0.6 {
                            self.def.speed
                        } else {
                            0.0
                        };
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

        // Noise: loud while actually walking on the ground, silent otherwise.
        self.noise = if self.on_ground && self.move_speed > 0.0 { 1.0 } else { 0.0 };
    }
}

impl HittableEntity for Entity {
    fn aabb_min(&self) -> [f32; 3] { self.aabb_min() }
    fn aabb_max(&self) -> [f32; 3] { self.aabb_max() }
    fn is_dead(&self) -> bool { self.is_dead() }
}

// Phase 1: every passive mob shares one implementation. The species is selected
// entirely by the EntityDef passed to `Entity::new` (loaded from assets/entities).
pub type Penguin = Entity;

// ─── Skeleton ─────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum WeaponType { BareHands, Axe, Sword }

#[derive(Clone, Copy, PartialEq, Debug)]
enum AggroState { Idle, Alert, Chasing, InCombat }

const COMBAT_RANGE: f32 = 2.0;   // Metres: enter/attack range
const CHASE_RANGE_MULT:  f32 = 2.0;    // give-up radius = vision_radius × this (16→32)
const SNEAK_CHASE_FACTOR: f32 = 0.5;   // sneaking shrinks the re-detect radius mid-chase
const ALERT_TIME:    f32 = 6.0;        // seconds spent investigating a noise → Idle
const ALERT_REACH:   f32 = 1.5;        // within this of the noise spot → done investigating
const CHASE_LOSE_GRACE: f32 = 0.6;     // seconds out of sight before a chase drops to Alert

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

/// A potential target the hostile AI can notice (the local player, or remotes).
pub struct Target {
    pub pos: [f32; 3],
    /// When sneaking, the target is silent and isn't heard at close range.
    pub sneaking: bool,
}

/// Heard: within `hearing_r` blocks and not sneaking — omnidirectional, through
/// walls. Hearing never grants aggro by itself; it only marks a spot to investigate.
fn heard_target(skel_pos: [f32; 3], target: &Target, hearing_r: f32) -> bool {
    if target.sneaking { return false; }
    let dx = target.pos[0] - skel_pos[0];
    let dz = target.pos[2] - skel_pos[2];
    dx * dx + dz * dz < hearing_r * hearing_r
}

/// Seen: within `vision_r` blocks with clear line of sight (eye to eye). When
/// `require_cone` is set (an idle, unsuspecting mob) the target must also be inside
/// the `vision_angle` facing cone; an alerted mob is actively looking, so the cone
/// is dropped. Sneaking does not hide you from sight.
fn seen_target(
    skel_pos: [f32; 3], skel_yaw: f32,
    target: &Target,
    vision_r: f32, vision_angle: f32, require_cone: bool,
    get_block: &impl Fn(i32, i32, i32) -> BlockType,
) -> bool {
    let dx = target.pos[0] - skel_pos[0];
    let dz = target.pos[2] - skel_pos[2];
    if dx * dx + dz * dz >= vision_r * vision_r { return false; }
    if require_cone {
        let angle_to = dz.atan2(dx).to_degrees();
        if angle_diff(angle_to, skel_yaw).abs() >= vision_angle * 0.5 { return false; }
    }
    has_line_of_sight(skel_pos, target.pos, get_block)
}


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

#[cfg(test)]
mod tests {
    use super::*;

    fn skeleton_def() -> Arc<EntityDef> {
        Arc::new(EntityDef {
            identifier: "skeleton".into(),
            habitat: Habitat::Land,
            max_health: 10.0,
            half_width: 0.25,
            height: 1.80,
            jump_speed: 8.0,
            hit_half_width: 0.35,
            render_scale: 1.0,
            speed: 3.5,
            turn_speed: 200.0,
            knockback_h: 6.0,
            knockback_v: 5.0,
            flee_speed_mult: 1.0,
            tameable: false,
            behavior: Behavior::Hostile,
            hearing_radius: 8.0,
            vision_radius: 16.0,
            vision_angle: 160.0,
            idle_chance: 0.05,
            idle_range: (0.5, 1.5),
            walk_range: (3.0, 8.0),
            loot: vec![],
        })
    }

    fn aggro_of(e: &Entity) -> AggroState {
        match e.brain { Brain::Hostile(h) => h.aggro, _ => panic!("not hostile") }
    }

    // Solid floor at y<64, air above — lets the skeleton stand and walk.
    fn floor(_x: i32, y: i32, _z: i32) -> BlockType {
        if y < 64 { BlockType::Stone } else { BlockType::Air }
    }

    const NIGHT: f32 = -std::f32::consts::FRAC_PI_2; // sun below horizon

    // Hearing a player OUT of the facing cone must not start a chase — it raises the
    // alert/investigate state instead (walk toward the noise), per the design.
    #[test]
    fn noise_behind_triggers_alert_not_chase() {
        let mut skel = Entity::new(0.0, 64.0, 0.0, skeleton_def()); // faces +x
        let behind = [-3.0, 64.0, 0.0]; // within hearing (8), outside the cone
        skel.update_hostile(0.05, floor, |_, _, _| 0, &[Target { pos: behind, sneaking: false }], NIGHT);
        assert_eq!(aggro_of(&skel), AggroState::Alert, "noise should only raise Alert, not aggro");
    }

    // While alerted, once the mob turns to face the noise and gets line of sight, it
    // escalates to a real chase.
    #[test]
    fn alert_escalates_to_chase_on_sight() {
        let mut skel = Entity::new(0.0, 64.0, 0.0, skeleton_def());
        let behind = [-3.0, 64.0, 0.0];
        let targets = [Target { pos: behind, sneaking: false }];
        let mut became_chase = false;
        for _ in 0..40 { // ~2 s: enough to turn ~180° and acquire sight
            skel.update_hostile(0.05, floor, |_, _, _| 0, &targets, NIGHT);
            if matches!(aggro_of(&skel), AggroState::Chasing | AggroState::InCombat) {
                became_chase = true;
                break;
            }
        }
        assert!(became_chase, "alerted skeleton never escalated to a chase on sight");
    }

    // Once aggroed, a skeleton must actually close distance on a stationary target.
    #[test]
    fn chases_toward_target() {
        let mut skel = Entity::new(0.0, 64.0, 0.0, skeleton_def());
        let tgt = [5.0, 64.0, 0.0];
        let targets = [Target { pos: tgt, sneaking: false }];
        let night = -std::f32::consts::FRAC_PI_2;
        let d0 = (tgt[0] - skel.position[0]).hypot(tgt[2] - skel.position[2]);
        for _ in 0..60 { // ~3 s
            skel.update_hostile(0.05, floor, |_, _, _| 0, &targets, night);
        }
        let d1 = (tgt[0] - skel.position[0]).hypot(tgt[2] - skel.position[2]);
        assert!(d1 < d0 - 1.0, "skeleton did not approach target: {d0:.2} -> {d1:.2} (pos {:?})", skel.position);
    }

    // A fresh skeleton (yaw 0 ⇒ faces +x) must see a sneaking player straight ahead
    // (beyond hearing range, inside the vision cone), but not one directly behind.
    #[test]
    fn vision_cone_points_where_it_faces() {
        let night = -std::f32::consts::FRAC_PI_2;
        let mut ahead = Entity::new(0.0, 64.0, 0.0, skeleton_def());
        assert_eq!(ahead.yaw, 0.0, "test assumes the skeleton spawns facing +x");
        let front = [10.0, 64.0, 0.0]; // +x, in front, 10m (vision 16, hearing 8)
        ahead.update_hostile(0.05, floor, |_, _, _| 0, &[Target { pos: front, sneaking: true }], night);
        assert_ne!(aggro_of(&ahead), AggroState::Idle, "did not see a player straight ahead");

        let mut behind = Entity::new(0.0, 64.0, 0.0, skeleton_def());
        let back = [-10.0, 64.0, 0.0]; // -x, behind
        behind.update_hostile(0.05, floor, |_, _, _| 0, &[Target { pos: back, sneaking: true }], night);
        assert_eq!(aggro_of(&behind), AggroState::Idle, "saw a sneaking player behind its back");
    }
}
