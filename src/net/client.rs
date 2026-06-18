use std::collections::HashMap;
use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, SystemTime};

use renet::{RenetClient, DefaultChannel};
use renet_netcode::{NetcodeClientTransport, ClientAuthentication};

use super::messages::{ClientMessage, ServerMessage, PeerView, ease, PROTOCOL_ID};

struct RemotePlayer {
    position: [f32; 3],
    yaw: f32,
    pitch: f32,
    health: u8,
    sneaking: bool,
    anim_time: f32,
    moving: bool,
    move_amount: f32,
    crouch_t: f32,
}

pub struct GameClient {
    client: RenetClient,
    transport: NetcodeClientTransport,
    remote_players: HashMap<u64, RemotePlayer>,
}

impl GameClient {
    pub fn connect(server_addr: SocketAddr) -> anyhow::Result<Self> {
        let current_time = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or(Duration::ZERO);

        let client_id = current_time.as_micros() as u64;

        let socket = UdpSocket::bind("0.0.0.0:0")?;
        socket.set_nonblocking(true)?;

        let authentication = ClientAuthentication::Unsecure {
            protocol_id: PROTOCOL_ID,
            client_id,
            server_addr,
            user_data: None,
        };

        let transport = NetcodeClientTransport::new(current_time, authentication, socket)?;
        let client = RenetClient::new(renet::ConnectionConfig::default());

        Ok(GameClient {
            client,
            transport,
            remote_players: HashMap::new(),
        })
    }

    #[allow(dead_code)]
    pub fn is_connected(&self) -> bool {
        self.client.is_connected()
    }

    pub fn is_disconnected(&self) -> bool {
        self.client.is_disconnected()
    }

    pub fn update(&mut self, dt: f32) -> Vec<ServerMessage> {
        let duration = Duration::from_secs_f32(dt);

        self.transport.update(duration, &mut self.client).ok();
        self.client.update(duration);

        // Advance each peer's walk clock and ease crouch + walk amplitude.
        for p in self.remote_players.values_mut() {
            p.anim_time += dt;
            ease(&mut p.crouch_t, if p.sneaking { 1.0 } else { 0.0 }, dt / 0.15);
            ease(&mut p.move_amount, if p.moving { 1.0 } else { 0.0 }, dt / 0.12);
        }

        let mut received = Vec::new();

        // Process reliable messages
        while let Some(bytes) = self.client.receive_message(DefaultChannel::ReliableOrdered) {
            if let Ok(msg) = bincode::deserialize::<ServerMessage>(&bytes) {
                match &msg {
                    ServerMessage::PeerJoined { id } => {
                        self.remote_players.insert(*id, RemotePlayer {
                            position: [0.0, 0.0, 0.0],
                            yaw: 0.0,
                            pitch: 0.0,
                            health: 100,
                            sneaking: false,
                            anim_time: 0.0,
                            moving: false,
                            move_amount: 0.0,
                            crouch_t: 0.0,
                        });
                    }
                    ServerMessage::PeerLeft { id } => {
                        self.remote_players.remove(id);
                    }
                    _ => {}
                }
                received.push(msg);
            }
        }

        // Process unreliable messages
        while let Some(bytes) = self.client.receive_message(DefaultChannel::Unreliable) {
            if let Ok(msg) = bincode::deserialize::<ServerMessage>(&bytes) {
                if let ServerMessage::PeerState { id, x, y, z, yaw, pitch, health, sneaking } = &msg {
                    let player = self.remote_players.entry(*id).or_insert(RemotePlayer {
                        position: [0.0, 0.0, 0.0],
                        yaw: 0.0,
                        pitch: 0.0,
                        health: 100,
                        sneaking: false,
                        anim_time: 0.0,
                        moving: false,
                        move_amount: 0.0,
                        crouch_t: 0.0,
                    });
                    let dx = *x - player.position[0];
                    let dz = *z - player.position[2];
                    player.moving = dx * dx + dz * dz > 1e-6;
                    player.position = [*x, *y, *z];
                    player.yaw = *yaw;
                    player.pitch = *pitch;
                    player.health = *health;
                    player.sneaking = *sneaking;
                }
                received.push(msg);
            }
        }

        self.transport.send_packets(&mut self.client).ok();

        received
    }

    pub fn send_position(&mut self, x: f32, y: f32, z: f32, yaw: f32, pitch: f32, health: u8, sneaking: bool) {
        let msg = ClientMessage::PlayerState { x, y, z, yaw, pitch, health, sneaking };
        if let Ok(bytes) = bincode::serialize(&msg) {
            self.client.send_message(DefaultChannel::Unreliable, bytes);
        }
    }

    pub fn send_block_break(&mut self, x: i32, y: i32, z: i32) {
        let msg = ClientMessage::BreakBlock { x, y, z };
        if let Ok(bytes) = bincode::serialize(&msg) {
            self.client.send_message(DefaultChannel::ReliableOrdered, bytes);
        }
    }

    pub fn send_block_place(&mut self, x: i32, y: i32, z: i32, block_id: u8) {
        let msg = ClientMessage::PlaceBlock { x, y, z, block_id };
        if let Ok(bytes) = bincode::serialize(&msg) {
            self.client.send_message(DefaultChannel::ReliableOrdered, bytes);
        }
    }

    pub fn send_attack_entity(&mut self, index: u32, push_x: f32, push_z: f32) {
        let msg = ClientMessage::AttackEntity { index, push_x, push_z };
        if let Ok(bytes) = bincode::serialize(&msg) {
            self.client.send_message(DefaultChannel::ReliableOrdered, bytes);
        }
    }

    pub fn send_interact_entity(&mut self, index: u32) {
        let msg = ClientMessage::InteractEntity { index };
        if let Ok(bytes) = bincode::serialize(&msg) {
            self.client.send_message(DefaultChannel::ReliableOrdered, bytes);
        }
    }

    pub fn send_pickup_item(&mut self, x: f32, y: f32, z: f32) {
        let msg = ClientMessage::PickupItem { x, y, z };
        if let Ok(bytes) = bincode::serialize(&msg) {
            self.client.send_message(DefaultChannel::ReliableOrdered, bytes);
        }
    }

    pub fn remote_players(&self) -> Vec<PeerView> {
        self.remote_players
            .values()
            .map(|p| PeerView {
                pos: p.position, yaw: p.yaw, health: p.health,
                sneaking: p.sneaking, anim_time: p.anim_time, move_amount: p.move_amount,
                crouch_t: p.crouch_t, pitch: p.pitch,
            })
            .collect()
    }
}
