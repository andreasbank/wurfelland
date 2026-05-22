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

/// Minecraft-style cell-based tree placement.
/// The world is divided into CELL×CELL blocks; each cell holds at most one tree
/// at a random position. `freq` = 1-in-N cells has a tree.
/// This eliminates the diagonal-stripe artifact of per-column linear hashes.
fn should_place_tree(world_x: i32, world_z: i32, freq: u32) -> bool {
    if freq == 0 { return false; }
    const CELL: i32 = 8;
    let cell_x = world_x.div_euclid(CELL);
    let cell_z = world_z.div_euclid(CELL);

    // Well-mixed hash of the cell coordinates.
    let mut h = (cell_x as u32).wrapping_mul(1_664_525)
        .wrapping_add((cell_z as u32).wrapping_mul(1_013_904_223));
    h ^= h >> 16;
    h = h.wrapping_mul(0x45d9f3b);
    h ^= h >> 16;

    if h % freq != 0 { return false; }

    // Pick a deterministic random offset within the cell.
    let h2 = h.wrapping_mul(22_695_477).wrapping_add(1_013_904_223);
    let tx = (h2 % CELL as u32) as i32;
    let h3 = h2.wrapping_mul(22_695_477).wrapping_add(1_013_904_223);
    let tz = (h3 % CELL as u32) as i32;

    world_x.rem_euclid(CELL) == tx && world_z.rem_euclid(CELL) == tz
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
    /// True when sky light needs to be (re)computed before the next mesh build.
    sky_dirty: bool,
    /// True once sky light has been computed at least once; mesh dispatch waits for this.
    sky_ready: bool,
    // Precomputed translation matrix — the chunk never moves, so compute once.
    model: glam::Mat4,
    /// Sky-light per block: 15 = unobstructed sky above, 0 = underground.
    sky_light: Box<SkyLight>,
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
        // Mountains: base 130 + amplitude 52 = 182.  Add a small buffer → 195.
        const MAX_SURF_Y: i32 = 195;
        let model = glam::Mat4::from_translation(glam::Vec3::new(
            position[0] as f32 * 16.0,
            position[1] as f32 * 16.0,
            position[2] as f32 * 16.0,
        ));
        if wy_base > MAX_SURF_Y + 1 {
            // Entire chunk is above the highest possible terrain — leave all-air.
            let sky_light = Box::new([[[0u8; 16]; 16]; 16]);
            return Chunk { position, blocks, mesh: None, needs_rebuild: true,
                           sky_dirty: true, sky_ready: false, model, sky_light };
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
                        if underwater { p.sub_surface_block }
                        else if wy >= SEA_LEVEL + 33 { BlockType::Snow }
                        else { p.surface_block }
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

        // ── Wild wheat patches ───────────────────────────────────────────────
        // Two candidate patch centres per chunk. Each has a ~1-in-80 chance of
        // actually spawning. When it does, 4–8 stalks are scattered in a radius-2
        // area around the centre, each landing only on exposed Grass surfaces.
        for attempt in 0u32..2 {
            let cx = position[0] as u32;
            let cz = position[2] as u32;
            let h0 = cx.wrapping_mul(1_234_567).wrapping_add(cz.wrapping_mul(7_654_321))
                        .wrapping_add(attempt.wrapping_mul(999_983));
            if h0 % 80 != 0 { continue; }

            // Pick a centre within the chunk (avoid the very edge so the cluster fits)
            let cx_local = ((h0 >> 8)  % 12 + 2) as usize;
            let cz_local = ((h0 >> 16) % 12 + 2) as usize;

            // Scatter 4–8 plants in a ±2 block radius
            let plant_count = 4 + (h0 >> 24) % 5; // 4..=8
            for i in 0..plant_count {
                let ph = h0.wrapping_mul(31).wrapping_add(i.wrapping_mul(6_700_417));
                let dx = ((ph        & 0xF) % 5) as i32 - 2; // -2..=2
                let dz = ((ph >> 4)  & 0xF) as i32 % 5 - 2;

                let lx = (cx_local as i32 + dx).clamp(0, 15) as usize;
                let lz = (cz_local as i32 + dz).clamp(0, 15) as usize;

                let surf_wy   = surface[lx][lz];
                let local_surf = surf_wy - wy_base;
                if local_surf < 0 || local_surf >= 15 { continue; }
                let local_surf = local_surf as usize;
                let above      = local_surf + 1;

                if blocks[lx][local_surf][lz] != BlockType::Grass { continue; }
                if blocks[lx][above][lz]      != BlockType::Air   { continue; }

                // Random starting stage 0–2 so they're not all identical
                let stage = ((ph >> 8) % 3) as u8;
                blocks[lx][above][lz] = BlockType::Wheat(stage);
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
                    let ch = if wy < 85 {
                        cheese.get([wx_f * 0.008, wy_f * 0.010, wz_f * 0.008])
                    } else {
                        -1.0
                    };

                    // Slightly widen spaghetti tunnels deeper down (more cavernous).
                    let depth_bonus = ((80 - wy).max(0) as f64) * 0.000_15;

                    let is_cave = sq < 0.020 + depth_bonus   // spaghetti
                        || sq < 0.006                         // noodle (always)
                        || ch > 0.55;                         // cheese

                    if is_cave {
                        blocks[x][ly][z] = BlockType::Air;
                    }
                }
            }
        }

        // ── Lava fill (Bedrock-style) ─────────────────────────────────────────
        // Any air pocket carved at Y=1..9 (above bedrock row, below Y=10) becomes lava.
        // This naturally fills cave floors near the bottom of the world, matching
        // how Minecraft Bedrock generates underground lava lakes.
        for x in 0..16usize {
            for ly in 0..16usize {
                let wy = wy_base + ly as i32;
                if wy < 1 || wy >= 10 { continue; }
                for z in 0..16usize {
                    if blocks[x][ly][z] == BlockType::Air {
                        blocks[x][ly][z] = BlockType::Lava;
                    }
                }
            }
        }

        // ── Copper ore veins ─────────────────────────────────────────────────
        // Generates Y=0..112 with a triangular peak at Y=48, matching Minecraft.
        // ~6 vein attempts per chunk column; each vein walks 4–7 blocks.
        let ore_seed = seed.wrapping_mul(2_654_435_761).wrapping_add(
            (position[0].wrapping_mul(1_234_567) ^ position[1].wrapping_mul(89) ^ position[2].wrapping_mul(7_654_321)) as u32
        );
        let mut rng = ore_seed;
        let next = |r: &mut u32| -> u32 { *r ^= *r << 13; *r ^= *r >> 17; *r ^= *r << 5; *r };

        for _ in 0..6 {
            let rx    = (next(&mut rng) % 16) as i32;
            let rz    = (next(&mut rng) % 16) as i32;
            // Triangular distribution: pick max(r1,r2) biased toward Y=48
            let r1    = next(&mut rng) % 112;
            let r2    = next(&mut rng) % 112;
            let start_wy = (r1.min(r2) + 24) as i32; // shift peak toward Y=48
            let vein_len = 4 + (next(&mut rng) % 4) as i32;

            for step in 0..vein_len {
                let wy = start_wy + step;
                if wy < 1 || wy > 112 { continue; }
                let ly = wy - wy_base;
                if ly < 0 || ly >= 16 { continue; }
                let lx = rx as usize;
                let lz = rz as usize;
                if blocks[lx][ly as usize][lz] == BlockType::Stone {
                    blocks[lx][ly as usize][lz] = BlockType::CopperOre;
                }
                // Spread one block in a random direction for blob shape
                let spread_x = ((next(&mut rng) % 3) as i32 - 1 + rx).clamp(0, 15) as usize;
                let spread_z = ((next(&mut rng) % 3) as i32 - 1 + rz).clamp(0, 15) as usize;
                let sly = ly as usize;
                if blocks[spread_x][sly][spread_z] == BlockType::Stone {
                    blocks[spread_x][sly][spread_z] = BlockType::CopperOre;
                }
            }
        }

        // ── Coal ore veins ───────────────────────────────────────────────────
        // Generates Y=0..160, peak at Y=96. More common than copper (~10 attempts).
        // Veins 5–9 blocks, appears at and above sea level in cliff faces too.
        let coal_seed = seed.wrapping_mul(1_442_695_037).wrapping_add(
            (position[0].wrapping_mul(9_999_991) ^ position[1].wrapping_mul(179) ^ position[2].wrapping_mul(6_700_417)) as u32
        );
        let mut crng = coal_seed;

        for _ in 0..10 {
            let rx       = (next(&mut crng) % 16) as i32;
            let rz       = (next(&mut crng) % 16) as i32;
            // Triangular peak at Y=96: bias two uniform samples toward the middle
            let r1       = next(&mut crng) % 160;
            let r2       = next(&mut crng) % 160;
            let start_wy = ((r1 + r2) / 2 + 16) as i32; // average shifts peak to ~96
            let vein_len = 5 + (next(&mut crng) % 5) as i32;

            for step in 0..vein_len {
                let wy = start_wy + step;
                if wy < 1 || wy > 160 { continue; }
                let ly = wy - wy_base;
                if ly < 0 || ly >= 16 { continue; }
                let lx = rx as usize;
                let lz = rz as usize;
                if blocks[lx][ly as usize][lz] == BlockType::Stone {
                    blocks[lx][ly as usize][lz] = BlockType::CoalOre;
                }
                let spread_x = ((next(&mut crng) % 3) as i32 - 1 + rx).clamp(0, 15) as usize;
                let spread_z = ((next(&mut crng) % 3) as i32 - 1 + rz).clamp(0, 15) as usize;
                let sly = ly as usize;
                if blocks[spread_x][sly][spread_z] == BlockType::Stone {
                    blocks[spread_x][sly][spread_z] = BlockType::CoalOre;
                }
            }
        }

        // ── Iron ore veins ───────────────────────────────────────────────────
        // Two distributions matching Minecraft 1.18+:
        //   Main  — triangular peak at Y=16, range Y=0..80,  ~9 attempts, veins 4–8 blocks.
        //   Upper — triangular peak at Y=128, range Y=64..192, ~3 attempts, veins 3–5 blocks.
        let iron_seed = seed.wrapping_mul(2_891_336_453).wrapping_add(
            (position[0].wrapping_mul(1_574_083) ^ position[1].wrapping_mul(311) ^ position[2].wrapping_mul(5_771_977)) as u32
        );
        let mut irng = iron_seed;

        // Main distribution: peak Y=16, range 0..80, 9 attempts.
        for _ in 0..9 {
            let rx       = (next(&mut irng) % 16) as i32;
            let rz       = (next(&mut irng) % 16) as i32;
            let r1       = next(&mut irng) % 80;
            let r2       = next(&mut irng) % 80;
            let start_wy = r1.min(r2) as i32; // triangular: peak at 0, bias toward low Y
            let vein_len = 4 + (next(&mut irng) % 5) as i32;

            for step in 0..vein_len {
                let wy = start_wy + step;
                if wy < 1 || wy > 80 { continue; }
                let ly = wy - wy_base;
                if ly < 0 || ly >= 16 { continue; }
                let lx = rx as usize;
                let lz = rz as usize;
                if blocks[lx][ly as usize][lz] == BlockType::Stone {
                    blocks[lx][ly as usize][lz] = BlockType::IronOre;
                }
                let spread_x = ((next(&mut irng) % 3) as i32 - 1 + rx).clamp(0, 15) as usize;
                let spread_z = ((next(&mut irng) % 3) as i32 - 1 + rz).clamp(0, 15) as usize;
                if blocks[spread_x][ly as usize][spread_z] == BlockType::Stone {
                    blocks[spread_x][ly as usize][spread_z] = BlockType::IronOre;
                }
            }
        }

        // Upper distribution: peak Y=128, range 64..192, 3 attempts.
        for _ in 0..3 {
            let rx       = (next(&mut irng) % 16) as i32;
            let rz       = (next(&mut irng) % 16) as i32;
            let r1       = next(&mut irng) % 128 + 64;
            let r2       = next(&mut irng) % 128 + 64;
            let start_wy = ((r1 + r2) / 2) as i32; // average of two → triangular peak at 128
            let vein_len = 3 + (next(&mut irng) % 3) as i32;

            for step in 0..vein_len {
                let wy = start_wy + step;
                if wy < 64 || wy > 192 { continue; }
                let ly = wy - wy_base;
                if ly < 0 || ly >= 16 { continue; }
                let lx = rx as usize;
                let lz = rz as usize;
                if blocks[lx][ly as usize][lz] == BlockType::Stone {
                    blocks[lx][ly as usize][lz] = BlockType::IronOre;
                }
                let spread_x = ((next(&mut irng) % 3) as i32 - 1 + rx).clamp(0, 15) as usize;
                let spread_z = ((next(&mut irng) % 3) as i32 - 1 + rz).clamp(0, 15) as usize;
                if blocks[spread_x][ly as usize][spread_z] == BlockType::Stone {
                    blocks[spread_x][ly as usize][spread_z] = BlockType::IronOre;
                }
            }
        }

        let sky_light = Box::new([[[0u8; 16]; 16]; 16]);
        Chunk { position, blocks, mesh: None, needs_rebuild: true,
                sky_dirty: true, sky_ready: false, model, sky_light }
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
        self.needs_rebuild && self.sky_ready
    }

    pub fn mark_for_rebuild(&mut self) {
        self.needs_rebuild = true;
    }

    /// Called just before dispatching a mesh thread so we don't re-dispatch next frame.
    pub fn mark_mesh_dispatched(&mut self) {
        self.needs_rebuild = false;
    }

    pub fn needs_sky_rebuild(&self) -> bool { self.sky_dirty }
    #[allow(dead_code)]
    pub fn is_sky_ready(&self)      -> bool { self.sky_ready }

    pub fn mark_sky_dirty(&mut self) { self.sky_dirty = true; }

    /// Called just before dispatching a sky thread.
    pub fn mark_sky_dispatched(&mut self) { self.sky_dirty = false; }

    /// Returns a copy of the stored sky-light for passing to a mesh thread.
    pub fn sky_snapshot(&self) -> Box<SkyLight> { Box::new(*self.sky_light) }

    /// Store newly computed sky-light. Marks sky as ready so meshing can proceed.
    pub fn update_sky_light(&mut self, new_sky: Box<SkyLight>) {
        self.sky_light = new_sky;
        self.sky_ready = true;
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
        // For cross-chunk faces: if the neighbour block is lava, return its emission
        // level directly — this avoids a one-frame dark flash before the cascade
        // propagates the updated sky edges back to this chunk's mesh.
        if nx <  0  {
            if edges.left [ny as usize][nz as usize] == BlockType::Lava { return 14.0 / 15.0; }
            return edges.left_sky [ny as usize][nz as usize] as f32 / 15.0;
        }
        if nx >= 16 {
            if edges.right[ny as usize][nz as usize] == BlockType::Lava { return 14.0 / 15.0; }
            return edges.right_sky[ny as usize][nz as usize] as f32 / 15.0;
        }
        if nz <  0  {
            if edges.back [ny as usize][nx as usize] == BlockType::Lava { return 14.0 / 15.0; }
            return edges.back_sky [ny as usize][nx as usize] as f32 / 15.0;
        }
        if nz >= 16 {
            if edges.front[ny as usize][nx as usize] == BlockType::Lava { return 14.0 / 15.0; }
            return edges.front_sky[ny as usize][nx as usize] as f32 / 15.0;
        }
        if ny <  0  {
            if edges.below[nx as usize][nz as usize] == BlockType::Lava { return 14.0 / 15.0; }
            return edges.below_sky[nx as usize][nz as usize] as f32 / 15.0;
        }
        if ny >= 16 {
            if edges.above[nx as usize][nz as usize] == BlockType::Lava { return 14.0 / 15.0; }
            return edges.above_sky[nx as usize][nz as usize] as f32 / 15.0;
        }
        sky[nx as usize][ny as usize][nz as usize] as f32 / 15.0
    }

    /// Build vertex data off the main thread.
    /// Returns vertices and the corrected sky-light array (seeded from above_sky) so the
    /// chunk can update its stored sky_light and neighbours can read correct edge values.
    /// Compute sky-light for this chunk from block data and neighbour sky edges.
    /// Run on a background thread; result is stored via `update_sky_light`.
    pub fn build_sky(blocks: &Blocks, edges: &NeighborEdges) -> Box<SkyLight> {
        use std::collections::VecDeque;

        // Vertical scan: direct sky columns only (above_sky == 15).
        let mut sky = [[[0u8; 16]; 16]; 16];
        for x in 0..16usize {
            for z in 0..16usize {
                if edges.above_sky[x][z] != 15 { continue; }
                for y in (0..16usize).rev() {
                    if blocks[x][y][z].is_opaque() { break; }
                    sky[x][y][z] = 15;
                }
            }
        }

        // BFS: spread from sky columns + neighbour edges, diminishing by 1 per hop.
        let mut queue: VecDeque<(i32, i32, i32)> = VecDeque::new();
        for x in 0..16usize {
            for y in 0..16usize {
                for z in 0..16usize {
                    if sky[x][y][z] > 0 { queue.push_back((x as i32, y as i32, z as i32)); }
                }
            }
        }
        let seed = |sky: &mut [[[u8;16];16];16], x: usize, y: usize, z: usize, val: u8,
                        queue: &mut VecDeque<(i32,i32,i32)>| {
            if val > 0 && !blocks[x][y][z].is_opaque() && sky[x][y][z] < val {
                sky[x][y][z] = val;
                queue.push_back((x as i32, y as i32, z as i32));
            }
        };
        for y in 0..16usize {
            for z in 0..16usize {
                seed(&mut sky, 0,  y, z, edges.left_sky [y][z].saturating_sub(1), &mut queue);
                seed(&mut sky, 15, y, z, edges.right_sky[y][z].saturating_sub(1), &mut queue);
            }
            for x in 0..16usize {
                seed(&mut sky, x, y, 0,  edges.back_sky [y][x].saturating_sub(1), &mut queue);
                seed(&mut sky, x, y, 15, edges.front_sky[y][x].saturating_sub(1), &mut queue);
            }
        }
        for x in 0..16usize {
            for z in 0..16usize {
                let a = edges.above_sky[x][z];
                if a > 0 && a < 15 { seed(&mut sky, x, 15, z, a.saturating_sub(1), &mut queue); }
                // Propagate light upward from the chunk below (e.g. lava glow rising).
                let b = edges.below_sky[x][z];
                if b > 0 { seed(&mut sky, x, 0, z, b.saturating_sub(1), &mut queue); }
            }
        }
        // Block-light emitters: lava at level 15.
        // Lava is opaque so we seed its 6 non-opaque neighbours at 14 directly.
        // Within-chunk lava:
        for x in 0..16usize {
            for y in 0..16usize {
                for z in 0..16usize {
                    if blocks[x][y][z] != BlockType::Lava { continue; }
                    for (dx, dy, dz) in [(-1i32,0,0),(1,0,0),(0,-1,0),(0,1,0),(0,0,-1i32),(0,0,1)] {
                        let (nx, ny, nz) = (x as i32+dx, y as i32+dy, z as i32+dz);
                        if nx < 0 || nx >= 16 || ny < 0 || ny >= 16 || nz < 0 || nz >= 16 { continue; }
                        seed(&mut sky, nx as usize, ny as usize, nz as usize, 14, &mut queue);
                    }
                }
            }
        }
        // Lava in neighbouring chunks: seed our border blocks at 14.
        for y in 0..16usize {
            for z in 0..16usize {
                if edges.left [y][z] == BlockType::Lava { seed(&mut sky,  0, y, z, 14, &mut queue); }
                if edges.right[y][z] == BlockType::Lava { seed(&mut sky, 15, y, z, 14, &mut queue); }
            }
            for x in 0..16usize {
                if edges.back [y][x] == BlockType::Lava { seed(&mut sky, x, y,  0, 14, &mut queue); }
                if edges.front[y][x] == BlockType::Lava { seed(&mut sky, x, y, 15, 14, &mut queue); }
            }
        }
        for x in 0..16usize {
            for z in 0..16usize {
                if edges.above[x][z] == BlockType::Lava { seed(&mut sky, x, 15, z, 14, &mut queue); }
                if edges.below[x][z] == BlockType::Lava { seed(&mut sky, x,  0, z, 14, &mut queue); }
            }
        }

        while let Some((x, y, z)) = queue.pop_front() {
            let cur = sky[x as usize][y as usize][z as usize];
            if cur == 0 { continue; }
            let spread = cur - 1;
            for (dx, dy, dz) in [(-1i32,0,0),(1,0,0),(0,-1,0),(0,1,0),(0,0,-1i32),(0,0,1)] {
                let (nx, ny, nz) = (x+dx, y+dy, z+dz);
                if nx < 0 || nx >= 16 || ny < 0 || ny >= 16 || nz < 0 || nz >= 16 { continue; }
                let (nxu, nyu, nzu) = (nx as usize, ny as usize, nz as usize);
                if blocks[nxu][nyu][nzu].is_opaque() { continue; }
                if sky[nxu][nyu][nzu] < spread {
                    sky[nxu][nyu][nzu] = spread;
                    queue.push_back((nx, ny, nz));
                }
            }
        }
        // Lava blocks are opaque so the BFS never writes a sky value into them,
        // but adjacent block faces read their neighbor's sky_light to shade themselves.
        // Assign lava its emission level so every bordering face sees a non-zero value,
        // including faces in neighboring chunks reading this chunk's edge via left_sky etc.
        for x in 0..16usize {
            for y in 0..16usize {
                for z in 0..16usize {
                    if blocks[x][y][z] == BlockType::Lava {
                        sky[x][y][z] = 14;
                    }
                }
            }
        }
        Box::new(sky)
    }

    /// True for blocks handled by the greedy solid pass (excludes water / vegetation).
    #[inline(always)]
    fn is_greedy_eligible(block: BlockType) -> bool {
        block.is_solid() && !block.is_fluid()
            && block != BlockType::TallGrass
            && block != BlockType::GrassShort
            && !matches!(block, BlockType::Wheat(_))
    }

    /// Emit a merged quad (6 vertices × 14 floats) into `out`.
    /// `pos[4]` and `uv[4]` describe corners v0..v3. AO-diagonal flip applied.
    fn emit_greedy_quad(
        out:       &mut Vec<f32>,
        pos:       [[f32; 3]; 4],
        uv:        [[f32; 2]; 4],
        tile_base: [f32; 2],
        normal:    [f32; 3],
        ao:        [f32; 4],
        sky:       f32,
        color:     [f32; 3],
        brightness: f32,
    ) {
        // Use the diagonal whose two triangles each include a brighter corner.
        let indices: [usize; 6] = if ao[0] + ao[2] < ao[1] + ao[3] {
            [1, 2, 3, 1, 3, 0]
        } else {
            [0, 1, 2, 0, 2, 3]
        };
        for &vi in &indices {
            let light = brightness * ao[vi];
            out.push(pos[vi][0]);
            out.push(pos[vi][1]);
            out.push(pos[vi][2]);
            out.push(color[0] * light);
            out.push(color[1] * light);
            out.push(color[2] * light);
            out.push(uv[vi][0]);
            out.push(uv[vi][1]);
            out.push(normal[0]);
            out.push(normal[1]);
            out.push(normal[2]);
            out.push(sky);
            out.push(tile_base[0]);
            out.push(tile_base[1]);
        }
    }

    /// Build geometry for this chunk. Sky-light must already be computed and
    /// stored; pass a snapshot via `sky_snapshot()` before dispatching the thread.
    pub fn build_vertices(blocks: &Blocks, edges: &NeighborEdges, sky: &SkyLight, water_levels: &WaterLevels) -> Vec<f32> {
        let mut vertices = Vec::with_capacity(9216);

        // ── Pass 1: water and vegetation (no greedy merging) ─────────────────
        for x in 0..16usize {
            for y in 0..16usize {
                for z in 0..16usize {
                    let block = blocks[x][y][z];
                    let own_sl = sky[x][y][z] as f32 / 15.0;
                    match block {
                        BlockType::Air => {}
                        BlockType::TallGrass => {
                            vertices.extend(Self::cross_vertices(x as f32, y as f32, z as f32, block, 1.0, own_sl));
                        }
                        BlockType::GrassShort => {
                            vertices.extend(Self::cross_vertices(x as f32, y as f32, z as f32, block, 0.45, own_sl));
                        }
                        BlockType::Wheat(stage) => {
                            let h = BlockType::wheat_height(stage);
                            vertices.extend(Self::cross_vertices(x as f32, y as f32, z as f32, block, h, own_sl));
                        }
                        BlockType::Water => {
                            let (lxi, lyi, lzi) = (x as i32, y as i32, z as i32);
                            for face in [Face::Right, Face::Left, Face::Up, Face::Down, Face::Front, Face::Back] {
                                if !Self::is_face_visible(blocks, x, y, z, face, edges) { continue; }
                                let corners: [f32; 4] = match face {
                                    Face::Up => [
                                        Self::water_corner_h(lxi,   lyi, lzi+1, blocks, water_levels, edges),
                                        Self::water_corner_h(lxi+1, lyi, lzi+1, blocks, water_levels, edges),
                                        Self::water_corner_h(lxi+1, lyi, lzi,   blocks, water_levels, edges),
                                        Self::water_corner_h(lxi,   lyi, lzi,   blocks, water_levels, edges),
                                    ],
                                    Face::Right => [0.0, Self::water_corner_h(lxi+1, lyi, lzi,   blocks, water_levels, edges),
                                                       Self::water_corner_h(lxi+1, lyi, lzi+1, blocks, water_levels, edges), 0.0],
                                    Face::Left  => [0.0, Self::water_corner_h(lxi,   lyi, lzi+1, blocks, water_levels, edges),
                                                       Self::water_corner_h(lxi,   lyi, lzi,   blocks, water_levels, edges), 0.0],
                                    Face::Front => [0.0, Self::water_corner_h(lxi+1, lyi, lzi+1, blocks, water_levels, edges),
                                                       Self::water_corner_h(lxi,   lyi, lzi+1, blocks, water_levels, edges), 0.0],
                                    Face::Back  => [0.0, Self::water_corner_h(lxi,   lyi, lzi,   blocks, water_levels, edges),
                                                       Self::water_corner_h(lxi+1, lyi, lzi,   blocks, water_levels, edges), 0.0],
                                    Face::Down  => [0.0; 4],
                                };
                                vertices.extend(Self::water_face_vertices(x as f32, y as f32, z as f32, face, block, corners, own_sl));
                            }
                        }
                        BlockType::Lava => {
                            // Self-luminous: sky_light sentinel -1.0 tells the shader to use
                            // sun_light=1.0 unconditionally, bypassing shadows and time-of-day.
                            for face in [Face::Right, Face::Left, Face::Up, Face::Down, Face::Front, Face::Back] {
                                if !Self::is_face_visible(blocks, x, y, z, face, edges) { continue; }
                                let corners: [f32; 4] = match face {
                                    Face::Up                                              => [1.0; 4],
                                    Face::Right | Face::Left | Face::Front | Face::Back  => [0.0, 1.0, 1.0, 0.0],
                                    Face::Down                                            => [0.0; 4],
                                };
                                vertices.extend(Self::water_face_vertices(x as f32, y as f32, z as f32, face, block, corners, -1.0));
                            }
                        }
                        _ => {} // solid blocks handled by greedy passes below
                    }
                }
            }
        }

        // ── Pass 2: greedy meshing for solid blocks ───────────────────────────
        // Each cell in the 16×16 mask stores block type + AO bits + sky bits.
        // Two cells merge only if ALL fields match (identical AO → correct merged corners).
        #[derive(Copy, Clone, PartialEq, Eq)]
        struct GCell { block: BlockType, ao: [u32; 4], sky: u32 }

        // Helper: build mask + greedy rects for one face direction and one layer.
        // Returns the (pos, uv, ao, sky, block) for each found rectangle.
        // We inline the 6 face directions to avoid function-pointer overhead.

        // ── Up face (Y+): layer=y, grid=(x,z), u=x, v=z ─────────────────────
        for ly in 0..16usize {
            let mut mask = [[None::<GCell>; 16]; 16]; // [lx][lz]
            for lx in 0..16usize {
                for lz in 0..16usize {
                    let b = blocks[lx][ly][lz];
                    if !Self::is_greedy_eligible(b) { continue; }
                    if Self::is_face_visible(blocks, lx, ly, lz, Face::Up, edges) {
                        let sky_val = Self::neighbor_sky(sky, edges, lx as i32, ly as i32, lz as i32, Face::Up);
                        let ao = Self::compute_ao(blocks, edges, lx as i32, ly as i32, lz as i32, Face::Up);
                        mask[lx][lz] = Some(GCell { block: b, ao: ao.map(f32::to_bits), sky: sky_val.to_bits() });
                    }
                }
            }
            for lx in 0..16usize {
                let mut lz = 0usize;
                while lz < 16 {
                    if let Some(cell) = mask[lx][lz] {
                        let mut w = 1usize; // width in z
                        while lz + w < 16 && mask[lx][lz + w] == Some(cell) { w += 1; }
                        let mut h = 1usize; // height in x
                        'h: while lx + h < 16 {
                            for dz in 0..w { if mask[lx + h][lz + dz] != Some(cell) { break 'h; } }
                            h += 1;
                        }
                        for dx in 0..h { for dz in 0..w { mask[lx + dx][lz + dz] = None; } }
                        let (lxf, lyf, lzf, wf, hf) = (lx as f32, ly as f32, lz as f32, w as f32, h as f32);
                        let ao = cell.ao.map(f32::from_bits);
                        let sky_f = f32::from_bits(cell.sky);
                        // v0=(lx,ly+1,lz+w) v1=(lx+h,ly+1,lz+w) v2=(lx+h,ly+1,lz) v3=(lx,ly+1,lz)
                        Self::emit_greedy_quad(&mut vertices,
                            [[lxf, lyf+1.0, lzf+wf], [lxf+hf, lyf+1.0, lzf+wf], [lxf+hf, lyf+1.0, lzf], [lxf, lyf+1.0, lzf]],
                            [[0.0, wf], [hf, wf], [hf, 0.0], [0.0, 0.0]],
                            Self::tile_base_for(cell.block.texture_id(Face::Up)),
                            [0.0, 1.0, 0.0], ao, sky_f, cell.block.color(), 1.0);
                        lz += w;
                    } else { lz += 1; }
                }
            }
        }

        // ── Down face (Y-): layer=y, grid=(x,z), u=x, v=z ───────────────────
        for ly in 0..16usize {
            let mut mask = [[None::<GCell>; 16]; 16];
            for lx in 0..16usize {
                for lz in 0..16usize {
                    let b = blocks[lx][ly][lz];
                    if !Self::is_greedy_eligible(b) { continue; }
                    if Self::is_face_visible(blocks, lx, ly, lz, Face::Down, edges) {
                        let sky_val = Self::neighbor_sky(sky, edges, lx as i32, ly as i32, lz as i32, Face::Down);
                        let ao = Self::compute_ao(blocks, edges, lx as i32, ly as i32, lz as i32, Face::Down);
                        mask[lx][lz] = Some(GCell { block: b, ao: ao.map(f32::to_bits), sky: sky_val.to_bits() });
                    }
                }
            }
            for lx in 0..16usize {
                let mut lz = 0usize;
                while lz < 16 {
                    if let Some(cell) = mask[lx][lz] {
                        let mut w = 1usize;
                        while lz + w < 16 && mask[lx][lz + w] == Some(cell) { w += 1; }
                        let mut h = 1usize;
                        'h: while lx + h < 16 {
                            for dz in 0..w { if mask[lx + h][lz + dz] != Some(cell) { break 'h; } }
                            h += 1;
                        }
                        for dx in 0..h { for dz in 0..w { mask[lx + dx][lz + dz] = None; } }
                        let (lxf, lyf, lzf, wf, hf) = (lx as f32, ly as f32, lz as f32, w as f32, h as f32);
                        let ao = cell.ao.map(f32::from_bits);
                        let sky_f = f32::from_bits(cell.sky);
                        // v0=(lx,ly,lz) v1=(lx+h,ly,lz) v2=(lx+h,ly,lz+w) v3=(lx,ly,lz+w)
                        Self::emit_greedy_quad(&mut vertices,
                            [[lxf, lyf, lzf], [lxf+hf, lyf, lzf], [lxf+hf, lyf, lzf+wf], [lxf, lyf, lzf+wf]],
                            [[0.0, 0.0], [hf, 0.0], [hf, wf], [0.0, wf]],
                            Self::tile_base_for(cell.block.texture_id(Face::Down)),
                            [0.0, -1.0, 0.0], ao, sky_f, cell.block.color(), 0.5);
                        lz += w;
                    } else { lz += 1; }
                }
            }
        }

        // ── Right face (X+): layer=x, grid=(z,y), w=z, h=y ──────────────────
        for lx in 0..16usize {
            let mut mask = [[None::<GCell>; 16]; 16]; // [lz][ly]
            for lz in 0..16usize {
                for ly in 0..16usize {
                    let b = blocks[lx][ly][lz];
                    if !Self::is_greedy_eligible(b) { continue; }
                    if Self::is_face_visible(blocks, lx, ly, lz, Face::Right, edges) {
                        let sky_val = Self::neighbor_sky(sky, edges, lx as i32, ly as i32, lz as i32, Face::Right);
                        let ao = Self::compute_ao(blocks, edges, lx as i32, ly as i32, lz as i32, Face::Right);
                        mask[lz][ly] = Some(GCell { block: b, ao: ao.map(f32::to_bits), sky: sky_val.to_bits() });
                    }
                }
            }
            for lz in 0..16usize {
                let mut ly = 0usize;
                while ly < 16 {
                    if let Some(cell) = mask[lz][ly] {
                        let mut h = 1usize; // height in y
                        while ly + h < 16 && mask[lz][ly + h] == Some(cell) { h += 1; }
                        let mut w = 1usize; // width in z
                        'w: while lz + w < 16 {
                            for dy in 0..h { if mask[lz + w][ly + dy] != Some(cell) { break 'w; } }
                            w += 1;
                        }
                        for dz in 0..w { for dy in 0..h { mask[lz + dz][ly + dy] = None; } }
                        let (lxf, lyf, lzf, wf, hf) = (lx as f32, ly as f32, lz as f32, w as f32, h as f32);
                        let ao = cell.ao.map(f32::from_bits);
                        let sky_f = f32::from_bits(cell.sky);
                        // v0=(lx+1,ly,lz) uv=(w,0)  v1=(lx+1,ly+h,lz) uv=(w,h)
                        // v2=(lx+1,ly+h,lz+w) uv=(0,h)  v3=(lx+1,ly,lz+w) uv=(0,0)
                        Self::emit_greedy_quad(&mut vertices,
                            [[lxf+1.0, lyf, lzf], [lxf+1.0, lyf+hf, lzf], [lxf+1.0, lyf+hf, lzf+wf], [lxf+1.0, lyf, lzf+wf]],
                            [[wf, 0.0], [wf, hf], [0.0, hf], [0.0, 0.0]],
                            Self::tile_base_for(cell.block.texture_id(Face::Right)),
                            [1.0, 0.0, 0.0], ao, sky_f, cell.block.color(), 0.65);
                        ly += h;
                    } else { ly += 1; }
                }
            }
        }

        // ── Left face (X-): layer=x, grid=(z,y), w=z, h=y ───────────────────
        for lx in 0..16usize {
            let mut mask = [[None::<GCell>; 16]; 16];
            for lz in 0..16usize {
                for ly in 0..16usize {
                    let b = blocks[lx][ly][lz];
                    if !Self::is_greedy_eligible(b) { continue; }
                    if Self::is_face_visible(blocks, lx, ly, lz, Face::Left, edges) {
                        let sky_val = Self::neighbor_sky(sky, edges, lx as i32, ly as i32, lz as i32, Face::Left);
                        let ao = Self::compute_ao(blocks, edges, lx as i32, ly as i32, lz as i32, Face::Left);
                        mask[lz][ly] = Some(GCell { block: b, ao: ao.map(f32::to_bits), sky: sky_val.to_bits() });
                    }
                }
            }
            for lz in 0..16usize {
                let mut ly = 0usize;
                while ly < 16 {
                    if let Some(cell) = mask[lz][ly] {
                        let mut h = 1usize;
                        while ly + h < 16 && mask[lz][ly + h] == Some(cell) { h += 1; }
                        let mut w = 1usize;
                        'w: while lz + w < 16 {
                            for dy in 0..h { if mask[lz + w][ly + dy] != Some(cell) { break 'w; } }
                            w += 1;
                        }
                        for dz in 0..w { for dy in 0..h { mask[lz + dz][ly + dy] = None; } }
                        let (lxf, lyf, lzf, wf, hf) = (lx as f32, ly as f32, lz as f32, w as f32, h as f32);
                        let ao = cell.ao.map(f32::from_bits);
                        let sky_f = f32::from_bits(cell.sky);
                        // v0=(lx,ly,lz+w) uv=(0,0)  v1=(lx,ly+h,lz+w) uv=(0,h)
                        // v2=(lx,ly+h,lz) uv=(w,h)  v3=(lx,ly,lz) uv=(w,0)
                        Self::emit_greedy_quad(&mut vertices,
                            [[lxf, lyf, lzf+wf], [lxf, lyf+hf, lzf+wf], [lxf, lyf+hf, lzf], [lxf, lyf, lzf]],
                            [[0.0, 0.0], [0.0, hf], [wf, hf], [wf, 0.0]],
                            Self::tile_base_for(cell.block.texture_id(Face::Left)),
                            [-1.0, 0.0, 0.0], ao, sky_f, cell.block.color(), 0.65);
                        ly += h;
                    } else { ly += 1; }
                }
            }
        }

        // ── Front face (Z+): layer=z, grid=(x,y), w=x, h=y ──────────────────
        for lz in 0..16usize {
            let mut mask = [[None::<GCell>; 16]; 16]; // [lx][ly]
            for lx in 0..16usize {
                for ly in 0..16usize {
                    let b = blocks[lx][ly][lz];
                    if !Self::is_greedy_eligible(b) { continue; }
                    if Self::is_face_visible(blocks, lx, ly, lz, Face::Front, edges) {
                        let sky_val = Self::neighbor_sky(sky, edges, lx as i32, ly as i32, lz as i32, Face::Front);
                        let ao = Self::compute_ao(blocks, edges, lx as i32, ly as i32, lz as i32, Face::Front);
                        mask[lx][ly] = Some(GCell { block: b, ao: ao.map(f32::to_bits), sky: sky_val.to_bits() });
                    }
                }
            }
            for lx in 0..16usize {
                let mut ly = 0usize;
                while ly < 16 {
                    if let Some(cell) = mask[lx][ly] {
                        let mut h = 1usize; // height in y
                        while ly + h < 16 && mask[lx][ly + h] == Some(cell) { h += 1; }
                        let mut w = 1usize; // width in x
                        'w: while lx + w < 16 {
                            for dy in 0..h { if mask[lx + w][ly + dy] != Some(cell) { break 'w; } }
                            w += 1;
                        }
                        for dx in 0..w { for dy in 0..h { mask[lx + dx][ly + dy] = None; } }
                        let (lxf, lyf, lzf, wf, hf) = (lx as f32, ly as f32, lz as f32, w as f32, h as f32);
                        let ao = cell.ao.map(f32::from_bits);
                        let sky_f = f32::from_bits(cell.sky);
                        // v0=(lx+w,ly,lz+1) uv=(w,0)  v1=(lx+w,ly+h,lz+1) uv=(w,h)
                        // v2=(lx,ly+h,lz+1) uv=(0,h)  v3=(lx,ly,lz+1) uv=(0,0)
                        Self::emit_greedy_quad(&mut vertices,
                            [[lxf+wf, lyf, lzf+1.0], [lxf+wf, lyf+hf, lzf+1.0], [lxf, lyf+hf, lzf+1.0], [lxf, lyf, lzf+1.0]],
                            [[wf, 0.0], [wf, hf], [0.0, hf], [0.0, 0.0]],
                            Self::tile_base_for(cell.block.texture_id(Face::Front)),
                            [0.0, 0.0, 1.0], ao, sky_f, cell.block.color(), 0.8);
                        ly += h;
                    } else { ly += 1; }
                }
            }
        }

        // ── Back face (Z-): layer=z, grid=(x,y), w=x, h=y ───────────────────
        for lz in 0..16usize {
            let mut mask = [[None::<GCell>; 16]; 16];
            for lx in 0..16usize {
                for ly in 0..16usize {
                    let b = blocks[lx][ly][lz];
                    if !Self::is_greedy_eligible(b) { continue; }
                    if Self::is_face_visible(blocks, lx, ly, lz, Face::Back, edges) {
                        let sky_val = Self::neighbor_sky(sky, edges, lx as i32, ly as i32, lz as i32, Face::Back);
                        let ao = Self::compute_ao(blocks, edges, lx as i32, ly as i32, lz as i32, Face::Back);
                        mask[lx][ly] = Some(GCell { block: b, ao: ao.map(f32::to_bits), sky: sky_val.to_bits() });
                    }
                }
            }
            for lx in 0..16usize {
                let mut ly = 0usize;
                while ly < 16 {
                    if let Some(cell) = mask[lx][ly] {
                        let mut h = 1usize;
                        while ly + h < 16 && mask[lx][ly + h] == Some(cell) { h += 1; }
                        let mut w = 1usize;
                        'w: while lx + w < 16 {
                            for dy in 0..h { if mask[lx + w][ly + dy] != Some(cell) { break 'w; } }
                            w += 1;
                        }
                        for dx in 0..w { for dy in 0..h { mask[lx + dx][ly + dy] = None; } }
                        let (lxf, lyf, lzf, wf, hf) = (lx as f32, ly as f32, lz as f32, w as f32, h as f32);
                        let ao = cell.ao.map(f32::from_bits);
                        let sky_f = f32::from_bits(cell.sky);
                        // v0=(lx,ly,lz) uv=(0,0)  v1=(lx,ly+h,lz) uv=(0,h)
                        // v2=(lx+w,ly+h,lz) uv=(w,h)  v3=(lx+w,ly,lz) uv=(w,0)
                        Self::emit_greedy_quad(&mut vertices,
                            [[lxf, lyf, lzf], [lxf, lyf+hf, lzf], [lxf+wf, lyf+hf, lzf], [lxf+wf, lyf, lzf]],
                            [[0.0, 0.0], [0.0, hf], [wf, hf], [wf, 0.0]],
                            Self::tile_base_for(cell.block.texture_id(Face::Back)),
                            [0.0, 0.0, -1.0], ao, sky_f, cell.block.color(), 0.8);
                        ly += h;
                    } else { ly += 1; }
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
        let tex_id = block_type.texture_id(face);
        let tb     = Self::tile_base_for(tex_id);
        let ts     = 1.0_f32 / 16.0;
        let atlas_tc = face.texture_coords(tex_id, 16);
        let tex: [[f32; 2]; 4] = atlas_tc.map(|[u, v]| [(u - tb[0]) / ts, (v - tb[1]) / ts]);
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
            verts.push(tb[0]);
            verts.push(tb[1]);
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

        // Lava renders below full block height (0.9 scale), so solid faces
        // bordering lava must remain visible to fill the gap.
        !neighbor.is_opaque() || neighbor == BlockType::Lava
    }

    // Solid lookup for AO that crosses chunk boundaries via neighbour edge data.
    // when the position steps outside the current chunk.  This prevents AO from
    // treating neighbour chunk blocks as air, which was causing a subtle bright
    // seam at every chunk border.
    fn block_is_solid_ao(blocks: &Blocks, edges: &NeighborEdges, x: i32, y: i32, z: i32) -> bool {
        let xb = x < 0 || x >= 16;
        let yb = y < 0 || y >= 16;
        let zb = z < 0 || z >= 16;
        // Two or more dimensions out of bounds = chunk corner we can't look up.
        if (xb as u8) + (yb as u8) + (zb as u8) > 1 { return false; }
        if !xb && !yb && !zb {
            blocks[x as usize][y as usize][z as usize].is_solid()
        } else if xb {
            let (y, z) = (y as usize, z as usize);
            if x < 0 { edges.left [y][z].is_solid() } else { edges.right[y][z].is_solid() }
        } else if zb {
            let (y, x) = (y as usize, x as usize);
            if z < 0 { edges.back [y][x].is_solid() } else { edges.front[y][x].is_solid() }
        } else {
            let (x, z) = (x as usize, z as usize);
            if y < 0 { edges.below[x][z].is_solid() } else { edges.above[x][z].is_solid() }
        }
    }

    fn compute_ao(blocks: &Blocks, edges: &NeighborEdges, x: i32, y: i32, z: i32, face: Face) -> [f32; 4] {
        let neighbors = face.ao_neighbors();
        let mut ao = [1.0f32; 4];
        for (vi, [s1, s2, c]) in neighbors.iter().enumerate() {
            let side1  = Self::block_is_solid_ao(blocks, edges, x + s1.0, y + s1.1, z + s1.2);
            let side2  = Self::block_is_solid_ao(blocks, edges, x + s2.0, y + s2.1, z + s2.2);
            let corner = Self::block_is_solid_ao(blocks, edges, x + c.0,  y + c.1,  z + c.2);
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

        let tex_id = block.texture_id(Face::Front);
        let tb = Self::tile_base_for(tex_id);
        // Tile-local UVs: u 0..1, v (1-height)..1 (show bottom portion of tile).
        let u0 = 0.0_f32;
        let u1 = 1.0_f32;
        let v1 = 1.0_f32;
        let v0 = 1.0 - height;

        let mut v: Vec<f32> = Vec::new();

        let mut quad = |p: [[f32; 3]; 4]| {
            let uvs = [[u0, v1], [u1, v1], [u1, v0], [u0, v0]];
            let (nx, ny, nz) = (0.0f32, 1.0, 0.0);
            for &i in &[0usize, 1, 2, 0, 2, 3] {
                v.extend_from_slice(&[p[i][0], p[i][1], p[i][2], r, g, b, uvs[i][0], uvs[i][1], nx, ny, nz, sky_light, tb[0], tb[1]]);
            }
            for &i in &[2usize, 1, 0, 3, 2, 0] {
                v.extend_from_slice(&[p[i][0], p[i][1], p[i][2], r, g, b, uvs[i][0], uvs[i][1], nx, ny, nz, sky_light, tb[0], tb[1]]);
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

    /// Tile base UV [col/16, row/16] for the given atlas texture index.
    #[inline(always)]
    fn tile_base_for(texture_id: u32) -> [f32; 2] {
        [(texture_id % 16) as f32 / 16.0, (texture_id / 16) as f32 / 16.0]
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
