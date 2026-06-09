use std::mem;
use std::os::raw::c_void;
use std::f32::consts::FRAC_PI_2;
use crate::renderer::utils::{compile_shader, link_program};
use crate::renderer::shadow_pass::{ShadowPass, NUM_CASCADES, CASCADE_ENDS};
use crate::world::entity::{Chicken, Pig, Skeleton, Cat, Cow};
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
const BODY_OFF:   i32 = 0;
const LWING_OFF:  i32 = VPB * 4;
const RWING_OFF:  i32 = VPB * 5;
const LLEG_OFF:   i32 = VPB * 6;

// Pig mesh layout:
//   [0]   Body
//   [1]   Head
//   [2]   Snout
//   [3]   Front-left leg   ← animated
//   [4]   Front-right leg  ← animated (opposite phase)
//   [5]   Back-left leg    ← animated (opposite to front-left)
//   [6]   Back-right leg   ← animated (opposite to front-right)
const PIG_STATIC_CNT: i32 = VPB * 3; // body + head + snout (drawn as one call)
const PIG_FL_LEG: i32 = VPB * 3;
const PIG_FR_LEG: i32 = VPB * 4;
const PIG_BL_LEG: i32 = VPB * 5;
const PIG_BR_LEG: i32 = VPB * 6;

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
const CAT_STATIC_CNT: i32 = VPB * 5; // body + head + 2 ears + tail
const CAT_FL_LEG: i32 = VPB * 5;
const CAT_FR_LEG: i32 = VPB * 6;
const CAT_BL_LEG: i32 = VPB * 7;
const CAT_BR_LEG: i32 = VPB * 8;

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
const COW_STATIC_CNT: i32 = VPB * 5; // body + head + snout + 2 horns
const COW_FL_LEG: i32 = VPB * 5;
const COW_FR_LEG: i32 = VPB * 6;
const COW_BL_LEG: i32 = VPB * 7;
const COW_BR_LEG: i32 = VPB * 8;

// Skeleton mesh layout:
//   [0]   Head  (skull)       — static
//   [1]   Body  (ribcage)     — static
//   [2]   Left arm            ← animated (swings on X around shoulder)
//   [3]   Right arm           ← animated (opposite phase)
//   [4]   Left leg            ← animated (swings on X around hip)
//   [5]   Right leg           ← animated (opposite to left arm)
const SKEL_STATIC_CNT: i32 = VPB * 2; // head + body
const SKEL_LARM: i32 = VPB * 2;
const SKEL_RARM: i32 = VPB * 3;
const SKEL_LLEG: i32 = VPB * 4;
const SKEL_RLEG: i32 = VPB * 5;

fn build_cat_mesh() -> Vec<f32> {
    let mut v = Vec::new();
    let fur  = [0.80f32, 0.50, 0.18]; // orange tabby body
    let head = [0.83f32, 0.54, 0.22]; // slightly lighter head
    let ear  = [0.90f32, 0.64, 0.50]; // pale inner ear
    let leg  = [0.77f32, 0.47, 0.16]; // slightly darker legs

    // Body
    add_box(&mut v, -0.17, 0.22, -0.22,  0.17, 0.52,  0.22, fur[0], fur[1], fur[2]);
    // Head (jutting forward -Z)
    add_box(&mut v, -0.13, 0.45, -0.36,  0.13, 0.72, -0.18, head[0], head[1], head[2]);
    // Left ear
    add_box(&mut v, -0.13, 0.68, -0.34, -0.05, 0.82, -0.24, ear[0], ear[1], ear[2]);
    // Right ear
    add_box(&mut v,  0.05, 0.68, -0.34,  0.13, 0.82, -0.24, ear[0], ear[1], ear[2]);
    // Tail (behind body)
    add_box(&mut v, -0.04, 0.22,  0.22,  0.04, 0.58,  0.30, fur[0], fur[1], fur[2]);
    // Front-left leg
    add_box(&mut v, -0.14, 0.00, -0.16, -0.06, 0.22, -0.07, leg[0], leg[1], leg[2]);
    // Front-right leg
    add_box(&mut v,  0.06, 0.00, -0.16,  0.14, 0.22, -0.07, leg[0], leg[1], leg[2]);
    // Back-left leg
    add_box(&mut v, -0.14, 0.00,  0.07, -0.06, 0.22,  0.16, leg[0], leg[1], leg[2]);
    // Back-right leg
    add_box(&mut v,  0.06, 0.00,  0.07,  0.14, 0.22,  0.16, leg[0], leg[1], leg[2]);

    v
}

