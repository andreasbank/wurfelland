use std::mem;
use std::os::raw::c_void;
use std::f32::consts::FRAC_PI_2;
use crate::renderer::utils::{compile_shader, link_program, load_png_texture};

// Vertex format: [x, y, z, shade, u, v] — 6 floats
const STRIDE: usize = 6;

// Skin atlas is 64×32 pixels.
const TW: f32 = 64.0;
const TH: f32 = 32.0;

fn push_vertex(verts: &mut Vec<f32>, x: f32, y: f32, z: f32, shade: f32, u: f32, v: f32) {
    verts.extend_from_slice(&[x, y, z, shade, u, v]);
}

// Pixel rect (image space, y=0 at top) → [u_min, v_min, u_max, v_max].
// v_min maps to the top of the face, v_max to the bottom.
fn px(x0: u32, y0: u32, x1: u32, y1: u32) -> [f32; 4] {
    [x0 as f32 / TW, y0 as f32 / TH, x1 as f32 / TW, y1 as f32 / TH]
}

// p0..p3: CCW winding viewed from outside. p[0]=BL, p[1]=BR, p[2]=TR, p[3]=TL.
// uv = [u_min, v_min, u_max, v_max] — v_min = top of skin region.
fn add_face(verts: &mut Vec<f32>, p: [[f32; 3]; 4], shade: f32, uv: [f32; 4]) {
    let [u0, v0, u1, v1] = uv;
    push_vertex(verts, p[0][0], p[0][1], p[0][2], shade, u0, v1);
    push_vertex(verts, p[1][0], p[1][1], p[1][2], shade, u1, v1);
    push_vertex(verts, p[2][0], p[2][1], p[2][2], shade, u1, v0);
    push_vertex(verts, p[0][0], p[0][1], p[0][2], shade, u0, v1);
    push_vertex(verts, p[2][0], p[2][1], p[2][2], shade, u1, v0);
    push_vertex(verts, p[3][0], p[3][1], p[3][2], shade, u0, v0);
}

// Emits 6 textured faces for an axis-aligned box.
// UV face order: top(+Y), bottom(-Y), front(+Z), back(-Z), left(-X), right(+X).
fn add_box(verts: &mut Vec<f32>,
           x0: f32, y0: f32, z0: f32, x1: f32, y1: f32, z1: f32,
           top: [f32;4], bot: [f32;4], front: [f32;4], back: [f32;4],
           left: [f32;4], right: [f32;4]) {
    add_face(verts, [[x0,y1,z0],[x1,y1,z0],[x1,y1,z1],[x0,y1,z1]], 1.00, top);
    add_face(verts, [[x0,y0,z1],[x1,y0,z1],[x1,y0,z0],[x0,y0,z0]], 0.50, bot);
    add_face(verts, [[x0,y0,z1],[x1,y0,z1],[x1,y1,z1],[x0,y1,z1]], 0.80, front);
    add_face(verts, [[x1,y0,z0],[x0,y0,z0],[x0,y1,z0],[x1,y1,z0]], 0.80, back);
    add_face(verts, [[x0,y0,z0],[x0,y0,z1],[x0,y1,z1],[x0,y1,z0]], 0.65, left);
    add_face(verts, [[x1,y0,z1],[x1,y0,z0],[x1,y1,z0],[x1,y1,z1]], 0.65, right);
}

