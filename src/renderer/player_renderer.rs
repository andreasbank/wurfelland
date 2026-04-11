use std::mem;
use std::os::raw::c_void;
use std::f32::consts::FRAC_PI_2;
use crate::renderer::utils::{compile_shader, link_program};

// Vertex format: [x, y, z, shade, u, v] — 6 floats
const STRIDE: usize = 6;

fn push_vertex(verts: &mut Vec<f32>, x: f32, y: f32, z: f32, shade: f32, u: f32, v: f32) {
    verts.extend_from_slice(&[x, y, z, shade, u, v]);
}

// p0..p3 are the four corners in CCW winding order when viewed from outside
fn add_face(verts: &mut Vec<f32>, p: [[f32; 3]; 4], shade: f32) {
    let uv = [[0.0f32, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
    push_vertex(verts, p[0][0], p[0][1], p[0][2], shade, uv[0][0], uv[0][1]);
    push_vertex(verts, p[1][0], p[1][1], p[1][2], shade, uv[1][0], uv[1][1]);
    push_vertex(verts, p[2][0], p[2][1], p[2][2], shade, uv[2][0], uv[2][1]);
    push_vertex(verts, p[0][0], p[0][1], p[0][2], shade, uv[0][0], uv[0][1]);
    push_vertex(verts, p[2][0], p[2][1], p[2][2], shade, uv[2][0], uv[2][1]);
    push_vertex(verts, p[3][0], p[3][1], p[3][2], shade, uv[3][0], uv[3][1]);
}

fn add_box(verts: &mut Vec<f32>, x0: f32, y0: f32, z0: f32, x1: f32, y1: f32, z1: f32) {
    // Top (+Y)
    add_face(verts, [[x0,y1,z0],[x1,y1,z0],[x1,y1,z1],[x0,y1,z1]], 1.0);
    // Bottom (-Y)
    add_face(verts, [[x0,y0,z1],[x1,y0,z1],[x1,y0,z0],[x0,y0,z0]], 0.5);
    // Front (+Z)
    add_face(verts, [[x0,y0,z1],[x1,y0,z1],[x1,y1,z1],[x0,y1,z1]], 0.8);
    // Back (-Z)
    add_face(verts, [[x1,y0,z0],[x0,y0,z0],[x0,y1,z0],[x1,y1,z0]], 0.8);
    // Left (-X)
    add_face(verts, [[x0,y0,z0],[x0,y0,z1],[x0,y1,z1],[x0,y1,z0]], 0.65);
    // Right (+X)
    add_face(verts, [[x1,y0,z1],[x1,y0,z0],[x1,y1,z0],[x1,y1,z1]], 0.65);
}

fn build_player_mesh() -> Vec<f32> {
    let mut v = Vec::new();
    // Head  (0.4 × 0.4 × 0.4, centered, top at 1.85)
    add_box(&mut v, -0.20,  1.45, -0.20,  0.20,  1.85,  0.20);
    // Torso (0.4 wide, 0.7 tall, 0.2 deep)
    add_box(&mut v, -0.20,  0.75, -0.10,  0.20,  1.45,  0.10);
    // Left arm
    add_box(&mut v, -0.35,  0.75, -0.10, -0.20,  1.45,  0.10);
    // Right arm
    add_box(&mut v,  0.20,  0.75, -0.10,  0.35,  1.45,  0.10);
    // Left leg
    add_box(&mut v, -0.15,  0.00, -0.10,  0.00,  0.75,  0.10);
    // Right leg
    add_box(&mut v,  0.00,  0.00, -0.10,  0.15,  0.75,  0.10);
    v
}

// 16×16 gray-tiled texture: light gray fill, dark gray 1-px border
fn create_player_texture() -> u32 {
    const SZ: usize = 16;
    let mut px = vec![0u8; SZ * SZ * 4];
    for y in 0..SZ {
        for x in 0..SZ {
            let border = x == 0 || y == 0 || x == SZ - 1 || y == SZ - 1;
            let val: u8 = if border { 110 } else { 205 };
            let i = (y * SZ + x) * 4;
            px[i] = val; px[i+1] = val; px[i+2] = val; px[i+3] = 255;
        }
    }
    unsafe {
        let mut id = 0u32;
        gl::GenTextures(1, &mut id);
        gl::BindTexture(gl::TEXTURE_2D, id);
        gl::TexImage2D(gl::TEXTURE_2D, 0, gl::RGBA as i32,
            SZ as i32, SZ as i32, 0,
            gl::RGBA, gl::UNSIGNED_BYTE, px.as_ptr() as *const _);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as i32);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as i32);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::REPEAT as i32);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::REPEAT as i32);
        gl::BindTexture(gl::TEXTURE_2D, 0);
        id
    }
}

pub enum PlayerDrawMode {
    ArmsOnly,
}

