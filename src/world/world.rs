use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use crate::world::chunk::{Chunk, Blocks, NeighborEdges};
use crate::world::block::BlockType;
use crate::renderer::ChunkRenderer;

// Phase 1: terrain data arrives from worker thread
struct BlockReady {
    position: [i32; 3],
    chunk: Chunk,
}

// Phase 2: vertex data arrives from mesh thread
struct MeshReady {
    position: [i32; 3],
    vertices: Vec<f32>,
}

pub struct World {
    chunks: HashMap<[i32; 3], Chunk>,
    pending_blocks: HashSet<[i32; 3]>,  // terrain threads in flight
    pending_meshes: HashSet<[i32; 3]>,  // mesh threads in flight
    block_tx: Sender<BlockReady>,
    block_rx: Receiver<BlockReady>,
    mesh_tx: Sender<MeshReady>,
    mesh_rx: Receiver<MeshReady>,
    loaded_radius: i32,
    player_chunk: [i32; 3],
}

impl World {
    pub fn new(loaded_radius: i32) -> Self {
        let (block_tx, block_rx) = mpsc::channel();
        let (mesh_tx, mesh_rx) = mpsc::channel();
        World {
            chunks: HashMap::new(),
            pending_blocks: HashSet::new(),
            pending_meshes: HashSet::new(),
            block_tx,
            block_rx,
            mesh_tx,
            mesh_rx,
            loaded_radius,
            player_chunk: [i32::MAX; 3],
        }
    }

    pub fn update(&mut self, player_pos: [f32; 3]) {
        let new_chunk = [
            (player_pos[0] / 16.0).floor() as i32,
            0,
            (player_pos[2] / 16.0).floor() as i32,
        ];

        if new_chunk != self.player_chunk {
            self.load_chunks_around(new_chunk, self.loaded_radius);
            self.unload_distant_chunks(new_chunk, self.loaded_radius);
            self.player_chunk = new_chunk;
        }

        self.finalize_blocks(4);
        self.finalize_meshes(2);
    }

    fn load_chunks_around(&mut self, center: [i32; 3], radius: i32) {
        for x in -radius..=radius {
            for z in -radius..=radius {
                let pos = [center[0] + x, 0, center[2] + z];
                if self.chunks.contains_key(&pos)
                    || self.pending_blocks.contains(&pos)
                    || self.pending_meshes.contains(&pos)
                {
                    continue;
                }
                self.pending_blocks.insert(pos);
                let tx = self.block_tx.clone();
                thread::spawn(move || {
                    let chunk = Chunk::generate(pos);
                    let _ = tx.send(BlockReady { position: pos, chunk });
                });
            }
        }
    }

    /// Drain finished terrain chunks, then dispatch mesh threads for any chunk
    /// whose neighbor edges can now be snapshotted from loaded chunks.
    fn finalize_blocks(&mut self, max: usize) {
        // Collect finished terrain chunks
        let mut arrived = Vec::new();
        for _ in 0..max {
            match self.block_rx.try_recv() {
                Ok(r) => arrived.push(r),
                Err(_) => break,
            }
        }

        for ready in arrived {
            self.pending_blocks.remove(&ready.position);
            self.chunks.insert(ready.position, ready.chunk);

            // Invalidate the 4 horizontal neighbors so they get re-meshed with
            // correct border faces now that this chunk exists.
            let [cx, _, cz] = ready.position;
            for npos in [
                [cx + 1, 0, cz],
                [cx - 1, 0, cz],
                [cx, 0, cz + 1],
                [cx, 0, cz - 1],
            ] {
                if let Some(neighbor) = self.chunks.get_mut(&npos) {
                    neighbor.mark_for_rebuild();
                }
            }
        }

        // Dispatch mesh threads for all chunks that need a mesh and aren't
        // already being meshed.
        let needs_mesh: Vec<[i32; 3]> = self.chunks.iter()
            .filter(|(pos, c)| c.needs_mesh() && !self.pending_meshes.contains(*pos))
            .map(|(pos, _)| *pos)
            .collect();

        for pos in needs_mesh {
            let [cx, _, cz] = pos;

            // Snapshot block data and neighbor edges — all copies, no borrows held.
            let blocks: Blocks = self.chunks[&pos].blocks_snapshot();
            let edges = NeighborEdges {
                // "right" edge = lx=0 face of the +X neighbor
                right: self.chunks.get(&[cx + 1, 0, cz])
                    .map(|c| c.edge_right()).unwrap_or([[BlockType::Air; 16]; 16]),
                // "left" edge = lx=15 face of the -X neighbor
                left: self.chunks.get(&[cx - 1, 0, cz])
                    .map(|c| c.edge_left()).unwrap_or([[BlockType::Air; 16]; 16]),
                // "front" edge = lz=0 face of the +Z neighbor
                front: self.chunks.get(&[cx, 0, cz + 1])
                    .map(|c| c.edge_front()).unwrap_or([[BlockType::Air; 16]; 16]),
                // "back" edge = lz=15 face of the -Z neighbor
                back: self.chunks.get(&[cx, 0, cz - 1])
                    .map(|c| c.edge_back()).unwrap_or([[BlockType::Air; 16]; 16]),
            };

            self.chunks.get_mut(&pos).unwrap().mark_mesh_dispatched();
            self.pending_meshes.insert(pos);

            let tx = self.mesh_tx.clone();
            thread::spawn(move || {
                let vertices = Chunk::build_vertices(&blocks, &edges);
                let _ = tx.send(MeshReady { position: pos, vertices });
            });
        }
    }

