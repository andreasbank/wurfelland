use crate::renderer::ChunkMesh;
use crate::world::face::Face;
use crate::world::BlockType;
use crate::world::biome::Biome;
use crate::world::{SEA_LEVEL, WORLD_HEIGHT_CHUNKS};
use glam::Vec3;
use crate::camera::frustum::Frustum;
use noise::{NoiseFn, Perlin};

pub type Blocks = [[[BlockType; 16]; 16]; 16];

/// Per-block water levels for a chunk (0 = not water, 1–7 = flowing, 8 = source).
pub type WaterLevels = [[[u8; 16]; 16]; 16];

/// Per-block sky-light levels (0 = no sky access, 15 = full sky).
pub type SkyLight = [[[u8; 16]; 16]; 16];

/// Single-block-deep faces of all six adjacent chunks for border face culling.
/// Horizontal: `right[y][z]`, `left[y][z]`, `front[y][x]`, `back[y][x]`
/// Vertical:   `above[x][z]` = bottom face (ly=0) of the chunk above,
///             `below[x][z]` = top face (ly=15) of the chunk below.
pub struct NeighborEdges {
    pub right: [[BlockType; 16]; 16],
    pub left:  [[BlockType; 16]; 16],
    pub front: [[BlockType; 16]; 16],
    pub back:  [[BlockType; 16]; 16],
    pub above: [[BlockType; 16]; 16],
    pub below: [[BlockType; 16]; 16],
    pub wl_right: [[u8; 16]; 16],
    pub wl_left:  [[u8; 16]; 16],
    pub wl_front: [[u8; 16]; 16],
    pub wl_back:  [[u8; 16]; 16],
    pub right_loaded: bool,
    pub left_loaded:  bool,
    pub front_loaded: bool,
    pub back_loaded:  bool,
    pub above_loaded: bool,
    pub below_loaded: bool,
    /// Sky-light values at ly=0 of the chunk directly above (0 or 15).
    /// All-15 when no above chunk is loaded (open sky assumed).
    pub above_sky: [[u8; 16]; 16],
    /// Sky-light at lx=0 of the +X neighbor chunk, indexed [y][z].
    pub right_sky: [[u8; 16]; 16],
    /// Sky-light at lx=15 of the -X neighbor chunk, indexed [y][z].
    pub left_sky:  [[u8; 16]; 16],
    /// Sky-light at lz=0 of the +Z neighbor chunk, indexed [y][x].
    pub front_sky: [[u8; 16]; 16],
    /// Sky-light at lz=15 of the -Z neighbor chunk, indexed [y][x].
    pub back_sky:  [[u8; 16]; 16],
    /// Sky-light at ly=15 of the -Y neighbor chunk, indexed [x][z].
    pub below_sky: [[u8; 16]; 16],
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
    /// Sky-light per block: 15 = unobstructed sky above, 0 = underground.
    /// Computed once after generate(); used to seed the chunk below via above_sky.
    sky_light: Box<SkyLight>,
}

/// Column-scan sky-light: assumes open sky enters from above this chunk.
/// Each (x,z) column is lit (=15) from the top down until the first opaque
/// block; everything at or below that block is 0.
fn compute_sky_light(blocks: &Blocks) -> Box<SkyLight> {
    let mut sky = Box::new([[[0u8; 16]; 16]; 16]);
    for x in 0..16usize {
        for z in 0..16usize {
            let mut lit = true;
            for y in (0..16usize).rev() {
                if lit {
                    sky[x][y][z] = 15;
                    if blocks[x][y][z].is_opaque() {
                        lit = false;
                    }
                }
            }
        }
    }
    sky
}

