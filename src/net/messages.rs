use serde::{Serialize, Deserialize};

pub const PROTOCOL_ID: u64 = 0x5755_5246_454C_4E44; // "WURFELND"
pub const SERVER_PORT: u16 = 25565;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum ClientMessage {
    PlayerState { x: f32, y: f32, z: f32, yaw: f32, pitch: f32, health: u8 },
    BreakBlock  { x: i32, y: i32, z: i32 },
    PlaceBlock  { x: i32, y: i32, z: i32, block_id: u8 },
    AttackEntity   { index: u32, push_x: f32, push_z: f32 },
    InteractEntity { index: u32 },
    PickupItem  { x: f32, y: f32, z: f32 },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct NetEntity {
    /// Entity-def identifier (species) so the client can build the right mob.
    pub kind: String,
    pub x: f32, pub y: f32, pub z: f32,
    pub yaw: f32,
    pub health: f32,
    pub sitting: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct NetItem {
    pub x: f32, pub y: f32, pub z: f32,
    pub item_id: u8,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum ServerMessage {
    WorldInfo    { seed: u32 },
    PeerJoined   { id: u64 },
    PeerLeft     { id: u64 },
    PeerState    { id: u64, x: f32, y: f32, z: f32, yaw: f32, pitch: f32, health: u8 },
    BlockChange  { x: i32, y: i32, z: i32, block_id: u8 },
    EntityUpdate { entities: Vec<NetEntity> },
    TimeUpdate   { sun_angle: f32 },
    ItemUpdate   { items: Vec<NetItem> },
    InventoryAdd { item_id: u8 },
}