// Skin atlas layout (64×32 PNG, y=0 at top):
//
//  HEAD — 8 px per face
//   top    (8,0)→(16,8)     bottom (16,0)→(24,8)
//   right  (0,8)→(8,16)     front  (8,8)→(16,16)
//   left   (16,8)→(24,16)   back   (24,8)→(32,16)
//
//  TORSO — 4W × 7H × 2D px
//   top    (34,0)→(38,2)    bottom (38,0)→(42,2)
//   left   (32,2)→(34,9)    front  (34,2)→(38,9)
//   right  (38,2)→(40,9)    back   (40,2)→(44,9)
//
//  LEFT ARM — 2W × 7H × 2D px (at x=44)
//   top    (46,0)→(48,2)    bottom (48,0)→(50,2)
//   left   (44,2)→(46,9)    front  (46,2)→(48,9)
//   right  (48,2)→(50,9)    back   (50,2)→(52,9)
//
//  RIGHT ARM — 2W × 7H × 2D px (at x=52)
//   top    (54,0)→(56,2)    bottom (56,0)→(58,2)
//   left   (52,2)→(54,9)    front  (54,2)→(56,9)
//   right  (56,2)→(58,9)    back   (58,2)→(60,9)
//
//  LEFT LEG — 4W × 8H × 2D px (at x=0, y=16)
//   top    (2,16)→(6,18)    bottom (6,16)→(10,18)
//   left   (0,18)→(2,26)    front  (2,18)→(6,26)
//   right  (6,18)→(8,26)    back   (8,18)→(12,26)
//
//  RIGHT LEG — 4W × 8H × 2D px (at x=12, y=16)
//   top    (14,16)→(18,18)  bottom (18,16)→(22,18)
//   left   (12,18)→(14,26)  front  (14,18)→(18,26)
//   right  (18,18)→(20,26)  back   (20,18)→(24,26)

fn build_player_mesh() -> Vec<f32> {
    let mut v = Vec::new();

    // Head
    add_box(&mut v, -0.20, 1.45, -0.20,  0.20, 1.85,  0.20,
        px( 8, 0,16, 8), px(16, 0,24, 8),  // top, bottom
        px( 8, 8,16,16), px(24, 8,32,16),  // front, back
        px(16, 8,24,16), px( 0, 8, 8,16),  // left(-X), right(+X)
    );

    // Torso
    add_box(&mut v, -0.20, 0.75, -0.10,  0.20, 1.45,  0.10,
        px(34, 0,38, 2), px(38, 0,42, 2),  // top, bottom
        px(34, 2,38, 9), px(40, 2,44, 9),  // front, back
        px(32, 2,34, 9), px(38, 2,40, 9),  // left(-X), right(+X)
    );

    // Left arm  (-X side)
    add_box(&mut v, -0.35, 0.75, -0.10, -0.20, 1.45,  0.10,
        px(46, 0,48, 2), px(48, 0,50, 2),  // top, bottom
        px(46, 2,48, 9), px(50, 2,52, 9),  // front, back
        px(44, 2,46, 9), px(48, 2,50, 9),  // left(-X), right(+X)
    );

    // Right arm (+X side)
    add_box(&mut v,  0.20, 0.75, -0.10,  0.35, 1.45,  0.10,
        px(54, 0,56, 2), px(56, 0,58, 2),  // top, bottom
        px(54, 2,56, 9), px(58, 2,60, 9),  // front, back
        px(52, 2,54, 9), px(56, 2,58, 9),  // left(-X), right(+X)
    );

    // Left leg
    add_box(&mut v, -0.15, 0.00, -0.10,  0.00, 0.75,  0.10,
        px( 2,16, 6,18), px( 6,16,10,18),  // top, bottom
        px( 2,18, 6,26), px( 8,18,12,26),  // front, back
        px( 0,18, 2,26), px( 6,18, 8,26),  // left(-X), right(+X)
    );

    // Right leg
    add_box(&mut v,  0.00, 0.00, -0.10,  0.15, 0.75,  0.10,
        px(14,16,18,18), px(18,16,22,18),  // top, bottom
        px(14,18,18,26), px(20,18,24,26),  // front, back
        px(12,18,14,26), px(18,18,20,26),  // left(-X), right(+X)
    );

    v
}

