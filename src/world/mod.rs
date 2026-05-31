pub mod world;
pub use world::World;

/// World height in chunk units (16 blocks each).  256 blocks total.
pub const WORLD_HEIGHT_CHUNKS: i32 = 16;
/// Sea level in world-block coordinates.  127 blocks of underground space (y=0..127)
/// matches modern Minecraft's depth (-64 to 63).
pub const SEA_LEVEL: i32 = 127;

pub mod biome;

pub mod chunk;

pub mod block;
pub use block::BlockType;

pub mod face;
pub use face::Face;

pub mod item;
pub use item::{ItemType, ItemEntity};

pub mod entity_def;
pub use entity_def::EntityRegistry;

pub mod entity;
pub use entity::{Chicken, Pig, Penguin, Skeleton, Cat, Cow, nearest_entity_hit};

// ── Placed workbench prop ─────────────────────────────────────────────────────
pub struct WorkbenchProp {
    /// World position of the primary (first-placed) block.
    pub pos: [i32; 3],
    /// Offset from pos to the second block along X or Z (e.g. dx=1 → second block at pos+[1,0,0]).
    pub dx: i32,
    pub dz: i32,
}

impl WorkbenchProp {
    /// World-space centre of the 2-block footprint, at ground level.
    pub fn center(&self) -> [f32; 3] {
        [
            self.pos[0] as f32 + self.dx as f32 * 0.5 + 0.5,
            self.pos[1] as f32,
            self.pos[2] as f32 + self.dz as f32 * 0.5 + 0.5,
        ]
    }

    /// Rotation around Y so the long axis (mesh X) aligns with dx/dz direction.
    pub fn yaw(&self) -> f32 {
        use std::f32::consts::{PI, FRAC_PI_2};
        if self.dx == 1      { 0.0       }
        else if self.dx == -1 { PI        }
        else if self.dz == 1  { FRAC_PI_2 }
        else                   { -FRAC_PI_2 }
    }

    /// True if this workbench occupies the given world block position.
    pub fn contains_block(&self, x: i32, y: i32, z: i32) -> bool {
        let [px, py, pz] = self.pos;
        (x == px && y == py && z == pz)
            || (x == px + self.dx && y == py && z == pz + self.dz)
    }
}