// Vertex layout of the mesh (each box = 6 faces × 6 verts = 36 verts):
//   [0..36)   head
//   [36..72)  torso
//   [72..108) left arm
//   [108..144) right arm
//   [144..180) left leg
//   [180..216) right leg
const HEAD_VERTS:  i32 = 36;
const TORSO_VERTS: i32 = 36;
const ARM_VERTS:   i32 = 36; // per arm
const ARMS_START:  i32 = HEAD_VERTS + TORSO_VERTS;

pub struct PlayerRenderer {
    vao: u32,
    vbo: u32,
    shader: u32,
    mvp_loc: i32,
    tex_id: u32,
}

impl PlayerRenderer {
    pub fn new() -> Self {
        let mesh = build_player_mesh();

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
            // attrib 0: position (vec3)
            gl::VertexAttribPointer(0, 3, gl::FLOAT, gl::FALSE, stride, std::ptr::null());
            gl::EnableVertexAttribArray(0);
            // attrib 1: shade (float)
            gl::VertexAttribPointer(1, 1, gl::FLOAT, gl::FALSE, stride,
                (3 * mem::size_of::<f32>()) as *const c_void);
            gl::EnableVertexAttribArray(1);
            // attrib 2: uv (vec2)
            gl::VertexAttribPointer(2, 2, gl::FLOAT, gl::FALSE, stride,
                (4 * mem::size_of::<f32>()) as *const c_void);
            gl::EnableVertexAttribArray(2);

            gl::BindBuffer(gl::ARRAY_BUFFER, 0);
            gl::BindVertexArray(0);

            let vert = compile_shader(gl::VERTEX_SHADER, r#"#version 330 core
                layout(location = 0) in vec3 aPos;
                layout(location = 1) in float aShade;
                layout(location = 2) in vec2 aUV;
                uniform mat4 mvp;
                out float shade;
                out vec2 uv;
                void main() {
                    gl_Position = mvp * vec4(aPos, 1.0);
                    shade = aShade;
                    uv = aUV;
                }"#).unwrap();

            let frag = compile_shader(gl::FRAGMENT_SHADER, r#"#version 330 core
                in float shade;
                in vec2 uv;
                out vec4 FragColor;
                uniform sampler2D tex;
                void main() {
                    FragColor = vec4(texture(tex, uv).rgb * shade, 1.0);
                }"#).unwrap();

            let shader = link_program(vert, frag).unwrap();
            let mvp_loc = gl::GetUniformLocation(shader, c"mvp".as_ptr());
            let tex_id = create_player_texture();

            PlayerRenderer { vao, vbo, shader, mvp_loc, tex_id }
        }
    }

    /// Draw a player model at `position` (feet coords) rotated by `yaw` (degrees).
    /// `swing_angle`: right-arm rotation in radians around the shoulder (negative = swing forward/down).
    pub fn draw(&self, position: [f32; 3], yaw: f32,
                view: &glam::Mat4, projection: &glam::Mat4,
                mode: PlayerDrawMode, swing_angle: f32) {
        let rot_angle = -(yaw.to_radians() + FRAC_PI_2);
        let model = glam::Mat4::from_translation(glam::Vec3::from(position))
            * glam::Mat4::from_rotation_y(rot_angle);
        let mvp = *projection * *view * model;

        unsafe {
            gl::Disable(gl::CULL_FACE);
            gl::UseProgram(self.shader);
            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, self.tex_id);
            gl::BindVertexArray(self.vao);
            let _ = mode;

            // Left arm — no swing
            gl::UniformMatrix4fv(self.mvp_loc, 1, gl::FALSE, mvp.to_cols_array().as_ptr());
            gl::DrawArrays(gl::TRIANGLES, ARMS_START, ARM_VERTS);

            // Right arm — pivot-rotate around shoulder joint (model-local x=0.275, y=1.45, z=0)
            let pivot = glam::Vec3::new(0.275, 1.45, 0.0);
            let arm_local = glam::Mat4::from_translation(pivot)
                * glam::Mat4::from_rotation_x(swing_angle)
                * glam::Mat4::from_translation(-pivot);
            let mvp_right = *projection * *view * model * arm_local;
            gl::UniformMatrix4fv(self.mvp_loc, 1, gl::FALSE, mvp_right.to_cols_array().as_ptr());
            gl::DrawArrays(gl::TRIANGLES, ARMS_START + ARM_VERTS, ARM_VERTS);

            gl::BindVertexArray(0);
            gl::Enable(gl::CULL_FACE);
        }
    }
}

impl Drop for PlayerRenderer {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteVertexArrays(1, &self.vao);
            gl::DeleteBuffers(1, &self.vbo);
            gl::DeleteProgram(self.shader);
            gl::DeleteTextures(1, &self.tex_id);
        }
    }
}