fn build_skeleton_mesh() -> Vec<f32> {
    let mut v = Vec::new();
    let bn = [0.90f32, 0.88, 0.82]; // bone ivory
    let sk = [0.92f32, 0.90, 0.86]; // skull slightly lighter

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

pub struct EntityRenderer {
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
        let (cat_vao, cat_vbo) = Self::upload_mesh(&build_cat_mesh());
        let (cow_vao, cow_vbo) = Self::upload_mesh(&build_cow_mesh());
        let (workbench_vao, workbench_vbo) = Self::upload_mesh(&build_workbench_mesh());
        let (bed_vao, bed_vbo) = Self::upload_mesh(&build_bed_mesh());
        let (furnace_vao, furnace_vbo) = Self::upload_mesh(&build_furnace_mesh());

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

            EntityRenderer {
                vao, vbo, pig_vao, pig_vbo, skel_vao, skel_vbo, cat_vao, cat_vbo, cow_vao, cow_vbo,
                workbench_vao, workbench_vbo, bed_vao, bed_vbo, furnace_vao, furnace_vbo, shader,
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

    pub fn draw_chickens(&self, chickens: &[Chicken], view: &glam::Mat4, projection: &glam::Mat4,
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
            gl::BindVertexArray(self.vao);

            for chicken in chickens {
                gl::Uniform1f(self.block_light_loc, chicken.block_light);
                // Base model: translate to world position, rotate to face yaw
                let rot_y = -(chicken.yaw.to_radians() + FRAC_PI_2);
                let model = glam::Mat4::from_translation(glam::Vec3::from(chicken.position))
                    * glam::Mat4::from_rotation_y(rot_y);
                let mvp = *projection * *view * model;

                gl::UniformMatrix4fv(self.model_loc, 1, gl::FALSE, model.to_cols_array().as_ptr());
                // Static parts: body + head + beak + wattle (4 boxes = 144 verts)
                gl::UniformMatrix4fv(self.mvp_loc, 1, gl::FALSE, mvp.to_cols_array().as_ptr());
                gl::DrawArrays(gl::TRIANGLES, BODY_OFF, VPB * 4);
                // Legs (2 boxes)
                gl::DrawArrays(gl::TRIANGLES, LLEG_OFF, VPB * 2);

                // Wings: flap around the hinge edge (rotation_z at attachment x)
                let flap = (chicken.anim_time * 9.0).sin() * 0.45;

                // Left wing: hinge at top-left of wing attachment
                let lp = glam::Vec3::new(-0.20, 0.68, 0.0);
                let lw = glam::Mat4::from_translation(lp)
                    * glam::Mat4::from_rotation_z(flap)
                    * glam::Mat4::from_translation(-lp);
                gl::UniformMatrix4fv(self.mvp_loc, 1, gl::FALSE,
                    (*projection * *view * model * lw).to_cols_array().as_ptr());
                gl::DrawArrays(gl::TRIANGLES, LWING_OFF, VPB);

                // Right wing: mirror flap direction
                let rp = glam::Vec3::new(0.20, 0.68, 0.0);
                let rw = glam::Mat4::from_translation(rp)
                    * glam::Mat4::from_rotation_z(-flap)
                    * glam::Mat4::from_translation(-rp);
                gl::UniformMatrix4fv(self.mvp_loc, 1, gl::FALSE,
                    (*projection * *view * model * rw).to_cols_array().as_ptr());
                gl::DrawArrays(gl::TRIANGLES, RWING_OFF, VPB);
            }

            gl::BindVertexArray(0);
            gl::Enable(gl::CULL_FACE);
        }

        for chicken in chickens {
            let frac = chicken.health / chicken.def.max_health;
            if frac < 1.0 {
                self.draw_health_bar(chicken.position, frac, 1.05, view, projection);
            }
        }
    }

    pub fn draw_pigs(&self, pigs: &[Pig], view: &glam::Mat4, projection: &glam::Mat4,
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
            gl::BindVertexArray(self.pig_vao);

            for pig in pigs {
                gl::Uniform1f(self.block_light_loc, pig.block_light);
                let rot_y = -(pig.yaw.to_radians() + FRAC_PI_2);
                let model = glam::Mat4::from_translation(glam::Vec3::from(pig.position))
                    * glam::Mat4::from_rotation_y(rot_y);
                let mvp = *projection * *view * model;

                gl::UniformMatrix4fv(self.model_loc, 1, gl::FALSE, model.to_cols_array().as_ptr());
                // Static parts: body + head + snout
                gl::UniformMatrix4fv(self.mvp_loc, 1, gl::FALSE, mvp.to_cols_array().as_ptr());
                gl::DrawArrays(gl::TRIANGLES, 0, PIG_STATIC_CNT);

                // Leg animation: front-left & back-right swing together, front-right & back-left opposite
                let swing = (pig.anim_time * 6.5 * (pig.move_speed_norm())).sin() * 0.55;

                let pivot_y = 0.35_f32;

                // Front-left leg (pivot at top-center of leg: x=-0.135, y=0.35, z=-0.205)
                let fl_p = glam::Vec3::new(-0.135, pivot_y, -0.205);
                let fl_m = glam::Mat4::from_translation(fl_p)
                    * glam::Mat4::from_rotation_x(swing)
                    * glam::Mat4::from_translation(-fl_p);
                gl::UniformMatrix4fv(self.mvp_loc, 1, gl::FALSE,
                    (*projection * *view * model * fl_m).to_cols_array().as_ptr());
                gl::DrawArrays(gl::TRIANGLES, PIG_FL_LEG, VPB);

                // Front-right leg (opposite phase)
                let fr_p = glam::Vec3::new(0.135, pivot_y, -0.205);
                let fr_m = glam::Mat4::from_translation(fr_p)
                    * glam::Mat4::from_rotation_x(-swing)
                    * glam::Mat4::from_translation(-fr_p);
                gl::UniformMatrix4fv(self.mvp_loc, 1, gl::FALSE,
                    (*projection * *view * model * fr_m).to_cols_array().as_ptr());
                gl::DrawArrays(gl::TRIANGLES, PIG_FR_LEG, VPB);

                // Back-left leg (opposite to front-left)
                let bl_p = glam::Vec3::new(-0.135, pivot_y, 0.205);
                let bl_m = glam::Mat4::from_translation(bl_p)
                    * glam::Mat4::from_rotation_x(-swing)
                    * glam::Mat4::from_translation(-bl_p);
                gl::UniformMatrix4fv(self.mvp_loc, 1, gl::FALSE,
                    (*projection * *view * model * bl_m).to_cols_array().as_ptr());
                gl::DrawArrays(gl::TRIANGLES, PIG_BL_LEG, VPB);

                // Back-right leg (opposite to front-right)
                let br_p = glam::Vec3::new(0.135, pivot_y, 0.205);
                let br_m = glam::Mat4::from_translation(br_p)
                    * glam::Mat4::from_rotation_x(swing)
                    * glam::Mat4::from_translation(-br_p);
                gl::UniformMatrix4fv(self.mvp_loc, 1, gl::FALSE,
                    (*projection * *view * model * br_m).to_cols_array().as_ptr());
                gl::DrawArrays(gl::TRIANGLES, PIG_BR_LEG, VPB);
            }

            gl::BindVertexArray(0);
            gl::Enable(gl::CULL_FACE);
        }

        for pig in pigs {
            let frac = pig.health / pig.def.max_health;
            if frac < 1.0 {
                self.draw_health_bar(pig.position, frac, 1.05, view, projection);
            }
        }
    }

    /// Render all chickens into the currently active shadow cascade.
    pub fn draw_shadows(&self, chickens: &[Chicken], shadow_pass: &ShadowPass) {
        for chicken in chickens {
            let rot_y = -(chicken.yaw.to_radians() + FRAC_PI_2);
            let model = glam::Mat4::from_translation(glam::Vec3::from(chicken.position))
                * glam::Mat4::from_rotation_y(rot_y);
            shadow_pass.draw_solid_mesh(self.vao, 0, VPB * 8, &model);
        }
    }

    pub fn draw_pig_shadows(&self, pigs: &[Pig], shadow_pass: &ShadowPass) {
        for pig in pigs {
            let rot_y = -(pig.yaw.to_radians() + FRAC_PI_2);
            let model = glam::Mat4::from_translation(glam::Vec3::from(pig.position))
                * glam::Mat4::from_rotation_y(rot_y);
            shadow_pass.draw_solid_mesh(self.pig_vao, 0, VPB * 7, &model);
        }
    }

    pub fn draw_skeletons(
        &self, skeletons: &[Skeleton],
        view: &glam::Mat4, projection: &glam::Mat4,
        fog_start: f32, fog_end: f32, screen_w: f32, screen_h: f32, sky_tex: u32,
        fog_override: f32, fog_color_override: glam::Vec3,
        ambient_light: f32, directional_light: f32, sun_dir: glam::Vec3,
        shadow_tex: u32, light_space: &[glam::Mat4; NUM_CASCADES],
        texel_sizes: &[f32; NUM_CASCADES],
        torch_pos: glam::Vec3, torch_strength: f32,
    ) {
        unsafe {
            gl::Disable(gl::CULL_FACE);
            self.bind_frame_uniforms(fog_start, fog_end, screen_w, screen_h, sky_tex,
                fog_override, fog_color_override, ambient_light, directional_light, sun_dir,
                shadow_tex, light_space, texel_sizes, torch_pos, torch_strength);
            gl::BindVertexArray(self.skel_vao);

            for skel in skeletons {
                gl::Uniform1f(self.block_light_loc, skel.block_light);
                let rot_y = -(skel.yaw.to_radians() + FRAC_PI_2);
                let model = glam::Mat4::from_translation(glam::Vec3::from(skel.position))
                    * glam::Mat4::from_rotation_y(rot_y);
                let mvp = *projection * *view * model;

                gl::UniformMatrix4fv(self.model_loc, 1, gl::FALSE, model.to_cols_array().as_ptr());
                // Static: head + body
                gl::UniformMatrix4fv(self.mvp_loc, 1, gl::FALSE, mvp.to_cols_array().as_ptr());
                gl::DrawArrays(gl::TRIANGLES, 0, SKEL_STATIC_CNT);

                // Limb animation
                let walk_swing = (skel.anim_time * 7.0 * skel.move_speed_norm()).sin() * 0.60;
                let pivot_arm_y = 1.45_f32;
                let pivot_leg_y = 0.65_f32;

                // Attack arm swing: both arms lunge forward together.
                let (la_angle, ra_angle) = if skel.attack_anim > 0.0 {
                    let t = 1.0 - skel.attack_anim / 0.5;
                    let a = (std::f32::consts::PI * t).sin() * 1.4;
                    (a, a)
                } else {
                    (walk_swing, -walk_swing)
                };

                // Left arm
                let la_p = glam::Vec3::new(-0.21, pivot_arm_y, 0.0);
                let la_m = glam::Mat4::from_translation(la_p)
                    * glam::Mat4::from_rotation_x(la_angle)
                    * glam::Mat4::from_translation(-la_p);
                gl::UniformMatrix4fv(self.mvp_loc, 1, gl::FALSE,
                    (*projection * *view * model * la_m).to_cols_array().as_ptr());
                gl::DrawArrays(gl::TRIANGLES, SKEL_LARM, VPB);

                // Right arm
                let ra_p = glam::Vec3::new(0.21, pivot_arm_y, 0.0);
                let ra_m = glam::Mat4::from_translation(ra_p)
                    * glam::Mat4::from_rotation_x(ra_angle)
                    * glam::Mat4::from_translation(-ra_p);
                gl::UniformMatrix4fv(self.mvp_loc, 1, gl::FALSE,
                    (*projection * *view * model * ra_m).to_cols_array().as_ptr());
                gl::DrawArrays(gl::TRIANGLES, SKEL_RARM, VPB);

                // Left leg (opposite to left arm → same as right arm)
                let ll_p = glam::Vec3::new(-0.08, pivot_leg_y, 0.0);
                let ll_m = glam::Mat4::from_translation(ll_p)
                    * glam::Mat4::from_rotation_x(-walk_swing)
                    * glam::Mat4::from_translation(-ll_p);
                gl::UniformMatrix4fv(self.mvp_loc, 1, gl::FALSE,
                    (*projection * *view * model * ll_m).to_cols_array().as_ptr());
                gl::DrawArrays(gl::TRIANGLES, SKEL_LLEG, VPB);

                // Right leg (opposite to right arm → same as left arm)
                let rl_p = glam::Vec3::new(0.08, pivot_leg_y, 0.0);
                let rl_m = glam::Mat4::from_translation(rl_p)
                    * glam::Mat4::from_rotation_x(walk_swing)
                    * glam::Mat4::from_translation(-rl_p);
                gl::UniformMatrix4fv(self.mvp_loc, 1, gl::FALSE,
                    (*projection * *view * model * rl_m).to_cols_array().as_ptr());
                gl::DrawArrays(gl::TRIANGLES, SKEL_RLEG, VPB);
            }

            gl::BindVertexArray(0);
            gl::Enable(gl::CULL_FACE);
        }

        for skel in skeletons {
            let frac = skel.health / skel.def.max_health;
            if frac < 1.0 {
                self.draw_health_bar(skel.position, frac, 1.95, view, projection);
            }
        }
    }

    pub fn draw_skeleton_shadows(&self, skeletons: &[Skeleton], shadow_pass: &ShadowPass) {
        for skel in skeletons {
            let rot_y = -(skel.yaw.to_radians() + FRAC_PI_2);
            let model = glam::Mat4::from_translation(glam::Vec3::from(skel.position))
                * glam::Mat4::from_rotation_y(rot_y);
            shadow_pass.draw_solid_mesh(self.skel_vao, 0, VPB * 6, &model);
        }
    }

    pub fn draw_cats(&self, cats: &[Cat], view: &glam::Mat4, projection: &glam::Mat4,
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
            gl::BindVertexArray(self.cat_vao);

            for cat in cats {
                gl::Uniform1f(self.block_light_loc, cat.block_light);
                let rot_y = -(cat.yaw.to_radians() + FRAC_PI_2);
                // When sitting, lower the model slightly so the cat crouches
                let sit_offset = if cat.sitting { -0.12 } else { 0.0 };
                let pos = glam::Vec3::new(cat.position[0], cat.position[1] + sit_offset, cat.position[2]);
                let model = glam::Mat4::from_translation(pos)
                    * glam::Mat4::from_rotation_y(rot_y);
                let mvp = *projection * *view * model;

                gl::UniformMatrix4fv(self.model_loc, 1, gl::FALSE, model.to_cols_array().as_ptr());
                // Static parts: body + head + ears + tail
                gl::UniformMatrix4fv(self.mvp_loc, 1, gl::FALSE, mvp.to_cols_array().as_ptr());
                gl::DrawArrays(gl::TRIANGLES, 0, CAT_STATIC_CNT);

                if cat.sitting {
                    // Draw legs as static when sitting
                    gl::DrawArrays(gl::TRIANGLES, CAT_FL_LEG, VPB * 4);
                } else {
                    let swing = (cat.anim_time * 7.0 * cat.move_speed_norm()).sin() * 0.50;
                    let pivot_y = 0.22_f32;

                    let fl_p = glam::Vec3::new(-0.10, pivot_y, -0.115);
                    let fl_m = glam::Mat4::from_translation(fl_p)
                        * glam::Mat4::from_rotation_x(swing)
                        * glam::Mat4::from_translation(-fl_p);
                    gl::UniformMatrix4fv(self.mvp_loc, 1, gl::FALSE,
                        (*projection * *view * model * fl_m).to_cols_array().as_ptr());
                    gl::DrawArrays(gl::TRIANGLES, CAT_FL_LEG, VPB);

                    let fr_p = glam::Vec3::new(0.10, pivot_y, -0.115);
                    let fr_m = glam::Mat4::from_translation(fr_p)
                        * glam::Mat4::from_rotation_x(-swing)
                        * glam::Mat4::from_translation(-fr_p);
                    gl::UniformMatrix4fv(self.mvp_loc, 1, gl::FALSE,
                        (*projection * *view * model * fr_m).to_cols_array().as_ptr());
                    gl::DrawArrays(gl::TRIANGLES, CAT_FR_LEG, VPB);

                    let bl_p = glam::Vec3::new(-0.10, pivot_y,  0.115);
                    let bl_m = glam::Mat4::from_translation(bl_p)
                        * glam::Mat4::from_rotation_x(-swing)
                        * glam::Mat4::from_translation(-bl_p);
                    gl::UniformMatrix4fv(self.mvp_loc, 1, gl::FALSE,
                        (*projection * *view * model * bl_m).to_cols_array().as_ptr());
                    gl::DrawArrays(gl::TRIANGLES, CAT_BL_LEG, VPB);

                    let br_p = glam::Vec3::new(0.10, pivot_y,  0.115);
                    let br_m = glam::Mat4::from_translation(br_p)
                        * glam::Mat4::from_rotation_x(swing)
                        * glam::Mat4::from_translation(-br_p);
                    gl::UniformMatrix4fv(self.mvp_loc, 1, gl::FALSE,
                        (*projection * *view * model * br_m).to_cols_array().as_ptr());
                    gl::DrawArrays(gl::TRIANGLES, CAT_BR_LEG, VPB);
                }
            }

            gl::BindVertexArray(0);
            gl::Enable(gl::CULL_FACE);
        }

        for cat in cats {
            let frac = cat.health / cat.def.max_health;
            if frac < 1.0 {
                self.draw_health_bar(cat.position, frac, 0.90, view, projection);
            }
        }
    }

    pub fn draw_cat_shadows(&self, cats: &[Cat], shadow_pass: &ShadowPass) {
        for cat in cats {
            let rot_y = -(cat.yaw.to_radians() + FRAC_PI_2);
            let sit_offset = if cat.sitting { -0.12 } else { 0.0 };
            let pos = glam::Vec3::new(cat.position[0], cat.position[1] + sit_offset, cat.position[2]);
            let model = glam::Mat4::from_translation(pos)
                * glam::Mat4::from_rotation_y(rot_y);
            shadow_pass.draw_solid_mesh(self.cat_vao, 0, VPB * 9, &model);
        }
    }

    pub fn draw_cows(&self, cows: &[Cow], view: &glam::Mat4, projection: &glam::Mat4,
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
            gl::BindVertexArray(self.cow_vao);

            for cow in cows {
                gl::Uniform1f(self.block_light_loc, cow.block_light);
                let rot_y = -(cow.yaw.to_radians() + FRAC_PI_2);
                let model = glam::Mat4::from_translation(glam::Vec3::from(cow.position))
                    * glam::Mat4::from_rotation_y(rot_y);
                let mvp = *projection * *view * model;

                gl::UniformMatrix4fv(self.model_loc, 1, gl::FALSE, model.to_cols_array().as_ptr());
                // Static parts: body + head + snout + 2 horns
                gl::UniformMatrix4fv(self.mvp_loc, 1, gl::FALSE, mvp.to_cols_array().as_ptr());
                gl::DrawArrays(gl::TRIANGLES, 0, COW_STATIC_CNT);

                let swing = (cow.anim_time * 5.5 * cow.move_speed_norm()).sin() * 0.55;
                let pivot_y = 0.40_f32;

                // Front-left leg (swings with back-right)
                let fl_p = glam::Vec3::new(-0.16, pivot_y, -0.23);
                let fl_m = glam::Mat4::from_translation(fl_p)
                    * glam::Mat4::from_rotation_x(swing)
                    * glam::Mat4::from_translation(-fl_p);
                gl::UniformMatrix4fv(self.mvp_loc, 1, gl::FALSE,
                    (*projection * *view * model * fl_m).to_cols_array().as_ptr());
                gl::DrawArrays(gl::TRIANGLES, COW_FL_LEG, VPB);

                // Front-right leg (opposite phase)
                let fr_p = glam::Vec3::new(0.16, pivot_y, -0.23);
                let fr_m = glam::Mat4::from_translation(fr_p)
                    * glam::Mat4::from_rotation_x(-swing)
                    * glam::Mat4::from_translation(-fr_p);
                gl::UniformMatrix4fv(self.mvp_loc, 1, gl::FALSE,
                    (*projection * *view * model * fr_m).to_cols_array().as_ptr());
                gl::DrawArrays(gl::TRIANGLES, COW_FR_LEG, VPB);

                // Back-left leg (opposite to front-left)
                let bl_p = glam::Vec3::new(-0.16, pivot_y, 0.23);
                let bl_m = glam::Mat4::from_translation(bl_p)
                    * glam::Mat4::from_rotation_x(-swing)
                    * glam::Mat4::from_translation(-bl_p);
                gl::UniformMatrix4fv(self.mvp_loc, 1, gl::FALSE,
                    (*projection * *view * model * bl_m).to_cols_array().as_ptr());
                gl::DrawArrays(gl::TRIANGLES, COW_BL_LEG, VPB);

                // Back-right leg (opposite to front-right)
                let br_p = glam::Vec3::new(0.16, pivot_y, 0.23);
                let br_m = glam::Mat4::from_translation(br_p)
                    * glam::Mat4::from_rotation_x(swing)
                    * glam::Mat4::from_translation(-br_p);
                gl::UniformMatrix4fv(self.mvp_loc, 1, gl::FALSE,
                    (*projection * *view * model * br_m).to_cols_array().as_ptr());
                gl::DrawArrays(gl::TRIANGLES, COW_BR_LEG, VPB);
            }

            gl::BindVertexArray(0);
            gl::Enable(gl::CULL_FACE);
        }

        for cow in cows {
            let frac = cow.health / cow.def.max_health;
            if frac < 1.0 {
                self.draw_health_bar(cow.position, frac, 1.55, view, projection);
            }
        }
    }

    pub fn draw_cow_shadows(&self, cows: &[Cow], shadow_pass: &ShadowPass) {
        for cow in cows {
            let rot_y = -(cow.yaw.to_radians() + FRAC_PI_2);
            let model = glam::Mat4::from_translation(glam::Vec3::from(cow.position))
                * glam::Mat4::from_rotation_y(rot_y);
            shadow_pass.draw_solid_mesh(self.cow_vao, 0, VPB * 9, &model);
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
            gl::DeleteProgram(self.shader);
            gl::DeleteVertexArrays(1, &self.bar_vao);
            gl::DeleteBuffers(1, &self.bar_vbo);
            gl::DeleteProgram(self.bar_shader);
        }
    }
}
