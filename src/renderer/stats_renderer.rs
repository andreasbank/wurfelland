use crate::renderer::ui::{create_text_texture_scaled, TextTexture};
use crate::renderer::utils::{compile_shader, link_program};
use crate::world::world::WorldStats;
use std::mem;
use std::os::raw::c_void;
use std::ptr;

// Title rendered at this scale; body one step smaller.
const TITLE_SCALE: usize = 3;
const BODY_SCALE:  usize = 2;

// Pixel-space layout constants.
const PAD:       f32 = 9.0;   // panel edge padding
const TITLE_LH:  f32 = 27.0;  // title line height (21px char + 6px gap)
const BODY_LH:   f32 = 18.0;  // body line height  (14px char + 4px gap)
const SEP_H:     f32 = 5.0;   // visual separator height
const ACCENT_W:  f32 = 2.0;   // left/top accent bar width

// Colours  [r, g, b, a]
const COL_TITLE: [f32; 4] = [1.0, 0.75, 0.15, 1.0]; // amber
const COL_PERF:  [f32; 4] = [0.85, 0.95, 1.0,  1.0]; // cool white
const COL_POS:   [f32; 4] = [0.55, 1.0,  0.65, 1.0]; // soft green
const COL_CHUNK: [f32; 4] = [1.0,  1.0,  0.55, 1.0]; // soft yellow
const COL_SHAD:  [f32; 4] = [0.0,  0.0,  0.0,  0.72];// drop-shadow

pub struct StatsRenderer {
    vao:            u32,
    vbo:            u32,
    flat_prog:      u32,
    flat_rect_loc:  i32,
    flat_color_loc: i32,
    text_prog:      u32,
    text_rect_loc:  i32,
    text_uv_max:    i32,
    text_tint_loc:  i32,
}

