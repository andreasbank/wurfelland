use std::mem;
use std::os::raw::c_void;
use std::f32::consts::FRAC_PI_2;
use crate::renderer::utils::{compile_shader, link_program};
use crate::renderer::shadow_pass::{ShadowPass, NUM_CASCADES, CASCADE_ENDS};
use crate::world::entity::{Entity, WeaponType};
use crate::world::{WorkbenchProp, BedProp, FurnaceProp};

// Vertex format: [x, y, z, r, g, b, nx, ny, nz] — 9 floats
const STRIDE: usize = 9;

fn push_vertex(verts: &mut Vec<f32>, x: f32, y: f32, z: f32, r: f32, g: f32, b: f32,
               nx: f32, ny: f32, nz: f32) {
    verts.extend_from_slice(&[x, y, z, r, g, b, nx, ny, nz]);
}

fn add_face(verts: &mut Vec<f32>, p: [[f32; 3]; 4], shade: f32, r: f32, g: f32, b: f32,
            nx: f32, ny: f32, nz: f32) {
    for &i in &[0usize, 1, 2, 0, 2, 3] {
        push_vertex(verts, p[i][0], p[i][1], p[i][2], r * shade, g * shade, b * shade, nx, ny, nz);
    }
}

fn add_box(verts: &mut Vec<f32>, x0: f32, y0: f32, z0: f32, x1: f32, y1: f32, z1: f32,
           r: f32, g: f32, b: f32) {
    add_face(verts, [[x0,y1,z0],[x1,y1,z0],[x1,y1,z1],[x0,y1,z1]], 1.00, r, g, b,  0., 1., 0.); // top
    add_face(verts, [[x0,y0,z1],[x1,y0,z1],[x1,y0,z0],[x0,y0,z0]], 0.50, r, g, b,  0.,-1., 0.); // bottom
    add_face(verts, [[x0,y0,z1],[x1,y0,z1],[x1,y1,z1],[x0,y1,z1]], 0.80, r, g, b,  0., 0., 1.); // front +Z
    add_face(verts, [[x1,y0,z0],[x0,y0,z0],[x0,y1,z0],[x1,y1,z0]], 0.80, r, g, b,  0., 0.,-1.); // back  -Z
    add_face(verts, [[x0,y0,z0],[x0,y0,z1],[x0,y1,z1],[x0,y1,z0]], 0.65, r, g, b, -1., 0., 0.); // left  -X
    add_face(verts, [[x1,y0,z1],[x1,y0,z0],[x1,y1,z0],[x1,y1,z1]], 0.65, r, g, b,  1., 0., 0.); // right +X
}

/// One bone of a geo-loaded mob: its box range in the built mesh and the pivot
/// (in block-space model coordinates) to rotate it about.
struct GeoBoneRange { name: String, first_box: i32, box_count: i32, pivot: [f32; 3] }

/// Build an entity-format mesh (with normals, for the lit mob shader) from a
/// `.geo.json`'s bones, scaled from MC units to block space, centred on XZ with
/// its feet at y=0 — matching how the hand-built box mobs are authored. Returns
/// the mesh plus, per bone, where its boxes live and its pivot, so limbs can be
/// animated by `draw_one_mob`.
fn build_geo_mob(bones: &[crate::renderer::geo_model::GeoBoneData], scale: f32)
    -> (Vec<f32>, Vec<GeoBoneRange>)
{
    let (mut min_x, mut max_x) = (f32::MAX, f32::MIN);
    let (mut min_z, mut max_z) = (f32::MAX, f32::MIN);
    let mut min_y = f32::MAX;
    for b in bones {
        for c in &b.cubes {
            min_x = min_x.min(c.origin[0]); max_x = max_x.max(c.origin[0] + c.size[0]);
            min_z = min_z.min(c.origin[2]); max_z = max_z.max(c.origin[2] + c.size[2]);
            min_y = min_y.min(c.origin[1]);
        }
    }
    let cx = (min_x + max_x) * 0.5;
    let cz = (min_z + max_z) * 0.5;
    let tx = |x: f32| (x - cx) * scale;
    let ty = |y: f32| (y - min_y) * scale;
    let tz = |z: f32| (z - cz) * scale;

    let mut mesh = Vec::new();
    let mut ranges = Vec::new();
    let mut box_idx = 0i32;
    for b in bones {
        let first = box_idx;
        for c in &b.cubes {
            let (o, s) = (c.origin, c.size);
            add_box(&mut mesh,
                tx(o[0]), ty(o[1]), tz(o[2]),
                tx(o[0] + s[0]), ty(o[1] + s[1]), tz(o[2] + s[2]),
                b.color.0, b.color.1, b.color.2);
            box_idx += 1;
        }
        if box_idx > first {
            ranges.push(GeoBoneRange {
                name: b.name.clone(), first_box: first, box_count: box_idx - first,
                pivot: [tx(b.pivot[0]), ty(b.pivot[1]), tz(b.pivot[2])],
            });
        }
    }
    (mesh, ranges)
}

// Chicken mesh layout (36 verts per box, 6 faces × 6 verts):
//   [0]   Body
//   [1]   Head
//   [2]   Beak
//   [3]   Wattle
//   [4]   Left wing   ← animated
//   [5]   Right wing  ← animated
//   [6]   Left leg
//   [7]   Right leg
const VPB: i32 = 36; // verts per box

// Pig mesh layout:
//   [0]   Body
//   [1]   Head
//   [2]   Snout
//   [3]   Front-left leg   ← animated
//   [4]   Front-right leg  ← animated (opposite phase)
//   [5]   Back-left leg    ← animated (opposite to front-left)
//   [6]   Back-right leg   ← animated (opposite to front-right)

// Cat mesh layout:
//   [0]   Body
//   [1]   Head
//   [2]   Left ear
//   [3]   Right ear
//   [4]   Tail
//   [5]   Front-left leg   ← animated
//   [6]   Front-right leg  ← animated (opposite phase)
//   [7]   Back-left leg    ← animated (opposite to front-left)
//   [8]   Back-right leg   ← animated (opposite to front-right)

// Cow mesh layout:
//   [0]   Body             — static
//   [1]   Head             — static
//   [2]   Snout            — static
//   [3]   Left horn        — static
//   [4]   Right horn       — static
//   [5]   Front-left leg   ← animated
//   [6]   Front-right leg  ← animated (opposite phase)
//   [7]   Back-left leg    ← animated (opposite to front-left)
//   [8]   Back-right leg   ← animated (opposite to front-right)

// Skeleton mesh layout:
//   [0]   Head  (skull)       — static
//   [1]   Body  (ribcage)     — static
//   [2]   Left arm            ← animated (swings on X around shoulder)
//   [3]   Right arm           ← animated (opposite phase)
//   [4]   Left leg            ← animated (swings on X around hip)
//   [5]   Right leg           ← animated (opposite to left arm)
// Static skin detail (eye sockets, nose, mouth, ribs, pelvis) appended after limbs.

// Sword held in skeleton right arm (x≈0.15..0.27, arm centre x=0.21).
// Grip overlaps the base of the arm; guard and blade hang below.
fn build_sword_weapon_mesh() -> Vec<f32> {
    let mut v = Vec::new();
    let silver = [0.80f32, 0.82, 0.87]; // blade
    let brown  = [0.35f32, 0.22, 0.10]; // grip
    let gray   = [0.60f32, 0.62, 0.65]; // guard
    // Grip (lower part of arm + a bit below)
    add_box(&mut v,  0.185, 0.40, -0.022,  0.235, 0.65,  0.022, brown[0], brown[1], brown[2]);
    // Guard (crosspiece)
    add_box(&mut v,  0.10,  0.60, -0.025,  0.32,  0.67,  0.025, gray[0],  gray[1],  gray[2]);
    // Blade (thin, long)
    add_box(&mut v,  0.204, 0.00, -0.012,  0.216, 0.60,  0.012, silver[0], silver[1], silver[2]);
    v
}

