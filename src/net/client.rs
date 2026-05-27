use std::collections::HashMap;
use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, SystemTime};

use renet::{RenetClient, DefaultChannel};
use renet_netcode::{NetcodeClientTransport, ClientAuthentication};

use super::messages::{ClientMessage, ServerMessage, PROTOCOL_ID};

struct RemotePlayer {
    position: [f32; 3],
    yaw: f32,
    health: u8,
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

        let mut received = Vec::new();

        // Process reliable messages
        while let Some(bytes) = self.client.receive_message(DefaultChannel::ReliableOrdered) {
            if let Ok(msg) = bincode::deserialize::<ServerMessage>(&bytes) {
                match &msg {
                    ServerMessage::PeerJoined { id } => {
                        self.remote_players.insert(*id, RemotePlayer {
                            position: [0.0, 0.0, 0.0],
                            yaw: 0.0,
                            health: 100,
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
                if let ServerMessage::PeerState { id, x, y, z, yaw, health, .. } = &msg {
                    let player = self.remote_players.entry(*id).or_insert(RemotePlayer {
                        position: [0.0, 0.0, 0.0],
                        yaw: 0.0,
                        health: 100,
                    });
                    player.position = [*x, *y, *z];
                    player.yaw = *yaw;
                    player.health = *health;
                }
                received.push(msg);
            }
        }

        self.transport.send_packets(&mut self.client).ok();

        received
    }

    pub fn send_position(&mut self, x: f32, y: f32, z: f32, yaw: f32, pitch: f32, health: u8) {
        let msg = ClientMessage::PlayerState { x, y, z, yaw, pitch, health };
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

    pub fn send_attack_entity(&mut self, kind: u8, index: u32, push_x: f32, push_z: f32) {
        let msg = ClientMessage::AttackEntity { kind, index, push_x, push_z };
        if let Ok(bytes) = bincode::serialize(&msg) {
            self.client.send_message(DefaultChannel::ReliableOrdered, bytes);
        }
    }

    pub fn send_interact_entity(&mut self, kind: u8, index: u32) {
        let msg = ClientMessage::InteractEntity { kind, index };
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

    pub fn remote_players(&self) -> Vec<([f32; 3], f32, u8)> {
        self.remote_players
            .values()
            .map(|p| (p.position, p.yaw, p.health))
            .collect()
    }
}
