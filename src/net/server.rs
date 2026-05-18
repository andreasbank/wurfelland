use std::collections::HashMap;
use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, SystemTime};

use renet::{RenetServer, ServerEvent, DefaultChannel};
use renet_netcode::{NetcodeServerTransport, ServerAuthentication, ServerConfig};

use super::messages::{ClientMessage, ServerMessage, PROTOCOL_ID};

struct RemotePlayer {
    position: [f32; 3],
    yaw: f32,
}

pub struct GameServer {
    server: RenetServer,
    transport: NetcodeServerTransport,
    remote_players: HashMap<u64, RemotePlayer>,
    pending_block_breaks:  Vec<[i32; 3]>,
    pending_block_places:  Vec<[i32; 4]>, // [x, y, z, block_id]
}

impl GameServer {
    pub fn new(port: u16) -> anyhow::Result<Self> {
        let current_time = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or(Duration::ZERO);

        let server_addr: SocketAddr = format!("0.0.0.0:{}", port).parse()?;
        let socket = UdpSocket::bind(server_addr)?;
        socket.set_nonblocking(true)?;

        let server_config = ServerConfig {
            current_time,
            max_clients: 64,
            protocol_id: PROTOCOL_ID,
            public_addresses: vec![server_addr],
            authentication: ServerAuthentication::Unsecure,
        };

        let transport = NetcodeServerTransport::new(server_config, socket)?;
        let server = RenetServer::new(renet::ConnectionConfig::default());

        Ok(GameServer {
            server,
            transport,
            remote_players: HashMap::new(),
            pending_block_breaks: Vec::new(),
            pending_block_places: Vec::new(),
        })
    }

    pub fn update(&mut self, dt: f32) {
        let duration = Duration::from_secs_f32(dt);

        self.transport.update(duration, &mut self.server).ok();
        self.server.update(duration);

        // Handle server events (connect / disconnect)
        while let Some(event) = self.server.get_event() {
            match event {
                ServerEvent::ClientConnected { client_id } => {
                    self.remote_players.insert(client_id, RemotePlayer {
                        position: [0.0, 0.0, 0.0],
                        yaw: 0.0,
                    });
                    // Broadcast PeerJoined to all OTHER clients
                    let msg = ServerMessage::PeerJoined { id: client_id };
                    if let Ok(bytes) = bincode::serialize(&msg) {
                        self.server.broadcast_message_except(
                            client_id,
                            DefaultChannel::ReliableOrdered,
                            bytes,
                        );
                    }
                }
                ServerEvent::ClientDisconnected { client_id, .. } => {
                    self.remote_players.remove(&client_id);
                    // Broadcast PeerLeft to all clients
                    let msg = ServerMessage::PeerLeft { id: client_id };
                    if let Ok(bytes) = bincode::serialize(&msg) {
                        self.server.broadcast_message(
                            DefaultChannel::ReliableOrdered,
                            bytes,
                        );
                    }
                }
            }
        }

        // Process incoming messages from each client
        let client_ids: Vec<u64> = self.server.clients_id().into_iter().collect();
        for client_id in client_ids {
            // Process reliable messages
            while let Some(bytes) = self.server.receive_message(client_id, DefaultChannel::ReliableOrdered) {
                if let Ok(msg) = bincode::deserialize::<ClientMessage>(&bytes) {
                    match msg {
                        ClientMessage::BreakBlock { x, y, z } => {
                            self.pending_block_breaks.push([x, y, z]);
                            let out = ServerMessage::BlockChange { x, y, z, block_id: 0 };
                            if let Ok(out_bytes) = bincode::serialize(&out) {
                                self.server.broadcast_message_except(
                                    client_id,
                                    DefaultChannel::ReliableOrdered,
                                    out_bytes,
                                );
                            }
                        }
                        ClientMessage::PlaceBlock { x, y, z, block_id } => {
                            self.pending_block_places.push([x, y, z, block_id as i32]);
                            let out = ServerMessage::BlockChange { x, y, z, block_id };
                            if let Ok(out_bytes) = bincode::serialize(&out) {
                                self.server.broadcast_message_except(
                                    client_id,
                                    DefaultChannel::ReliableOrdered,
                                    out_bytes,
                                );
                            }
                        }
                        ClientMessage::PlayerState { .. } => {
                            // Ignore on reliable channel (positions sent unreliably)
                        }
                    }
                }
            }
            // Process unreliable messages
            while let Some(bytes) = self.server.receive_message(client_id, DefaultChannel::Unreliable) {
                if let Ok(msg) = bincode::deserialize::<ClientMessage>(&bytes) {
                    if let ClientMessage::PlayerState { x, y, z, yaw, pitch } = msg {
                        if let Some(player) = self.remote_players.get_mut(&client_id) {
                            player.position = [x, y, z];
                            player.yaw = yaw;
                        }
                        // Rebroadcast to all other clients as PeerState
                        let out = ServerMessage::PeerState { id: client_id, x, y, z, yaw, pitch };
                        if let Ok(out_bytes) = bincode::serialize(&out) {
                            self.server.broadcast_message_except(
                                client_id,
                                DefaultChannel::Unreliable,
                                out_bytes,
                            );
                        }
                    }
                }
            }
        }

        self.transport.send_packets(&mut self.server);
    }

    pub fn drain_block_breaks(&mut self) -> Vec<[i32; 3]> {
        std::mem::take(&mut self.pending_block_breaks)
    }

    pub fn drain_block_places(&mut self) -> Vec<[i32; 4]> {
        std::mem::take(&mut self.pending_block_places)
    }

    pub fn broadcast_block_change(&mut self, x: i32, y: i32, z: i32, block_id: u8) {
        let msg = ServerMessage::BlockChange { x, y, z, block_id };
        if let Ok(bytes) = bincode::serialize(&msg) {
            self.server.broadcast_message(DefaultChannel::ReliableOrdered, bytes);
        }
    }

    pub fn broadcast_host_position(&mut self, x: f32, y: f32, z: f32, yaw: f32, pitch: f32) {
        // Use sentinel id u64::MAX for the host
        let msg = ServerMessage::PeerState { id: u64::MAX, x, y, z, yaw, pitch };
        if let Ok(bytes) = bincode::serialize(&msg) {
            self.server.broadcast_message(DefaultChannel::Unreliable, bytes);
        }
    }

    pub fn remote_players(&self) -> Vec<([f32; 3], f32)> {
        self.remote_players
            .values()
            .map(|p| (p.position, p.yaw))
            .collect()
    }
}