// Axe held in skeleton right arm.
fn build_axe_weapon_mesh() -> Vec<f32> {
    let mut v = Vec::new();
    let wood = [0.38f32, 0.24, 0.11]; // handle
    let iron = [0.52f32, 0.53, 0.57]; // axe head
    // Handle
    add_box(&mut v,  0.185, 0.28, -0.022,  0.235, 0.65,  0.022, wood[0], wood[1], wood[2]);
    // Axe head (wider, slightly asymmetric — blade faces away from body)
    add_box(&mut v,  0.08,  0.44, -0.045,  0.34,  0.68,  0.045, iron[0], iron[1], iron[2]);
    v
}

fn build_skeleton_mesh() -> Vec<f32> {
    let mut v = Vec::new();
    let bn = [0.90f32, 0.88, 0.82]; // bone ivory
    let sk = [0.92f32, 0.90, 0.86]; // skull slightly lighter

    // ── Base boxes [0..5] — order is load-bearing: the draw code animates the
    //    limbs at fixed offsets SKEL_LARM..SKEL_RLEG.  Do not reorder. ──
    // Head (skull): y=1.50..1.80, centered ±0.15
    add_box(&mut v, -0.15, 1.50, -0.15,  0.15, 1.80,  0.15, sk[0], sk[1], sk[2]);
    // Body (ribcage): y=0.65..1.50, thin
    add_box(&mut v, -0.12, 0.65, -0.08,  0.12, 1.50,  0.08, bn[0], bn[1], bn[2]);
    // Left arm: y=0.65..1.45, outside left
    add_box(&mut v, -0.27, 0.65, -0.06, -0.15, 1.45,  0.06, bn[0], bn[1], bn[2]);
    // Right arm: y=0.65..1.45, outside right
    add_box(&mut v,  0.15, 0.65, -0.06,  0.27, 1.45,  0.06, bn[0], bn[1], bn[2]);
    // Left leg: y=0..0.65, left of centre
    add_box(&mut v, -0.13, 0.00, -0.06, -0.03, 0.65,  0.06, bn[0], bn[1], bn[2]);
    // Right leg: y=0..0.65, right of centre
    add_box(&mut v,  0.03, 0.00, -0.06,  0.13, 0.65,  0.06, bn[0], bn[1], bn[2]);

    // ── Skin detail [6..] — static decoration drawn with the body transform.
    //    Appended after the animated limbs so limb offsets stay fixed.
    //    The skeleton faces -Z, so facial/rib detail sits on the -Z faces. ──
    let socket = [0.09f32, 0.08, 0.07]; // dark eye/nose/mouth hollows
    let groove = [0.46f32, 0.44, 0.40]; // shadowed rib gaps

    // Eye sockets (protrude slightly past the skull face at z=-0.15)
    add_box(&mut v, -0.105, 1.62, -0.158, -0.025, 1.70, -0.149, socket[0], socket[1], socket[2]);
    add_box(&mut v,  0.025, 1.62, -0.158,  0.105, 1.70, -0.149, socket[0], socket[1], socket[2]);
    // Nasal cavity
    add_box(&mut v, -0.022, 1.55, -0.158,  0.022, 1.61, -0.149, socket[0], socket[1], socket[2]);
    // Mouth / teeth line
    add_box(&mut v, -0.100, 1.51, -0.156,  0.100, 1.535, -0.149, socket[0], socket[1], socket[2]);
    // Ribs (dark grooves across the ribcage front)
    add_box(&mut v, -0.110, 1.30, -0.092,  0.110, 1.325, -0.082, groove[0], groove[1], groove[2]);
    add_box(&mut v, -0.110, 1.16, -0.092,  0.110, 1.185, -0.082, groove[0], groove[1], groove[2]);
    add_box(&mut v, -0.110, 1.02, -0.092,  0.110, 1.045, -0.082, groove[0], groove[1], groove[2]);
    // Pelvis (hip bone) bridging the leg tops
    add_box(&mut v, -0.135, 0.56, -0.075,  0.135, 0.70,  0.075, bn[0], bn[1], bn[2]);

    v
}

fn build_pig_mesh() -> Vec<f32> {
    let mut v = Vec::new();
    let pk = [0.90f32, 0.65, 0.60]; // body / head pink
    let sn = [0.95f32, 0.73, 0.70]; // snout lighter pink

    // Body
    add_box(&mut v, -0.25, 0.35, -0.35,  0.25, 0.85,  0.35, pk[0], pk[1], pk[2]);
    // Head (juts forward)
    add_box(&mut v, -0.22, 0.52, -0.58,  0.22, 0.90, -0.35, pk[0], pk[1], pk[2]);
    // Snout
    add_box(&mut v, -0.11, 0.58, -0.66,  0.11, 0.74, -0.58, sn[0], sn[1], sn[2]);
    // Front-left leg
    add_box(&mut v, -0.20, 0.00, -0.27, -0.07, 0.35, -0.14, pk[0], pk[1], pk[2]);
    // Front-right leg
    add_box(&mut v,  0.07, 0.00, -0.27,  0.20, 0.35, -0.14, pk[0], pk[1], pk[2]);
    // Back-left leg
    add_box(&mut v, -0.20, 0.00,  0.14, -0.07, 0.35,  0.27, pk[0], pk[1], pk[2]);
    // Back-right leg
    add_box(&mut v,  0.07, 0.00,  0.14,  0.20, 0.35,  0.27, pk[0], pk[1], pk[2]);

    v
}

fn build_cow_mesh() -> Vec<f32> {
    let mut v = Vec::new();
    let body = [0.45f32, 0.30, 0.15]; // warm dark brown body
    let head = [0.50f32, 0.34, 0.18]; // slightly lighter head
    let snout = [0.78f32, 0.58, 0.44]; // pinkish muzzle
    let horn = [0.88f32, 0.82, 0.60]; // cream horns
    let leg  = [0.40f32, 0.26, 0.12]; // darker brown legs

    // Body
    add_box(&mut v, -0.41, 0.54, -0.57,  0.41, 1.35,  0.57, body[0], body[1], body[2]);
    // Head (juts forward -Z)
    add_box(&mut v, -0.34, 0.84, -0.97,  0.34, 1.49, -0.57, head[0], head[1], head[2]);
    // Snout
    add_box(&mut v, -0.19, 0.88, -1.11,  0.19, 1.14, -0.97, snout[0], snout[1], snout[2]);
    // Left horn
    add_box(&mut v, -0.43, 1.43, -0.84, -0.32, 1.65, -0.70, horn[0], horn[1], horn[2]);
    // Right horn
    add_box(&mut v,  0.32, 1.43, -0.84,  0.43, 1.65, -0.70, horn[0], horn[1], horn[2]);
    // Front-left leg
    add_box(&mut v, -0.31, 0.00, -0.41, -0.12, 0.54, -0.22, leg[0], leg[1], leg[2]);
    // Front-right leg
    add_box(&mut v,  0.12, 0.00, -0.41,  0.31, 0.54, -0.22, leg[0], leg[1], leg[2]);
    // Back-left leg
    add_box(&mut v, -0.31, 0.00,  0.22, -0.12, 0.54,  0.41, leg[0], leg[1], leg[2]);
    // Back-right leg
    add_box(&mut v,  0.12, 0.00,  0.22,  0.31, 0.54,  0.41, leg[0], leg[1], leg[2]);

    v
}

