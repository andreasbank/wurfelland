pub mod world;
pub use world::World;

/// World height in chunk units (16 blocks each).  256 blocks total = classic Minecraft height.
pub const WORLD_HEIGHT_CHUNKS: i32 = 16;
/// Sea level in world-block coordinates, matching Minecraft classic.
pub const SEA_LEVEL: i32 = 63;

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
pub use entity::{Chicken, Pig, Penguin, nearest_entity_hit};