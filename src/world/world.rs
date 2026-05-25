use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Instant;

use crate::world::chunk::{Chunk, NeighborEdges, WaterLevels, SkyLight};
use crate::world::block::{BlockType, CROPS};
use crate::world::{SEA_LEVEL, WORLD_HEIGHT_CHUNKS};
use crate::renderer::ChunkRenderer;
use crate::renderer::ShadowPass;
use crate::camera::frustum::Frustum;

pub struct WorldStats {
    pub loaded:           usize,
    pub meshed:           usize,
    pub terrain_queued:   usize,
    pub terrain_inflight: usize,
    pub mesh_inflight:    usize,
    pub gen_per_sec:      f32,
    pub mesh_per_sec:     f32,
}

/// Only generate chunks up to this Y level; above is always air.
/// Mountains peak at ~182 (cy=11), buffer to 13 chunk-layers (Y=0..207).
const MAX_TERRAIN_CY: i32 = 13;

// Phase 1: terrain data arrives from worker thread
struct BlockReady {
    position: [i32; 3],
    chunk: Chunk,
}

// Phase 2: sky-light BFS result arrives from sky thread
struct SkyReady {
    position: [i32; 3],
    sky: Box<SkyLight>,
}

// Phase 3: vertex data arrives from mesh thread
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
    sky_tx:   Sender<SkyReady>,
    sky_rx:   Receiver<SkyReady>,
    pending_sky: HashSet<[i32; 3]>,
    mesh_tx: Sender<MeshReady>,
    mesh_rx: Receiver<MeshReady>,
    loaded_radius: i32,
    seed: u32,
    player_chunk: [i32; 3],
    pending_water: HashMap<[i32; 3], (f32, u8)>, // position → (countdown, level to place)
    active_water: HashSet<[i32; 3]>,              // water blocks that currently have air below them
    active_spread: HashSet<[i32; 3]>,             // water blocks (level>2) that still have air neighbors at same Y
    water_levels: HashMap<[i32; 3], Box<[[[u8; 16]; 16]; 16]>>, // chunk-coord → [lx][ly][lz] water level
    pending_lava: HashMap<[i32; 3], (f32, u8)>,  // position → (countdown ~3s, level to place)
    active_lava: HashSet<[i32; 3]>,               // lava blocks with air/water below
    active_lava_spread: HashSet<[i32; 3]>,        // lava blocks (level>1) with air/water neighbors at same Y
    lava_levels: HashMap<[i32; 3], Box<[[[u8; 16]; 16]; 16]>>,  // chunk-coord → [lx][ly][lz] lava level
    active_crops: HashMap<[i32; 3], (u8, f32)>,  // pos → (crop_id, seconds until next stage)
    // Player-authored block changes (breaks/placements); applied to newly generated
    // chunks so loaded saves always see the same terrain modifications.
    block_changes: HashMap<[i32; 3], BlockType>,
    // Per-frame column events consumed by main.rs for entity dormancy.
    unloaded_columns: Vec<[i32; 2]>,
    loaded_columns:   Vec<[i32; 2]>,
    notified_columns: HashSet<[i32; 2]>, // columns already announced as loaded
    // Throughput tracking — reset every ~0.5 s.
    gen_completed:   u32,
    mesh_completed:  u32,
    rate_instant:    Instant,
    last_gen_rate:   f32,
    last_mesh_rate:  f32,
}

impl World {
    pub fn new(loaded_radius: i32, seed: u32) -> Self {
        let (block_tx, block_rx) = mpsc::channel();
        let (sky_tx,   sky_rx)   = mpsc::channel();
        let (mesh_tx,  mesh_rx)  = mpsc::channel();
        World {
            chunks: HashMap::new(),
            terrain_queue: HashSet::new(),
            pending_blocks: HashSet::new(),
            pending_sky:   HashSet::new(),
            pending_meshes: HashSet::new(),
            block_tx,
            block_rx,
            sky_tx,
            sky_rx,
            mesh_tx,
            mesh_rx,
            loaded_radius,
            seed,
            player_chunk: [i32::MAX; 3],
            pending_water: HashMap::new(),
            active_water: HashSet::new(),
            active_spread: HashSet::new(),
            water_levels: HashMap::new(),
            pending_lava: HashMap::new(),
            active_lava: HashSet::new(),
            active_lava_spread: HashSet::new(),
            lava_levels: HashMap::new(),
            active_crops: HashMap::new(),
            block_changes: HashMap::new(),
            unloaded_columns: Vec::new(),
            loaded_columns:   Vec::new(),
            notified_columns: HashSet::new(),
            gen_completed:  0,
            mesh_completed: 0,
            rate_instant:   Instant::now(),
            last_gen_rate:  0.0,
            last_mesh_rate: 0.0,
        }
    }

    pub fn seed(&self) -> u32 { self.seed }

    /// Like `set_block` but also records the change so it survives save/load.
    /// Call this for all player-initiated block breaks and placements.
    pub fn set_block_recorded(&mut self, wx: i32, wy: i32, wz: i32, block: BlockType) {
        self.set_block(wx, wy, wz, block);
        if wy >= 0 && wy < WORLD_HEIGHT_CHUNKS * 16 {
            self.block_changes.insert([wx, wy, wz], block);
        }
    }

    pub fn get_block_changes(&self) -> &HashMap<[i32; 3], BlockType> {
        &self.block_changes
    }

    /// Replace the recorded block-change table and immediately apply any changes
    /// that fall inside already-loaded chunks.
    pub fn load_block_changes(&mut self, changes: HashMap<[i32; 3], BlockType>) {
        for (&[wx, wy, wz], &block) in &changes {
            self.set_block(wx, wy, wz, block);
        }
        self.block_changes = changes;
    }

