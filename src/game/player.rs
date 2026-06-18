use crate::world::item::ItemType;

pub const INVENTORY_COLS: usize = 3;
pub const INVENTORY_ROWS: usize = 6;
pub const INVENTORY_SIZE: usize = INVENTORY_COLS * INVENTORY_ROWS;

const GRAVITY: f32 = -20.0;
const WATER_GRAVITY: f32 = -5.0;
const JUMP_SPEED: f32 = 8.0;
const MOVE_SPEED: f32 = 5.0;
const RUN_SPEED: f32 = 10.0;
const SNEAK_SPEED: f32 = 1.6;       // crouch-walk speed
const EYE_HEIGHT: f32 = 1.6;        // standing camera height above feet
const SNEAK_EYE_HEIGHT: f32 = 1.3;  // crouched camera height
const WATER_SPEED: f32 = 2.2;
const SWIM_UP_SPEED: f32 = 3.5;
const MAX_WATER_FALL: f32 = -2.0;
const HALF_WIDTH: f32 = 0.3; // Player is 0.6 wide
const PLAYER_HEIGHT: f32 = 1.8;
const SNEAK_HEIGHT: f32 = 1.5; // shorter collision box while crouched
const FLY_SPEED: f32 = 21.0;
const FLY_MOVE_SPEED: f32 = MOVE_SPEED * 6.3;

pub struct Player {
    pub health: u32,
    pub position: [f32; 3], // feet position
    pub yaw: f32,
    pub pitch: f32,
    pub velocity: [f32; 3],
    pub on_ground: bool,
    pub flying: bool,
    /// Crouch-sneak: slower, lower camera, no footstep noise.
    pub sneaking: bool,
    /// Footstep loudness this frame (0 = silent). Low while sneaking.
    pub noise: f32,
    pub inventory: [Option<(ItemType, u32)>; INVENTORY_SIZE],
    mouse_sensitivity: f32,
    fall_peak_y: f32,
    prev_on_ground: bool,
}

impl Player {
    pub fn new() -> Self {
        // Players start with an empty inventory. Any starting loadout is applied
        // by the New Game handler (DEBUG builds only) in main.rs.
        let inventory = [None; INVENTORY_SIZE];
        Player {
            health: 100,
            position: [0.0, 64.0, 0.0],
            yaw: -90.0,
            pitch: 0.0,
            velocity: [0.0; 3],
            on_ground: false,
            flying: false,
            sneaking: false,
            noise: 0.0,
            inventory,
            mouse_sensitivity: 0.1,
            fall_peak_y: 64.0,
            prev_on_ground: true,
        }
    }

    /// Places `count` of `item` in inventory (stacks onto existing, then empty slot).
    /// Returns true on success, false if inventory is full.
    pub fn pick_up_stack(&mut self, item: ItemType, count: u32) -> bool {
        for slot in self.inventory.iter_mut() {
            if let Some((t, c)) = slot {
                if *t == item { *c += count; return true; }
            }
        }
        if let Some(slot) = self.inventory.iter_mut().find(|s| s.is_none()) {
            *slot = Some((item, count));
            return true;
        }
        false
    }

    /// Places the item in the first empty inventory slot.
    /// Returns true if picked up, false if inventory is full.
    pub fn pick_up(&mut self, item: ItemType) -> bool {
        // Stack onto existing slot of the same type first
        for slot in self.inventory.iter_mut() {
            if let Some((t, count)) = slot {
                if *t == item { *count += 1; return true; }
            }
        }
        // Otherwise place in first empty slot
        if let Some(slot) = self.inventory.iter_mut().find(|s| s.is_none()) {
            *slot = Some((item, 1));
            true
        } else {
            false
        }
    }

    pub fn process_mouse_movement(&mut self, xoffset: f32, yoffset: f32) {
        self.yaw += xoffset * self.mouse_sensitivity;
        self.pitch += yoffset * self.mouse_sensitivity;
        self.pitch = self.pitch.clamp(-89.0, 89.0);
    }