pub enum PlayerDrawMode {
    ArmsOnly,
    Full,
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
const LEG_VERTS:   i32 = 36; // per leg
const ARMS_START:  i32 = HEAD_VERTS + TORSO_VERTS;            // 72: left arm, then right arm
const LEGS_START:  i32 = ARMS_START + ARM_VERTS * 2;          // 144: left leg, then right leg

// Joint pivots in model space (feet at y=0).
const HIP_Y:      f32 = 0.75;   // upper-body bend / leg swing pivot height
const SHOULDER_Y: f32 = 1.45;   // arm swing pivot height
const ARM_X:      f32 = 0.275;  // arm shoulder x (±)
const LEG_X:      f32 = 0.075;  // leg hip x (±)

pub struct PlayerRenderer {
    vao: u32,
    vbo: u32,
    shader: u32,
    mvp_loc: i32,
    tex_id: u32,
    fog_start_loc: i32,
    fog_end_loc: i32,
    fog_override_loc: i32,
    fog_color_override_loc: i32,
    screen_size_loc: i32,
    sky_sampler_loc: i32,
    fpv_arm_vao: u32,
    fpv_arm_vbo: u32,
    fpv_arm_verts: i32,
    bar_vao: u32,
    bar_vbo: u32,
    bar_shader: u32,
    bar_mvp_loc: i32,
    bar_color_loc: i32,
}

impl PlayerRenderer {
    pub fn new(skin_path: &str) -> Self {
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
                out float fragDist;
                void main() {
                    gl_Position = mvp * vec4(aPos, 1.0);
                    shade = aShade;
                    uv = aUV;
                    fragDist = gl_Position.w;
                }"#).unwrap();

            let frag = compile_shader(gl::FRAGMENT_SHADER, r#"#version 330 core
                in float shade;
                in vec2 uv;
                in float fragDist;
                out vec4 FragColor;
                uniform sampler2D tex;
                uniform sampler2D u_sky_sampler;
                uniform vec2 u_screen_size;
                uniform float u_fog_start;
                uniform float u_fog_end;
                uniform float u_fog_override;
                uniform vec3  u_fog_color_override;
                void main() {
                    vec3 color = texture(tex, uv).rgb * shade;
                    vec2 screenUV = gl_FragCoord.xy / u_screen_size;
                    vec3 skyFog = texture(u_sky_sampler, screenUV).rgb;
                    vec3 fogColor = mix(skyFog, u_fog_color_override, u_fog_override);
                    float fog_factor = clamp((fragDist - u_fog_start) / (u_fog_end - u_fog_start), 0.0, 1.0);
                    FragColor = vec4(mix(color, fogColor, fog_factor), 1.0);
                }"#).unwrap();

            let shader = link_program(vert, frag).unwrap();
            let mvp_loc         = gl::GetUniformLocation(shader, c"mvp".as_ptr());
            let fog_start_loc          = gl::GetUniformLocation(shader, c"u_fog_start".as_ptr());
            let fog_end_loc            = gl::GetUniformLocation(shader, c"u_fog_end".as_ptr());
            let fog_override_loc       = gl::GetUniformLocation(shader, c"u_fog_override".as_ptr());
            let fog_color_override_loc = gl::GetUniformLocation(shader, c"u_fog_color_override".as_ptr());
            let screen_size_loc        = gl::GetUniformLocation(shader, c"u_screen_size".as_ptr());
            let sky_sampler_loc        = gl::GetUniformLocation(shader, c"u_sky_sampler".as_ptr());
            let tex_id = load_png_texture(skin_path);

            // FPV forearm — uses the right-arm skin region
            let mut arm_v: Vec<f32> = Vec::new();
            add_box(&mut arm_v, -0.045, 0.0, -0.055, 0.045, 0.25, 0.055,
                px(54, 0,56, 2), px(56, 0,58, 2),  // top, bottom
                px(54, 2,56, 9), px(58, 2,60, 9),  // front, back
                px(52, 2,54, 9), px(56, 2,58, 9),  // left, right
            );
            let fpv_arm_verts = (arm_v.len() / STRIDE) as i32;
            let (mut fpv_arm_vao, mut fpv_arm_vbo) = (0u32, 0u32);
            gl::GenVertexArrays(1, &mut fpv_arm_vao);
            gl::GenBuffers(1, &mut fpv_arm_vbo);
            gl::BindVertexArray(fpv_arm_vao);
            gl::BindBuffer(gl::ARRAY_BUFFER, fpv_arm_vbo);
            gl::BufferData(gl::ARRAY_BUFFER,
                (arm_v.len() * mem::size_of::<f32>()) as isize,
                arm_v.as_ptr() as *const c_void, gl::STATIC_DRAW);
            gl::VertexAttribPointer(0, 3, gl::FLOAT, gl::FALSE, stride, std::ptr::null());
            gl::EnableVertexAttribArray(0);
            gl::VertexAttribPointer(1, 1, gl::FLOAT, gl::FALSE, stride,
                (3 * mem::size_of::<f32>()) as *const c_void);
            gl::EnableVertexAttribArray(1);
            gl::VertexAttribPointer(2, 2, gl::FLOAT, gl::FALSE, stride,
                (4 * mem::size_of::<f32>()) as *const c_void);
            gl::EnableVertexAttribArray(2);
            gl::BindBuffer(gl::ARRAY_BUFFER, 0);
            gl::BindVertexArray(0);

            // Unit quad [0,0,0]→[1,1,0] for health bar drawing
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
            let bar_shader   = link_program(bar_vert_src, bar_frag_src).unwrap();
            let bar_mvp_loc   = gl::GetUniformLocation(bar_shader, c"mvp".as_ptr());
            let bar_color_loc = gl::GetUniformLocation(bar_shader, c"u_color".as_ptr());

            PlayerRenderer { vao, vbo, shader, mvp_loc, tex_id, fog_start_loc, fog_end_loc, fog_override_loc, fog_color_override_loc, screen_size_loc, sky_sampler_loc, fpv_arm_vao, fpv_arm_vbo, fpv_arm_verts, bar_vao, bar_vbo, bar_shader, bar_mvp_loc, bar_color_loc }
        }
    }

