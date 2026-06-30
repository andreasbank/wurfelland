//! Procedural Bedrock-style trees.
//!
//! A tree is generated purely from its world-space origin + the world seed, so
//! every chunk that the tree overlaps regenerates the *identical* set of blocks
//! and keeps only the ones inside its own bounds (see `Chunk::generate`).  This
//! lets trunks and branches span chunk borders with no shared mutable state.
//!
//! Emission rule (handled by the caller's `emit` closure): at any world
//! position a `Log` always wins over `Leaves`, regardless of emission order, so
//! the result is consistent no matter which chunk computed it.

use crate::world::BlockType;

/// Horizontal/vertical reach of the largest tree, in blocks.  The chunk
/// generator scans this many blocks of margin around itself for tree origins.
pub const MAX_TREE_RADIUS: i32 = 8;

/// Deterministic splitmix64 RNG — same seed always yields the same tree.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed)
    }
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn u32(&mut self) -> u32 {
        (self.next() >> 32) as u32
    }
    /// Inclusive integer range `[lo, hi]`.
    fn range(&mut self, lo: i32, hi: i32) -> i32 {
        if hi <= lo {
            return lo;
        }
        lo + (self.u32() % ((hi - lo + 1) as u32)) as i32
    }
    /// True with probability 1-in-`n`.
    fn chance(&mut self, n: u32) -> bool {
        self.u32() % n == 0
    }
}

/// Eight horizontal directions a branch can grow in.
const DIRS: [(i32, i32); 8] = [
    (1, 0), (-1, 0), (0, 1), (0, -1),
    (1, 1), (1, -1), (-1, 1), (-1, -1),
];

/// Generate a full tree at `(ox, base_wy, oz)` where `base_wy` is the world-Y of
/// the first log above the ground block.  `emit(wx, wy, wz, block)` receives
/// every block; the caller decides which fall inside the chunk being built.
pub fn generate_tree(
    ox: i32,
    base_wy: i32,
    oz: i32,
    world_seed: u32,
    emit: &mut impl FnMut(i32, i32, i32, BlockType),
) {
    let seed = (ox as i64 as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (oz as i64 as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
        ^ (world_seed as u64).wrapping_mul(0x1656_67B1_9E37_79F9);
    let mut rng = Rng::new(seed);

    // Size class: small / medium / large (mirrors the old 20-way roll).
    let roll = rng.u32() % 20;
    let (trunk_h, n_branches, blob_r) = if roll < 10 {
        (rng.range(5, 6), 2, 2)
    } else if roll < 17 {
        (rng.range(7, 9), rng.range(3, 4), 2)
    } else {
        (rng.range(10, 12), rng.range(4, 5), 3)
    };

    // Trunk, with an optional one-block lean partway up.
    let lean_at = if rng.chance(2) {
        rng.range(trunk_h / 2, trunk_h - 2)
    } else {
        i32::MAX
    };
    let (lean_dx, lean_dz) = DIRS[(rng.u32() % 4) as usize]; // cardinal lean only
    let mut tx = ox;
    let mut tz = oz;
    for i in 0..trunk_h {
        if i == lean_at {
            tx += lean_dx;
            tz += lean_dz;
        }
        emit(tx, base_wy + i, tz, BlockType::Log);
    }
    let top_y = base_wy + trunk_h - 1;

    // Branches: spring from the upper trunk, growing out and up to a leaf blob.
    for _ in 0..n_branches {
        let start_y = base_wy + rng.range(trunk_h / 2, trunk_h - 1);
        let (dx, dz) = DIRS[(rng.u32() % 8) as usize];
        let len = rng.range(2, 4);
        let mut bx = tx;
        let mut bz = tz;
        let mut by = start_y;
        for step in 0..len {
            bx += dx;
            bz += dz;
            if step % 2 == 1 {
                by += 1; // rise every other block for an upward arc
            }
            emit(bx, by, bz, BlockType::Log);
        }
        leaf_blob(bx, by, bz, blob_r, &mut rng, emit);
    }

    // Crown above the trunk top.
    leaf_blob(tx, top_y + 1, tz, blob_r, &mut rng, emit);
}

/// A roughly spherical clump of leaves, with outer corners randomly trimmed for
/// an organic silhouette.
fn leaf_blob(
    cx: i32,
    cy: i32,
    cz: i32,
    r: i32,
    rng: &mut Rng,
    emit: &mut impl FnMut(i32, i32, i32, BlockType),
) {
    let r2 = r * r;
    for dy in -r..=r {
        for dx in -r..=r {
            for dz in -r..=r {
                let d2 = dx * dx + dy * dy + dz * dz;
                if d2 > r2 + 1 {
                    continue;
                }
                // Trim the outermost shell ~half the time so the ball isn't a
                // perfect sphere.
                if d2 >= r2 - 1 && rng.chance(2) {
                    continue;
                }
                emit(cx + dx, cy + dy, cz + dz, BlockType::Leaves);
            }
        }
    }
}