    pub fn update(&mut self, player_pos: [f32; 3]) {
        let new_chunk = [
            (player_pos[0] / 16.0).floor() as i32,
            0,
            (player_pos[2] / 16.0).floor() as i32,
        ];

        if new_chunk != self.player_chunk {
            // Inner radius: full pipeline (terrain + mesh).
            self.load_chunks_around(new_chunk, self.loaded_radius);
            // Outer buffer ring: terrain only — blocks are pre-generated so meshing
            // is instant the moment the player steps into range.
            self.load_chunks_around_terrain_only(new_chunk, self.loaded_radius + 2);
            self.unload_distant_chunks(new_chunk, self.loaded_radius + 2);
            self.player_chunk = new_chunk;
        }

        // Finalize before dispatch so freed thread slots are visible immediately.
        self.finalize_blocks(16);
        self.finalize_sky(16);
        self.finalize_meshes(8);
        self.dispatch_terrain_threads();
        self.dispatch_sky_threads();
        self.dispatch_mesh_threads();
    }

    /// Queue terrain generation for the outer buffer ring (no meshing — data only).
    /// Queue terrain generation for the outer buffer ring (no meshing — data only).
    fn load_chunks_around_terrain_only(&mut self, center: [i32; 3], radius: i32) {
        let r2 = radius * radius;
        for x in -radius..=radius {
            for z in -radius..=radius {
                if x * x + z * z > r2 { continue; }
                for cy in 6..=10 {  // surface band only for the buffer ring
                    let pos = [center[0] + x, cy, center[2] + z];
                    if !self.chunks.contains_key(&pos)
                        && !self.terrain_queue.contains(&pos)
                        && !self.pending_blocks.contains(&pos)
                    {
                        self.terrain_queue.insert(pos);
                    }
                }
            }
        }
    }

    fn load_chunks_around(&mut self, center: [i32; 3], radius: i32) {
        let r2 = radius * radius;
        for x in -radius..=radius {
            for z in -radius..=radius {
                if x * x + z * z > r2 { continue; }
                for cy in 0..MAX_TERRAIN_CY {
                    let pos = [center[0] + x, cy, center[2] + z];
                    if !self.chunks.contains_key(&pos)
                        && !self.terrain_queue.contains(&pos)
                        && !self.pending_blocks.contains(&pos)
                    {
                        self.terrain_queue.insert(pos);
                    }
                }
            }
        }
    }

