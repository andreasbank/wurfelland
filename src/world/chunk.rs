use crate::renderer::ChunkMesh;
use crate::world::face::Face;
use crate::world::BlockType;
use crate::world::biome::Biome;
use glam::Vec3;
use crate::camera::frustum::Frustum;
use noise::{NoiseFn, Perlin};

pub type Blocks = [[[BlockType; 16]; 16]; 16];

/// Per-block water levels for a chunk (0 = not water, 1–7 = flowing, 8 = source).
pub type WaterLevels = [[[u8; 16]; 16]; 16];

/// The single-block-deep face of an adjacent chunk, needed for border face culling.
/// `right[y][z]`  = blocks[0][y][z]  of the chunk to our +X
/// `left[y][z]`   = blocks[15][y][z] of the chunk to our -X
/// `front[y][x]`  = blocks[x][y][0]  of the chunk to our +Z
/// `back[y][x]`   = blocks[x][y][15] of the chunk to our -Z
pub struct NeighborEdges {
    pub right: [[BlockType; 16]; 16],
    pub left:  [[BlockType; 16]; 16],
    pub front: [[BlockType; 16]; 16],
    pub back:  [[BlockType; 16]; 16],
    /// Water levels at lx=0 of the +X neighbour  [y][z]
    pub wl_right: [[u8; 16]; 16],
    /// Water levels at lx=15 of the -X neighbour [y][z]
    pub wl_left:  [[u8; 16]; 16],
    /// Water levels at lz=0 of the +Z neighbour  [y][x]
    pub wl_front: [[u8; 16]; 16],
    /// Water levels at lz=15 of the -Z neighbour [y][x]
    pub wl_back:  [[u8; 16]; 16],
    /// Whether each neighbouring chunk was actually loaded when the edges were snapshotted.
    /// Used so fluid face-visibility can treat an unloaded neighbour as same-fluid
    /// (avoiding a seam wall) without hiding legitimate faces on solid blocks.
    pub right_loaded: bool,
    pub left_loaded:  bool,
    pub front_loaded: bool,
    pub back_loaded:  bool,
}

impl NeighborEdges {}

/// 1-in-`freq` chance per column for tree placement.
fn should_place_tree(world_x: i32, world_z: i32, freq: u32) -> bool {
    if freq == 0 { return false; }
    let h = world_x.wrapping_mul(374761393_i32)
                   .wrapping_add(world_z.wrapping_mul(668265263_i32));
    (h.unsigned_abs() % freq) == 0
}


/// Small tree: 4-block trunk, tight 3×3 canopy.
fn plant_tree_small(blocks: &mut Blocks, lx: usize, surf_y: usize, lz: usize) {
    if lx < 1 || lx > 14 || lz < 1 || lz > 14 { return; }
    if surf_y + 5 >= 16 { return; }
    for dy in 1..=4 { blocks[lx][surf_y + dy][lz] = BlockType::Log; }
    for layer in [3usize, 4] {
        for dx in -1i32..=1 { for dz in -1i32..=1 {
            let by = surf_y + layer;
            let bx = (lx as i32 + dx) as usize;
            let bz = (lz as i32 + dz) as usize;
            if blocks[bx][by][bz] != BlockType::Log { blocks[bx][by][bz] = BlockType::Leaves; }
        }}
    }
    blocks[lx][surf_y + 5][lz] = BlockType::Leaves;
}

/// Medium tree: 5-block trunk, 3×3 canopy + cross top.
fn plant_tree_medium(blocks: &mut Blocks, lx: usize, surf_y: usize, lz: usize) {
    if lx < 1 || lx > 14 || lz < 1 || lz > 14 { return; }
    if surf_y + 7 >= 16 { return; }
    for dy in 1..=5 { blocks[lx][surf_y + dy][lz] = BlockType::Log; }
    for layer in [4usize, 5] {
        for dx in -1i32..=1 { for dz in -1i32..=1 {
            let by = surf_y + layer;
            let bx = (lx as i32 + dx) as usize;
            let bz = (lz as i32 + dz) as usize;
            if blocks[bx][by][bz] != BlockType::Log { blocks[bx][by][bz] = BlockType::Leaves; }
        }}
    }
    for (dx, dz) in [(0i32,0i32),(1,0),(-1,0),(0,1),(0,-1)] {
        let bx = (lx as i32 + dx) as usize;
        let bz = (lz as i32 + dz) as usize;
        if blocks[bx][surf_y + 6][bz] != BlockType::Log { blocks[bx][surf_y + 6][bz] = BlockType::Leaves; }
    }
    blocks[lx][surf_y + 7][lz] = BlockType::Leaves;
}