impl StatsRenderer {
    pub fn new() -> Self {
        unsafe {
            let verts: [f32; 8] = [0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0];
            let (mut vao, mut vbo) = (0u32, 0u32);
            gl::GenVertexArrays(1, &mut vao);
            gl::GenBuffers(1, &mut vbo);
            gl::BindVertexArray(vao);
            gl::BindBuffer(gl::ARRAY_BUFFER, vbo);
            gl::BufferData(
                gl::ARRAY_BUFFER,
                (verts.len() * mem::size_of::<f32>()) as isize,
                verts.as_ptr() as *const c_void,
                gl::STATIC_DRAW,
            );
            gl::VertexAttribPointer(0, 2, gl::FLOAT, gl::FALSE, 0, ptr::null());
            gl::EnableVertexAttribArray(0);
            gl::BindVertexArray(0);

            let flat_vert = compile_shader(gl::VERTEX_SHADER, r#"#version 330 core
                layout(location=0) in vec2 aPos;
                uniform vec4 rect;
                void main() {
                    vec2 p = vec2(mix(rect.x,rect.z,aPos.x), mix(rect.y,rect.w,aPos.y));
                    gl_Position = vec4(p.x*2.0-1.0, -(p.y*2.0-1.0), 0.0, 1.0);
                }"#).unwrap();
            let flat_frag = compile_shader(gl::FRAGMENT_SHADER, r#"#version 330 core
                out vec4 FragColor;
                uniform vec4 color;
                void main() { FragColor = color; }"#).unwrap();
            let flat_prog      = link_program(flat_vert, flat_frag).unwrap();
            let flat_rect_loc  = gl::GetUniformLocation(flat_prog,  c"rect".as_ptr());
            let flat_color_loc = gl::GetUniformLocation(flat_prog,  c"color".as_ptr());

            let text_vert = compile_shader(gl::VERTEX_SHADER, r#"#version 330 core
                layout(location=0) in vec2 aPos;
                uniform vec4 rect;
                uniform vec2 uv_max;
                out vec2 TexCoord;
                void main() {
                    vec2 p = vec2(mix(rect.x,rect.z,aPos.x), mix(rect.y,rect.w,aPos.y));
                    gl_Position = vec4(p.x*2.0-1.0, -(p.y*2.0-1.0), 0.0, 1.0);
                    TexCoord = aPos * uv_max;
                }"#).unwrap();
            let text_frag = compile_shader(gl::FRAGMENT_SHADER, r#"#version 330 core
                in  vec2 TexCoord;
                out vec4 FragColor;
                uniform sampler2D tex;
                uniform vec4 tint;
                void main() {
                    float a = texture(tex, TexCoord).a;
                    FragColor = vec4(tint.rgb, tint.a * a);
                }"#).unwrap();
            let text_prog     = link_program(text_vert, text_frag).unwrap();
            let text_rect_loc = gl::GetUniformLocation(text_prog, c"rect".as_ptr());
            let text_uv_max   = gl::GetUniformLocation(text_prog, c"uv_max".as_ptr());
            let text_tint_loc = gl::GetUniformLocation(text_prog, c"tint".as_ptr());

            StatsRenderer { vao, vbo, flat_prog, flat_rect_loc, flat_color_loc,
                            text_prog, text_rect_loc, text_uv_max, text_tint_loc }
        }
    }

    // ── low-level draw helpers (pixel coords, Y-down from top-left) ───────────

    fn draw_rect(&self, x: f32, y: f32, w: f32, h: f32, win_w: f32, win_h: f32, c: [f32; 4]) {
        unsafe {
            gl::UseProgram(self.flat_prog);
            gl::Uniform4f(self.flat_rect_loc,  x/win_w, y/win_h, (x+w)/win_w, (y+h)/win_h);
            gl::Uniform4f(self.flat_color_loc, c[0], c[1], c[2], c[3]);
            gl::BindVertexArray(self.vao);
            gl::DrawArrays(gl::TRIANGLE_FAN, 0, 4);
        }
    }

    fn draw_tex(&self, tex: &TextTexture, x: f32, y: f32, win_w: f32, win_h: f32, tint: [f32; 4]) {
        let w = tex.pixel_width  as f32;
        let h = tex.pixel_height as f32;
        unsafe {
            gl::UseProgram(self.text_prog);
            gl::Uniform4f(self.text_rect_loc, x/win_w, y/win_h, (x+w)/win_w, (y+h)/win_h);
            gl::Uniform2f(self.text_uv_max,   tex.uv_max.0, tex.uv_max.1);
            gl::Uniform4f(self.text_tint_loc, tint[0], tint[1], tint[2], tint[3]);
            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, tex.id);
            gl::BindVertexArray(self.vao);
            gl::DrawArrays(gl::TRIANGLE_FAN, 0, 4);
        }
    }

    fn draw_line(&self, tex: &TextTexture, x: f32, y: f32, win_w: f32, win_h: f32, color: [f32; 4]) {
        self.draw_tex(tex, x + 1.0, y + 1.0, win_w, win_h, COL_SHAD);
        self.draw_tex(tex, x,       y,       win_w, win_h, color);
    }

    // ── public draw ───────────────────────────────────────────────────────────

    pub fn draw(
        &self,
        fps: u32, cpu_pct: f32, mem_mb: u64,
        player_pos: [f32; 3],
        stats: &WorldStats, drawn: usize,
        win_w: f32, win_h: f32,
    ) {
        // Body stat lines: fixed 16-char format — label(8) + value(8).
        // 8 value chars fits "9999 MB", "99.9 GB", "-12345.1", etc. without overflow.
        let s = |label: &str, val: &str| -> TextTexture {
            create_text_texture_scaled(&format!("{:<8}{:>8}", label, val), BODY_SCALE)
        };

        // MEM: show as X.XGB so it always fits in 8 chars ("  0.1GB" .. "999.9GB")
        let mem_str = format!("{:.1}GB", mem_mb as f32 / 1024.0);

        let title_tex    = create_text_texture_scaled("WURFELLAND", TITLE_SCALE);
        let fps_tex      = s("FPS",      &fps.to_string());
        let cpu_tex      = s("CPU",      &format!("{:.0}%", cpu_pct));
        let mem_tex      = s("MEM",      &mem_str);
        let x_tex        = s("X",        &format!("{:.1}", player_pos[0]));
        let y_tex        = s("Y",        &format!("{:.1}", player_pos[1]));
        let z_tex        = s("Z",        &format!("{:.1}", player_pos[2]));
        let loaded_tex   = s("LOADED",   &stats.loaded.to_string());
        let meshed_tex   = s("MESHED",   &stats.meshed.to_string());
        let drawn_tex    = s("DRAWN",    &drawn.to_string());
        let genq_tex     = s("GEN Q",    &stats.terrain_queued.to_string());
        let genrun_tex   = s("GEN RUN",  &stats.terrain_inflight.to_string());
        let meshrun_tex  = s("MESH RUN", &stats.mesh_inflight.to_string());

        // Panel sizing
        // Title "WURFELLAND" at scale 3: (5*10+9*2)*3 = 204 px wide
        // Body lines 16 chars at scale 2: (5*16+15*2)*2 = 220 px wide
        let text_w   = 220.0_f32;
        let panel_w  = ACCENT_W + PAD + text_w + PAD;
        let panel_h  = PAD
            + TITLE_LH                // title
            + SEP_H                   // separator
            + 3.0 * BODY_LH           // fps, cpu, mem
            + SEP_H                   // separator
            + 3.0 * BODY_LH           // x, y, z
            + SEP_H                   // separator
            + 6.0 * BODY_LH           // 6 chunk stats
            + PAD;

        unsafe {
            gl::Disable(gl::DEPTH_TEST);
            gl::Disable(gl::CULL_FACE);
            gl::Enable(gl::BLEND);
            gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);
        }

        // Panel background
        self.draw_rect(0.0, 0.0, panel_w, panel_h, win_w, win_h,
                       [0.04, 0.06, 0.10, 0.86]);
        // Top accent bar
        self.draw_rect(0.0, 0.0, panel_w, ACCENT_W, win_w, win_h,
                       [0.30, 0.52, 0.88, 1.0]);
        // Left accent bar
        self.draw_rect(0.0, 0.0, ACCENT_W, panel_h, win_w, win_h,
                       [0.30, 0.52, 0.88, 1.0]);

        let tx = ACCENT_W + PAD;          // text x origin
        let mut ty = PAD;                 // current y cursor

        // Title
        self.draw_line(&title_tex, tx, ty, win_w, win_h, COL_TITLE);
        ty += TITLE_LH;

        // Separator
        self.draw_rect(tx, ty + 1.0, text_w, 1.0, win_w, win_h,
                       [0.30, 0.52, 0.88, 0.45]);
        ty += SEP_H;

        // Performance stats
        for tex in &[&fps_tex, &cpu_tex, &mem_tex] {
            self.draw_line(tex, tx, ty, win_w, win_h, COL_PERF);
            ty += BODY_LH;
        }

        // Separator
        self.draw_rect(tx, ty + 1.0, text_w, 1.0, win_w, win_h,
                       [0.30, 0.52, 0.88, 0.45]);
        ty += SEP_H;

        // Position
        for tex in &[&x_tex, &y_tex, &z_tex] {
            self.draw_line(tex, tx, ty, win_w, win_h, COL_POS);
            ty += BODY_LH;
        }

        // Separator
        self.draw_rect(tx, ty + 1.0, text_w, 1.0, win_w, win_h,
                       [0.30, 0.52, 0.88, 0.45]);
        ty += SEP_H;

        // Chunk stats
        for tex in &[&loaded_tex, &meshed_tex, &drawn_tex,
                     &genq_tex, &genrun_tex, &meshrun_tex] {
            self.draw_line(tex, tx, ty, win_w, win_h, COL_CHUNK);
            ty += BODY_LH;
        }

        unsafe {
            gl::Disable(gl::BLEND);
            gl::Enable(gl::CULL_FACE);
            gl::Enable(gl::DEPTH_TEST);
        }
    }
}

impl Drop for StatsRenderer {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteVertexArrays(1, &self.vao);
            gl::DeleteBuffers(1, &self.vbo);
            gl::DeleteProgram(self.flat_prog);
            gl::DeleteProgram(self.text_prog);
        }
    }
}