    /// MVP for the held item rendered in view (camera) space — always visible in
    /// the bottom-right of the screen regardless of world position.
    /// `swing_t`: 0.0 = resting, 1.0 = impact (item swung toward crosshair).
    pub fn hand_item_mvp_fpv(swing_t: f32, projection: &glam::Mat4) -> glam::Mat4 {
        // Camera space: +X right, +Y up, -Z into screen.
        // Item is partially off-screen to the right at rest; swings left+up on hit.
        let rest   = glam::Vec3::new( 0.30, -0.14, -0.50);
        let impact = glam::Vec3::new( 0.13, -0.03, -0.50);
        let pos    = rest.lerp(impact, swing_t);

        let rot_x = -0.28 - swing_t * 0.38;
        let rot_z =  0.30 - swing_t * 0.22;

        *projection
            * glam::Mat4::from_translation(pos)
            * glam::Mat4::from_rotation_x(rot_x)
            * glam::Mat4::from_rotation_z(rot_z)
    }

    /// Draw a blocky forearm in view/camera space so the player looks like they
    /// are holding the item. Call this before `draw_held` with the same `swing_t`.
    pub fn draw_fpv_arm(&self, swing_t: f32, projection: &glam::Mat4,
                        screen_w: f32, screen_h: f32, sky_tex: u32) {
        // Wrist position matches the item handle area; elbow exits off-screen right.
        let rest_pos   = glam::Vec3::new(0.33, -0.22, -0.50);
        let impact_pos = glam::Vec3::new(0.18, -0.10, -0.50);
        let pos = rest_pos.lerp(impact_pos, swing_t);
        let swing_rx = swing_t * -0.28;

        // translate to wrist, then tilt arm so elbow exits upper-right of screen
        let model = glam::Mat4::from_translation(pos)
            * glam::Mat4::from_rotation_z(-0.55)
            * glam::Mat4::from_rotation_x(-0.15 + swing_rx);
        let mvp = *projection * model;

        unsafe {
            gl::Disable(gl::CULL_FACE);
            gl::UseProgram(self.shader);
            gl::UniformMatrix4fv(self.mvp_loc, 1, gl::FALSE, mvp.to_cols_array().as_ptr());
            // No fog on the FPV arm (view-space; set fog very far away)
            gl::Uniform1f(self.fog_start_loc, 1000.0);
            gl::Uniform1f(self.fog_end_loc,   2000.0);
            gl::Uniform1f(self.fog_override_loc, 0.0);
            gl::Uniform3f(self.fog_color_override_loc, 0.0, 0.0, 0.0);
            gl::Uniform2f(self.screen_size_loc, screen_w, screen_h);
            gl::Uniform1i(self.sky_sampler_loc, 4);
            gl::ActiveTexture(gl::TEXTURE4);
            gl::BindTexture(gl::TEXTURE_2D, sky_tex);
            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, self.tex_id);
            gl::BindVertexArray(self.fpv_arm_vao);
            gl::DrawArrays(gl::TRIANGLES, 0, self.fpv_arm_verts);
            gl::BindVertexArray(0);
            gl::Enable(gl::CULL_FACE);
        }
    }