/// Large tree: 6-block trunk, wide 5×5 canopy layers.
fn plant_tree_large(blocks: &mut Blocks, lx: usize, surf_y: usize, lz: usize) {
    if lx < 2 || lx > 13 || lz < 2 || lz > 13 { return; }
    if surf_y + 8 >= 16 { return; }
    for dy in 1..=6 { blocks[lx][surf_y + dy][lz] = BlockType::Log; }
    // Two 5×5 (corners clipped) layers near canopy base
    for layer in [5usize, 6] {
        for dx in -2i32..=2 { for dz in -2i32..=2 {
            if dx.abs() == 2 && dz.abs() == 2 { continue; }
            let by = surf_y + layer;
            let bx = (lx as i32 + dx) as usize;
            let bz = (lz as i32 + dz) as usize;
            if blocks[bx][by][bz] != BlockType::Log { blocks[bx][by][bz] = BlockType::Leaves; }
        }}
    }
    // 3×3 at trunk top
    for dx in -1i32..=1 { for dz in -1i32..=1 {
        let bx = (lx as i32 + dx) as usize;
        let bz = (lz as i32 + dz) as usize;
        if blocks[bx][surf_y + 7][bz] != BlockType::Log { blocks[bx][surf_y + 7][bz] = BlockType::Leaves; }
    }}
    blocks[lx][surf_y + 8][lz] = BlockType::Leaves;
}

pub struct Chunk {
    pub position: [i32; 3],
    blocks: Blocks,
    pub mesh: Option<ChunkMesh>,
    needs_rebuild: bool,
    // Precomputed translation matrix — the chunk never moves, so compute once.
    model: glam::Mat4,
}