    /// Upload finished vertex buffers to the GPU (main thread only).
    fn finalize_meshes(&mut self, max: usize) {
        for _ in 0..max {
            match self.mesh_rx.try_recv() {
                Ok(ready) => {
                    self.pending_meshes.remove(&ready.position);
                    if let Some(chunk) = self.chunks.get_mut(&ready.position) {
                        chunk.finalize_mesh(ready.vertices);
                    }
                }
                Err(_) => break,
            }
        }
    }

    // Drain all pending during startup
    pub fn finalize_all_pending(&mut self) {
        std::thread::sleep(std::time::Duration::from_millis(5));
        // Drain terrain threads
        while let Ok(ready) = self.block_rx.try_recv() {
            self.pending_blocks.remove(&ready.position);
            self.chunks.insert(ready.position, ready.chunk);
        }
        // Build meshes synchronously (startup, neighbors don't need invalidation)
        let positions: Vec<[i32; 3]> = self.chunks.keys().copied().collect();
        for pos in positions {
            if !self.chunks[&pos].needs_mesh() { continue; }
            let [cx, _, cz] = pos;
            let blocks = self.chunks[&pos].blocks_snapshot();
            let edges = NeighborEdges {
                right: self.chunks.get(&[cx + 1, 0, cz])
                    .map(|c| c.edge_right()).unwrap_or([[BlockType::Air; 16]; 16]),
                left: self.chunks.get(&[cx - 1, 0, cz])
                    .map(|c| c.edge_left()).unwrap_or([[BlockType::Air; 16]; 16]),
                front: self.chunks.get(&[cx, 0, cz + 1])
                    .map(|c| c.edge_front()).unwrap_or([[BlockType::Air; 16]; 16]),
                back: self.chunks.get(&[cx, 0, cz - 1])
                    .map(|c| c.edge_back()).unwrap_or([[BlockType::Air; 16]; 16]),
            };
            let vertices = Chunk::build_vertices(&blocks, &edges);
            self.chunks.get_mut(&pos).unwrap().finalize_mesh(vertices);
        }
    }

    fn unload_distant_chunks(&mut self, center: [i32; 3], radius: i32) {
        self.chunks.retain(|&pos, _| {
            let dx = pos[0] - center[0];
            let dz = pos[2] - center[2];
            dx * dx + dz * dz <= radius * radius
        });
    }

    pub fn get_block(&self, wx: i32, wy: i32, wz: i32) -> BlockType {
        if wy < 0 || wy >= 16 {
            return BlockType::Air;
        }
        let chunk_pos = [wx.div_euclid(16), 0, wz.div_euclid(16)];
        let lx = wx.rem_euclid(16) as usize;
        let ly = wy as usize;
        let lz = wz.rem_euclid(16) as usize;
        if let Some(chunk) = self.chunks.get(&chunk_pos) {
            chunk.get_block(lx, ly, lz)
        } else {
            BlockType::Air
        }
    }

    pub fn surface_height(&self, wx: i32, wz: i32) -> i32 {
        for y in (0..16).rev() {
            if self.get_block(wx, y, wz).is_solid() {
                return y + 1;
            }
        }
        0
    }

    pub fn raycast(&self, origin: [f32; 3], dir: [f32; 3], max_dist: f32) -> Option<[i32; 3]> {
        let dx = dir[0];
        let dy = dir[1];
        let dz = dir[2];

        let mut bx = origin[0].floor() as i32;
        let mut by = origin[1].floor() as i32;
        let mut bz = origin[2].floor() as i32;

        let step_x: i32 = if dx >= 0.0 { 1 } else { -1 };
        let step_y: i32 = if dy >= 0.0 { 1 } else { -1 };
        let step_z: i32 = if dz >= 0.0 { 1 } else { -1 };

        let t_delta_x = if dx.abs() < 1e-9 { f32::INFINITY } else { 1.0 / dx.abs() };
        let t_delta_y = if dy.abs() < 1e-9 { f32::INFINITY } else { 1.0 / dy.abs() };
        let t_delta_z = if dz.abs() < 1e-9 { f32::INFINITY } else { 1.0 / dz.abs() };

        let mut t_max_x = if dx >= 0.0 { (bx as f32 + 1.0 - origin[0]) / dx.abs() } else { (origin[0] - bx as f32) / dx.abs() };
        let mut t_max_y = if dy >= 0.0 { (by as f32 + 1.0 - origin[1]) / dy.abs() } else { (origin[1] - by as f32) / dy.abs() };
        let mut t_max_z = if dz >= 0.0 { (bz as f32 + 1.0 - origin[2]) / dz.abs() } else { (origin[2] - bz as f32) / dz.abs() };

        loop {
            let t = t_max_x.min(t_max_y).min(t_max_z);
            if t > max_dist { return None; }

            if self.get_block(bx, by, bz).is_targetable() {
                return Some([bx, by, bz]);
            }

            if t_max_x <= t_max_y && t_max_x <= t_max_z {
                bx += step_x; t_max_x += t_delta_x;
            } else if t_max_y <= t_max_z {
                by += step_y; t_max_y += t_delta_y;
            } else {
                bz += step_z; t_max_z += t_delta_z;
            }
        }
    }

    pub fn draw(&self, renderer: &ChunkRenderer, camera: &crate::camera::Camera) {
        let frustum = camera.frustum();
        let visible: Vec<&Chunk> = self.chunks.values()
            .filter(|c| c.is_in_frustum(&frustum))
            .collect();

        // Pass 1: opaque + cutout geometry (full depth writes)
        renderer.set_transparent_pass(false);
        for chunk in &visible {
            renderer.draw_chunk(chunk);
        }

        // Pass 2: semi-transparent geometry (water) — depth test on, no depth writes
        renderer.set_transparent_pass(true);
        for chunk in &visible {
            renderer.draw_chunk(chunk);
        }
    }
}