fn build_chicken_mesh() -> Vec<f32> {
    let mut v = Vec::new();
    let wh = [0.95f32, 0.95, 0.85]; // feather white
    let or = [1.00f32, 0.60, 0.10]; // beak / legs orange
    let rd = [0.85f32, 0.10, 0.10]; // wattle red
    let lg = [0.78f32, 0.78, 0.75]; // wing light gray

    // Body: feet at y=0, body runs y 0.25–0.70
    add_box(&mut v, -0.20, 0.25, -0.22,  0.20, 0.70,  0.22, wh[0], wh[1], wh[2]);
    // Head: slightly forward (-Z from body center)
    add_box(&mut v, -0.15, 0.65, -0.38,  0.15, 0.90, -0.10, wh[0], wh[1], wh[2]);
    // Beak
    add_box(&mut v, -0.05, 0.73, -0.50,  0.05, 0.82, -0.38, or[0], or[1], or[2]);
    // Wattle (red flap under beak)
    add_box(&mut v, -0.03, 0.64, -0.46,  0.03, 0.74, -0.38, rd[0], rd[1], rd[2]);
    // Left wing (thin panel, hinge at x=-0.20)
    add_box(&mut v, -0.22, 0.28, -0.20, -0.20, 0.68,  0.20, lg[0], lg[1], lg[2]);
    // Right wing (hinge at x=+0.20)
    add_box(&mut v,  0.20, 0.28, -0.20,  0.22, 0.68,  0.20, lg[0], lg[1], lg[2]);
    // Left leg
    add_box(&mut v, -0.10, 0.00, -0.06, -0.03, 0.25,  0.06, or[0], or[1], or[2]);
    // Right leg
    add_box(&mut v,  0.03, 0.00, -0.06,  0.10, 0.25,  0.06, or[0], or[1], or[2]);

    v
}

// Workbench mesh: 2-block wide table centred at origin, long axis = X.
// Spans x:[-1,+1], z:[-0.5,+0.5], y:[0,0.82].
fn build_workbench_mesh() -> Vec<f32> {
    let mut v = Vec::new();
    let leg   = [0.38f32, 0.24, 0.11]; // dark wood legs
    let top   = [0.62f32, 0.43, 0.21]; // medium wood tabletop
    let shelf = [0.34f32, 0.21, 0.09]; // lower shelf
    let mark  = [0.28f32, 0.16, 0.06]; // crafting-mark inset

    // Four corner legs (x0→x1, y0→y1, z0→z1)
    add_box(&mut v, -1.00, 0.00, -0.50, -0.84, 0.68, -0.34, leg[0], leg[1], leg[2]);
    add_box(&mut v,  0.84, 0.00, -0.50,  1.00, 0.68, -0.34, leg[0], leg[1], leg[2]);
    add_box(&mut v, -1.00, 0.00,  0.34, -0.84, 0.68,  0.50, leg[0], leg[1], leg[2]);
    add_box(&mut v,  0.84, 0.00,  0.34,  1.00, 0.68,  0.50, leg[0], leg[1], leg[2]);

    // Tabletop slab
    add_box(&mut v, -1.00, 0.68, -0.50,  1.00, 0.82,  0.50, top[0], top[1], top[2]);

    // Lower shelf / stretcher
    add_box(&mut v, -0.86, 0.26, -0.38,  0.86, 0.32,  0.38, shelf[0], shelf[1], shelf[2]);

    // Crafting-mark inset on the tabletop surface
    add_box(&mut v, -0.38, 0.82, -0.28,  0.38, 0.85,  0.28, mark[0], mark[1], mark[2]);

    v
}

const WORKBENCH_VERT_COUNT: i32 = VPB * 7; // 4 legs + top + shelf + mark

// Bed mesh: head at x=+1, foot at x=-1. Long axis = X.
// Spans x:[-1,+1], z:[-0.5,+0.5], y:[0,0.70].
fn build_bed_mesh() -> Vec<f32> {
    let mut v = Vec::new();
    let frame    = [0.32f32, 0.20, 0.09]; // dark wood frame
    let mattress = [0.72f32, 0.14, 0.14]; // red mattress
    let pillow   = [0.93f32, 0.88, 0.80]; // cream pillow

    // Four corner legs
    add_box(&mut v, -1.00, 0.00, -0.50, -0.86, 0.28, -0.36, frame[0], frame[1], frame[2]);
    add_box(&mut v, -1.00, 0.00,  0.36, -0.86, 0.28,  0.50, frame[0], frame[1], frame[2]);
    add_box(&mut v,  0.86, 0.00, -0.50,  1.00, 0.28, -0.36, frame[0], frame[1], frame[2]);
    add_box(&mut v,  0.86, 0.00,  0.36,  1.00, 0.28,  0.50, frame[0], frame[1], frame[2]);

    // Headboard (x=+1 end, tall)
    add_box(&mut v, 0.86, 0.00, -0.50, 1.00, 0.70, 0.50, frame[0], frame[1], frame[2]);

    // Footboard (x=-1 end, shorter)
    add_box(&mut v, -1.00, 0.00, -0.50, -0.86, 0.45, 0.50, frame[0], frame[1], frame[2]);

    // Side rails connecting foot to head
    add_box(&mut v, -0.86, 0.10,  0.36,  0.86, 0.28,  0.50, frame[0], frame[1], frame[2]);
    add_box(&mut v, -0.86, 0.10, -0.50,  0.86, 0.28, -0.36, frame[0], frame[1], frame[2]);

    // Mattress
    add_box(&mut v, -0.84, 0.28, -0.34, 0.84, 0.42, 0.34, mattress[0], mattress[1], mattress[2]);

    // Pillow (at head end)
    add_box(&mut v, 0.54, 0.42, -0.26, 0.84, 0.50, 0.26, pillow[0], pillow[1], pillow[2]);

    v
}

const BED_VERT_COUNT: i32 = VPB * 10; // 4 legs + headboard + footboard + 2 rails + mattress + pillow

// Furnace mesh: 2-block wide stone kiln, long axis = X.
// Spans x:[-1,+1], z:[-0.5,+0.5], y:[0,1.05].
// Two glowing fire chambers face the -Z side.
fn build_furnace_mesh() -> Vec<f32> {
    let mut v = Vec::new();
    let stone   = [0.40f32, 0.40, 0.40]; // medium gray body
    let dark    = [0.10f32, 0.07, 0.04]; // dark door frame
    let glow    = [0.92f32, 0.48, 0.08]; // orange fire glow
    let chimney = [0.50f32, 0.50, 0.50]; // lighter gray chimney

    // Main stone body
    add_box(&mut v, -1.00, 0.00, -0.50,  1.00, 0.88,  0.50, stone[0], stone[1], stone[2]);
    // Left door frame (protrudes slightly south of stone body)
    add_box(&mut v, -0.76, 0.05, -0.52, -0.07, 0.62, -0.50, dark[0],  dark[1],  dark[2]);
    // Left fire glow (in front of door frame, smaller)
    add_box(&mut v, -0.69, 0.12, -0.54, -0.14, 0.55, -0.51, glow[0],  glow[1],  glow[2]);
    // Right door frame
    add_box(&mut v,  0.07, 0.05, -0.52,  0.76, 0.62, -0.50, dark[0],  dark[1],  dark[2]);
    // Right fire glow
    add_box(&mut v,  0.14, 0.12, -0.54,  0.69, 0.55, -0.51, glow[0],  glow[1],  glow[2]);
    // Chimney stack on top
    add_box(&mut v, -0.22, 0.88, -0.20,  0.22, 1.05,  0.20, chimney[0], chimney[1], chimney[2]);

    v
}

const FURNACE_VERT_COUNT: i32 = VPB * 6; // body + 2 door frames + 2 glows + chimney

// ── Data-driven mob animation ──────────────────────────────────────────────
// Every box-built mob (chicken/pig/cat/cow/skeleton) is described by a MobModel
// table instead of a bespoke draw function. One generic `draw_one_mob` walks the
// parts and applies each animation, so adding a species is data, not code.

#[derive(Clone, Copy)]
enum Axis { X, Z }