    // Sets horizontal velocity based on input — apply_physics does the actual moving
    pub fn swim_up(&mut self) {
        self.velocity[1] = self.velocity[1].max(SWIM_UP_SPEED);
    }

    pub fn fly_up(&mut self) {
        self.velocity[1] = FLY_SPEED;
    }

    pub fn fly_down(&mut self) {
        self.velocity[1] = -FLY_SPEED;
    }

    pub fn stop_flying(&mut self) {
        self.flying = false;
        self.velocity[1] = 0.0;
    }

    /// Camera height above the feet — lowered while sneaking (crouch).
    pub fn eye_height(&self) -> f32 {
        if self.sneaking && !self.flying { SNEAK_EYE_HEIGHT } else { EYE_HEIGHT }
    }

    pub fn walk(&mut self, forward: bool, back: bool, left: bool, right: bool, running: bool, sneaking: bool, in_water: bool, flying: bool) {
        let speed = if flying { FLY_MOVE_SPEED }
            else if in_water { WATER_SPEED }
            else if sneaking { SNEAK_SPEED }
            else if running { RUN_SPEED }
            else { MOVE_SPEED };
        let mut vx = 0.0f32;
        let mut vz = 0.0f32;

        if forward {
            vx += self.yaw.to_radians().cos();
            vz += self.yaw.to_radians().sin();
        }
        if back {
            vx -= self.yaw.to_radians().cos();
            vz -= self.yaw.to_radians().sin();
        }
        if left {
            vx -= (self.yaw + 90.0).to_radians().cos();
            vz -= (self.yaw + 90.0).to_radians().sin();
        }
        if right {
            vx += (self.yaw + 90.0).to_radians().cos();
            vz += (self.yaw + 90.0).to_radians().sin();
        }

        // Normalize so diagonal movement isn't faster
        let len = (vx * vx + vz * vz).sqrt();
        if len > 0.0 {
            self.velocity[0] = vx / len * speed;
            self.velocity[2] = vz / len * speed;
        } else {
            self.velocity[0] = 0.0;
            self.velocity[2] = 0.0;
        }
    }

    /// Call once per frame after `apply_physics`. Returns fall damage to apply
    /// (0 if the landing was safe). Caller decides whether to apply it (god mode).
    pub fn tick_fall(&mut self, in_water: bool) -> u32 {
        if in_water {
            self.fall_peak_y = self.position[1];
            self.prev_on_ground = self.on_ground;
            return 0;
        }
        let damage = if self.on_ground && !self.prev_on_ground {
            // Just landed — compute how far we fell from the peak
            let fall_dist = (self.fall_peak_y - self.position[1]).max(0.0);
            if fall_dist > 3.0 {
                ((fall_dist - 3.0).floor() as u32).saturating_mul(5)
            } else {
                0
            }
        } else {
            0
        };
        if self.on_ground {
            self.fall_peak_y = self.position[1];
        } else {
            self.fall_peak_y = self.fall_peak_y.max(self.position[1]);
        }
        self.prev_on_ground = self.on_ground;
        damage
    }

    pub fn jump(&mut self) {
        if self.on_ground {
            self.velocity[1] = JUMP_SPEED;
            self.on_ground = false;
        }
    }

    /// Current collision height — shorter while crouch-sneaking.
    pub fn collision_height(&self) -> f32 {
        if self.sneaking { SNEAK_HEIGHT } else { PLAYER_HEIGHT }
    }

    /// Update the crouch state from input. While airborne/flying you can't sneak;
    /// when releasing sneak you stay crouched if there's no headroom to stand up.
    pub fn update_sneak(&mut self, wants: bool, is_solid: impl Fn(i32, i32, i32) -> bool) {
        self.sneaking = if self.flying {
            false
        } else if wants {
            true
        } else {
            // Releasing: only stand if the full-height box is clear above.
            self.aabb_collides_h(PLAYER_HEIGHT, &is_solid)
        };
    }