impl Chunk {
    pub fn generate(position: [i32; 3], seed: u32) -> Self {
        let terrain = Perlin::new(seed);
        let temp    = Perlin::new(seed.wrapping_add(58));
        let moist   = Perlin::new(seed.wrapping_add(158));
        let cont    = Perlin::new(seed.wrapping_add(258));

        let mut blocks     = [[[BlockType::Air; 16]; 16]; 16];
        let mut surface    = [[0i32; 16]; 16]; // world-Y of surface block per XZ column
        let mut biome_grid = [[Biome::Plains; 16]; 16];

        let wy_base = position[1] * 16; // world Y at the bottom of this chunk

        // Max possible surf_y across all biomes — chunks above this are always air.
        // Mountains: base 66 + amplitude 52 = 118.  Add a small buffer → 130.
        const MAX_SURF_Y: i32 = 130;
        let model = glam::Mat4::from_translation(glam::Vec3::new(
            position[0] as f32 * 16.0,
            position[1] as f32 * 16.0,
            position[2] as f32 * 16.0,
        ));
        if wy_base > MAX_SURF_Y + 1 {
            // Entire chunk is above the highest possible terrain — leave all-air.
            let sky_light = compute_sky_light(&blocks);
            return Chunk { position, blocks, mesh: None, needs_rebuild: true, model, sky_light };
        }

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

                // Blend terrain height over a 5-point cross (±16 blocks) so biome
                // edges fade in gradually instead of creating instant cliffs.
                // For mountain samples the continentalness value biases the terrain
                // noise upward so the centre of a mountain biome (high continentalness)
                // is always the highest point — preventing inverted / hollow mountains.
                const BLEND_D: f64 = 16.0;
                let mut blended_surf = 0.0f32;
                for (ox, oz) in [(0.0f64,0.0f64),(BLEND_D,0.0),(-BLEND_D,0.0),(0.0,BLEND_D),(0.0,-BLEND_D)] {
                    let sx = wx + ox;
                    let sz = wz + oz;
                    let c_raw = cont.get([sx * 0.005, sz * 0.005]);
                    let b = Biome::from_noise(
                        temp .get([sx * 0.003, sz * 0.003]),
                        moist.get([sx * 0.003, sz * 0.003]),
                        c_raw,
                    );
                    let bp = b.params();
                    let terrain_nv = ((terrain.get([sx * bp.scale, sz * bp.scale]) + 1.0) / 2.0) as f32;
                    let nv = if b == Biome::Mountains {
                        // c_t = 0 at the biome edge (cont=0.80), 1 at the deepest centre (cont=1.0).
                        // Lerp terrain_nv toward 1 as c_t rises so the continentalness peak
                        // always sits at the top of the mountain.
                        let c_norm = ((c_raw + 1.0) / 2.0) as f32;
                        let c_t = ((c_norm - 0.80) / 0.20).clamp(0.0, 1.0);
                        (terrain_nv * (1.0 - c_t * 0.5) + c_t * 0.6).min(1.0)
                    } else {
                        terrain_nv
                    };
                    blended_surf += bp.base_height + nv * bp.amplitude;
                }
                let surf_y = (blended_surf / 5.0) as i32;
                let surf_y = surf_y.clamp(1, WORLD_HEIGHT_CHUNKS * 16 - 2);
                surface[x][z] = surf_y;

                let underwater = surf_y < SEA_LEVEL;

                for ly in 0..16usize {
                    let wy = wy_base + ly as i32; // world Y of this block
                    blocks[x][ly][z] = if wy == 0 {
                        BlockType::Stone // bedrock row
                    } else if wy < surf_y - 3 {
                        BlockType::Stone
                    } else if wy < surf_y {
                        p.sub_surface_block
                    } else if wy == surf_y {
                        if underwater { p.sub_surface_block } else { p.surface_block }
                    } else if underwater && wy <= SEA_LEVEL {
                        BlockType::Water
                    } else {
                        BlockType::Air
                    };
                }
            }
        }

        // ── Trees ────────────────────────────────────────────────────────────
        // Only place trees whose surface block falls within this chunk's Y range.
        for x in 0..16usize {
            for z in 0..16usize {
                let surf_wy = surface[x][z];
                let local_surf = surf_wy - wy_base;
                if local_surf < 0 || local_surf >= 16 { continue; }
                let local_surf = local_surf as usize;

                let world_x = position[0] * 16 + x as i32;
                let world_z = position[2] * 16 + z as i32;
                let p = biome_grid[x][z].params();
                if should_place_tree(world_x, world_z, p.tree_freq)
                    && surf_wy > SEA_LEVEL
                    && blocks[x][local_surf][z] == BlockType::Grass
                {
                    let sh = (world_x.wrapping_mul(1_723_459)
                              ^ world_z.wrapping_mul(9_876_543)) as u32;
                    match sh % 20 {
                        0..=9   => plant_tree_small (&mut blocks, x, local_surf, z),
                        10..=16 => plant_tree_medium(&mut blocks, x, local_surf, z),
                        _       => plant_tree_large (&mut blocks, x, local_surf, z),
                    }
                }
            }
        }

        // ── Grass (patch-based) ──────────────────────────────────────────────
        for x in 0..16usize {
            for z in 0..16usize {
                let p = biome_grid[x][z].params();
                if p.grass_freq == 0 { continue; }

                let surf_wy = surface[x][z];
                let local_surf = surf_wy - wy_base;
                if local_surf < 0 || local_surf >= 15 { continue; } // need room for above block
                let local_surf = local_surf as usize;
                let above = local_surf + 1;

                if blocks[x][local_surf][z] != BlockType::Grass { continue; }
                if blocks[x][above][z]      != BlockType::Air   { continue; }

                let world_x = position[0] * 16 + x as i32;
                let world_z = position[2] * 16 + z as i32;

                let px = world_x.div_euclid(6);
                let pz = world_z.div_euclid(6);
                let ph = (px.wrapping_mul(374_761_393_i32)
                          ^ pz.wrapping_mul(668_265_263_i32)) as u32;
                if ph % p.grass_freq != 0 { continue; }

                let ch = (world_x.wrapping_mul(1_234_567_i32)
                          ^ world_z.wrapping_mul(7_654_321_i32)) as u32;
                if ch % 10 >= 7 { continue; }

                match (ch / 10) % 5 {
                    0 | 1 => { blocks[x][above][z] = BlockType::GrassShort; }
                    2 | 3 => { blocks[x][above][z] = BlockType::TallGrass; }
                    _ => {
                        blocks[x][above][z] = BlockType::TallGrass;
                        if above + 1 < 16 && blocks[x][above + 1][z] == BlockType::Air {
                            blocks[x][above + 1][z] = BlockType::TallGrass;
                        }
                    }
                }
            }
        }

        // ── Cave carving ──────────────────────────────────────────────────────
        //
        // Three cave types matching Minecraft 1.18+:
        //
        //  Spaghetti  – two 3-D noise fields; carve where *both* are near zero.
        //               The intersection of two near-zero surfaces is a thin tube.
        //               Slightly higher Y frequency → passages run more horizontal.
        //
        //  Cheese     – one 3-D noise field; carve where the value is high.
        //               Creates large, irregular open caverns.  Only below Y=50.
        //
        //  Noodle     – tighter threshold on the same two spaghetti fields;
        //               produces occasional very narrow "needle" passages.
        //
        // Rules:
        //  • Y=0 is always bedrock — never carved.
        //  • Never carve within 5 blocks of the surface (prevents sky holes).
        //  • Non-solid blocks (Air, Water, vegetation) are skipped.
        let cave1  = Perlin::new(seed.wrapping_add(500));
        let cave2  = Perlin::new(seed.wrapping_add(501));
        let cheese = Perlin::new(seed.wrapping_add(502));

        for x in 0..16usize {
            for z in 0..16usize {
                let wx_f = (position[0] * 16 + x as i32) as f64;
                let wz_f = (position[2] * 16 + z as i32) as f64;
                let surf  = surface[x][z];

                for ly in 0..16usize {
                    let wy = wy_base + ly as i32;
                    // Preserve bedrock and a cap near the surface.
                    if wy <= 0 || wy >= surf - 5 { continue; }
                    // Only carve solid terrain — leave Air/Water/vegetation alone.
                    if !blocks[x][ly][z].is_solid() { continue; }

                    let wy_f = wy as f64;

                    // Spaghetti / noodle caves.
                    let s1 = cave1.get([wx_f * 0.020, wy_f * 0.025, wz_f * 0.020]);
                    let s2 = cave2.get([wx_f * 0.020, wy_f * 0.025, wz_f * 0.020]);
                    let sq = s1 * s1 + s2 * s2;

                    // Cheese caves — large voids, deeper only.
                    let ch = if wy < 50 {
                        cheese.get([wx_f * 0.008, wy_f * 0.010, wz_f * 0.008])
                    } else {
                        -1.0
                    };

                    // Slightly widen spaghetti tunnels deeper down (more cavernous).
                    let depth_bonus = ((50 - wy).max(0) as f64) * 0.000_15;

                    let is_cave = sq < 0.020 + depth_bonus   // spaghetti
                        || sq < 0.006                         // noodle (always)
                        || ch > 0.55;                         // cheese

                    if is_cave {
                        blocks[x][ly][z] = BlockType::Air;
                    }
                }
            }
        }

        let sky_light = compute_sky_light(&blocks);
        Chunk { position, blocks, mesh: None, needs_rebuild: true, model, sky_light }
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

    /// Sky-light edges — mirror the block edge_* methods so NeighborEdges can
    /// look up sky values across chunk boundaries in every direction.
    pub fn sky_light_bottom(&self) -> [[u8; 16]; 16] {   // ly=0  → above_sky for chunk below
        let mut e = [[0u8; 16]; 16];
        for x in 0..16 { for z in 0..16 { e[x][z] = self.sky_light[x][0][z]; } }
        e
    }
    pub fn sky_light_edge_top(&self) -> [[u8; 16]; 16] { // ly=15 → below_sky for chunk above
        let mut e = [[0u8; 16]; 16];
        for x in 0..16 { for z in 0..16 { e[x][z] = self.sky_light[x][15][z]; } }
        e
    }
    pub fn sky_light_edge_right(&self) -> [[u8; 16]; 16] { // lx=0  → for left neighbor
        let mut e = [[0u8; 16]; 16];
        for y in 0..16 { for z in 0..16 { e[y][z] = self.sky_light[0][y][z]; } }
        e
    }
    pub fn sky_light_edge_left(&self) -> [[u8; 16]; 16] {  // lx=15 → for right neighbor
        let mut e = [[0u8; 16]; 16];
        for y in 0..16 { for z in 0..16 { e[y][z] = self.sky_light[15][y][z]; } }
        e
    }
    pub fn sky_light_edge_front(&self) -> [[u8; 16]; 16] { // lz=0  → for back neighbor
        let mut e = [[0u8; 16]; 16];
        for y in 0..16 { for x in 0..16 { e[y][x] = self.sky_light[x][y][0]; } }
        e
    }
    pub fn sky_light_edge_back(&self) -> [[u8; 16]; 16] {  // lz=15 → for front neighbor
        let mut e = [[0u8; 16]; 16];
        for y in 0..16 { for x in 0..16 { e[y][x] = self.sky_light[x][y][15]; } }
        e
    }

    /// Bottom face (ly=0) — exposed to the chunk below.
    pub fn edge_bottom(&self) -> [[BlockType; 16]; 16] {
        let mut e = [[BlockType::Air; 16]; 16];
        for x in 0..16 { for z in 0..16 { e[x][z] = self.blocks[x][0][z]; } }
        e
    }

    /// Top face (ly=15) — exposed to the chunk above.
    pub fn edge_top(&self) -> [[BlockType; 16]; 16] {
        let mut e = [[BlockType::Air; 16]; 16];
        for x in 0..16 { for z in 0..16 { e[x][z] = self.blocks[x][15][z]; } }
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

    /// Replace stored sky-light with the corrected values from the last mesh build.
    pub fn update_sky_light(&mut self, new_sky: Box<SkyLight>) {
        self.sky_light = new_sky;
    }

    /// Returns true if any edge face of `new_sky` differs from the current stored sky_light.
    /// Only edges matter because that is what neighbour chunks read.
    pub fn sky_edges_changed(&self, new_sky: &SkyLight) -> bool {
        for y in 0..16usize {
            for z in 0..16usize {
                if self.sky_light[0][y][z]  != new_sky[0][y][z]  { return true; }
                if self.sky_light[15][y][z] != new_sky[15][y][z] { return true; }
            }
            for x in 0..16usize {
                if self.sky_light[x][y][0]  != new_sky[x][y][0]  { return true; }
                if self.sky_light[x][y][15] != new_sky[x][y][15] { return true; }
            }
        }
        for x in 0..16usize {
            for z in 0..16usize {
                if self.sky_light[x][0][z]  != new_sky[x][0][z]  { return true; }
                if self.sky_light[x][15][z] != new_sky[x][15][z] { return true; }
            }
        }
        false
    }

    pub fn finalize_mesh(&mut self, vertices: Vec<f32>) {
        self.mesh = Some(ChunkMesh::from_vertices(&vertices));
        // Do not clear needs_rebuild here. mark_mesh_dispatched already cleared it at
        // dispatch time. If mark_for_rebuild was called while the mesh thread was in
        // flight (e.g. a neighbor arrived), that pending rebuild must be preserved.
    }

    /// Sky-light (0.0–1.0) of the block adjacent to face `f` of block (x,y,z).
    /// Uses the full set of neighbor sky edges for all 6 boundaries.
    fn neighbor_sky(sky: &SkyLight, edges: &NeighborEdges,
                    x: i32, y: i32, z: i32, f: Face) -> f32 {
        let (nx, ny, nz) = match f {
            Face::Up    => (x,     y + 1, z    ),
            Face::Down  => (x,     y - 1, z    ),
            Face::Right => (x + 1, y,     z    ),
            Face::Left  => (x - 1, y,     z    ),
            Face::Front => (x,     y,     z + 1),
            Face::Back  => (x,     y,     z - 1),
        };
        // Each face changes exactly one coordinate, so only one branch fires.
        if nx <  0  { return edges.left_sky [ny as usize][nz as usize] as f32 / 15.0; }
        if nx >= 16 { return edges.right_sky[ny as usize][nz as usize] as f32 / 15.0; }
        if nz <  0  { return edges.back_sky [ny as usize][nx as usize] as f32 / 15.0; }
        if nz >= 16 { return edges.front_sky[ny as usize][nx as usize] as f32 / 15.0; }
        if ny <  0  { return edges.below_sky[nx as usize][nz as usize] as f32 / 15.0; }
        if ny >= 16 { return edges.above_sky[nx as usize][nz as usize] as f32 / 15.0; }
        sky[nx as usize][ny as usize][nz as usize] as f32 / 15.0
    }

    /// Build vertex data off the main thread.
    /// Returns vertices and the corrected sky-light array (seeded from above_sky) so the
    /// chunk can update its stored sky_light and neighbours can read correct edge values.
    pub fn build_vertices(blocks: &Blocks, edges: &NeighborEdges, water_levels: &WaterLevels) -> (Vec<f32>, Box<SkyLight>) {
        // Rough estimate: ~10% of blocks are surface blocks with ~3 visible faces on average.
        // 16³ * 0.10 * 3 faces * 6 verts * 12 floats ≈ 8748. Sized to avoid early reallocs.
        let mut vertices = Vec::with_capacity(9216);

        // Sky-light flood fill: seed from all six neighbour edges, then BFS through
        // non-opaque blocks. This correctly lights any cavity connected to open sky
        // via a sideways opening, not just vertically exposed columns.
        let mut sky = [[[0u8; 16]; 16]; 16];
        let mut queue: std::collections::VecDeque<(usize, usize, usize)> = std::collections::VecDeque::new();

        macro_rules! seed {
            ($x:expr, $y:expr, $z:expr) => {
                if !blocks[$x][$y][$z].is_opaque() && sky[$x][$y][$z] == 0 {
                    sky[$x][$y][$z] = 15;
                    queue.push_back(($x, $y, $z));
                }
            };
        }

        for x in 0..16usize {
            for z in 0..16usize {
                if edges.above_sky[x][z] > 0 { seed!(x, 15, z); }
                if edges.below_sky[x][z] > 0 { seed!(x,  0, z); }
            }
        }
        for y in 0..16usize {
            for z in 0..16usize {
                if edges.left_sky [y][z] > 0 { seed!( 0, y, z); }
                if edges.right_sky[y][z] > 0 { seed!(15, y, z); }
            }
            for x in 0..16usize {
                if edges.back_sky [y][x] > 0 { seed!(x, y,  0); }
                if edges.front_sky[y][x] > 0 { seed!(x, y, 15); }
            }
        }

        while let Some((x, y, z)) = queue.pop_front() {
            macro_rules! spread {
                ($nx:expr, $ny:expr, $nz:expr) => {
                    let (nx, ny, nz) = ($nx as usize, $ny as usize, $nz as usize);
                    if !blocks[nx][ny][nz].is_opaque() && sky[nx][ny][nz] == 0 {
                        sky[nx][ny][nz] = 15;
                        queue.push_back((nx, ny, nz));
                    }
                };
            }
            if x > 0  { spread!(x - 1, y, z); }
            if x < 15 { spread!(x + 1, y, z); }
            if y > 0  { spread!(x, y - 1, z); }
            if y < 15 { spread!(x, y + 1, z); }
            if z > 0  { spread!(x, y, z - 1); }
            if z < 15 { spread!(x, y, z + 1); }
        }

        for x in 0..16usize {
            for y in 0..16usize {
                for z in 0..16usize {
                    let block = blocks[x][y][z];
                    // Block's own sky_light — used for vegetation/water that are always outdoors.
                    let own_sl = sky[x][y][z] as f32 / 15.0;

                    if block == BlockType::Air {
                        continue;
                    }

                    if block == BlockType::TallGrass {
                        vertices.extend(Self::cross_vertices(x as f32, y as f32, z as f32, block, 1.0, own_sl));
                        continue;
                    }
                    if block == BlockType::GrassShort {
                        vertices.extend(Self::cross_vertices(x as f32, y as f32, z as f32, block, 0.45, own_sl));
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
                                x as f32, y as f32, z as f32, face, block, corners, own_sl,
                            ));
                        }
                        continue;
                    }

                    for face in [Face::Right, Face::Left, Face::Up, Face::Down, Face::Front, Face::Back] {
                        if Self::is_face_visible(blocks, x, y, z, face, edges) {
                            // Use sky_light of the adjacent open block, not this block's own value.
                            // A cliff side-face looks into outdoor air (sky=15) and must be lit.
                            let sl = Self::neighbor_sky(&sky, edges, x as i32, y as i32, z as i32, face);
                            let ao = Self::compute_ao(blocks, x as i32, y as i32, z as i32, face);
                            vertices.extend(Self::face_vertices_real_(
                                x as f32, y as f32, z as f32, face, block, ao, sl,
                            ));
                        }
                    }
                }
            }
        }

        (vertices, Box::new(sky))
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
                           block_type: BlockType, corners: [f32; 4], sky_light: f32) -> Vec<f32> {
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
            verts.push(sky_light);
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
            let at_unloaded = (ix < 0  && !edges.left_loaded)
                || (ix >= 16 && !edges.right_loaded)
                || (iz < 0  && !edges.back_loaded)
                || (iz >= 16 && !edges.front_loaded)
                || (iy < 0  && !edges.below_loaded)
                || (iy >= 16 && !edges.above_loaded);
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
        } else if iy < 0 {
            edges.below[x][z]
        } else if iy >= 16 {
            edges.above[x][z]
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

    fn cross_vertices(x: f32, y: f32, z: f32, block: BlockType, height: f32, sky_light: f32) -> Vec<f32> {
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
                v.extend_from_slice(&[p[i][0], p[i][1], p[i][2], r, g, b, uvs[i][0], uvs[i][1], nx, ny, nz, sky_light]);
            }
            for &i in &[2usize, 1, 0, 3, 2, 0] {
                v.extend_from_slice(&[p[i][0], p[i][1], p[i][2], r, g, b, uvs[i][0], uvs[i][1], nx, ny, nz, sky_light]);
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

    fn face_vertices_real_(x: f32, y: f32, z: f32, face: Face, block_type: BlockType, ao: [f32; 4], sky_light: f32) -> Vec<f32> {
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

        // Vertex layout: [x, y, z,  r, g, b,  u, v,  nx, ny, nz,  sky_light] = 12 floats
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
            vertices.push(sky_light);
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
