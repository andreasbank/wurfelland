use crate::world::item::ItemType;

pub const INVENTORY_COLS: usize = 3;
pub const INVENTORY_ROWS: usize = 6;
pub const INVENTORY_SIZE: usize = INVENTORY_COLS * INVENTORY_ROWS;

const GRAVITY: f32 = -20.0;
const JUMP_SPEED: f32 = 8.0;
const MOVE_SPEED: f32 = 5.0;
const RUN_SPEED: f32 = 10.0;
const HALF_WIDTH: f32 = 0.3; // Player is 0.6 wide
const PLAYER_HEIGHT: f32 = 1.8;

pub struct Player {
    pub health: u32,
    pub position: [f32; 3], // feet position
    pub yaw: f32,
    pub pitch: f32,
    pub velocity: [f32; 3],
    pub on_ground: bool,
    pub inventory: [Option<(ItemType, u32)>; INVENTORY_SIZE],
    mouse_sensitivity: f32,
}

impl Player {
    pub fn new() -> Self {
        Player {
            health: 100,
            position: [0.0, 64.0, 0.0],
            yaw: -90.0,
            pitch: 0.0,
            velocity: [0.0; 3],
            on_ground: false,
            inventory: [None; INVENTORY_SIZE],
            mouse_sensitivity: 0.1,
        }
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
    pub fn walk(&mut self, forward: bool, back: bool, left: bool, right: bool, running: bool) {
        let speed = if running { RUN_SPEED } else { MOVE_SPEED };
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

    pub fn jump(&mut self) {
        if self.on_ground {
            self.velocity[1] = JUMP_SPEED;
            self.on_ground = false;
        }
    }

    pub fn apply_physics(&mut self, delta_time: f32, is_solid: impl Fn(i32, i32, i32) -> bool) {
        // Gravity
        self.velocity[1] += GRAVITY * delta_time;
        self.velocity[1] = self.velocity[1].max(-50.0);

        // Move X, resolve X collisions
        self.position[0] += self.velocity[0] * delta_time;
        if self.aabb_collides(&is_solid) {
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
                self.position[1] = (self.position[1] + PLAYER_HEIGHT).floor() - PLAYER_HEIGHT;
            }
            self.velocity[1] = 0.0;
        } else {
            self.on_ground = false;
        }

        // Move Z, resolve Z collisions
        self.position[2] += self.velocity[2] * delta_time;
        if self.aabb_collides(&is_solid) {
            self.position[2] -= self.velocity[2] * delta_time;
            self.velocity[2] = 0.0;
        }
    }

    // Returns true if the player's AABB overlaps any solid block
    fn aabb_collides(&self, is_solid: &impl Fn(i32, i32, i32) -> bool) -> bool {
        let min_x = (self.position[0] - HALF_WIDTH).floor() as i32;
        let max_x = (self.position[0] + HALF_WIDTH - 0.001).floor() as i32;
        let min_y = self.position[1].floor() as i32;
        let max_y = (self.position[1] + PLAYER_HEIGHT - 0.001).floor() as i32;
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