    /// True if a solid block sits directly under the player's footprint — used by
    /// the sneak edge-guard to stop you walking off a ledge while crouched.
    fn supported(&self, is_solid: &impl Fn(i32, i32, i32) -> bool) -> bool {
        let by = (self.position[1] - 0.5).floor() as i32;
        let min_x = (self.position[0] - HALF_WIDTH).floor() as i32;
        let max_x = (self.position[0] + HALF_WIDTH - 0.001).floor() as i32;
        let min_z = (self.position[2] - HALF_WIDTH).floor() as i32;
        let max_z = (self.position[2] + HALF_WIDTH - 0.001).floor() as i32;
        for x in min_x..=max_x {
            for z in min_z..=max_z {
                if is_solid(x, by, z) { return true; }
            }
        }
        false
    }

    pub fn apply_physics(&mut self, delta_time: f32, in_water: bool, is_solid: impl Fn(i32, i32, i32) -> bool) {
        // Sneak edge-guard: while crouched on the ground, don't let a horizontal
        // move carry the player off a ledge (Minecraft-style).
        let edge_guard = self.sneaking && self.on_ground && !self.flying && !in_water;
        if self.flying {
            // Gravity-free: vertical velocity comes entirely from fly_up/fly_down input.
            // Damp it so the player hovers when no key is held.
            self.velocity[1] *= (1.0 - 15.0 * delta_time).max(0.0);
        } else if in_water {
            // Clamp immediately so diving in at terminal velocity doesn't punch through the floor
            self.velocity[1] = self.velocity[1].max(MAX_WATER_FALL);
            self.velocity[1] += WATER_GRAVITY * delta_time;
            self.velocity[1] = self.velocity[1].max(MAX_WATER_FALL);
        } else {
            self.velocity[1] += GRAVITY * delta_time;
            self.velocity[1] = self.velocity[1].max(-50.0);
        }

        // Move X, resolve X collisions
        self.position[0] += self.velocity[0] * delta_time;
        if self.aabb_collides(&is_solid)
            || (edge_guard && !self.supported(&is_solid)) {
            self.position[0] -= self.velocity[0] * delta_time;
            self.velocity[0] = 0.0;
        }

        // Move Y, resolve Y collisions
        self.position[1] += self.velocity[1] * delta_time;
        if self.aabb_collides(&is_solid) {
            if self.velocity[1] < 0.0 {
                // Snap feet to top of block below
                self.position[1] = self.position[1].floor() + 1.0;
                self.on_ground = true;
            } else {
                // Hit ceiling — snap head to bottom of block above
                let h = self.collision_height();
                self.position[1] = (self.position[1] + h).floor() - h;
            }
            self.velocity[1] = 0.0;
        } else {
            self.on_ground = false;
        }

        // Move Z, resolve Z collisions
        self.position[2] += self.velocity[2] * delta_time;
        if self.aabb_collides(&is_solid)
            || (edge_guard && !self.supported(&is_solid)) {
            self.position[2] -= self.velocity[2] * delta_time;
            self.velocity[2] = 0.0;
        }
    }

    // Returns true if the player's AABB overlaps any solid block.
    fn aabb_collides(&self, is_solid: &impl Fn(i32, i32, i32) -> bool) -> bool {
        self.aabb_collides_h(self.collision_height(), is_solid)
    }

    // AABB-vs-solid test at an arbitrary height (for crouch/stand-up checks).
    fn aabb_collides_h(&self, height: f32, is_solid: &impl Fn(i32, i32, i32) -> bool) -> bool {
        let min_x = (self.position[0] - HALF_WIDTH).floor() as i32;
        let max_x = (self.position[0] + HALF_WIDTH - 0.001).floor() as i32;
        let min_y = self.position[1].floor() as i32;
        let max_y = (self.position[1] + height - 0.001).floor() as i32;
        let min_z = (self.position[2] - HALF_WIDTH).floor() as i32;
        let max_z = (self.position[2] + HALF_WIDTH - 0.001).floor() as i32;

        for x in min_x..=max_x {
            for y in min_y..=max_y {
                for z in min_z..=max_z {
                    if is_solid(x, y, z) {
                        return true;
                    }
                }
            }
        }
        false
    }

}