impl Chunk {
    pub fn generate(position: [i32; 3], seed: u32) -> Self {
        let terrain = Perlin::new(seed);
        let temp    = Perlin::new(seed.wrapping_add(58));
        let moist   = Perlin::new(seed.wrapping_add(158));
        let cont    = Perlin::new(seed.wrapping_add(258));

        let mut blocks     = [[[BlockType::Air; 16]; 16]; 16];
        let mut surface    = [[0usize; 16]; 16];
        let mut biome_grid = [[Biome::Plains; 16]; 16];

        const SEA_LEVEL: usize = 10;

        // ── Block placement ──────────────────────────────────────────────────
        for x in 0..16usize {
            for z in 0..16usize {
                let wx = (position[0] * 16 + x as i32) as f64;
                let wz = (position[2] * 16 + z as i32) as f64;

                let biome = Biome::from_noise(
                    temp .get([wx * 0.003, wz * 0.003]),
                    moist.get([wx * 0.003, wz * 0.003]),
                    cont .get([wx * 0.005, wz * 0.005]),
                );
                biome_grid[x][z] = biome;
                let p = biome.params();

                let noise_val = (terrain.get([wx * p.scale, wz * p.scale]) + 1.0) / 2.0;
                let surf_y = (p.base_height + noise_val as f32 * p.amplitude) as usize;
                let surf_y = surf_y.min(15);
                surface[x][z] = surf_y;

                let underwater = surf_y < SEA_LEVEL;
                for y in 0..16usize {
                    blocks[x][y][z] = if y < surf_y.saturating_sub(3) {
                        BlockType::Stone
                    } else if y < surf_y {
                        p.sub_surface_block
                    } else if y == surf_y {
                        if underwater { p.sub_surface_block } else { p.surface_block }
                    } else {
                        BlockType::Air
                    };
                }

                if underwater {
                    for y in (surf_y + 1)..=SEA_LEVEL {
                        if y < 16 { blocks[x][y][z] = BlockType::Water; }
                    }
                }
            }
        }

        // ── Trees ────────────────────────────────────────────────────────────
        for x in 0..16usize {
            for z in 0..16usize {
                let world_x = position[0] * 16 + x as i32;
                let world_z = position[2] * 16 + z as i32;
                let p    = biome_grid[x][z].params();
                let surf = surface[x][z];
                if should_place_tree(world_x, world_z, p.tree_freq)
                    && surf >= SEA_LEVEL
                    && blocks[x][surf][z] == BlockType::Grass
                {
                    // Pick tree size with a separate hash: ~50% small, 35% medium, 15% large.
                    let sh = (world_x.wrapping_mul(1_723_459)
                              ^ world_z.wrapping_mul(9_876_543)) as u32;
                    match sh % 20 {
                        0..=9  => plant_tree_small (&mut blocks, x, surf, z),
                        10..=16 => plant_tree_medium(&mut blocks, x, surf, z),
                        _       => plant_tree_large (&mut blocks, x, surf, z),
                    }
                }
            }
        }

        // ── Grass (patch-based) ──────────────────────────────────────────────
        // The world is divided into 6×6-block patches. Each patch is either
        // active (grass) or empty, decided by a coarse hash. Within an active
        // patch ~70% of columns get a grass plant; the size (short/medium/tall)
        // is chosen per-column so patches contain a natural mix.
        for x in 0..16usize {
            for z in 0..16usize {
                let p     = biome_grid[x][z].params();
                if p.grass_freq == 0 { continue; }

                let surf  = surface[x][z];
                let above = surf + 1;
                if above >= 16 { continue; }
                if blocks[x][surf][z]  != BlockType::Grass { continue; }
                if blocks[x][above][z] != BlockType::Air   { continue; }

                let world_x = position[0] * 16 + x as i32;
                let world_z = position[2] * 16 + z as i32;

                // Patch-level hash: same for all columns in the same 6×6 cell.
                let px = world_x.div_euclid(6);
                let pz = world_z.div_euclid(6);
                let ph = (px.wrapping_mul(374_761_393_i32)
                          ^ pz.wrapping_mul(668_265_263_i32)) as u32;
                if ph % p.grass_freq != 0 { continue; } // patch inactive

                // Column-level hash: varies within the patch.
                let ch = (world_x.wrapping_mul(1_234_567_i32)
                          ^ world_z.wrapping_mul(7_654_321_i32)) as u32;
                if ch % 10 >= 7 { continue; } // ~70% fill within the patch

                // Size mix: 40% short, 40% medium, 20% full (stacked).
                match (ch / 10) % 5 {
                    0 | 1 => {
                        blocks[x][above][z] = BlockType::GrassShort;
                    }
                    2 | 3 => {
                        blocks[x][above][z] = BlockType::TallGrass;
                    }
                    _ => {
                        blocks[x][above][z] = BlockType::TallGrass;
                        if above + 1 < 16 && blocks[x][above + 1][z] == BlockType::Air {
                            blocks[x][above + 1][z] = BlockType::TallGrass;
                        }
                    }
                }
            }
        }

        let model = glam::Mat4::from_translation(glam::Vec3::new(
            position[0] as f32 * 16.0,
            position[1] as f32 * 16.0,
            position[2] as f32 * 16.0,
        ));
        Chunk { position, blocks, mesh: None, needs_rebuild: true, model }
    }

    pub fn get_block(&self, x: usize, y: usize, z: usize) -> BlockType {
        self.blocks[x][y][z]
    }

    pub fn set_block(&mut self, x: usize, y: usize, z: usize, block: BlockType) {
        self.blocks[x][y][z] = block;
    }

    /// Returns a copy of the block data — cheap since BlockType is Copy.
    pub fn blocks_snapshot(&self) -> Blocks {
        self.blocks
    }