/// How a single mesh box (VPB verts) animates.
#[derive(Clone, Copy)]
enum PartAnim {
    /// Drawn with the body transform.
    Static,
    /// Limb swinging about `pivot`: angle = sin(anim·speed·move)·amp·phase.
    /// `scale_by_move` ties the swing to walking speed; `freeze_when_sitting`
    /// holds it at rest for tamed pets that are sitting.
    Swing {
        pivot: [f32; 3], axis: Axis, speed: f32, amp: f32, phase: f32,
        scale_by_move: bool, freeze_when_sitting: bool,
    },
    /// Skeleton arm: walk-swings, lunges (both arms together) while attacking,
    /// and optionally carries the held weapon under its transform.
    Arm { pivot: [f32; 3], speed: f32, amp: f32, phase: f32, holds_weapon: bool },
}

#[derive(Clone, Copy)]
struct Part { box_idx: i32, anim: PartAnim }

/// Declarative render description for one species.
struct MobModel {
    identifier: &'static str,
    vao: u32,
    parts: Vec<Part>,
    shadow_verts: i32,      // whole-mesh vertex count rendered into the shadow map
    health_bar_y: f32,
    lower_when_sitting: bool,
}

// Shorthand constructors for the model table.
fn stat(box_idx: i32) -> Part { Part { box_idx, anim: PartAnim::Static } }
fn leg(box_idx: i32, pivot: [f32; 3], speed: f32, amp: f32, phase: f32, freeze: bool) -> Part {
    Part { box_idx, anim: PartAnim::Swing {
        pivot, axis: Axis::X, speed, amp, phase, scale_by_move: true, freeze_when_sitting: freeze,
    } }
}

pub struct EntityRenderer {
    mob_models: Vec<MobModel>,
    vao: u32,
    vbo: u32,
    pig_vao: u32,
    pig_vbo: u32,
    skel_vao: u32,
    skel_vbo: u32,
    cat_vao: u32,
    cat_vbo: u32,
    cow_vao: u32,
    cow_vbo: u32,
    workbench_vao: u32,
    workbench_vbo: u32,
    bed_vao: u32,
    bed_vbo: u32,
    furnace_vao: u32,
    furnace_vbo: u32,
    sword_vao: u32,
    sword_vbo: u32,
    axe_vao: u32,
    axe_vbo: u32,
    shader: u32,
    mvp_loc: i32,
    model_loc: i32,
    fog_start_loc: i32,
    fog_end_loc: i32,
    fog_override_loc: i32,
    fog_color_override_loc: i32,
    screen_size_loc: i32,
    sky_sampler_loc: i32,
    ambient_light_loc: i32,
    directional_light_loc: i32,
    light_dir_loc: i32,
    shadow_maps_loc: i32,
    light_space_loc: i32,
    cascade_ends_loc: i32,
    texel_sizes_loc: i32,
    torch_pos_loc: i32,
    torch_strength_loc: i32,
    block_light_loc: i32,
    bar_vao: u32,
    bar_vbo: u32,
    bar_shader: u32,
    bar_mvp_loc: i32,
    bar_color_loc: i32,
}

impl EntityRenderer {
    fn upload_mesh(mesh: &[f32]) -> (u32, u32) {
        unsafe {
            let mut vao = 0u32;
            let mut vbo = 0u32;
            gl::GenVertexArrays(1, &mut vao);
            gl::GenBuffers(1, &mut vbo);
            gl::BindVertexArray(vao);
            gl::BindBuffer(gl::ARRAY_BUFFER, vbo);
            gl::BufferData(
                gl::ARRAY_BUFFER,
                (mesh.len() * mem::size_of::<f32>()) as isize,
                mesh.as_ptr() as *const c_void,
                gl::STATIC_DRAW,
            );
            let stride = (STRIDE * mem::size_of::<f32>()) as i32;
            gl::VertexAttribPointer(0, 3, gl::FLOAT, gl::FALSE, stride, std::ptr::null());
            gl::EnableVertexAttribArray(0);
            gl::VertexAttribPointer(1, 3, gl::FLOAT, gl::FALSE, stride,
                (3 * mem::size_of::<f32>()) as *const c_void);
            gl::EnableVertexAttribArray(1);
            gl::VertexAttribPointer(2, 3, gl::FLOAT, gl::FALSE, stride,
                (6 * mem::size_of::<f32>()) as *const c_void);
            gl::EnableVertexAttribArray(2);
            gl::BindBuffer(gl::ARRAY_BUFFER, 0);
            gl::BindVertexArray(0);
            (vao, vbo)
        }
    }