    /// Draw a player model at `position` (feet coords) rotated by `yaw` (degrees).
    /// `swing_angle`: right-arm rotation in radians around the shoulder (negative = swing forward/down).
    /// `crouch`: 0 = standing, 1 = fully crouched (bends the upper body forward).
    /// `anim_time`/`move_amount`: drive the walk cycle. `move_amount` (0..1) scales the limb
    /// swing so it eases in/out instead of snapping when starting/stopping.
    /// `pitch`: head look angle in degrees (tilts the head up/down in third person).
    pub fn draw(&self, position: [f32; 3], yaw: f32,
                view: &glam::Mat4, projection: &glam::Mat4,
                mode: PlayerDrawMode, swing_angle: f32,
                crouch: f32, anim_time: f32, move_amount: f32, pitch: f32,
                fog_start: f32, fog_end: f32, screen_w: f32, screen_h: f32, sky_tex: u32,
                fog_override: f32, fog_color_override: glam::Vec3) {
        let rot_angle = FRAC_PI_2 - yaw.to_radians();
        let model = glam::Mat4::from_translation(glam::Vec3::from(position))
            * glam::Mat4::from_rotation_y(rot_angle);
        let mvp = *projection * *view * model;

        // ── Pose rig ───────────────────────────────────────────────────────────
        // Rotate a limb about `pivot` on the X axis (fore/aft swing & body bend).
        let joint = |pivot: glam::Vec3, angle: f32| {
            glam::Mat4::from_translation(pivot)
                * glam::Mat4::from_rotation_x(angle)
                * glam::Mat4::from_translation(-pivot)
        };
        let crouch = crouch.clamp(0.0, 1.0);
        // Upper body bends forward about the hips and sinks a little when crouching.
        // (Identity at crouch = 0, so the standing pose is unchanged.)
        let upper = glam::Mat4::from_translation(glam::Vec3::new(0.0, -0.22 * crouch, 0.0))
            * joint(glam::Vec3::new(0.0, HIP_Y, 0.0), 0.50 * crouch);
        // Walk cycle: legs swing fore/aft, arms swing opposite. `move_amount` eases the
        // amplitude in/out so limbs blend to neutral instead of snapping at start/stop.
        let walk = (anim_time * 8.0).sin() * 0.5 * move_amount.clamp(0.0, 1.0);
        // Head follows the look pitch. Sign chosen so looking up tilts the head back;
        // flip if it reads inverted in third person.
        let head_pitch = (-pitch.to_radians()).clamp(-1.2, 1.2);

        unsafe {
            gl::Disable(gl::CULL_FACE);
            gl::UseProgram(self.shader);
            gl::Uniform1f(self.fog_start_loc, fog_start);
            gl::Uniform1f(self.fog_end_loc, fog_end);
            gl::Uniform1f(self.fog_override_loc, fog_override);
            gl::Uniform3f(self.fog_color_override_loc, fog_color_override.x, fog_color_override.y, fog_color_override.z);
            gl::Uniform2f(self.screen_size_loc, screen_w, screen_h);
            gl::Uniform1i(self.sky_sampler_loc, 4);
            gl::ActiveTexture(gl::TEXTURE4);
            gl::BindTexture(gl::TEXTURE_2D, sky_tex);
            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, self.tex_id);
            gl::BindVertexArray(self.vao);

            match mode {
                PlayerDrawMode::Full => {
                    let set_mvp = |m: glam::Mat4| {
                        let mvp = *projection * *view * m;
                        gl::UniformMatrix4fv(self.mvp_loc, 1, gl::FALSE, mvp.to_cols_array().as_ptr());
                    };

                    // ── Upper body under the crouch bend: torso fixed, head pitches ──
                    let upper_m = model * upper;
                    set_mvp(upper_m * joint(glam::Vec3::new(0.0, SHOULDER_Y, 0.0), head_pitch));
                    gl::DrawArrays(gl::TRIANGLES, 0, HEAD_VERTS);
                    set_mvp(upper_m);
                    gl::DrawArrays(gl::TRIANGLES, HEAD_VERTS, TORSO_VERTS);

                    // ── Legs swing about the hips (opposite phases) ──
                    set_mvp(model * joint(glam::Vec3::new(-LEG_X, HIP_Y, 0.0),  walk));
                    gl::DrawArrays(gl::TRIANGLES, LEGS_START, LEG_VERTS);
                    set_mvp(model * joint(glam::Vec3::new( LEG_X, HIP_Y, 0.0), -walk));
                    gl::DrawArrays(gl::TRIANGLES, LEGS_START + LEG_VERTS, LEG_VERTS);

                    // ── Arms: attached to the upper body, swing opposite the legs ──
                    // Left arm swings with the walk cycle.
                    set_mvp(upper_m * joint(glam::Vec3::new(-ARM_X, SHOULDER_Y, 0.0), -walk));
                    gl::DrawArrays(gl::TRIANGLES, ARMS_START, ARM_VERTS);
                    // Right arm: the attack swing overrides the walk swing when active.
                    let right_arm_angle = if swing_angle != 0.0 { swing_angle } else { walk };
                    set_mvp(upper_m * joint(glam::Vec3::new(ARM_X, SHOULDER_Y, 0.0), right_arm_angle));
                    gl::DrawArrays(gl::TRIANGLES, ARMS_START + ARM_VERTS, ARM_VERTS);
                }
                PlayerDrawMode::ArmsOnly => {
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
                }
            }

            gl::BindVertexArray(0);
            gl::Enable(gl::CULL_FACE);
        }
    }

    /// Draw a billboard health bar above a remote player's head.
    /// `health_frac`: 0.0 = dead, 1.0 = full health.
    pub fn draw_health_bar(&self, position: [f32; 3], health_frac: f32, crouch: f32,
                           view: &glam::Mat4, projection: &glam::Mat4) {
        const BAR_W: f32 = 0.5;
        const BAR_H: f32 = 0.05;
        const BAR_Y: f32 = 2.05; // above head top (head top = 1.85)
        // Crouching lowers the head ~0.3, so drop the bar to match.
        let bar_y = BAR_Y - 0.3 * crouch.clamp(0.0, 1.0);

        // Extract camera right/up/forward from the view matrix rows
        let cam_right = glam::Vec3::new(view.x_axis.x, view.y_axis.x, view.z_axis.x);
        let cam_up    = glam::Vec3::new(view.x_axis.y, view.y_axis.y, view.z_axis.y);
        let cam_fwd   = cam_right.cross(cam_up);

        // Bottom-left corner of the full bar, centered horizontally
        let origin = glam::Vec3::new(position[0], position[1] + bar_y, position[2])
            - cam_right * (BAR_W * 0.5)
            - cam_up    * (BAR_H * 0.5);

        unsafe {
            gl::Disable(gl::CULL_FACE);
            gl::UseProgram(self.bar_shader);
            gl::BindVertexArray(self.bar_vao);

            // Black background (full width)
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

            // Green foreground (scaled by health fraction)
            // PolygonOffset pulls the green quad slightly toward the camera so it
            // wins the depth test against the coplanar black background.
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
}

impl Drop for PlayerRenderer {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteVertexArrays(1, &self.vao);
            gl::DeleteBuffers(1, &self.vbo);
            gl::DeleteVertexArrays(1, &self.fpv_arm_vao);
            gl::DeleteBuffers(1, &self.fpv_arm_vbo);
            gl::DeleteVertexArrays(1, &self.bar_vao);
            gl::DeleteBuffers(1, &self.bar_vbo);
            gl::DeleteProgram(self.shader);
            gl::DeleteProgram(self.bar_shader);
            gl::DeleteTextures(1, &self.tex_id);
        }
    }
}