    /// The face of this chunk that borders the chunk to our +X (their lx=0).
    pub fn edge_right(&self) -> [[BlockType; 16]; 16] {
        let mut e = [[BlockType::Air; 16]; 16];
        for y in 0..16 { for z in 0..16 { e[y][z] = self.blocks[0][y][z]; } }
        e
    }

    /// The face of this chunk that borders the chunk to our -X (their lx=15).
    pub fn edge_left(&self) -> [[BlockType; 16]; 16] {
        let mut e = [[BlockType::Air; 16]; 16];
        for y in 0..16 { for z in 0..16 { e[y][z] = self.blocks[15][y][z]; } }
        e
    }

    /// The face of this chunk that borders the chunk to our +Z (their lz=0).
    pub fn edge_front(&self) -> [[BlockType; 16]; 16] {
        let mut e = [[BlockType::Air; 16]; 16];
        for y in 0..16 { for x in 0..16 { e[y][x] = self.blocks[x][y][0]; } }
        e
    }

    /// The face of this chunk that borders the chunk to our -Z (their lz=15).
    pub fn edge_back(&self) -> [[BlockType; 16]; 16] {
        let mut e = [[BlockType::Air; 16]; 16];
        for y in 0..16 { for x in 0..16 { e[y][x] = self.blocks[x][y][15]; } }
        e
    }

    pub fn model_matrix(&self) -> glam::Mat4 {
        self.model
    }

    pub fn needs_mesh(&self) -> bool {
        self.needs_rebuild
    }

    pub fn mark_for_rebuild(&mut self) {
        self.needs_rebuild = true;
    }

    /// Called just before dispatching a mesh thread so we don't re-dispatch next frame.
    pub fn mark_mesh_dispatched(&mut self) {
        self.needs_rebuild = false;
    }

    pub fn finalize_mesh(&mut self, vertices: Vec<f32>) {
        self.mesh = Some(ChunkMesh::from_vertices(&vertices));
        // Do not clear needs_rebuild here. mark_mesh_dispatched already cleared it at
        // dispatch time. If mark_for_rebuild was called while the mesh thread was in
        // flight (e.g. a neighbor arrived), that pending rebuild must be preserved.
    }