    pub fn new() -> Self {
        let (vao, vbo) = Self::upload_mesh(&build_chicken_mesh());
        let (pig_vao, pig_vbo) = Self::upload_mesh(&build_pig_mesh());
        let (skel_vao, skel_vbo) = Self::upload_mesh(&build_skeleton_mesh());
        // Cat: data-driven from cat.geo.json (legs split into pivoted bones so the
        // walk gait survives). Falls back to no cat if the model fails to load.
        const CAT_GEO_SCALE: f32 = 0.034; // MC units → blocks (~0.85 tall, like the box cat)
        let (cat_vao, cat_vbo, cat_model): (u32, u32, Option<MobModel>) =
            match crate::renderer::geo_model::load_bones("assets/models/cat/cat.geo.json") {
                Ok(bones) => {
                    let (mesh, ranges) = build_geo_mob(&bones, CAT_GEO_SCALE);
                    let (vao, vbo) = Self::upload_mesh(&mesh);
                    let total_boxes = (mesh.len() / STRIDE) as i32 / VPB;
                    let mut parts = Vec::new();
                    for r in &ranges {
                        if r.name.contains("leg") {
                            // Diagonal gait: FL & BR together, FR & BL opposite.
                            let phase = match r.name.as_str() {
                                "front_left_leg" | "back_right_leg" => 1.0,
                                _ => -1.0,
                            };
                            parts.push(Part { box_idx: r.first_box, anim: PartAnim::Swing {
                                pivot: r.pivot, axis: Axis::X, speed: 7.0, amp: 0.50, phase,
                                scale_by_move: true, freeze_when_sitting: true,
                            } });
                        } else {
                            for i in 0..r.box_count { parts.push(stat(r.first_box + i)); }
                        }
                    }
                    let model = MobModel {
                        identifier: "cat", vao, parts,
                        shadow_verts: total_boxes * VPB,
                        health_bar_y: 0.90, lower_when_sitting: true,
                    };
                    (vao, vbo, Some(model))
                }
                Err(e) => { eprintln!("[entity_renderer] cat.geo.json: {e}"); (0, 0, None) }
            };
        let (cow_vao, cow_vbo) = Self::upload_mesh(&build_cow_mesh());
        let (workbench_vao, workbench_vbo) = Self::upload_mesh(&build_workbench_mesh());
        let (bed_vao, bed_vbo) = Self::upload_mesh(&build_bed_mesh());
        let (furnace_vao, furnace_vbo) = Self::upload_mesh(&build_furnace_mesh());
        let (sword_vao, sword_vbo) = Self::upload_mesh(&build_sword_weapon_mesh());
        let (axe_vao, axe_vbo) = Self::upload_mesh(&build_axe_weapon_mesh());

        unsafe {

            let vert = compile_shader(gl::VERTEX_SHADER, r#"#version 330 core
                layout(location = 0) in vec3 aPos;
                layout(location = 1) in vec3 aColor;
                layout(location = 2) in vec3 aNormal;
                uniform mat4 mvp;
                uniform mat4 u_model;
                out vec3 vColor;
                out vec3 vWorldPos;
                out vec3 vNormal;
                out float fragDist;
                void main() {
                    gl_Position = mvp * vec4(aPos, 1.0);
                    vColor      = aColor;
                    fragDist    = gl_Position.w;
                    vWorldPos   = (u_model * vec4(aPos, 1.0)).xyz;
                    vNormal     = mat3(u_model) * aNormal;
                }"#).unwrap();

            let frag = compile_shader(gl::FRAGMENT_SHADER, r#"#version 330 core
                #define NUM_CASCADES 3
                in vec3 vColor;
                in vec3 vWorldPos;
                in vec3 vNormal;
                in float fragDist;
                out vec4 FragColor;
                uniform sampler2D      u_sky_sampler;
                uniform sampler2DArray u_shadow_maps;
                uniform vec2  u_screen_size;
                uniform float u_fog_start;
                uniform float u_fog_end;
                uniform float u_fog_override;
                uniform vec3  u_fog_color_override;
                uniform float u_ambient_light;
                uniform float u_directional_light;
                uniform vec3  u_light_dir;
                uniform mat4  u_light_space[NUM_CASCADES];
                uniform float u_cascade_ends[NUM_CASCADES];
                uniform float u_texel_sizes[NUM_CASCADES];
                uniform vec3  u_torch_pos;
                uniform float u_torch_strength;
                uniform float u_block_light;

                float calcShadow(vec3 worldPos, float viewDist) {
                    if (dot(normalize(vNormal), -normalize(u_light_dir)) <= 0.0) return 0.0;
                    int cascade = NUM_CASCADES - 1;
                    for (int i = 0; i < NUM_CASCADES - 1; i++) {
                        if (viewDist < u_cascade_ends[i]) { cascade = i; break; }
                    }
                    // Offset the sample point upward — same as the chunk renderer's
                    // floor-normal offset. Avoids false shadowing from adjacent raised
                    // blocks without drifting the XY sample into a different block's shadow.
                    vec3 biased = worldPos + vec3(0.0, u_texel_sizes[cascade] * 2.0, 0.0);
                    vec4 fragPosLS  = u_light_space[cascade] * vec4(biased, 1.0);
                    vec3 projCoords = fragPosLS.xyz / fragPosLS.w * 0.5 + 0.5;
                    if (projCoords.z > 1.0) return 0.0;
                    float shadow = 0.0;
                    const vec2 texelSize = vec2(1.0 / 2048.0);
                    for (int x = -1; x <= 1; ++x)
                        for (int y = -1; y <= 1; ++y)
                            shadow += projCoords.z > texture(u_shadow_maps,
                                vec3(projCoords.xy + vec2(x, y) * texelSize, cascade)).r ? 1.0 : 0.0;
                    return shadow / 9.0;
                }

                void main() {
                    vec2 screenUV  = gl_FragCoord.xy / u_screen_size;
                    vec3 skyFog    = texture(u_sky_sampler, screenUV).rgb;
                    vec3 fogColor  = mix(skyFog, u_fog_color_override, u_fog_override);
                    float fog_factor = clamp((fragDist - u_fog_start) / (u_fog_end - u_fog_start), 0.0, 1.0);
                    float shadow    = calcShadow(vWorldPos, fragDist);
                    float cave_amb  = 0.03;
                    float sun = u_ambient_light + u_directional_light * (1.0 - shadow);
                    float effective = mix(cave_amb, sun, u_block_light);
                    float torch_dist = length(vWorldPos - u_torch_pos);
                    float torch_atten = max(0.0, 1.0 - torch_dist / 12.0);
                    torch_atten = torch_atten * sqrt(torch_atten);
                    vec3 torch_contrib = torch_atten * u_torch_strength * 1.8 * vec3(1.0, 0.82, 0.55);
                    FragColor = vec4(mix(vColor * (vec3(effective) + torch_contrib), fogColor, fog_factor), 1.0);
                }
            "#).unwrap();

            let shader = link_program(vert, frag).unwrap();
            let mvp_loc                = gl::GetUniformLocation(shader, c"mvp".as_ptr());
            let model_loc              = gl::GetUniformLocation(shader, c"u_model".as_ptr());
            let fog_start_loc          = gl::GetUniformLocation(shader, c"u_fog_start".as_ptr());
            let fog_end_loc            = gl::GetUniformLocation(shader, c"u_fog_end".as_ptr());
            let fog_override_loc       = gl::GetUniformLocation(shader, c"u_fog_override".as_ptr());
            let fog_color_override_loc = gl::GetUniformLocation(shader, c"u_fog_color_override".as_ptr());
            let screen_size_loc        = gl::GetUniformLocation(shader, c"u_screen_size".as_ptr());
            let sky_sampler_loc        = gl::GetUniformLocation(shader, c"u_sky_sampler".as_ptr());
            let ambient_light_loc      = gl::GetUniformLocation(shader, c"u_ambient_light".as_ptr());
            let directional_light_loc  = gl::GetUniformLocation(shader, c"u_directional_light".as_ptr());
            let light_dir_loc          = gl::GetUniformLocation(shader, c"u_light_dir".as_ptr());
            let shadow_maps_loc        = gl::GetUniformLocation(shader, c"u_shadow_maps".as_ptr());
            let light_space_loc        = gl::GetUniformLocation(shader, c"u_light_space".as_ptr());
            let cascade_ends_loc       = gl::GetUniformLocation(shader, c"u_cascade_ends".as_ptr());
            let texel_sizes_loc        = gl::GetUniformLocation(shader, c"u_texel_sizes".as_ptr());
            let torch_pos_loc          = gl::GetUniformLocation(shader, c"u_torch_pos".as_ptr());
            let torch_strength_loc     = gl::GetUniformLocation(shader, c"u_torch_strength".as_ptr());
            let block_light_loc        = gl::GetUniformLocation(shader, c"u_block_light".as_ptr());

            // Billboard health bar — unit quad reused for background + fill
            let bar_verts: [f32; 18] = [
                0.0, 0.0, 0.0,  1.0, 0.0, 0.0,  1.0, 1.0, 0.0,
                0.0, 0.0, 0.0,  1.0, 1.0, 0.0,  0.0, 1.0, 0.0,
            ];
            let (mut bar_vao, mut bar_vbo) = (0u32, 0u32);
            gl::GenVertexArrays(1, &mut bar_vao);
            gl::GenBuffers(1, &mut bar_vbo);
            gl::BindVertexArray(bar_vao);
            gl::BindBuffer(gl::ARRAY_BUFFER, bar_vbo);
            gl::BufferData(gl::ARRAY_BUFFER,
                (bar_verts.len() * mem::size_of::<f32>()) as isize,
                bar_verts.as_ptr() as *const c_void, gl::STATIC_DRAW);
            gl::VertexAttribPointer(0, 3, gl::FLOAT, gl::FALSE,
                (3 * mem::size_of::<f32>()) as i32, std::ptr::null());
            gl::EnableVertexAttribArray(0);
            gl::BindBuffer(gl::ARRAY_BUFFER, 0);
            gl::BindVertexArray(0);

            let bar_vert_src = compile_shader(gl::VERTEX_SHADER, r#"#version 330 core
                layout(location = 0) in vec3 aPos;
                uniform mat4 mvp;
                void main() { gl_Position = mvp * vec4(aPos, 1.0); }"#).unwrap();
            let bar_frag_src = compile_shader(gl::FRAGMENT_SHADER, r#"#version 330 core
                uniform vec4 u_color;
                out vec4 FragColor;
                void main() { FragColor = u_color; }"#).unwrap();
            let bar_shader    = link_program(bar_vert_src, bar_frag_src).unwrap();
            let bar_mvp_loc   = gl::GetUniformLocation(bar_shader, c"mvp".as_ptr());
            let bar_color_loc = gl::GetUniformLocation(bar_shader, c"u_color".as_ptr());

            // ── Per-species render/animation tables ────────────────────────────
            let mut mob_models = vec![
                MobModel {
                    identifier: "chicken", vao, shadow_verts: VPB * 8,
                    health_bar_y: 1.05, lower_when_sitting: false,
                    parts: vec![
                        stat(0), stat(1), stat(2), stat(3), stat(6), stat(7),
                        // Wings flap around their hinge edge (Z axis), independent of walking.
                        Part { box_idx: 4, anim: PartAnim::Swing { pivot: [-0.20, 0.68, 0.0], axis: Axis::Z, speed: 9.0, amp: 0.45, phase:  1.0, scale_by_move: false, freeze_when_sitting: false } },
                        Part { box_idx: 5, anim: PartAnim::Swing { pivot: [ 0.20, 0.68, 0.0], axis: Axis::Z, speed: 9.0, amp: 0.45, phase: -1.0, scale_by_move: false, freeze_when_sitting: false } },
                    ],
                },
                MobModel {
                    identifier: "pig", vao: pig_vao, shadow_verts: VPB * 7,
                    health_bar_y: 1.05, lower_when_sitting: false,
                    parts: vec![
                        stat(0), stat(1), stat(2),
                        leg(3, [-0.135, 0.35, -0.205], 6.5, 0.55,  1.0, false),
                        leg(4, [ 0.135, 0.35, -0.205], 6.5, 0.55, -1.0, false),
                        leg(5, [-0.135, 0.35,  0.205], 6.5, 0.55, -1.0, false),
                        leg(6, [ 0.135, 0.35,  0.205], 6.5, 0.55,  1.0, false),
                    ],
                },
                MobModel {
                    identifier: "cow", vao: cow_vao, shadow_verts: VPB * 9,
                    health_bar_y: 1.55, lower_when_sitting: false,
                    parts: vec![
                        stat(0), stat(1), stat(2), stat(3), stat(4),
                        leg(5, [-0.16, 0.40, -0.23], 5.5, 0.55,  1.0, false),
                        leg(6, [ 0.16, 0.40, -0.23], 5.5, 0.55, -1.0, false),
                        leg(7, [-0.16, 0.40,  0.23], 5.5, 0.55, -1.0, false),
                        leg(8, [ 0.16, 0.40,  0.23], 5.5, 0.55,  1.0, false),
                    ],
                },
                MobModel {
                    identifier: "skeleton", vao: skel_vao, shadow_verts: VPB * 6,
                    health_bar_y: 1.95, lower_when_sitting: false,
                    parts: vec![
                        stat(0), stat(1),
                        Part { box_idx: 2, anim: PartAnim::Arm { pivot: [-0.21, 1.45, 0.0], speed: 7.0, amp: 0.60, phase:  1.0, holds_weapon: false } },
                        Part { box_idx: 3, anim: PartAnim::Arm { pivot: [ 0.21, 1.45, 0.0], speed: 7.0, amp: 0.60, phase: -1.0, holds_weapon: true  } },
                        leg(4, [-0.08, 0.65, 0.0], 7.0, 0.60, -1.0, false),
                        leg(5, [ 0.08, 0.65, 0.0], 7.0, 0.60,  1.0, false),
                        stat(6), stat(7), stat(8), stat(9), stat(10), stat(11), stat(12), stat(13),
                    ],
                },
            ];
            // Cat is built from cat.geo.json (above); append it if it loaded.
            if let Some(cat) = cat_model { mob_models.push(cat); }

            EntityRenderer {
                mob_models,
                vao, vbo, pig_vao, pig_vbo, skel_vao, skel_vbo, cat_vao, cat_vbo, cow_vao, cow_vbo,
                workbench_vao, workbench_vbo, bed_vao, bed_vbo, furnace_vao, furnace_vbo,
                sword_vao, sword_vbo, axe_vao, axe_vbo,
                shader,
                mvp_loc, model_loc,
                fog_start_loc, fog_end_loc, fog_override_loc, fog_color_override_loc,
                screen_size_loc, sky_sampler_loc,
                ambient_light_loc, directional_light_loc, light_dir_loc,
                shadow_maps_loc, light_space_loc, cascade_ends_loc, texel_sizes_loc,
                torch_pos_loc, torch_strength_loc, block_light_loc,
                bar_vao, bar_vbo, bar_shader, bar_mvp_loc, bar_color_loc,
            }
        }
    }

    fn bind_frame_uniforms(
        &self,
        fog_start: f32, fog_end: f32, screen_w: f32, screen_h: f32, sky_tex: u32,
        fog_override: f32, fog_color_override: glam::Vec3,
        ambient_light: f32, directional_light: f32, sun_dir: glam::Vec3,
        shadow_tex: u32, light_space: &[glam::Mat4; NUM_CASCADES],
        texel_sizes: &[f32; NUM_CASCADES],
        torch_pos: glam::Vec3, torch_strength: f32,
    ) {
        unsafe {
            gl::UseProgram(self.shader);
            gl::Uniform1f(self.fog_start_loc,  fog_start);
            gl::Uniform1f(self.fog_end_loc,    fog_end);
            gl::Uniform1f(self.fog_override_loc, fog_override);
            gl::Uniform3f(self.fog_color_override_loc, fog_color_override.x, fog_color_override.y, fog_color_override.z);
            gl::Uniform2f(self.screen_size_loc, screen_w, screen_h);
            gl::Uniform1f(self.ambient_light_loc,     ambient_light);
            gl::Uniform1f(self.directional_light_loc, directional_light);
            gl::Uniform3f(self.light_dir_loc, sun_dir.x, sun_dir.y, sun_dir.z);
            gl::Uniform1fv(self.cascade_ends_loc,  NUM_CASCADES as i32, CASCADE_ENDS.as_ptr());
            gl::Uniform1fv(self.texel_sizes_loc,   NUM_CASCADES as i32, texel_sizes.as_ptr());
            gl::UniformMatrix4fv(self.light_space_loc, NUM_CASCADES as i32, gl::FALSE,
                light_space[0].as_ref().as_ptr());
            gl::Uniform3f(self.torch_pos_loc, torch_pos.x, torch_pos.y, torch_pos.z);
            gl::Uniform1f(self.torch_strength_loc, torch_strength);
            // Texture unit 4: sky (fog colour)
            gl::Uniform1i(self.sky_sampler_loc, 4);
            gl::ActiveTexture(gl::TEXTURE4);
            gl::BindTexture(gl::TEXTURE_2D, sky_tex);
            // Texture unit 5: shadow map array
            gl::Uniform1i(self.shadow_maps_loc, 5);
            gl::ActiveTexture(gl::TEXTURE5);
            gl::BindTexture(gl::TEXTURE_2D_ARRAY, shadow_tex);
        }
    }

    /// Draw a billboard health bar above an entity. `bar_y` is the Y offset above the entity's
    /// feet position where the bar should appear. Only call when health_frac < 1.0.
    pub fn draw_health_bar(
        &self,
        position: [f32; 3],
        health_frac: f32,
        bar_y: f32,
        view: &glam::Mat4,
        projection: &glam::Mat4,
    ) {
        const BAR_W: f32 = 0.5;
        const BAR_H: f32 = 0.05;

        let cam_right = glam::Vec3::new(view.x_axis.x, view.y_axis.x, view.z_axis.x);
        let cam_up    = glam::Vec3::new(view.x_axis.y, view.y_axis.y, view.z_axis.y);
        let cam_fwd   = cam_right.cross(cam_up);

        let origin = glam::Vec3::new(position[0], position[1] + bar_y, position[2])
            - cam_right * (BAR_W * 0.5)
            - cam_up    * (BAR_H * 0.5);

        unsafe {
            gl::Disable(gl::CULL_FACE);
            gl::UseProgram(self.bar_shader);
            gl::BindVertexArray(self.bar_vao);

            let bg = glam::Mat4::from_cols(
                (cam_right * BAR_W).extend(0.0),
                (cam_up    * BAR_H).extend(0.0),
                cam_fwd.extend(0.0),
                origin.extend(1.0),
            );
            gl::UniformMatrix4fv(self.bar_mvp_loc, 1, gl::FALSE,
                (*projection * *view * bg).to_cols_array().as_ptr());
            gl::Uniform4f(self.bar_color_loc, 0.0, 0.0, 0.0, 1.0);
            gl::DrawArrays(gl::TRIANGLES, 0, 6);

            let fw = (BAR_W * health_frac.clamp(0.0, 1.0)).max(0.0);
            if fw > 0.0 {
                gl::Enable(gl::POLYGON_OFFSET_FILL);
                gl::PolygonOffset(-1.0, -1.0);
                let fg = glam::Mat4::from_cols(
                    (cam_right * fw).extend(0.0),
                    (cam_up    * BAR_H).extend(0.0),
                    cam_fwd.extend(0.0),
                    origin.extend(1.0),
                );
                gl::UniformMatrix4fv(self.bar_mvp_loc, 1, gl::FALSE,
                    (*projection * *view * fg).to_cols_array().as_ptr());
                gl::Uniform4f(self.bar_color_loc, 0.18, 0.72, 0.18, 1.0);
                gl::DrawArrays(gl::TRIANGLES, 0, 6);
                gl::Disable(gl::POLYGON_OFFSET_FILL);
            }

            gl::BindVertexArray(0);
            gl::Enable(gl::CULL_FACE);
        }
    }

    /// Render every box-built mob in one pass, grouped by species VAO.
    pub fn draw_entities(&self, entities: &[Entity], view: &glam::Mat4, projection: &glam::Mat4,
                         fog_start: f32, fog_end: f32, screen_w: f32, screen_h: f32, sky_tex: u32,
                         fog_override: f32, fog_color_override: glam::Vec3,
                         ambient_light: f32, directional_light: f32, sun_dir: glam::Vec3,
                         shadow_tex: u32, light_space: &[glam::Mat4; NUM_CASCADES],
                         texel_sizes: &[f32; NUM_CASCADES],
                         torch_pos: glam::Vec3, torch_strength: f32) {
        unsafe {
            gl::Disable(gl::CULL_FACE);
            self.bind_frame_uniforms(fog_start, fog_end, screen_w, screen_h, sky_tex,
                fog_override, fog_color_override, ambient_light, directional_light, sun_dir,
                shadow_tex, light_space, texel_sizes, torch_pos, torch_strength);
            for model in &self.mob_models {
                gl::BindVertexArray(model.vao);
                for e in entities.iter().filter(|e| e.def.identifier == model.identifier) {
                    self.draw_one_mob(model, e, view, projection);
                }
            }
            gl::BindVertexArray(0);
            gl::Enable(gl::CULL_FACE);
        }

        // Health bars (own shader / blending), above each hurt mob.
        for e in entities {
            if let Some(model) = self.mob_models.iter().find(|m| m.identifier == e.def.identifier) {
                let frac = e.health / e.def.max_health;
                if frac < 1.0 {
                    self.draw_health_bar(e.position, frac, model.health_bar_y, view, projection);
                }
            }
        }
    }

    /// Draw one mob: body transform, per-part animation, and any held weapon.
    /// The caller must have already bound `model.vao`.
    fn draw_one_mob(&self, model: &MobModel, e: &Entity, view: &glam::Mat4, projection: &glam::Mat4) {
        unsafe {
            gl::Uniform1f(self.block_light_loc, e.block_light);
            let rot_y = -(e.yaw.to_radians() + FRAC_PI_2);
            let y_off = if model.lower_when_sitting && e.sitting { -0.12 } else { 0.0 };
            let pos = glam::Vec3::new(e.position[0], e.position[1] + y_off, e.position[2]);
            let base = glam::Mat4::from_translation(pos) * glam::Mat4::from_rotation_y(rot_y);
            gl::UniformMatrix4fv(self.model_loc, 1, gl::FALSE, base.to_cols_array().as_ptr());

            let mut weapon_xform: Option<glam::Mat4> = None;
            for part in &model.parts {
                let m = match part.anim {
                    PartAnim::Static => base,
                    PartAnim::Swing { pivot, axis, speed, amp, phase, scale_by_move, freeze_when_sitting } => {
                        let angle = if freeze_when_sitting && e.sitting {
                            0.0
                        } else {
                            let mv = if scale_by_move { e.move_speed_norm() } else { 1.0 };
                            (e.anim_time * speed * mv).sin() * amp * phase
                        };
                        let p = glam::Vec3::from(pivot);
                        let rot = match axis {
                            Axis::X => glam::Mat4::from_rotation_x(angle),
                            Axis::Z => glam::Mat4::from_rotation_z(angle),
                        };
                        base * glam::Mat4::from_translation(p) * rot * glam::Mat4::from_translation(-p)
                    }
                    PartAnim::Arm { pivot, speed, amp, phase, holds_weapon } => {
                        let angle = if e.attack_anim > 0.0 {
                            // Both arms lunge forward together at the swing peak.
                            let t = 1.0 - e.attack_anim / 0.5;
                            (std::f32::consts::PI * t).sin() * 1.4
                        } else {
                            (e.anim_time * speed * e.move_speed_norm()).sin() * amp * phase
                        };
                        let p = glam::Vec3::from(pivot);
                        let m = base * glam::Mat4::from_translation(p)
                            * glam::Mat4::from_rotation_x(angle)
                            * glam::Mat4::from_translation(-p);
                        if holds_weapon { weapon_xform = Some(m); }
                        m
                    }
                };
                let mvp = *projection * *view * m;
                gl::UniformMatrix4fv(self.mvp_loc, 1, gl::FALSE, mvp.to_cols_array().as_ptr());
                gl::DrawArrays(gl::TRIANGLES, part.box_idx * VPB, VPB);
            }

            // Weapon carried under the right-arm transform (skeletons).
            if let Some(wm) = weapon_xform {
                let (wvao, wverts) = match e.weapon {
                    WeaponType::Sword     => (self.sword_vao, VPB * 3),
                    WeaponType::Axe       => (self.axe_vao,   VPB * 2),
                    WeaponType::BareHands => (0, 0),
                };
                if wverts > 0 {
                    let mvp = *projection * *view * wm;
                    gl::BindVertexArray(wvao);
                    gl::UniformMatrix4fv(self.mvp_loc, 1, gl::FALSE, mvp.to_cols_array().as_ptr());
                    gl::DrawArrays(gl::TRIANGLES, 0, wverts);
                    gl::BindVertexArray(model.vao);
                }
            }
        }
    }

    /// Shadow pass: whole mesh at the body transform (limbs unanimated).
    pub fn draw_entity_shadows(&self, entities: &[Entity], shadow_pass: &ShadowPass) {
        for model in &self.mob_models {
            for e in entities.iter().filter(|e| e.def.identifier == model.identifier) {
                let rot_y = -(e.yaw.to_radians() + FRAC_PI_2);
                let y_off = if model.lower_when_sitting && e.sitting { -0.12 } else { 0.0 };
                let pos = glam::Vec3::new(e.position[0], e.position[1] + y_off, e.position[2]);
                let model_mat = glam::Mat4::from_translation(pos) * glam::Mat4::from_rotation_y(rot_y);
                shadow_pass.draw_solid_mesh(model.vao, 0, model.shadow_verts, &model_mat);
            }
        }
    }

    pub fn draw_workbenches(
        &self, workbenches: &[WorkbenchProp],
        view: &glam::Mat4, projection: &glam::Mat4,
        fog_start: f32, fog_end: f32, screen_w: f32, screen_h: f32, sky_tex: u32,
        fog_override: f32, fog_color_override: glam::Vec3,
        ambient_light: f32, directional_light: f32, sun_dir: glam::Vec3,
        shadow_tex: u32, light_space: &[glam::Mat4; NUM_CASCADES],
        texel_sizes: &[f32; NUM_CASCADES],
        torch_pos: glam::Vec3, torch_strength: f32,
    ) {
        if workbenches.is_empty() { return; }
        unsafe {
            gl::Disable(gl::CULL_FACE);
            self.bind_frame_uniforms(fog_start, fog_end, screen_w, screen_h, sky_tex,
                fog_override, fog_color_override, ambient_light, directional_light, sun_dir,
                shadow_tex, light_space, texel_sizes, torch_pos, torch_strength);
            gl::BindVertexArray(self.workbench_vao);

            for wb in workbenches {
                gl::Uniform1f(self.block_light_loc, 1.0);
                let c = wb.center();
                let model = glam::Mat4::from_translation(glam::Vec3::from(c))
                    * glam::Mat4::from_rotation_y(wb.yaw());
                let mvp = *projection * *view * model;
                gl::UniformMatrix4fv(self.model_loc, 1, gl::FALSE, model.to_cols_array().as_ptr());
                gl::UniformMatrix4fv(self.mvp_loc,   1, gl::FALSE, mvp.to_cols_array().as_ptr());
                gl::DrawArrays(gl::TRIANGLES, 0, WORKBENCH_VERT_COUNT);
            }

            gl::BindVertexArray(0);
            gl::Enable(gl::CULL_FACE);
        }
    }

    pub fn draw_workbench_shadows(&self, workbenches: &[WorkbenchProp], shadow_pass: &ShadowPass) {
        for wb in workbenches {
            let c = wb.center();
            let model = glam::Mat4::from_translation(glam::Vec3::from(c))
                * glam::Mat4::from_rotation_y(wb.yaw());
            shadow_pass.draw_solid_mesh(self.workbench_vao, 0, WORKBENCH_VERT_COUNT, &model);
        }
    }

    pub fn draw_beds(
        &self, beds: &[BedProp],
        view: &glam::Mat4, projection: &glam::Mat4,
        fog_start: f32, fog_end: f32, screen_w: f32, screen_h: f32, sky_tex: u32,
        fog_override: f32, fog_color_override: glam::Vec3,
        ambient_light: f32, directional_light: f32, sun_dir: glam::Vec3,
        shadow_tex: u32, light_space: &[glam::Mat4; NUM_CASCADES],
        texel_sizes: &[f32; NUM_CASCADES],
        torch_pos: glam::Vec3, torch_strength: f32,
    ) {
        if beds.is_empty() { return; }
        unsafe {
            gl::Disable(gl::CULL_FACE);
            self.bind_frame_uniforms(fog_start, fog_end, screen_w, screen_h, sky_tex,
                fog_override, fog_color_override, ambient_light, directional_light, sun_dir,
                shadow_tex, light_space, texel_sizes, torch_pos, torch_strength);
            gl::BindVertexArray(self.bed_vao);

            for bed in beds {
                gl::Uniform1f(self.block_light_loc, 1.0);
                let c = bed.center();
                let model = glam::Mat4::from_translation(glam::Vec3::from(c))
                    * glam::Mat4::from_rotation_y(bed.yaw());
                let mvp = *projection * *view * model;
                gl::UniformMatrix4fv(self.model_loc, 1, gl::FALSE, model.to_cols_array().as_ptr());
                gl::UniformMatrix4fv(self.mvp_loc,   1, gl::FALSE, mvp.to_cols_array().as_ptr());
                gl::DrawArrays(gl::TRIANGLES, 0, BED_VERT_COUNT);
            }

            gl::BindVertexArray(0);
            gl::Enable(gl::CULL_FACE);
        }
    }

    pub fn draw_bed_shadows(&self, beds: &[BedProp], shadow_pass: &ShadowPass) {
        for bed in beds {
            let c = bed.center();
            let model = glam::Mat4::from_translation(glam::Vec3::from(c))
                * glam::Mat4::from_rotation_y(bed.yaw());
            shadow_pass.draw_solid_mesh(self.bed_vao, 0, BED_VERT_COUNT, &model);
        }
    }

    pub fn draw_furnaces(
        &self, furnaces: &[FurnaceProp],
        view: &glam::Mat4, projection: &glam::Mat4,
        fog_start: f32, fog_end: f32, screen_w: f32, screen_h: f32, sky_tex: u32,
        fog_override: f32, fog_color_override: glam::Vec3,
        ambient_light: f32, directional_light: f32, sun_dir: glam::Vec3,
        shadow_tex: u32, light_space: &[glam::Mat4; NUM_CASCADES],
        texel_sizes: &[f32; NUM_CASCADES],
        torch_pos: glam::Vec3, torch_strength: f32,
    ) {
        if furnaces.is_empty() { return; }
        unsafe {
            gl::Disable(gl::CULL_FACE);
            self.bind_frame_uniforms(fog_start, fog_end, screen_w, screen_h, sky_tex,
                fog_override, fog_color_override, ambient_light, directional_light, sun_dir,
                shadow_tex, light_space, texel_sizes, torch_pos, torch_strength);
            gl::BindVertexArray(self.furnace_vao);

            for f in furnaces {
                gl::Uniform1f(self.block_light_loc, 1.0);
                let c = f.center();
                let model = glam::Mat4::from_translation(glam::Vec3::from(c))
                    * glam::Mat4::from_rotation_y(f.yaw());
                let mvp = *projection * *view * model;
                gl::UniformMatrix4fv(self.model_loc, 1, gl::FALSE, model.to_cols_array().as_ptr());
                gl::UniformMatrix4fv(self.mvp_loc,   1, gl::FALSE, mvp.to_cols_array().as_ptr());
                gl::DrawArrays(gl::TRIANGLES, 0, FURNACE_VERT_COUNT);
            }

            gl::BindVertexArray(0);
            gl::Enable(gl::CULL_FACE);
        }
    }

    pub fn draw_furnace_shadows(&self, furnaces: &[FurnaceProp], shadow_pass: &ShadowPass) {
        if furnaces.is_empty() { return; }
        unsafe { gl::Disable(gl::CULL_FACE); }
        for f in furnaces {
            let c = f.center();
            let model = glam::Mat4::from_translation(glam::Vec3::from(c))
                * glam::Mat4::from_rotation_y(f.yaw());
            shadow_pass.draw_solid_mesh(self.furnace_vao, 0, FURNACE_VERT_COUNT, &model);
        }
        unsafe { gl::Enable(gl::CULL_FACE); }
    }
}

impl Drop for EntityRenderer {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteVertexArrays(1, &self.vao);
            gl::DeleteBuffers(1, &self.vbo);
            gl::DeleteVertexArrays(1, &self.pig_vao);
            gl::DeleteBuffers(1, &self.pig_vbo);
            gl::DeleteVertexArrays(1, &self.skel_vao);
            gl::DeleteBuffers(1, &self.skel_vbo);
            gl::DeleteVertexArrays(1, &self.cat_vao);
            gl::DeleteBuffers(1, &self.cat_vbo);
            gl::DeleteVertexArrays(1, &self.cow_vao);
            gl::DeleteBuffers(1, &self.cow_vbo);
            gl::DeleteVertexArrays(1, &self.workbench_vao);
            gl::DeleteBuffers(1, &self.workbench_vbo);
            gl::DeleteVertexArrays(1, &self.bed_vao);
            gl::DeleteBuffers(1, &self.bed_vbo);
            gl::DeleteVertexArrays(1, &self.furnace_vao);
            gl::DeleteBuffers(1, &self.furnace_vbo);
            gl::DeleteVertexArrays(1, &self.sword_vao);
            gl::DeleteBuffers(1, &self.sword_vbo);
            gl::DeleteVertexArrays(1, &self.axe_vao);
            gl::DeleteBuffers(1, &self.axe_vbo);
            gl::DeleteProgram(self.shader);
            gl::DeleteVertexArrays(1, &self.bar_vao);
            gl::DeleteBuffers(1, &self.bar_vbo);
            gl::DeleteProgram(self.bar_shader);
        }
    }
}
