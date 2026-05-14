use std::sync::Arc;
use crate::world::block::BlockType;
use crate::world::item::ItemType;
use crate::world::entity_def::EntityDef;

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
pub struct Chicken {
    pub position: [f32; 3],
    pub yaw: f32,
    pub anim_time: f32,
    pub health: f32,
    pub def: Arc<EntityDef>,
    velocity: [f32; 3],
    on_ground: bool,
    wander_timer: f32,
    target_yaw: f32,
    move_speed: f32,
    knockback: f32,
}

impl Chicken {
    pub fn new(x: f32, y: f32, z: f32, def: Arc<EntityDef>) -> Self {
        let seed = x * 127.1 + z * 311.7;
        let init_yaw = seed.rem_euclid(360.0);
        let speed = def.speed;
        Chicken {
            position: [x, y, z],
            yaw: init_yaw,
            anim_time: seed.rem_euclid(6.28),
            health: def.max_health,
            def,
            velocity: [0.0; 3],
            on_ground: false,
            wander_timer: seed.rem_euclid(5.0),
            target_yaw: init_yaw,
            move_speed: speed,
            knockback: 0.0,
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
        self.velocity[0] = push_dir[0] * 6.0;
        self.velocity[1] = 5.0;
        self.velocity[2] = push_dir[2] * 6.0;
        self.on_ground = false;
        self.knockback = 0.4;
        self.target_yaw = push_dir[0].atan2(push_dir[2]).to_degrees() + 180.0;
        self.move_speed = self.def.speed;
        self.wander_timer = 3.0;
    }

    pub fn interact(&mut self) {}

    pub fn drops(&self) -> Vec<ItemType> {
        let s = self.anim_time;
        self.def.loot.iter().enumerate().filter_map(|(i, entry)| {
            let r = (s * (127.1 + i as f32 * 73.7)).rem_euclid(1.0);
            if r < entry.chance { Some(entry.item) } else { None }
        }).collect()
    }

    pub fn update(&mut self, dt: f32, get_block: impl Fn(i32, i32, i32) -> BlockType) {
        self.anim_time += dt;

        let habitat = self.def.habitat;
        if in_wrong_habitat(self.position, habitat, &get_block) {
            if let Some(ey) = find_escape_yaw(self.position, self.yaw, habitat, &get_block) {
                self.target_yaw = ey;
            }
            self.move_speed = self.def.speed;
        } else {
            self.wander_timer -= dt;
            if self.wander_timer <= 0.0 {
                let seed = self.position[0] * 73.1 + self.position[2] * 47.3 + self.anim_time * 37.9;
                let mut chosen = seed.rem_euclid(360.0);
                for attempt in 0..4u32 {
                    let candidate = (seed + attempt as f32 * 97.3).rem_euclid(360.0);
                    if direction_suits_habitat(self.position, candidate.to_radians(), habitat, &get_block) {
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

        let diff = angle_diff(self.target_yaw, self.yaw);
        let turn_step = 120.0_f32 * dt;
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

impl HittableEntity for Chicken {
    fn aabb_min(&self) -> [f32; 3] { self.aabb_min() }
    fn aabb_max(&self) -> [f32; 3] { self.aabb_max() }
    fn is_dead(&self) -> bool { self.is_dead() }
}

// ─── Pig ──────────────────────────────────────────────────────────────────────

pub struct Pig {
    pub position: [f32; 3],
    pub yaw: f32,
    pub anim_time: f32,
    pub health: f32,
    pub def: Arc<EntityDef>,
    velocity: [f32; 3],
    on_ground: bool,
    wander_timer: f32,
    target_yaw: f32,
    move_speed: f32,
    knockback: f32,
}

impl Pig {
    pub fn new(x: f32, y: f32, z: f32, def: Arc<EntityDef>) -> Self {
        let seed = x * 211.3 + z * 491.7;
        let init_yaw = seed.rem_euclid(360.0);
        let speed = def.speed;
        Pig {
            position: [x, y, z],
            yaw: init_yaw,
            anim_time: seed.rem_euclid(6.28),
            health: def.max_health,
            def,
            velocity: [0.0; 3],
            on_ground: false,
            wander_timer: seed.rem_euclid(6.0),
            target_yaw: init_yaw,
            move_speed: speed,
            knockback: 0.0,
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
        self.velocity[0] = push_dir[0] * 5.0;
        self.velocity[1] = 4.5;
        self.velocity[2] = push_dir[2] * 5.0;
        self.on_ground = false;
        self.knockback = 0.4;
        self.target_yaw = push_dir[0].atan2(push_dir[2]).to_degrees() + 180.0;
        self.move_speed = self.def.speed;
        self.wander_timer = 3.0;
    }

    pub fn drops(&self) -> Vec<ItemType> {
        let s = self.anim_time;
        self.def.loot.iter().enumerate().filter_map(|(i, entry)| {
            let r = (s * (211.3 + i as f32 * 97.1)).rem_euclid(1.0);
            if r < entry.chance { Some(entry.item) } else { None }
        }).collect()
    }

    pub fn move_speed_norm(&self) -> f32 {
        if self.move_speed > 0.0 { 1.0 } else { 0.0 }
    }

    pub fn update(&mut self, dt: f32, get_block: impl Fn(i32, i32, i32) -> BlockType) {
        self.anim_time += dt;

        let habitat = self.def.habitat;
        if in_wrong_habitat(self.position, habitat, &get_block) {
            if let Some(ey) = find_escape_yaw(self.position, self.yaw, habitat, &get_block) {
                self.target_yaw = ey;
            }
            self.move_speed = self.def.speed;
        } else {
            self.wander_timer -= dt;
            if self.wander_timer <= 0.0 {
                let seed = self.position[0] * 91.3 + self.position[2] * 53.7 + self.anim_time * 41.1;
                let mut chosen = seed.rem_euclid(360.0);
                for attempt in 0..4u32 {
                    let candidate = (seed + attempt as f32 * 113.7).rem_euclid(360.0);
                    if direction_suits_habitat(self.position, candidate.to_radians(), habitat, &get_block) {
                        chosen = candidate;
                        break;
                    }
                }
                self.target_yaw = chosen;

                let r = (seed * 11.7).rem_euclid(1.0);
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

        let diff = angle_diff(self.target_yaw, self.yaw);
        let turn_step = 90.0_f32 * dt;
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

impl HittableEntity for Pig {
    fn aabb_min(&self) -> [f32; 3] { self.aabb_min() }
    fn aabb_max(&self) -> [f32; 3] { self.aabb_max() }
    fn is_dead(&self) -> bool { self.is_dead() }
}

// ─── Penguin ─────────────────────────────────────────────────────────────────

pub struct Penguin {
    pub position: [f32; 3],
    pub yaw: f32,
    pub anim_time: f32,
    pub health: f32,
    pub def: Arc<EntityDef>,
    velocity: [f32; 3],
    on_ground: bool,
    wander_timer: f32,
    target_yaw: f32,
    move_speed: f32,
    knockback: f32,
}

impl Penguin {
    pub fn new(x: f32, y: f32, z: f32, def: Arc<EntityDef>) -> Self {
        let seed = x * 163.7 + z * 379.3;
        let init_yaw = seed.rem_euclid(360.0);
        let speed = def.speed;
        Penguin {
            position: [x, y, z],
            yaw: init_yaw,
            anim_time: seed.rem_euclid(6.28),
            health: def.max_health,
            def,
            velocity: [0.0; 3],
            on_ground: false,
            wander_timer: seed.rem_euclid(5.0),
            target_yaw: init_yaw,
            move_speed: speed,
            knockback: 0.0,
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
        self.velocity[0] = push_dir[0] * 5.0;
        self.velocity[1] = 4.5;
        self.velocity[2] = push_dir[2] * 5.0;
        self.on_ground = false;
        self.knockback = 0.4;
        self.target_yaw = push_dir[0].atan2(push_dir[2]).to_degrees() + 180.0;
        self.move_speed = self.def.speed;
        self.wander_timer = 3.0;
    }

    pub fn drops(&self) -> Vec<ItemType> {
        let s = self.anim_time;
        self.def.loot.iter().enumerate().filter_map(|(i, entry)| {
            let r = (s * (163.7 + i as f32 * 83.1)).rem_euclid(1.0);
            if r < entry.chance { Some(entry.item) } else { None }
        }).collect()
    }

    pub fn move_speed_norm(&self) -> f32 {
        if self.move_speed > 0.0 { 1.0 } else { 0.0 }
    }

    pub fn update(&mut self, dt: f32, get_block: impl Fn(i32, i32, i32) -> BlockType) {
        self.anim_time += dt;

        let habitat = self.def.habitat;
        if in_wrong_habitat(self.position, habitat, &get_block) {
            if let Some(ey) = find_escape_yaw(self.position, self.yaw, habitat, &get_block) {
                self.target_yaw = ey;
            }
            self.move_speed = self.def.speed;
        } else {
            self.wander_timer -= dt;
            if self.wander_timer <= 0.0 {
                let seed = self.position[0] * 103.7 + self.position[2] * 61.3 + self.anim_time * 29.7;
                let mut chosen = seed.rem_euclid(360.0);
                for attempt in 0..4u32 {
                    let candidate = (seed + attempt as f32 * 89.1).rem_euclid(360.0);
                    if direction_suits_habitat(self.position, candidate.to_radians(), habitat, &get_block) {
                        chosen = candidate;
                        break;
                    }
                }
                self.target_yaw = chosen;

                let r = (seed * 5.9).rem_euclid(1.0);
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

        let diff = angle_diff(self.target_yaw, self.yaw);
        let turn_step = 80.0_f32 * dt;
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

impl HittableEntity for Penguin {
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