    /// Build vertex data off the main thread.
    /// Takes a block snapshot + pre-extracted neighbor edge slices + water level snapshot.
    pub fn build_vertices(blocks: &Blocks, edges: &NeighborEdges, water_levels: &WaterLevels) -> Vec<f32> {
        // Rough estimate: ~10% of blocks are surface blocks with ~3 visible faces on average.
        // 16³ * 0.10 * 3 faces * 6 verts * 11 floats ≈ 8192. Sized to avoid early reallocs.
        let mut vertices = Vec::with_capacity(8192);

        for x in 0..16usize {
            for y in 0..16usize {
                for z in 0..16usize {
                    let block = blocks[x][y][z];

                    if block == BlockType::Air {
                        continue;
                    }

                    if block == BlockType::TallGrass {
                        vertices.extend(Self::cross_vertices(x as f32, y as f32, z as f32, block, 1.0));
                        continue;
                    }
                    if block == BlockType::GrassShort {
                        vertices.extend(Self::cross_vertices(x as f32, y as f32, z as f32, block, 0.45));
                        continue;
                    }

                    // Water gets variable-height geometry driven by water_levels.
                    if block == BlockType::Water {
                        let lxi = x as i32;
                        let lyi = y as i32;
                        let lzi = z as i32;
                        for face in [Face::Right, Face::Left, Face::Up, Face::Down, Face::Front, Face::Back] {
                            if !Self::is_face_visible(blocks, x, y, z, face, edges) { continue; }
                            let corners: [f32; 4] = match face {
                                Face::Up => [
                                    // v0=front-left (x,z+1), v1=front-right (x+1,z+1)
                                    // v2=back-right (x+1,z), v3=back-left  (x,z)
                                    Self::water_corner_h(lxi,   lyi, lzi+1, blocks, water_levels, edges),
                                    Self::water_corner_h(lxi+1, lyi, lzi+1, blocks, water_levels, edges),
                                    Self::water_corner_h(lxi+1, lyi, lzi,   blocks, water_levels, edges),
                                    Self::water_corner_h(lxi,   lyi, lzi,   blocks, water_levels, edges),
                                ],
                                Face::Right => [0.0,
                                    Self::water_corner_h(lxi+1, lyi, lzi,   blocks, water_levels, edges),
                                    Self::water_corner_h(lxi+1, lyi, lzi+1, blocks, water_levels, edges),
                                    0.0],
                                Face::Left => [0.0,
                                    Self::water_corner_h(lxi,   lyi, lzi+1, blocks, water_levels, edges),
                                    Self::water_corner_h(lxi,   lyi, lzi,   blocks, water_levels, edges),
                                    0.0],
                                Face::Front => [0.0,
                                    Self::water_corner_h(lxi+1, lyi, lzi+1, blocks, water_levels, edges),
                                    Self::water_corner_h(lxi,   lyi, lzi+1, blocks, water_levels, edges),
                                    0.0],
                                Face::Back => [0.0,
                                    Self::water_corner_h(lxi,   lyi, lzi,   blocks, water_levels, edges),
                                    Self::water_corner_h(lxi+1, lyi, lzi,   blocks, water_levels, edges),
                                    0.0],
                                Face::Down => [0.0; 4],
                            };
                            vertices.extend(Self::water_face_vertices(
                                x as f32, y as f32, z as f32, face, block, corners,
                            ));
                        }
                        continue;
                    }

                    for face in [Face::Right, Face::Left, Face::Up, Face::Down, Face::Front, Face::Back] {
                        if Self::is_face_visible(blocks, x, y, z, face, edges) {
                            let ao = Self::compute_ao(blocks, x as i32, y as i32, z as i32, face);
                            vertices.extend(Self::face_vertices_real_(
                                x as f32, y as f32, z as f32, face, block, ao,
                            ));
                        }
                    }
                }
            }
        }

        vertices
    }

    // ── Water helpers ────────────────────────────────────────────────────────

    /// Look up the water level at chunk-local coords, falling back to neighbor edges.
    fn water_level_local(lx: i32, ly: i32, lz: i32,
                         water_levels: &WaterLevels,
                         edges: &NeighborEdges) -> u8 {
        if ly < 0 || ly >= 16 { return 0; }
        if lx < 0 {
            if lz >= 0 && lz < 16 { edges.wl_left[ly as usize][lz as usize] } else { 0 }
        } else if lx >= 16 {
            if lz >= 0 && lz < 16 { edges.wl_right[ly as usize][lz as usize] } else { 0 }
        } else if lz < 0 {
            edges.wl_back[ly as usize][lx as usize]
        } else if lz >= 16 {
            edges.wl_front[ly as usize][lx as usize]
        } else {
            water_levels[lx as usize][ly as usize][lz as usize]
        }
    }

    /// Height [0.0, 1.0] for the water surface at corner (cx, cz).
    /// Corner (cx, cz) is shared by blocks (cx-1,cz-1), (cx,cz-1), (cx-1,cz), (cx,cz).
    fn water_corner_h(cx: i32, cy: i32, cz: i32,
                      _blocks: &Blocks,
                      water_levels: &WaterLevels,
                      edges: &NeighborEdges) -> f32 {
        let mut max_level: u8 = 0;
        for (bx, bz) in [(cx-1, cz-1), (cx, cz-1), (cx-1, cz), (cx, cz)] {
            let lvl = Self::water_level_local(bx, cy, bz, water_levels, edges);
            if lvl == 0 { continue; }
            // Block with water directly above it counts as full height.
            let above = Self::water_level_local(bx, cy + 1, bz, water_levels, edges);
            let eff = if above > 0 { 8 } else { lvl };
            max_level = max_level.max(eff);
        }
        max_level as f32 / 8.0
    }