    /// Promote queued terrain positions to in-flight threads, up to the cap.
    /// pending_blocks = in-flight only; terrain_queue = waiting for a slot.
    fn dispatch_terrain_threads(&mut self) {
        const MAX_TERRAIN_THREADS: usize = 8;
        let slots = MAX_TERRAIN_THREADS.saturating_sub(self.pending_blocks.len());
        if slots == 0 { return; }
        let pc = self.player_chunk;
        let mut candidates: Vec<[i32; 3]> = self.terrain_queue.iter().copied().collect();
        candidates.sort_unstable_by_key(|&[x, cy, z]| {
            let dx = x - pc[0]; let dz = z - pc[2];
            let xz = dx * dx + dz * dz;
            // Surface band (cy 6–10, world Y 96–175) loads before bedrock/sky slices.
            let y_penalty = if cy >= 6 && cy <= 10 { 0 } else { 100_000 };
            xz + y_penalty
        });
        let to_spawn: Vec<[i32; 3]> = candidates.into_iter().take(slots).collect();
        for pos in to_spawn {
            self.terrain_queue.remove(&pos);
            self.pending_blocks.insert(pos);
            let tx   = self.block_tx.clone();
            let seed = self.seed;
            thread::spawn(move || {
                let chunk = Chunk::generate(pos, seed);
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
            self.gen_completed += 1;
            let [cx, cy, cz] = ready.position;
            let mut chunk = ready.chunk;
            // Re-apply recorded player block changes that fall in this chunk.
            for (&[wx, wy, wz], &block) in &self.block_changes {
                let bcy = wy.div_euclid(16);
                if wx.div_euclid(16) == cx && bcy == cy && wz.div_euclid(16) == cz {
                    chunk.set_block(
                        wx.rem_euclid(16) as usize,
                        wy.rem_euclid(16) as usize,
                        wz.rem_euclid(16) as usize,
                        block,
                    );
                }
            }
            self.chunks.insert(ready.position, chunk);

            // Announce when enough of an XZ column is loaded to support entities.
            // cy 4 = world Y 64-79 (sea level where most entities stand); cy 10 = mountain tops.
            let col = [cx, cz];
            if !self.notified_columns.contains(&col)
                && (4i32..=10).all(|y| self.chunks.contains_key(&[cx, y, cz]))
            {
                self.notified_columns.insert(col);
                self.loaded_columns.push(col);
            }

            // Seed water_levels for rendering. One HashMap entry per chunk instead
            // of one per water block — avoids O(n) inserts for large water bodies.
            let snapshot = self.chunks[&ready.position].blocks_snapshot();
            let chunk_wl = self.water_levels
                .entry([cx, cy, cz])
                .or_insert_with(|| Box::new([[[0u8; 16]; 16]; 16]));
            for ly in 0..16usize {
                for lz in 0..16usize {
                    for lx in 0..16usize {
                        if snapshot[lx][ly][lz] == BlockType::Water && chunk_wl[lx][ly][lz] == 0 {
                            chunk_wl[lx][ly][lz] = 8;
                        }
                    }
                }
            }

            // Register growing crops for ticking (covers terrain-spawned and restored player blocks).
            for ly in 0..16usize {
                for lz in 0..16usize {
                    for lx in 0..16usize {
                        let pos = [cx * 16 + lx as i32,
                                   cy * 16 + ly as i32,
                                   cz * 16 + lz as i32];
                        if let BlockType::Crop(id, stage) = snapshot[lx][ly][lz] {
                            let def = &CROPS[id as usize];
                            let is_final = def.final_solid && stage == def.stages - 1;
                            if !is_final {
                                self.active_crops.entry(pos).or_insert((id, def.grow_secs));
                            }
                        }
                    }
                }
            }

            // Invalidate all six face-adjacent neighbors for correct border culling,
            // and mark them sky-dirty so lighting is recomputed at the new boundary.
            for npos in [
                [cx + 1, cy,     cz    ],
                [cx - 1, cy,     cz    ],
                [cx,     cy,     cz + 1],
                [cx,     cy,     cz - 1],
                [cx,     cy + 1, cz    ],
                [cx,     cy - 1, cz    ],
            ] {
                if let Some(neighbor) = self.chunks.get_mut(&npos) {
                    neighbor.mark_for_rebuild();
                    neighbor.mark_sky_dirty();
                }
            }
        }
    }

    fn dispatch_sky_threads(&mut self) {
        const MAX_SKY_THREADS: usize = 8;
        let slots = MAX_SKY_THREADS.saturating_sub(self.pending_sky.len());
        if slots == 0 { return; }
        let pc = self.player_chunk;
        let render_r2 = (self.loaded_radius + 2) * (self.loaded_radius + 2);
        let mut needs_sky: Vec<[i32; 3]> = self.chunks.iter()
            .filter(|(pos, c)| {
                let dx = pos[0] - pc[0]; let dz = pos[2] - pc[2];
                c.needs_sky_rebuild()
                    && !self.pending_sky.contains(*pos)
                    && dx * dx + dz * dz <= render_r2
            })
            .map(|(pos, _)| *pos)
            .collect();
        needs_sky.sort_unstable_by_key(|&[x, y, z]| {
            let dx = x - pc[0]; let dz = z - pc[2];
            (dx * dx + dz * dz, -(y as i64))
        });
        for pos in needs_sky.into_iter().take(slots) {
            let blocks = self.chunks[&pos].blocks_snapshot();
            let edges  = self.build_neighbor_edges(pos[0], pos[1], pos[2]);
            self.chunks.get_mut(&pos).unwrap().mark_sky_dispatched();
            self.pending_sky.insert(pos);
            let tx = self.sky_tx.clone();
            thread::spawn(move || {
                let sky = Chunk::build_sky(&blocks, &edges);
                let _ = tx.send(SkyReady { position: pos, sky });
            });
        }
    }

    fn finalize_sky(&mut self, max: usize) {
        for _ in 0..max {
            match self.sky_rx.try_recv() {
                Ok(ready) => {
                    self.pending_sky.remove(&ready.position);
                    let [cx, cy, cz] = ready.position;
                    let sky_changed = self.chunks.get(&ready.position)
                        .map(|c| c.sky_edges_changed(&ready.sky))
                        .unwrap_or(false);
                    if let Some(chunk) = self.chunks.get_mut(&ready.position) {
                        chunk.update_sky_light(ready.sky); // also sets sky_ready = true
                        chunk.mark_for_rebuild();
                    }
                    if sky_changed {
                        for npos in [
                            [cx-1,cy,cz],[cx+1,cy,cz],
                            [cx,cy-1,cz],[cx,cy+1,cz],
                            [cx,cy,cz-1],[cx,cy,cz+1],
                        ] {
                            if let Some(n) = self.chunks.get_mut(&npos) {
                                n.mark_sky_dirty();
                            }
                        }
                    }
                }
                Err(_) => break,
            }
        }
    }

    fn dispatch_mesh_threads(&mut self) {
        const MAX_MESH_THREADS: usize = 12;
        let mesh_slots = MAX_MESH_THREADS.saturating_sub(self.pending_meshes.len());
        if mesh_slots == 0 { return; }
        let pc = self.player_chunk;
        let render_r2 = self.loaded_radius * self.loaded_radius;
        let mut needs_mesh: Vec<[i32; 3]> = self.chunks.iter()
            .filter(|(pos, c)| {
                let dx = pos[0] - pc[0]; let dz = pos[2] - pc[2];
                c.needs_mesh()
                    && !self.pending_meshes.contains(*pos)
                    && dx * dx + dz * dz <= render_r2
            })
            .map(|(pos, _)| *pos)
            .collect();
        needs_mesh.sort_unstable_by_key(|&[x, y, z]| {
            let dx = x - pc[0]; let dz = z - pc[2];
            (dx * dx + dz * dz, -(y as i64))
        });
        for pos in needs_mesh.into_iter().take(mesh_slots) {
            let [cx, cy, cz] = pos;
            let blocks      = self.chunks[&pos].blocks_snapshot();
            let sky         = self.chunks[&pos].sky_snapshot();
            let water_levels = self.water_levels_snapshot(cx, cy, cz);
            let edges       = self.build_neighbor_edges(cx, cy, cz);
            self.chunks.get_mut(&pos).unwrap().mark_mesh_dispatched();
            self.pending_meshes.insert(pos);
            let tx = self.mesh_tx.clone();
            thread::spawn(move || {
                let vertices = Chunk::build_vertices(&blocks, &edges, &sky, &water_levels);
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
                    self.mesh_completed += 1;
                    if let Some(chunk) = self.chunks.get_mut(&ready.position) {
                        chunk.finalize_mesh(ready.vertices);
                    }
                }
                Err(_) => break,
            }
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
            let lvl = {
                let cxp = px.div_euclid(16); let cyp = wy.div_euclid(16); let czp = pz.div_euclid(16);
                self.water_levels.get(&[cxp, cyp, czp])
                    .map(|wl| wl[px.rem_euclid(16) as usize][wy.rem_euclid(16) as usize][pz.rem_euclid(16) as usize])
                    .unwrap_or(0)
            };
            if lvl > 2 && [[px+1,pz],[px-1,pz],[px,pz+1],[px,pz-1]].iter()
                .any(|&[nx,nz]| self.get_block(nx, wy, nz) == BlockType::Air)
            {
                self.active_spread.insert(pos);
            } else {
                self.active_spread.remove(&pos);
            }
        }
    }

    fn lava_level_at(&self, wx: i32, wy: i32, wz: i32) -> u8 {
        let cx = wx.div_euclid(16); let cy = wy.div_euclid(16); let cz = wz.div_euclid(16);
        let stored = self.lava_levels.get(&[cx, cy, cz])
            .map(|ll| ll[wx.rem_euclid(16) as usize][wy.rem_euclid(16) as usize][wz.rem_euclid(16) as usize])
            .unwrap_or(0);
        // Terrain-generated lava has no lava_levels entry; treat it as a source block.
        if stored == 0 && self.get_block(wx, wy, wz) == BlockType::Lava { 4 } else { stored }
    }

    fn refresh_active_lava(&mut self, wx: i32, wy: i32, wz: i32) {
        for (py, pz, px) in [(wy, wz, wx), (wy + 1, wz, wx)] {
            let pos = [px, py, pz];
            let below = self.get_block(px, py - 1, pz);
            if self.get_block(px, py, pz) == BlockType::Lava
                && (below == BlockType::Air || below == BlockType::Water)
            {
                self.active_lava.insert(pos);
            } else {
                self.active_lava.remove(&pos);
            }
        }
    }

    fn refresh_active_lava_spread(&mut self, wx: i32, wy: i32, wz: i32) {
        for (px, pz) in [(wx, wz), (wx+1, wz), (wx-1, wz), (wx, wz+1), (wx, wz-1)] {
            let pos = [px, wy, pz];
            let lvl = self.lava_level_at(px, wy, pz);
            let open = [[px+1,pz],[px-1,pz],[px,pz+1],[px,pz-1]].iter()
                .any(|&[nx,nz]| {
                    let b = self.get_block(nx, wy, nz);
                    b == BlockType::Air || b == BlockType::Water
                });
            if lvl > 1 && open {
                self.active_lava_spread.insert(pos);
            } else {
                self.active_lava_spread.remove(&pos);
            }
        }
    }

    pub fn tick_lava(&mut self, dt: f32) {
        // 1. Downward flow.
        let down: Vec<([i32; 3], u8)> = self.active_lava.iter().copied()
            .filter(|&[wx, wy, wz]| {
                let b = self.get_block(wx, wy, wz);
                let below = self.get_block(wx, wy - 1, wz);
                b == BlockType::Lava
                    && (below == BlockType::Air || below == BlockType::Water)
                    && !self.pending_lava.contains_key(&[wx, wy - 1, wz])
            })
            .map(|[wx, wy, wz]| ([wx, wy - 1, wz], 4u8))
            .collect();
        for (pos, lvl) in down {
            self.pending_lava.insert(pos, (3.0, lvl));
        }

        // 2. Horizontal spread — only blocks with level > 1 and open neighbors.
        let h: Vec<([i32; 3], u8)> = self.active_lava_spread.iter().copied()
            .filter_map(|[wx, wy, wz]| {
                let lvl = self.lava_level_at(wx, wy, wz);
                if lvl > 1 { Some(([wx, wy, wz], lvl)) } else { None }
            })
            .collect();
        for ([wx, wy, wz], lvl) in h {
            let new_lvl = lvl - 1;
            let mut still_active = false;
            for [nx, nz] in [[wx+1,wz],[wx-1,wz],[wx,wz+1],[wx,wz-1]] {
                let npos = [nx, wy, nz];
                let nb = self.get_block(nx, wy, nz);
                if nb == BlockType::Air || nb == BlockType::Water {
                    still_active = true;
                    if !self.pending_lava.contains_key(&npos) {
                        self.pending_lava.insert(npos, (3.0, new_lvl));
                    }
                }
            }
            if !still_active {
                self.active_lava_spread.remove(&[wx, wy, wz]);
            }
        }

        // 3. Tick timers.
        let mut to_fill: Vec<([i32; 3], u8)> = Vec::new();
        self.pending_lava.retain(|pos, (timer, lvl)| {
            *timer -= dt;
            if *timer <= 0.0 { to_fill.push((*pos, *lvl)); false } else { true }
        });

        // 4. Fill — handle water/lava interaction.
        for ([wx, wy, wz], lvl) in to_fill {
            let current = self.get_block(wx, wy, wz);
            if current == BlockType::Water {
                // Lava flowing into water → cobblestone at that cell.
                self.set_block(wx, wy, wz, BlockType::Cobblestone);
            } else if current == BlockType::Air {
                let adj_water = [[wx+1,wy,wz],[wx-1,wy,wz],[wx,wy+1,wz],
                                 [wx,wy,wz+1],[wx,wy,wz-1],[wx,wy-1,wz]]
                    .iter().any(|&[x,y,z]| self.get_block(x,y,z) == BlockType::Water);
                if adj_water {
                    // Lava hitting water wall → cobblestone plug.
                    self.set_block(wx, wy, wz, BlockType::Cobblestone);
                } else {
                    let cx = wx.div_euclid(16); let cy = wy.div_euclid(16); let cz = wz.div_euclid(16);
                    self.lava_levels
                        .entry([cx, cy, cz])
                        .or_insert_with(|| Box::new([[[0u8; 16]; 16]; 16]))
                        [wx.rem_euclid(16) as usize][wy.rem_euclid(16) as usize][wz.rem_euclid(16) as usize] = lvl;
                    self.set_block(wx, wy, wz, BlockType::Lava);
                }
            }
        }
    }

    /// Advance all growing crops and return every position that changed as (pos, new_block).
    /// Caller is responsible for broadcasting changes over the network.
    pub fn tick_crops(&mut self, dt: f32) -> Vec<([i32; 3], BlockType)> {
        let mut to_grow: Vec<[i32; 3]> = Vec::new();
        for (pos, (_, timer)) in self.active_crops.iter_mut() {
            *timer -= dt;
            if *timer <= 0.0 { to_grow.push(*pos); }
        }
        let mut grown = Vec::new();
        for [wx, wy, wz] in to_grow {
            match self.get_block(wx, wy, wz) {
                BlockType::Crop(id, stage) => {
                    let def = &CROPS[id as usize];
                    let next_stage = stage + 1;
                    if next_stage < def.stages {
                        let next = BlockType::Crop(id, next_stage);
                        self.set_block_recorded(wx, wy, wz, next);
                        grown.push(([wx, wy, wz], next));
                    } else {
                        self.active_crops.remove(&[wx, wy, wz]);
                    }
                }
                _ => { self.active_crops.remove(&[wx, wy, wz]); }
            }
        }
        grown
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
            .filter_map(|[wx, wy, wz]| {
                let cx = wx.div_euclid(16); let cy = wy.div_euclid(16); let cz = wz.div_euclid(16);
                self.water_levels.get(&[cx, cy, cz]).and_then(|wl| {
                    let lvl = wl[wx.rem_euclid(16) as usize][wy.rem_euclid(16) as usize][wz.rem_euclid(16) as usize];
                    if lvl > 0 { Some(([wx, wy, wz], lvl)) } else { None }
                })
            })
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
            let current = self.get_block(wx, wy, wz);
            if current == BlockType::Lava {
                // Water flowing into lava → cobblestone.
                self.set_block(wx, wy, wz, BlockType::Cobblestone);
            } else if current == BlockType::Air {
                let cx = wx.div_euclid(16); let cy = wy.div_euclid(16); let cz = wz.div_euclid(16);
                self.water_levels
                    .entry([cx, cy, cz])
                    .or_insert_with(|| Box::new([[[0u8; 16]; 16]; 16]))
                    [wx.rem_euclid(16) as usize][wy.rem_euclid(16) as usize][wz.rem_euclid(16) as usize] = lvl;
                self.set_block(wx, wy, wz, BlockType::Water);
                // Convert any adjacent lava to cobblestone.
                for [nx, ny, nz] in [[wx+1,wy,wz],[wx-1,wy,wz],[wx,wy+1,wz],
                                     [wx,wy,wz+1],[wx,wy,wz-1],[wx,wy-1,wz]] {
                    if self.get_block(nx, ny, nz) == BlockType::Lava {
                        self.set_block(nx, ny, nz, BlockType::Cobblestone);
                    }
                }
            }
        }
    }

    fn unload_distant_chunks(&mut self, center: [i32; 3], radius: i32) {
        let r2 = radius * radius;
        let mut removed: Vec<[i32; 3]> = Vec::new();
        self.chunks.retain(|&pos, _| {
            let dx = pos[0] - center[0];
            let dz = pos[2] - center[2];
            if dx * dx + dz * dz <= r2 { true } else { removed.push(pos); false }
        });
        self.terrain_queue.retain(|&pos| {
            let dx = pos[0] - center[0];
            let dz = pos[2] - center[2];
            dx * dx + dz * dz <= r2
        });
        // Drop water/lava data for each removed chunk in O(1) per chunk.
        for pos in &removed {
            self.water_levels.remove(pos);
            self.lava_levels.remove(pos);
        }
        // active_spread / active_lava_spread are small; retain is fine.
        self.active_spread.retain(|&[wx, wy, wz]| {
            self.chunks.contains_key(&[wx.div_euclid(16), wy.div_euclid(16), wz.div_euclid(16)])
        });
        self.active_lava_spread.retain(|&[wx, wy, wz]| {
            self.chunks.contains_key(&[wx.div_euclid(16), wy.div_euclid(16), wz.div_euclid(16)])
        });

        // Announce unique XZ columns that just lost all their chunks.
        let mut seen: HashSet<[i32; 2]> = HashSet::new();
        for &[cx, _, cz] in &removed {
            if seen.insert([cx, cz]) {
                self.unloaded_columns.push([cx, cz]);
                self.notified_columns.remove(&[cx, cz]);
            }
        }
    }

    /// Replace a block at world coords and mark the chunk (plus border neighbors) for rebuild.
    pub fn set_block(&mut self, wx: i32, wy: i32, wz: i32, block: BlockType) {
        if wy < 0 || wy >= WORLD_HEIGHT_CHUNKS * 16 { return; }
        let cx = wx.div_euclid(16);
        let cy = wy.div_euclid(16);
        let cz = wz.div_euclid(16);
        let lx = wx.rem_euclid(16) as usize;
        let ly = wy.rem_euclid(16) as usize;
        let lz = wz.rem_euclid(16) as usize;

        if block != BlockType::Water {
            if let Some(wl) = self.water_levels.get_mut(&[cx, cy, cz]) {
                wl[lx][ly][lz] = 0;
            }
        } else {
            let wl = self.water_levels.entry([cx, cy, cz]).or_insert_with(|| Box::new([[[0u8; 16]; 16]; 16]));
            if wl[lx][ly][lz] == 0 { wl[lx][ly][lz] = 8; }
        }
        if block != BlockType::Lava {
            if let Some(ll) = self.lava_levels.get_mut(&[cx, cy, cz]) {
                ll[lx][ly][lz] = 0;
            }
        } else {
            let ll = self.lava_levels.entry([cx, cy, cz]).or_insert_with(|| Box::new([[[0u8; 16]; 16]; 16]));
            if ll[lx][ly][lz] == 0 { ll[lx][ly][lz] = 4; }
        }

        if let Some(chunk) = self.chunks.get_mut(&[cx, cy, cz]) {
            chunk.set_block(lx, ly, lz, block);
            chunk.mark_for_rebuild();
            chunk.mark_sky_dirty();
        }
        if lx == 0  { if let Some(c) = self.chunks.get_mut(&[cx-1, cy, cz]) { c.mark_for_rebuild(); c.mark_sky_dirty(); } }
        if lx == 15 { if let Some(c) = self.chunks.get_mut(&[cx+1, cy, cz]) { c.mark_for_rebuild(); c.mark_sky_dirty(); } }
        if lz == 0  { if let Some(c) = self.chunks.get_mut(&[cx, cy, cz-1]) { c.mark_for_rebuild(); c.mark_sky_dirty(); } }
        if lz == 15 { if let Some(c) = self.chunks.get_mut(&[cx, cy, cz+1]) { c.mark_for_rebuild(); c.mark_sky_dirty(); } }
        if ly == 0  { if let Some(c) = self.chunks.get_mut(&[cx, cy-1, cz]) { c.mark_for_rebuild(); c.mark_sky_dirty(); } }
        if ly == 15 { if let Some(c) = self.chunks.get_mut(&[cx, cy+1, cz]) { c.mark_for_rebuild(); c.mark_sky_dirty(); } }
        self.refresh_active_water(wx, wy, wz);
        self.refresh_active_spread(wx, wy, wz);
        self.refresh_active_lava(wx, wy, wz);
        self.refresh_active_lava_spread(wx, wy, wz);
        // Register growing crops; remove when block is replaced or fully grown.
        let pos = [wx, wy, wz];
        match block {
            BlockType::Crop(id, stage) => {
                let def = &CROPS[id as usize];
                if def.final_solid && stage == def.stages - 1 {
                    self.active_crops.remove(&pos);
                } else {
                    self.active_crops.insert(pos, (id, def.grow_secs));
                }
            }
            _ => { self.active_crops.remove(&pos); }
        }
    }

    fn build_neighbor_edges(&self, cx: i32, cy: i32, cz: i32) -> NeighborEdges {
        let r_loaded = self.chunks.contains_key(&[cx + 1, cy, cz]);
        let l_loaded = self.chunks.contains_key(&[cx - 1, cy, cz]);
        let f_loaded = self.chunks.contains_key(&[cx, cy, cz + 1]);
        let b_loaded = self.chunks.contains_key(&[cx, cy, cz - 1]);
        let a_loaded = self.chunks.contains_key(&[cx, cy + 1, cz]);
        let d_loaded = self.chunks.contains_key(&[cx, cy - 1, cz]);
        NeighborEdges {
            right: if r_loaded { self.chunks[&[cx+1,cy,cz]].edge_right()  } else { [[BlockType::Air; 16]; 16] },
            left:  if l_loaded { self.chunks[&[cx-1,cy,cz]].edge_left()   } else { [[BlockType::Air; 16]; 16] },
            front: if f_loaded { self.chunks[&[cx,cy,cz+1]].edge_front()  } else { [[BlockType::Air; 16]; 16] },
            back:  if b_loaded { self.chunks[&[cx,cy,cz-1]].edge_back()   } else { [[BlockType::Air; 16]; 16] },
            above: if a_loaded { self.chunks[&[cx,cy+1,cz]].edge_bottom() } else { [[BlockType::Air; 16]; 16] },
            below: if d_loaded { self.chunks[&[cx,cy-1,cz]].edge_top()    } else { [[BlockType::Air; 16]; 16] },
            above_sky: if a_loaded { self.chunks[&[cx,cy+1,cz]].sky_light_bottom()      } else { [[15u8; 16]; 16] },
            right_sky: if r_loaded { self.chunks[&[cx+1,cy,cz]].sky_light_edge_right()  } else { [[0u8; 16]; 16] },
            left_sky:  if l_loaded { self.chunks[&[cx-1,cy,cz]].sky_light_edge_left()   } else { [[0u8; 16]; 16] },
            front_sky: if f_loaded { self.chunks[&[cx,cy,cz+1]].sky_light_edge_front()  } else { [[0u8; 16]; 16] },
            back_sky:  if b_loaded { self.chunks[&[cx,cy,cz-1]].sky_light_edge_back()   } else { [[0u8; 16]; 16] },
            below_sky: if d_loaded { self.chunks[&[cx,cy-1,cz]].sky_light_edge_top()    } else { [[0u8;  16]; 16] },
            wl_right: if r_loaded { self.water_edge_at_lx(cx+1, cy, cz, 0)  } else { [[0u8; 16]; 16] },
            wl_left:  if l_loaded { self.water_edge_at_lx(cx-1, cy, cz, 15) } else { [[0u8; 16]; 16] },
            wl_front: if f_loaded { self.water_edge_at_lz(cx, cy, cz+1, 0)  } else { [[0u8; 16]; 16] },
            wl_back:  if b_loaded { self.water_edge_at_lz(cx, cy, cz-1, 15) } else { [[0u8; 16]; 16] },
            right_loaded: r_loaded,
            left_loaded:  l_loaded,
            front_loaded: f_loaded,
            back_loaded:  b_loaded,
            above_loaded: a_loaded,
            below_loaded: d_loaded,
        }
    }

    // ── Water level helpers (used when snapshotting data for mesh threads) ───

    fn water_levels_snapshot(&self, cx: i32, cy: i32, cz: i32) -> WaterLevels {
        match self.water_levels.get(&[cx, cy, cz]) {
            Some(wl) => **wl,
            None => [[[0u8; 16]; 16]; 16],
        }
    }

    fn water_edge_at_lx(&self, cx: i32, cy: i32, cz: i32, lx: i32) -> [[u8; 16]; 16] {
        let mut e = [[0u8; 16]; 16];
        if let Some(wl) = self.water_levels.get(&[cx, cy, cz]) {
            for ly in 0..16usize {
                for lz in 0..16usize {
                    e[ly][lz] = wl[lx as usize][ly][lz];
                }
            }
        }
        e
    }

    fn water_edge_at_lz(&self, cx: i32, cy: i32, cz: i32, lz: i32) -> [[u8; 16]; 16] {
        let mut e = [[0u8; 16]; 16];
        if let Some(wl) = self.water_levels.get(&[cx, cy, cz]) {
            for ly in 0..16usize {
                for lx in 0..16usize {
                    e[ly][lx] = wl[lx][ly][lz as usize];
                }
            }
        }
        e
    }

    /// Returns a reference to the chunk at chunk-grid coordinates (cx, cz).
    /// Used by the minimap to batch block lookups per chunk instead of per block.
    /// High-throughput update used during game loading — drains the full pipeline
    /// each call so the spawn area becomes ready as fast as possible.
    pub fn update_loading(&mut self, player_pos: [f32; 3]) {
        let new_chunk = [
            (player_pos[0] / 16.0).floor() as i32,
            0,
            (player_pos[2] / 16.0).floor() as i32,
        ];
        if new_chunk != self.player_chunk {
            self.load_chunks_around(new_chunk, self.loaded_radius);
            self.load_chunks_around_terrain_only(new_chunk, self.loaded_radius + 2);
            self.player_chunk = new_chunk;
        }
        self.finalize_blocks(64);
        self.finalize_sky(64);
        self.finalize_meshes(64);
        self.dispatch_terrain_threads();
        self.dispatch_sky_threads();
        self.dispatch_mesh_threads();
    }

    pub fn set_radius(&mut self, radius: i32) {
        self.loaded_radius = radius;
        self.player_chunk = [i32::MAX; 3];
    }

    pub fn is_chunk_meshed(&self, cx: i32, cz: i32) -> bool {
        // Column is considered meshed when the surface chunk (cy=4, world Y 64–79) is ready.
        let surface_cy = SEA_LEVEL / 16;
        self.chunks.get(&[cx, surface_cy, cz]).map_or(false, |c| c.mesh.is_some())
    }

    /// XZ columns whose last chunk was removed this frame (call once per frame).
    pub fn take_unloaded_columns(&mut self) -> Vec<[i32; 2]> {
        std::mem::take(&mut self.unloaded_columns)
    }

    /// XZ columns whose entity-supporting band (cy 4–10) just became fully loaded (call once per frame).
    pub fn take_loaded_columns(&mut self) -> Vec<[i32; 2]> {
        std::mem::take(&mut self.loaded_columns)
    }

    pub fn is_chunk_loaded(&self, cx: i32, cy: i32, cz: i32) -> bool {
        self.chunks.contains_key(&[cx, cy, cz])
    }

    /// Allow a column to be re-announced via take_loaded_columns() once its missing chunks arrive.
    pub fn reset_column_notification(&mut self, cx: i32, cz: i32) {
        self.notified_columns.remove(&[cx, cz]);
    }

    /// All XZ columns whose entity-supporting band is currently fully loaded.
    /// Use once at game-start to seed the spawned-columns set from already-loaded terrain.
    pub fn all_loaded_columns(&self) -> Vec<[i32; 2]> {
        self.notified_columns.iter().copied().collect()
    }

    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

/// Count meshed chunks in the surface Y band (cy 6–10, world Y 96–175).
    /// Underground solid chunks and high-altitude air chunks mesh near-instantly
    /// and swamp the global count; this metric only tracks the visible terrain layers.
    pub fn meshed_surface_chunk_count(&self) -> usize {
        self.chunks.iter()
            .filter(|(pos, c)| pos[1] >= 6 && pos[1] <= 10 && c.mesh.is_some())
            .count()
    }

    /// Returns the chunk at (cx, cy, cz) in chunk-grid coordinates.
    pub fn chunk_at(&self, cx: i32, cy: i32, cz: i32) -> Option<&Chunk> {
        self.chunks.get(&[cx, cy, cz])
    }

    /// Returns the chunk containing the surface terrain at (cx, cz) — i.e. the
    /// chunk at the sea-level Y layer, useful for minimap and entity spawning.
    pub fn surface_chunk_at(&self, cx: i32, cz: i32) -> Option<&Chunk> {
        let surface_cy = SEA_LEVEL / 16;
        self.chunks.get(&[cx, surface_cy, cz])
    }

    pub fn get_block(&self, wx: i32, wy: i32, wz: i32) -> BlockType {
        if wy < 0 || wy >= WORLD_HEIGHT_CHUNKS * 16 { return BlockType::Air; }
        let cy = wy.div_euclid(16);
        let chunk_pos = [wx.div_euclid(16), cy, wz.div_euclid(16)];
        let lx = wx.rem_euclid(16) as usize;
        let ly = wy.rem_euclid(16) as usize;
        let lz = wz.rem_euclid(16) as usize;
        if let Some(chunk) = self.chunks.get(&chunk_pos) {
            chunk.get_block(lx, ly, lz)
        } else {
            BlockType::Air
        }
    }

    pub fn get_sky_light(&self, wx: i32, wy: i32, wz: i32) -> u8 {
        if wy < 0 || wy >= WORLD_HEIGHT_CHUNKS * 16 { return 0; }
        let cx = wx.div_euclid(16);
        let cy = wy.div_euclid(16);
        let cz = wz.div_euclid(16);
        let lx = wx.rem_euclid(16) as usize;
        let ly = wy.rem_euclid(16) as usize;
        let lz = wz.rem_euclid(16) as usize;
        if let Some(chunk) = self.chunks.get(&[cx, cy, cz]) {
            chunk.get_sky_light_at(lx, ly, lz)
        } else {
            0
        }
    }

    pub fn surface_height(&self, wx: i32, wz: i32) -> i32 {
        for y in (0..WORLD_HEIGHT_CHUNKS * 16).rev() {
            let b = self.get_block(wx, y, wz);
            if b.is_solid() && !matches!(b, BlockType::Log | BlockType::Leaves) {
                return y + 1;
            }
        }
        0
    }

    /// Find a safe above-ground spawn position near world coords (8, 8).
    /// Searches outward in rings, rejecting underwater columns and columns
    /// where a tree trunk would put the player inside a solid block.
    pub fn find_spawn_point(&self) -> [f32; 3] {
        for r in 0..=8i32 {
            for dx in -r..=r {
                for dz in -r..=r {
                    // Only visit the perimeter of each ring (not interior).
                    if r > 0 && dx.abs() != r && dz.abs() != r { continue; }
                    let wx = 8 + dx;
                    let wz = 8 + dz;
                    let ground = self.surface_height(wx, wz);
                    if ground == 0 { continue; }
                    if self.get_block(wx, ground, wz) == BlockType::Water { continue; }
                    let max_y = (WORLD_HEIGHT_CHUNKS * 16 - 2) as i32;
                    for y in ground..max_y {
                        if !self.get_block(wx, y, wz).is_solid()
                            && !self.get_block(wx, y + 1, wz).is_solid()
                        {
                            return [wx as f32 + 0.5, y as f32, wz as f32 + 0.5];
                        }
                    }
                }
            }
        }
        // Fallback — should never be reached with a loaded chunk.
        [8.5, self.surface_height(8, 8) as f32 + 2.0, 8.5]
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

        let mut t_prev = 0.0f32;
        loop {
            let t = t_max_x.min(t_max_y).min(t_max_z);
            if t > max_dist { return None; }

            let block = self.get_block(bx, by, bz);
            if block.is_targetable() {
                let sh = block.selection_height();
                let hit = sh >= 1.0 || {
                    let y0 = origin[1] + t_prev * dir[1] - by as f32;
                    let y1 = origin[1] + t      * dir[1] - by as f32;
                    let (y_lo, y_hi) = if y0 <= y1 { (y0, y1) } else { (y1, y0) };
                    y_hi >= 0.0 && y_lo < sh
                };
                if hit { return Some([bx, by, bz]); }
            }

            t_prev = t;
            if t_max_x <= t_max_y && t_max_x <= t_max_z {
                bx += step_x; t_max_x += t_delta_x;
            } else if t_max_y <= t_max_z {
                by += step_y; t_max_y += t_delta_y;
            } else {
                bz += step_z; t_max_z += t_delta_z;
            }
        }
    }

    /// Like raycast but also returns the adjacent (air) block position where a new block would be placed.
    pub fn raycast_face(&self, origin: [f32; 3], dir: [f32; 3], max_dist: f32) -> Option<([i32;3], [i32;3])> {
        let dx = dir[0]; let dy = dir[1]; let dz = dir[2];
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
        let mut adj = [bx, by, bz];
        let mut t_prev = 0.0f32;
        loop {
            let t = t_max_x.min(t_max_y).min(t_max_z);
            if t > max_dist { return None; }
            let block = self.get_block(bx, by, bz);
            if block.is_targetable() {
                let sh = block.selection_height();
                let hit = sh >= 1.0 || {
                    let y0 = origin[1] + t_prev * dir[1] - by as f32;
                    let y1 = origin[1] + t      * dir[1] - by as f32;
                    let (y_lo, y_hi) = if y0 <= y1 { (y0, y1) } else { (y1, y0) };
                    y_hi >= 0.0 && y_lo < sh
                };
                if hit { return Some(([bx, by, bz], adj)); }
            }
            t_prev = t;
            adj = [bx, by, bz];
            if t_max_x <= t_max_y && t_max_x <= t_max_z {
                bx += step_x; t_max_x += t_delta_x;
            } else if t_max_y <= t_max_z {
                by += step_y; t_max_y += t_delta_y;
            } else {
                bz += step_z; t_max_z += t_delta_z;
            }
        }
    }

    /// Render chunks into the active shadow cascade, culling against the cascade's
    /// own orthographic frustum. This eliminates most chunks from the tight near
    /// cascades (cascade 0 covers only 12 m, yet there are 81 loaded chunks).
    pub fn draw_shadow(&self, shadow_pass: &ShadowPass) {
        let lsm = shadow_pass.current_light_space_matrix();
        // Extract frustum planes from the light-space matrix (world → light clip).
        // Passing IDENTITY as view folds lsm into the "projection" slot so the
        // resulting mvp = lsm * I = lsm, giving correct world-space plane equations.
        let cascade_frustum = Frustum::from_view_projection(&glam::Mat4::IDENTITY, &lsm);
        for chunk in self.chunks.values() {
            if chunk.mesh.is_some() && chunk.is_in_frustum(&cascade_frustum) {
                shadow_pass.draw_chunk(chunk);
            }
        }
    }

    pub fn world_stats(&mut self) -> WorldStats {
        let elapsed = self.rate_instant.elapsed().as_secs_f32();
        if elapsed >= 0.5 {
            self.last_gen_rate  = self.gen_completed  as f32 / elapsed;
            self.last_mesh_rate = self.mesh_completed as f32 / elapsed;
            self.gen_completed  = 0;
            self.mesh_completed = 0;
            self.rate_instant   = Instant::now();
        }
        WorldStats {
            loaded:           self.chunks.len(),
            meshed:           self.chunks.values().filter(|c| c.mesh.is_some()).count(),
            terrain_queued:   self.terrain_queue.len(),
            terrain_inflight: self.pending_blocks.len(),
            mesh_inflight:    self.pending_meshes.len(),
            gen_per_sec:      self.last_gen_rate,
            mesh_per_sec:     self.last_mesh_rate,
        }
    }

    pub fn draw_opaque(&self, renderer: &ChunkRenderer, camera: &crate::camera::Camera) -> usize {
        let frustum = camera.frustum();
        renderer.set_transparent_pass(false);
        let mut drawn = 0;
        for chunk in self.chunks.values() {
            if chunk.mesh.is_some() && chunk.is_in_frustum(&frustum) {
                renderer.draw_chunk(chunk);
                drawn += 1;
            }
        }
        drawn
    }

    pub fn draw_transparent(&self, renderer: &ChunkRenderer, camera: &crate::camera::Camera) {
        let frustum = camera.frustum();
        renderer.set_transparent_pass(true);
        for chunk in self.chunks.values() {
            if chunk.mesh.is_some() && chunk.is_in_frustum(&frustum) {
                renderer.draw_chunk(chunk);
            }
        }
    }
}
