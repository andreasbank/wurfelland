use std::mem;
use std::os::raw::c_void;
use std::f32::consts::FRAC_PI_2;
use crate::renderer::utils::{compile_shader, link_program};
use crate::renderer::shadow_pass::ShadowPass;
use crate::world::entity::{Chicken, Pig};

// Vertex format: [x, y, z, r, g, b] — 6 floats
const STRIDE: usize = 6;

fn push_vertex(verts: &mut Vec<f32>, x: f32, y: f32, z: f32, r: f32, g: f32, b: f32) {
    verts.extend_from_slice(&[x, y, z, r, g, b]);
}

fn add_face(verts: &mut Vec<f32>, p: [[f32; 3]; 4], shade: f32, r: f32, g: f32, b: f32) {
    for &i in &[0usize, 1, 2, 0, 2, 3] {
        push_vertex(verts, p[i][0], p[i][1], p[i][2], r * shade, g * shade, b * shade);
    }
}

fn add_box(verts: &mut Vec<f32>, x0: f32, y0: f32, z0: f32, x1: f32, y1: f32, z1: f32,
           r: f32, g: f32, b: f32) {
    add_face(verts, [[x0,y1,z0],[x1,y1,z0],[x1,y1,z1],[x0,y1,z1]], 1.00, r, g, b); // top
    add_face(verts, [[x0,y0,z1],[x1,y0,z1],[x1,y0,z0],[x0,y0,z0]], 0.50, r, g, b); // bottom
    add_face(verts, [[x0,y0,z1],[x1,y0,z1],[x1,y1,z1],[x0,y1,z1]], 0.80, r, g, b); // front +Z
    add_face(verts, [[x1,y0,z0],[x0,y0,z0],[x0,y1,z0],[x1,y1,z0]], 0.80, r, g, b); // back  -Z
    add_face(verts, [[x0,y0,z0],[x0,y0,z1],[x0,y1,z1],[x0,y1,z0]], 0.65, r, g, b); // left  -X
    add_face(verts, [[x1,y0,z1],[x1,y0,z0],[x1,y1,z0],[x1,y1,z1]], 0.65, r, g, b); // right +X
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

pub struct EntityRenderer {
    vao: u32,
    vbo: u32,
    pig_vao: u32,
    pig_vbo: u32,
    shader: u32,
    mvp_loc: i32,
    fog_start_loc: i32,
    fog_end_loc: i32,
    screen_size_loc: i32,
    sky_sampler_loc: i32,
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
            gl::BindBuffer(gl::ARRAY_BUFFER, 0);
            gl::BindVertexArray(0);
            (vao, vbo)
        }
    }

    pub fn new() -> Self {
        let (vao, vbo) = Self::upload_mesh(&build_chicken_mesh());
        let (pig_vao, pig_vbo) = Self::upload_mesh(&build_pig_mesh());

        unsafe {

            let vert = compile_shader(gl::VERTEX_SHADER, r#"#version 330 core
                layout(location = 0) in vec3 aPos;
                layout(location = 1) in vec3 aColor;
                uniform mat4 mvp;
                out vec3 vColor;
                out float fragDist;
                void main() {
                    gl_Position = mvp * vec4(aPos, 1.0);
                    vColor = aColor;
                    fragDist = gl_Position.w;
                }"#).unwrap();

            let frag = compile_shader(gl::FRAGMENT_SHADER, r#"#version 330 core
                in vec3 vColor;
                in float fragDist;
                out vec4 FragColor;
                uniform sampler2D u_sky_sampler;
                uniform vec2 u_screen_size;
                uniform float u_fog_start;
                uniform float u_fog_end;
                void main() {
                    vec2 screenUV = gl_FragCoord.xy / u_screen_size;
                    vec3 skyFog = texture(u_sky_sampler, screenUV).rgb;
                    float fog_factor = clamp((fragDist - u_fog_start) / (u_fog_end - u_fog_start), 0.0, 1.0);
                    FragColor = vec4(mix(vColor, skyFog, fog_factor), 1.0);
                }
            "#).unwrap();

            let shader = link_program(vert, frag).unwrap();
            let mvp_loc          = gl::GetUniformLocation(shader, c"mvp".as_ptr());
            let fog_start_loc    = gl::GetUniformLocation(shader, c"u_fog_start".as_ptr());
            let fog_end_loc      = gl::GetUniformLocation(shader, c"u_fog_end".as_ptr());
            let screen_size_loc  = gl::GetUniformLocation(shader, c"u_screen_size".as_ptr());
            let sky_sampler_loc  = gl::GetUniformLocation(shader, c"u_sky_sampler".as_ptr());

            EntityRenderer { vao, vbo, pig_vao, pig_vbo, shader, mvp_loc, fog_start_loc, fog_end_loc, screen_size_loc, sky_sampler_loc }
        }
    }

    pub fn draw_chickens(&self, chickens: &[Chicken], view: &glam::Mat4, projection: &glam::Mat4,
                         fog_start: f32, fog_end: f32, screen_w: f32, screen_h: f32, sky_tex: u32) {
        unsafe {
            gl::Disable(gl::CULL_FACE);
            gl::UseProgram(self.shader);
            gl::Uniform1f(self.fog_start_loc, fog_start);
            gl::Uniform1f(self.fog_end_loc, fog_end);
            gl::Uniform2f(self.screen_size_loc, screen_w, screen_h);
            gl::Uniform1i(self.sky_sampler_loc, 4);
            gl::ActiveTexture(gl::TEXTURE4);
            gl::BindTexture(gl::TEXTURE_2D, sky_tex);
            gl::BindVertexArray(self.vao);

            for chicken in chickens {
                // Base model: translate to world position, rotate to face yaw
                let rot_y = -(chicken.yaw.to_radians() + FRAC_PI_2);
                let model = glam::Mat4::from_translation(glam::Vec3::from(chicken.position))
                    * glam::Mat4::from_rotation_y(rot_y);
                let mvp = *projection * *view * model;

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
    }

    pub fn draw_pigs(&self, pigs: &[Pig], view: &glam::Mat4, projection: &glam::Mat4,
                     fog_start: f32, fog_end: f32, screen_w: f32, screen_h: f32, sky_tex: u32) {
        unsafe {
            gl::Disable(gl::CULL_FACE);
            gl::UseProgram(self.shader);
            gl::Uniform1f(self.fog_start_loc, fog_start);
            gl::Uniform1f(self.fog_end_loc, fog_end);
            gl::Uniform2f(self.screen_size_loc, screen_w, screen_h);
            gl::Uniform1i(self.sky_sampler_loc, 4);
            gl::ActiveTexture(gl::TEXTURE4);
            gl::BindTexture(gl::TEXTURE_2D, sky_tex);
            gl::BindVertexArray(self.pig_vao);

            for pig in pigs {
                let rot_y = -(pig.yaw.to_radians() + FRAC_PI_2);
                let model = glam::Mat4::from_translation(glam::Vec3::from(pig.position))
                    * glam::Mat4::from_rotation_y(rot_y);
                let mvp = *projection * *view * model;

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
}

impl Drop for EntityRenderer {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteVertexArrays(1, &self.vao);
            gl::DeleteBuffers(1, &self.vbo);
            gl::DeleteVertexArrays(1, &self.pig_vao);
            gl::DeleteBuffers(1, &self.pig_vbo);
            gl::DeleteProgram(self.shader);
        }
    }
}
