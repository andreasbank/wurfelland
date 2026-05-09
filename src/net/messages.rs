use serde::{Serialize, Deserialize};

pub const PROTOCOL_ID: u64 = 0x5755_5246_454C_4E44; // "WURFELND"
pub const SERVER_PORT: u16 = 25565;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum ClientMessage {
    PlayerState { x: f32, y: f32, z: f32, yaw: f32, pitch: f32 },
    BreakBlock  { x: i32, y: i32, z: i32 },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum ServerMessage {
    PeerJoined  { id: u64 },
    PeerLeft    { id: u64 },
    PeerState   { id: u64, x: f32, y: f32, z: f32, yaw: f32, pitch: f32 },
    BlockChange { x: i32, y: i32, z: i32, block_id: u8 },
}
