use serde::{Serialize, Deserialize};

pub const PROTOCOL_ID: u64 = 0x5755_5246_454C_4E44; // "WURFELND"
pub const SERVER_PORT: u16 = 25565;

/// Move `v` toward `target` by at most `step` (linear ease used for crouch/walk blends).
pub fn ease(v: &mut f32, target: f32, step: f32) {
    if (target - *v).abs() <= step { *v = target; }
    else { *v += step.copysign(target - *v); }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum ClientMessage {
    PlayerState { x: f32, y: f32, z: f32, yaw: f32, pitch: f32, health: u8, sneaking: bool },
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

/// Render-side snapshot of a remote player (not sent over the wire — assembled
/// locally for drawing). `anim_time`/`move_amount` drive the walk cycle;
/// `crouch_t` drives the crouch; `pitch` tilts the head.
#[derive(Clone, Copy)]
pub struct PeerView {
    pub pos: [f32; 3],
    pub yaw: f32,
    pub health: u8,
    pub sneaking: bool,
    pub anim_time: f32,
    /// Smoothed walk amplitude (0 = still, 1 = full stride).
    pub move_amount: f32,
    /// Smoothed crouch amount (0 = standing, 1 = crouched).
    pub crouch_t: f32,
    /// Look pitch in degrees (tilts the head).
    pub pitch: f32,
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
    PeerState    { id: u64, x: f32, y: f32, z: f32, yaw: f32, pitch: f32, health: u8, sneaking: bool },
    BlockChange  { x: i32, y: i32, z: i32, block_id: u8 },
    EntityUpdate { entities: Vec<NetEntity> },
    TimeUpdate   { sun_angle: f32 },
    ItemUpdate   { items: Vec<NetItem> },
    InventoryAdd { item_id: u8 },
}
