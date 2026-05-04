use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use crate::world::chunk::{Chunk, Blocks, NeighborEdges, WaterLevels};
use crate::world::block::BlockType;
use crate::renderer::ChunkRenderer;
use crate::renderer::ShadowPass;

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
    terrain_queue: HashSet<[i32; 3]>,   // positions waiting for a terrain thread slot
    pending_blocks: HashSet<[i32; 3]>,  // terrain threads currently in flight
    pending_meshes: HashSet<[i32; 3]>,  // mesh threads currently in flight
    block_tx: Sender<BlockReady>,
    block_rx: Receiver<BlockReady>,
    mesh_tx: Sender<MeshReady>,
    mesh_rx: Receiver<MeshReady>,
    loaded_radius: i32,
    player_chunk: [i32; 3],
    pending_water: HashMap<[i32; 3], (f32, u8)>, // position → (countdown, level to place)
    active_water: HashSet<[i32; 3]>,              // water blocks that currently have air below them
    active_spread: HashSet<[i32; 3]>,             // water blocks (level>2) that still have air neighbors at same Y
    water_levels: HashMap<[i32; 3], u8>,          // world-coord → water level (1–7 flowing, 8 source)
}

impl World {
    pub fn new(loaded_radius: i32) -> Self {
        let (block_tx, block_rx) = mpsc::channel();
        let (mesh_tx, mesh_rx) = mpsc::channel();
        World {
            chunks: HashMap::new(),
            terrain_queue: HashSet::new(),
            pending_blocks: HashSet::new(),
            pending_meshes: HashSet::new(),
            block_tx,
            block_rx,
            mesh_tx,
            mesh_rx,
            loaded_radius,
            player_chunk: [i32::MAX; 3],
            pending_water: HashMap::new(),
            active_water: HashSet::new(),
            active_spread: HashSet::new(),
            water_levels: HashMap::new(),
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

        self.dispatch_terrain_threads();
        self.finalize_blocks(2);
        self.finalize_meshes(4);
    }

    fn load_chunks_around(&mut self, center: [i32; 3], radius: i32) {
        for x in -radius..=radius {
            for z in -radius..=radius {
                let pos = [center[0] + x, 0, center[2] + z];
                if !self.chunks.contains_key(&pos)
                    && !self.terrain_queue.contains(&pos)
                    && !self.pending_blocks.contains(&pos)
                {
                    self.terrain_queue.insert(pos);
                }
            }
        }
    }

    /// Promote queued terrain positions to in-flight threads, up to the cap.
    /// pending_blocks = in-flight only; terrain_queue = waiting for a slot.
    fn dispatch_terrain_threads(&mut self) {
        const MAX_TERRAIN_THREADS: usize = 4;
        let slots = MAX_TERRAIN_THREADS.saturating_sub(self.pending_blocks.len());
        let to_spawn: Vec<[i32; 3]> = self.terrain_queue.iter().copied().take(slots).collect();
        for pos in to_spawn {
            self.terrain_queue.remove(&pos);
            self.pending_blocks.insert(pos);
            let tx = self.block_tx.clone();
            thread::spawn(move || {
                let chunk = Chunk::generate(pos);
                let _ = tx.send(BlockReady { position: pos, chunk });
            });
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

            // Seed water metadata — only touch blocks that are actually water or
            // border water, avoiding a full 4096-iteration scan on the main thread.
            let [cx, _, cz] = ready.position;
            let snapshot = self.chunks[&ready.position].blocks_snapshot();
            let mut water_positions: Vec<[i32; 3]> = Vec::new();
            for ly in 0..16usize {
                for lz in 0..16usize {
                    for lx in 0..16usize {
                        if snapshot[lx][ly][lz] == BlockType::Water {
                            let wx = cx * 16 + lx as i32;
                            let wy = ly as i32;
                            let wz = cz * 16 + lz as i32;
                            self.water_levels.entry([wx, wy, wz]).or_insert(8);
                            water_positions.push([wx, wy, wz]);
                        }
                    }
                }
            }
            // Only call the refresh functions for water blocks and their immediate
            // vertical neighbors — avoids ~40k get_block calls for a mostly-dry chunk.
            for [wx, wy, wz] in water_positions {
                self.refresh_active_water(wx, wy, wz);
                self.refresh_active_spread(wx, wy, wz);
            }

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

        // Dispatch mesh threads up to the total in-flight cap, not just per-frame.
        // This prevents dozens of concurrent mesh threads competing for CPU cores.
        const MAX_MESH_THREADS: usize = 4;
        let mesh_slots = MAX_MESH_THREADS.saturating_sub(self.pending_meshes.len());
        let needs_mesh: Vec<[i32; 3]> = self.chunks.iter()
            .filter(|(pos, c)| c.needs_mesh() && !self.pending_meshes.contains(*pos))
            .map(|(pos, _)| *pos)
            .take(mesh_slots)
            .collect();

        for pos in needs_mesh {
            let [cx, _, cz] = pos;

            // Snapshot block data and neighbor edges — all copies, no borrows held.
            let blocks: Blocks = self.chunks[&pos].blocks_snapshot();
            let water_levels: WaterLevels = self.water_levels_snapshot(cx, cz);
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
                wl_right: self.water_edge_at_lx(cx + 1, cz, 0),
                wl_left:  self.water_edge_at_lx(cx - 1, cz, 15),
                wl_front: self.water_edge_at_lz(cx, cz + 1, 0),
                wl_back:  self.water_edge_at_lz(cx, cz - 1, 15),
            };

            self.chunks.get_mut(&pos).unwrap().mark_mesh_dispatched();
            self.pending_meshes.insert(pos);

            let tx = self.mesh_tx.clone();
            thread::spawn(move || {
                let vertices = Chunk::build_vertices(&blocks, &edges, &water_levels);
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
        self.dispatch_terrain_threads();
        // Drain terrain threads
        while let Ok(ready) = self.block_rx.try_recv() {
            self.pending_blocks.remove(&ready.position);
            self.chunks.insert(ready.position, ready.chunk);
        }
        // Seed water_levels from all loaded chunks
        let positions: Vec<[i32; 3]> = self.chunks.keys().copied().collect();
        for pos in &positions {
            let [cx, _, cz] = *pos;
            for ly in 0..16i32 {
                for lz in 0..16i32 {
                    for lx in 0..16i32 {
                        let wx = cx * 16 + lx;
                        let wz = cz * 16 + lz;
                        if self.chunks[pos].get_block(lx as usize, ly as usize, lz as usize) == BlockType::Water {
                            self.water_levels.entry([wx, ly, wz]).or_insert(8);
                        }
                    }
                }
            }
        }
        // Build meshes synchronously (startup, neighbors don't need invalidation)
        for pos in positions {
            if !self.chunks[&pos].needs_mesh() { continue; }
            let [cx, _, cz] = pos;
            let blocks = self.chunks[&pos].blocks_snapshot();
            let water_levels: WaterLevels = self.water_levels_snapshot(cx, cz);
            let edges = NeighborEdges {
                right: self.chunks.get(&[cx + 1, 0, cz])
                    .map(|c| c.edge_right()).unwrap_or([[BlockType::Air; 16]; 16]),
                left: self.chunks.get(&[cx - 1, 0, cz])
                    .map(|c| c.edge_left()).unwrap_or([[BlockType::Air; 16]; 16]),
                front: self.chunks.get(&[cx, 0, cz + 1])
                    .map(|c| c.edge_front()).unwrap_or([[BlockType::Air; 16]; 16]),
                back: self.chunks.get(&[cx, 0, cz - 1])
                    .map(|c| c.edge_back()).unwrap_or([[BlockType::Air; 16]; 16]),
                wl_right: self.water_edge_at_lx(cx + 1, cz, 0),
                wl_left:  self.water_edge_at_lx(cx - 1, cz, 15),
                wl_front: self.water_edge_at_lz(cx, cz + 1, 0),
                wl_back:  self.water_edge_at_lz(cx, cz - 1, 15),
            };
            let vertices = Chunk::build_vertices(&blocks, &edges, &water_levels);
            self.chunks.get_mut(&pos).unwrap().finalize_mesh(vertices);
        }
    }

    /// Updates active_water for the changed position and the one above it.
    /// Call after any set_block and after seeding a new chunk.
    fn refresh_active_water(&mut self, wx: i32, wy: i32, wz: i32) {
        for (py, pz, px) in [(wy, wz, wx), (wy + 1, wz, wx)] {
            let pos = [px, py, pz];
            if self.get_block(px, py, pz) == BlockType::Water
                && self.get_block(px, py - 1, pz) == BlockType::Air
            {
                self.active_water.insert(pos);
            } else {
                self.active_water.remove(&pos);
            }
        }
    }

    /// Updates active_spread for this position and its 4 horizontal neighbors.
    /// A water block belongs in active_spread if its level > 2 AND at least one horizontal
    /// neighbor at the same Y is air (meaning it still has somewhere to go).
    fn refresh_active_spread(&mut self, wx: i32, wy: i32, wz: i32) {
        for (px, pz) in [(wx, wz), (wx+1, wz), (wx-1, wz), (wx, wz+1), (wx, wz-1)] {
            let pos = [px, wy, pz];
            let lvl = self.water_levels.get(&pos).copied().unwrap_or(0);
            if lvl > 2 && [[px+1,pz],[px-1,pz],[px,pz+1],[px,pz-1]].iter()
                .any(|&[nx,nz]| self.get_block(nx, wy, nz) == BlockType::Air)
            {
                self.active_spread.insert(pos);
            } else {
                self.active_spread.remove(&pos);
            }
        }
    }

    pub fn tick_water(&mut self, dt: f32) {
        // 1. Downward flow: active_water tracks water blocks with air directly below.
        let down_candidates: Vec<([i32; 3], u8)> = self.active_water.iter().copied()
            .filter(|&[wx, wy, wz]|
                self.get_block(wx, wy, wz) == BlockType::Water &&
                self.get_block(wx, wy - 1, wz) == BlockType::Air &&
                !self.pending_water.contains_key(&[wx, wy - 1, wz])
            )
            .map(|[wx, wy, wz]| {
                // Fallen water lands at full source strength so it can spread 3 fresh
                // blocks from the landing point — same as Minecraft waterfall behaviour.
                ([wx, wy - 1, wz], 8u8)
            })
            .collect();
        for (pos, lvl) in down_candidates {
            self.pending_water.insert(pos, (1.0, lvl));
        }

        // 2. Horizontal flow: only check the active frontier — water blocks that still
        // have at least one air neighbor at the same Y. Settled blocks are not in this set.
        let h_candidates: Vec<([i32; 3], u8)> = self.active_spread.iter().copied()
            .filter_map(|pos| self.water_levels.get(&pos).map(|&lvl| (pos, lvl)))
            .collect();
        for ([wx, wy, wz], lvl) in h_candidates {
            let new_lvl = lvl - 1;
            let mut still_active = false;
            for [nx, nz] in [[wx + 1, wz], [wx - 1, wz], [wx, wz + 1], [wx, wz - 1]] {
                let npos = [nx, wy, nz];
                if self.get_block(nx, wy, nz) == BlockType::Air {
                    still_active = true;
                    if !self.pending_water.contains_key(&npos) {
                        self.pending_water.insert(npos, (1.0, new_lvl));
                    }
                }
            }
            if !still_active {
                self.active_spread.remove(&[wx, wy, wz]);
            }
        }

        // 3. Tick timers; collect positions ready to fill.
        let mut to_fill: Vec<([i32; 3], u8)> = Vec::new();
        self.pending_water.retain(|pos, (timer, lvl)| {
            *timer -= dt;
            if *timer <= 0.0 { to_fill.push((*pos, *lvl)); false } else { true }
        });

        // 4. Fill — only if still air (player may have placed something there).
        for ([wx, wy, wz], lvl) in to_fill {
            if self.get_block(wx, wy, wz) == BlockType::Air {
                self.water_levels.insert([wx, wy, wz], lvl);
                self.set_block(wx, wy, wz, BlockType::Water);
            }
        }
    }

    fn unload_distant_chunks(&mut self, center: [i32; 3], radius: i32) {
        self.chunks.retain(|&pos, _| {
            let dx = pos[0] - center[0];
            let dz = pos[2] - center[2];
            dx * dx + dz * dz <= radius * radius
        });
        // Also clear queued terrain jobs for unloaded chunks.
        self.terrain_queue.retain(|&pos| {
            let dx = pos[0] - center[0];
            let dz = pos[2] - center[2];
            dx * dx + dz * dz <= radius * radius
        });
        // Remove water metadata for unloaded chunks so tick_water doesn't process them.
        self.water_levels.retain(|&[wx, _, wz], _| {
            let cx = wx.div_euclid(16);
            let cz = wz.div_euclid(16);
            self.chunks.contains_key(&[cx, 0, cz])
        });
        self.active_spread.retain(|&[wx, _, wz]| {
            let cx = wx.div_euclid(16);
            let cz = wz.div_euclid(16);
            self.chunks.contains_key(&[cx, 0, cz])
        });
    }

    /// Replace a block at world coords and mark the chunk (plus border neighbors) for rebuild.
    pub fn set_block(&mut self, wx: i32, wy: i32, wz: i32, block: BlockType) {
        if wy < 0 || wy >= 16 { return; }
        let cx = wx.div_euclid(16);
        let cz = wz.div_euclid(16);
        let lx = wx.rem_euclid(16) as usize;
        let ly = wy as usize;
        let lz = wz.rem_euclid(16) as usize;

        // Sync water_levels: removing a water block clears its entry; placing water
        // from a non-tick_water source (e.g. future player placement) defaults to level 8.
        if block != BlockType::Water {
            self.water_levels.remove(&[wx, wy, wz]);
        } else {
            // tick_water pre-inserts the correct level before calling set_block,
            // so only insert default 8 if nothing is already there.
            self.water_levels.entry([wx, wy, wz]).or_insert(8);
        }

        if let Some(chunk) = self.chunks.get_mut(&[cx, 0, cz]) {
            chunk.set_block(lx, ly, lz, block);
            chunk.mark_for_rebuild();
        }
        // If the block sits on a chunk border, the adjacent chunk's face visibility
        // changes too — mark it dirty so it re-meshes on the next update.
        if lx == 0  { if let Some(c) = self.chunks.get_mut(&[cx-1, 0, cz]) { c.mark_for_rebuild(); } }
        if lx == 15 { if let Some(c) = self.chunks.get_mut(&[cx+1, 0, cz]) { c.mark_for_rebuild(); } }
        if lz == 0  { if let Some(c) = self.chunks.get_mut(&[cx, 0, cz-1]) { c.mark_for_rebuild(); } }
        if lz == 15 { if let Some(c) = self.chunks.get_mut(&[cx, 0, cz+1]) { c.mark_for_rebuild(); } }
        self.refresh_active_water(wx, wy, wz);
        self.refresh_active_spread(wx, wy, wz);
    }

    // ── Water level helpers (used when snapshotting data for mesh threads) ───

    /// Copy this chunk's water levels into a WaterLevels array.
    fn water_levels_snapshot(&self, cx: i32, cz: i32) -> WaterLevels {
        let mut wl = [[[0u8; 16]; 16]; 16];
        for lx in 0..16i32 {
            for ly in 0..16i32 {
                for lz in 0..16i32 {
                    if let Some(&lvl) = self.water_levels.get(&[cx * 16 + lx, ly, cz * 16 + lz]) {
                        wl[lx as usize][ly as usize][lz as usize] = lvl;
                    }
                }
            }
        }
        wl
    }

    /// Water levels at a fixed lx column of chunk (cx, cz), returned as [y][z].
    fn water_edge_at_lx(&self, cx: i32, cz: i32, lx: i32) -> [[u8; 16]; 16] {
        let mut e = [[0u8; 16]; 16];
        for ly in 0..16i32 {
            for lz in 0..16i32 {
                if let Some(&lvl) = self.water_levels.get(&[cx * 16 + lx, ly, cz * 16 + lz]) {
                    e[ly as usize][lz as usize] = lvl;
                }
            }
        }
        e
    }

    /// Water levels at a fixed lz column of chunk (cx, cz), returned as [y][x].
    fn water_edge_at_lz(&self, cx: i32, cz: i32, lz: i32) -> [[u8; 16]; 16] {
        let mut e = [[0u8; 16]; 16];
        for ly in 0..16i32 {
            for lx in 0..16i32 {
                if let Some(&lvl) = self.water_levels.get(&[cx * 16 + lx, ly, cz * 16 + lz]) {
                    e[ly as usize][lx as usize] = lvl;
                }
            }
        }
        e
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

    /// Render all loaded chunks into the shadow map (depth-only pass).
    /// No frustum culling here — we want geometry behind the camera to still cast shadows.
    pub fn draw_shadow(&self, shadow_pass: &ShadowPass) {
        for chunk in self.chunks.values() {
            shadow_pass.draw_chunk(chunk);
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