    /// Emit vertices for a water face using per-corner height offsets.
    /// `corners[i]` is the Y offset (0.0–1.0) for vertex i above the block's base Y.
    /// For Up: all 4 corners vary. For side faces: corners[1] and [2] are the top pair.
    fn water_face_vertices(x: f32, y: f32, z: f32, face: Face,
                           block_type: BlockType, corners: [f32; 4]) -> Vec<f32> {
        // Surface water sits 10% below a full block so wave crests never
        // protrude above neighbouring non-water blocks.
        const SURFACE_SCALE: f32 = 0.9;
        let mut pos = face.positions(x, y, z);
        match face {
            Face::Up => {
                for i in 0..4 { pos[i][1] = y + corners[i] * SURFACE_SCALE; }
            }
            Face::Right | Face::Left | Face::Front | Face::Back => {
                pos[1][1] = y + corners[1] * SURFACE_SCALE;
                pos[2][1] = y + corners[2] * SURFACE_SCALE;
            }
            Face::Down => {}
        }
        let tex = face.texture_coords(block_type.texture_id(face), 16);
        let [r, g, b] = block_type.color();
        let bright = match face {
            Face::Up                  => 1.0,
            Face::Down                => 0.5,
            Face::Front | Face::Back  => 0.8,
            Face::Left  | Face::Right => 0.65,
        };
        let normal = face.normal();
        let mut verts = Vec::new();
        for &vi in &[0usize, 1, 2, 0, 2, 3] {
            verts.push(pos[vi][0]);
            verts.push(pos[vi][1]);
            verts.push(pos[vi][2]);
            verts.push(r * bright);
            verts.push(g * bright);
            verts.push(b * bright);
            verts.push(tex[vi][0]);
            verts.push(tex[vi][1]);
            verts.push(normal[0]);
            verts.push(normal[1]);
            verts.push(normal[2]);
        }
        verts
    }

    fn is_face_visible(blocks: &Blocks, x: usize, y: usize, z: usize, face: Face, edges: &NeighborEdges) -> bool {
        let (ix, iy, iz) = match face {
            Face::Right => (x as i32 + 1, y as i32, z as i32),
            Face::Left  => (x as i32 - 1, y as i32, z as i32),
            Face::Up    => (x as i32, y as i32 + 1, z as i32),
            Face::Down  => (x as i32, y as i32 - 1, z as i32),
            Face::Front => (x as i32, y as i32, z as i32 + 1),
            Face::Back  => (x as i32, y as i32, z as i32 - 1),
        };

        let current = blocks[x][y][z];

        // Hide fluid faces at unloaded chunk boundaries to prevent water-wall seams.
        // Solid block faces use Air default (normal behaviour — face is visible).
        if current.is_fluid() {
            let at_unloaded = (ix < 0 && !edges.left_loaded)
                || (ix >= 16 && !edges.right_loaded)
                || (iz < 0  && !edges.back_loaded)
                || (iz >= 16 && !edges.front_loaded);
            if at_unloaded { return false; }
        }

        let neighbor = if ix < 0 {
            edges.left[y][z]
        } else if ix >= 16 {
            edges.right[y][z]
        } else if iz < 0 {
            edges.back[y][x]
        } else if iz >= 16 {
            edges.front[y][x]
        } else if iy < 0 || iy >= 16 {
            BlockType::Air
        } else {
            blocks[ix as usize][iy as usize][iz as usize]
        };

        if current.is_fluid() && current == neighbor {
            return false;
        }

        !neighbor.is_opaque()
    }

    fn block_is_solid_local(blocks: &Blocks, x: i32, y: i32, z: i32) -> bool {
        if x < 0 || x >= 16 || y < 0 || y >= 16 || z < 0 || z >= 16 {
            return false;
        }
        blocks[x as usize][y as usize][z as usize].is_solid()
    }

