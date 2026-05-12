use crate::world::block::BlockType;
use crate::world::item::ItemType;

pub trait HittableEntity {
    fn aabb_min(&self) -> [f32; 3];
    fn aabb_max(&self) -> [f32; 3];
    fn is_dead(&self) -> bool;
}

/// Describes which terrain type an entity is comfortable in.
/// Used by the shared wander/escape helpers below.
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum Habitat {
    Land,       // avoids water; escapes if it ends up in it (chickens, pigs)
    Water,      // avoids land; escapes if it ends up on it (future: seals)
    Amphibious, // happy in both (future: ducks)
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
const CHICKEN_SPEED: f32 = 1.5;
const CHICKEN_HALF_W: f32 = 0.20;
const CHICKEN_HEIGHT: f32 = 0.90;
const CHICKEN_JUMP_SPEED: f32 = 8.0;
const CHICKEN_HEALTH: f32 = 4.0;
const CHICKEN_HALF_HIT: f32 = 0.30; // hit-detection AABB half-width (wider than physics to cover head)

pub struct Chicken {
    pub position: [f32; 3],
    pub yaw: f32,       // current facing direction in degrees
    pub anim_time: f32, // drives wing flap animation, staggered per chicken
    pub health: f32,
    velocity: [f32; 3],
    on_ground: bool,
    wander_timer: f32,  // seconds until next AI decision
    target_yaw: f32,    // direction the chicken is walking toward
    move_speed: f32,    // 0 = idle, CHICKEN_SPEED = walking
    knockback: f32,     // > 0 while horizontal knockback velocity should be preserved
}

impl Chicken {
    pub fn new(x: f32, y: f32, z: f32) -> Self {
        let seed = x * 127.1 + z * 311.7;
        let init_yaw = seed.rem_euclid(360.0);
        Chicken {
            position: [x, y, z],
            yaw: init_yaw,
            anim_time: seed.rem_euclid(6.28), // stagger so not all sync'd
            health: CHICKEN_HEALTH,
            velocity: [0.0; 3],
            on_ground: false,
            wander_timer: seed.rem_euclid(5.0),
            target_yaw: init_yaw,
            move_speed: CHICKEN_SPEED,
            knockback: 0.0,
        }
    }

    pub fn is_dead(&self) -> bool { self.health <= 0.0 }

    /// AABB used for hit-detection — wider than physics AABB to cover the head.
    pub fn aabb_min(&self) -> [f32; 3] {
        [self.position[0] - CHICKEN_HALF_HIT, self.position[1], self.position[2] - CHICKEN_HALF_HIT]
    }
    pub fn aabb_max(&self) -> [f32; 3] {
        [self.position[0] + CHICKEN_HALF_HIT, self.position[1] + CHICKEN_HEIGHT, self.position[2] + CHICKEN_HALF_HIT]
    }

    pub fn take_hit(&mut self, push_dir: [f32; 3]) {
        self.health -= 1.0;
        // Knockback: launch in hit direction and upward.
        // knockback > 0 prevents update() from overwriting horizontal velocity.
        self.velocity[0] = push_dir[0] * 6.0;
        self.velocity[1] = 5.0;
        self.velocity[2] = push_dir[2] * 6.0;
        self.on_ground = false;
        self.knockback = 0.4;
        // Flee away from attacker
        self.target_yaw = push_dir[0].atan2(push_dir[2]).to_degrees() + 180.0;
        self.move_speed = CHICKEN_SPEED;
        self.wander_timer = 3.0;
    }

    pub fn interact(&mut self) {
        // stub — future: trading, petting, etc.
    }

    /// Roll drops at death: 50% feather, 5% egg, 20% chicken meat.
    /// Uses anim_time as a cheap per-chicken entropy source.
    pub fn drops(&self) -> Vec<ItemType> {
        let s = self.anim_time;
        let r1 = (s * 127.1).rem_euclid(1.0);
        let r2 = (s * 311.7 + 0.33).rem_euclid(1.0);
        let r3 = (s * 73.1  + 0.67).rem_euclid(1.0);
        let mut out = Vec::new();
        if r1 < 0.50 { out.push(ItemType::Feather); }
        if r2 < 0.05 { out.push(ItemType::Egg); }
        if r3 < 0.20 { out.push(ItemType::ChickenMeat); }
        out
    }

    pub fn update(&mut self, dt: f32, get_block: impl Fn(i32, i32, i32) -> BlockType) {
        self.anim_time += dt;

        // Wander AI
        if in_wrong_habitat(self.position, Habitat::Land, &get_block) {
            // Escape water: turn toward nearest land, move at full speed
            if let Some(ey) = find_escape_yaw(self.position, self.yaw, Habitat::Land, &get_block) {
                self.target_yaw = ey;
            }
            self.move_speed = CHICKEN_SPEED;
        } else {
            self.wander_timer -= dt;
            if self.wander_timer <= 0.0 {
                let seed = self.position[0] * 73.1 + self.position[2] * 47.3 + self.anim_time * 37.9;

                // Try up to 4 candidates; pick first direction that stays on land
                let mut chosen = seed.rem_euclid(360.0);
                for attempt in 0..4u32 {
                    let candidate = (seed + attempt as f32 * 97.3).rem_euclid(360.0);
                    if direction_suits_habitat(self.position, candidate.to_radians(), Habitat::Land, &get_block) {
                        chosen = candidate;
                        break;
                    }
                }
                self.target_yaw = chosen;

                // ~1 in 3 chance of pausing to look around
                if (seed * 13.0) as i32 % 3 == 0 {
                    self.move_speed = 0.0;
                    self.wander_timer = 2.0 + (seed * 0.01).rem_euclid(3.0);
                } else {
                    self.move_speed = CHICKEN_SPEED;
                    self.wander_timer = 5.0 + (seed * 0.01).rem_euclid(7.0);
                }
            }
        }

        // Smoothly turn toward target direction (120 deg/s)
        let diff = angle_diff(self.target_yaw, self.yaw);
        let turn_step = 120.0_f32 * dt;
        if diff.abs() <= turn_step {
            self.yaw = self.target_yaw;
        } else {
            self.yaw += diff.signum() * turn_step;
        }

        // Horizontal velocity from current facing — suppressed while knocked back.
        if self.knockback > 0.0 {
            self.knockback -= dt;
        } else {
            let yr = self.yaw.to_radians();
            self.velocity[0] = yr.cos() * self.move_speed;
            self.velocity[2] = yr.sin() * self.move_speed;
        }

        // Gravity while airborne
        if !self.on_ground {
            self.velocity[1] += GRAVITY * dt;
            self.velocity[1] = self.velocity[1].max(-50.0);
        }

        let is_solid = |x: i32, y: i32, z: i32| get_block(x, y, z).is_solid();

        // Move X — jump over one-block walls
        self.position[0] += self.velocity[0] * dt;
        if aabb_collides(self.position, CHICKEN_HALF_W, CHICKEN_HEIGHT, &is_solid) {
            self.position[0] -= self.velocity[0] * dt;
            self.velocity[0] = 0.0;
            if self.on_ground {
                self.velocity[1] = CHICKEN_JUMP_SPEED;
                self.on_ground = false;
            }
        }

        // Move Y
        self.position[1] += self.velocity[1] * dt;
        if aabb_collides(self.position, CHICKEN_HALF_W, CHICKEN_HEIGHT, &is_solid) {
            if self.velocity[1] < 0.0 {
                self.position[1] = self.position[1].floor() + 1.0;
                self.on_ground = true;
            } else {
                self.position[1] = (self.position[1] + CHICKEN_HEIGHT).floor() - CHICKEN_HEIGHT;
            }
            self.velocity[1] = 0.0;
        } else {
            self.on_ground = false;
        }

        // Move Z — jump over one-block walls
        self.position[2] += self.velocity[2] * dt;
        if aabb_collides(self.position, CHICKEN_HALF_W, CHICKEN_HEIGHT, &is_solid) {
            self.position[2] -= self.velocity[2] * dt;
            self.velocity[2] = 0.0;
            if self.on_ground {
                self.velocity[1] = CHICKEN_JUMP_SPEED;
                self.on_ground = false;
            }
        }
    }
}

impl HittableEntity for Chicken {
    fn aabb_min(&self) -> [f32; 3] { self.aabb_min() }
    fn aabb_max(&self) -> [f32; 3] { self.aabb_max() }
    fn is_dead(&self) -> bool { self.is_dead() }
}

// ─── Pig ──────────────────────────────────────────────────────────────────────

const PIG_SPEED: f32 = 1.2;
const PIG_HALF_W: f32 = 0.28;
const PIG_HEIGHT: f32 = 0.90;
const PIG_JUMP_SPEED: f32 = 7.0;
const PIG_HEALTH: f32 = 10.0;
const PIG_HALF_HIT: f32 = 0.38;

pub struct Pig {
    pub position: [f32; 3],
    pub yaw: f32,
    pub anim_time: f32,
    pub health: f32,
    velocity: [f32; 3],
    on_ground: bool,
    wander_timer: f32,
    target_yaw: f32,
    move_speed: f32,
    knockback: f32,
}

impl Pig {
    pub fn new(x: f32, y: f32, z: f32) -> Self {
        let seed = x * 211.3 + z * 491.7;
        let init_yaw = seed.rem_euclid(360.0);
        Pig {
            position: [x, y, z],
            yaw: init_yaw,
            anim_time: seed.rem_euclid(6.28),
            health: PIG_HEALTH,
            velocity: [0.0; 3],
            on_ground: false,
            wander_timer: seed.rem_euclid(6.0),
            target_yaw: init_yaw,
            move_speed: PIG_SPEED,
            knockback: 0.0,
        }
    }

    pub fn is_dead(&self) -> bool { self.health <= 0.0 }

    pub fn aabb_min(&self) -> [f32; 3] {
        [self.position[0] - PIG_HALF_HIT, self.position[1], self.position[2] - PIG_HALF_HIT]
    }
    pub fn aabb_max(&self) -> [f32; 3] {
        [self.position[0] + PIG_HALF_HIT, self.position[1] + PIG_HEIGHT, self.position[2] + PIG_HALF_HIT]
    }

    pub fn take_hit(&mut self, push_dir: [f32; 3]) {
        self.health -= 2.0;
        self.velocity[0] = push_dir[0] * 5.0;
        self.velocity[1] = 4.5;
        self.velocity[2] = push_dir[2] * 5.0;
        self.on_ground = false;
        self.knockback = 0.4;
        self.target_yaw = push_dir[0].atan2(push_dir[2]).to_degrees() + 180.0;
        self.move_speed = PIG_SPEED;
        self.wander_timer = 3.0;
    }

    pub fn drops(&self) -> Vec<ItemType> {
        vec![ItemType::PorkChop]
    }

    /// Returns 1.0 when walking at full speed, 0.0 when idle (for animation scaling).
    pub fn move_speed_norm(&self) -> f32 {
        if self.move_speed > 0.0 { 1.0 } else { 0.0 }
    }

    pub fn update(&mut self, dt: f32, get_block: impl Fn(i32, i32, i32) -> BlockType) {
        self.anim_time += dt;

        // Wander AI
        if in_wrong_habitat(self.position, Habitat::Land, &get_block) {
            // Escape water: turn toward nearest land, move at full speed
            if let Some(ey) = find_escape_yaw(self.position, self.yaw, Habitat::Land, &get_block) {
                self.target_yaw = ey;
            }
            self.move_speed = PIG_SPEED;
        } else {
            self.wander_timer -= dt;
            if self.wander_timer <= 0.0 {
                let seed = self.position[0] * 91.3 + self.position[2] * 53.7 + self.anim_time * 41.1;

                // Try up to 4 candidates; pick first direction that stays on land
                let mut chosen = seed.rem_euclid(360.0);
                for attempt in 0..4u32 {
                    let candidate = (seed + attempt as f32 * 113.7).rem_euclid(360.0);
                    if direction_suits_habitat(self.position, candidate.to_radians(), Habitat::Land, &get_block) {
                        chosen = candidate;
                        break;
                    }
                }
                self.target_yaw = chosen;

                if (seed * 17.0) as i32 % 4 == 0 {
                    self.move_speed = 0.0;
                    self.wander_timer = 3.0 + (seed * 0.01).rem_euclid(4.0);
                } else {
                    self.move_speed = PIG_SPEED;
                    self.wander_timer = 6.0 + (seed * 0.01).rem_euclid(8.0);
                }
            }
        }

        let diff = angle_diff(self.target_yaw, self.yaw);
        let turn_step = 90.0_f32 * dt;
        if diff.abs() <= turn_step {
            self.yaw = self.target_yaw;
        } else {
            self.yaw += diff.signum() * turn_step;
        }

        if self.knockback > 0.0 {
            self.knockback -= dt;
        } else {
            let yr = self.yaw.to_radians();
            self.velocity[0] = yr.cos() * self.move_speed;
            self.velocity[2] = yr.sin() * self.move_speed;
        }

        if !self.on_ground {
            self.velocity[1] += GRAVITY * dt;
            self.velocity[1] = self.velocity[1].max(-50.0);
        }

        let is_solid = |x: i32, y: i32, z: i32| get_block(x, y, z).is_solid();

        self.position[0] += self.velocity[0] * dt;
        if aabb_collides(self.position, PIG_HALF_W, PIG_HEIGHT, &is_solid) {
            self.position[0] -= self.velocity[0] * dt;
            self.velocity[0] = 0.0;
            if self.on_ground {
                self.velocity[1] = PIG_JUMP_SPEED;
                self.on_ground = false;
            }
        }

        self.position[1] += self.velocity[1] * dt;
        if aabb_collides(self.position, PIG_HALF_W, PIG_HEIGHT, &is_solid) {
            if self.velocity[1] < 0.0 {
                self.position[1] = self.position[1].floor() + 1.0;
                self.on_ground = true;
            } else {
                self.position[1] = (self.position[1] + PIG_HEIGHT).floor() - PIG_HEIGHT;
            }
            self.velocity[1] = 0.0;
        } else {
            self.on_ground = false;
        }

        self.position[2] += self.velocity[2] * dt;
        if aabb_collides(self.position, PIG_HALF_W, PIG_HEIGHT, &is_solid) {
            self.position[2] -= self.velocity[2] * dt;
            self.velocity[2] = 0.0;
            if self.on_ground {
                self.velocity[1] = PIG_JUMP_SPEED;
                self.on_ground = false;
            }
        }
    }
}

impl HittableEntity for Pig {
    fn aabb_min(&self) -> [f32; 3] { self.aabb_min() }
    fn aabb_max(&self) -> [f32; 3] { self.aabb_max() }
    fn is_dead(&self) -> bool { self.is_dead() }
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

fn angle_diff(target: f32, current: f32) -> f32 {
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