    fn compute_ao(blocks: &Blocks, x: i32, y: i32, z: i32, face: Face) -> [f32; 4] {
        let neighbors = face.ao_neighbors();
        let mut ao = [1.0f32; 4];
        for (vi, [s1, s2, c]) in neighbors.iter().enumerate() {
            let side1  = Self::block_is_solid_local(blocks, x + s1.0, y + s1.1, z + s1.2);
            let side2  = Self::block_is_solid_local(blocks, x + s2.0, y + s2.1, z + s2.2);
            let corner = Self::block_is_solid_local(blocks, x + c.0,  y + c.1,  z + c.2);
            ao[vi] = if side1 && side2 {
                0.4
            } else {
                1.0 - (side1 as i32 + side2 as i32 + corner as i32) as f32 * 0.2
            };
        }
        ao
    }

    fn cross_vertices(x: f32, y: f32, z: f32, block: BlockType, height: f32) -> Vec<f32> {
        let [r, g, b] = block.color();

        const N: f32 = 16.0;
        let ts = 1.0 / N;
        let tile_u = block.texture_id(Face::Front) as f32;
        let u0 = tile_u * ts;
        let u1 = (tile_u + 1.0) * ts;
        // Map UVs so shorter grass shows only the bottom portion of the tile.
        let v1 = ts;
        let v0 = v1 - ts * height;

        let mut v: Vec<f32> = Vec::new();

        let mut quad = |p: [[f32; 3]; 4]| {
            let uvs = [[u0, v1], [u1, v1], [u1, v0], [u0, v0]];
            let (nx, ny, nz) = (0.0f32, 1.0, 0.0);
            for &i in &[0usize, 1, 2, 0, 2, 3] {
                v.extend_from_slice(&[p[i][0], p[i][1], p[i][2], r, g, b, uvs[i][0], uvs[i][1], nx, ny, nz]);
            }
            for &i in &[2usize, 1, 0, 3, 2, 0] {
                v.extend_from_slice(&[p[i][0], p[i][1], p[i][2], r, g, b, uvs[i][0], uvs[i][1], nx, ny, nz]);
            }
        };

        let h = height;
        quad([
            [x,       y,     z      ],
            [x + 1.0, y,     z + 1.0],
            [x + 1.0, y + h, z + 1.0],
            [x,       y + h, z      ],
        ]);
        quad([
            [x + 1.0, y,     z      ],
            [x,       y,     z + 1.0],
            [x,       y + h, z + 1.0],
            [x + 1.0, y + h, z      ],
        ]);

        v
    }

    fn face_vertices_real_(x: f32, y: f32, z: f32, face: Face, block_type: BlockType, ao: [f32; 4]) -> Vec<f32> {
        let mut vertices = Vec::new();
        let positions  = face.positions(x, y, z);
        let tex_coords = face.texture_coords(block_type.texture_id(face), 16);
        let base_color = block_type.color();
        let normal     = face.normal();

        let brightness = match face {
            Face::Up    => 1.0,
            Face::Down  => 0.5,
            Face::Front | Face::Back => 0.8,
            Face::Left  | Face::Right => 0.65,
        };

        // Vertex layout: [x, y, z,  r, g, b,  u, v,  nx, ny, nz] = 11 floats
        for &vertex_idx in &[0usize, 1, 2, 0, 2, 3] {
            let light = brightness * ao[vertex_idx];
            vertices.push(positions[vertex_idx][0]);
            vertices.push(positions[vertex_idx][1]);
            vertices.push(positions[vertex_idx][2]);
            vertices.push(base_color[0] * light);
            vertices.push(base_color[1] * light);
            vertices.push(base_color[2] * light);
            vertices.push(tex_coords[vertex_idx][0]);
            vertices.push(tex_coords[vertex_idx][1]);
            vertices.push(normal[0]);
            vertices.push(normal[1]);
            vertices.push(normal[2]);
        }

        vertices
    }

    pub fn is_in_frustum(&self, frustum: &Frustum) -> bool {
        let min = Vec3::new(
            self.position[0] as f32 * 16.0,
            self.position[1] as f32 * 16.0,
            self.position[2] as f32 * 16.0,
        );
        let max = Vec3::new(min.x + 16.0, min.y + 16.0, min.z + 16.0);
        frustum.intersects_aabb(min, max)
    }

}
