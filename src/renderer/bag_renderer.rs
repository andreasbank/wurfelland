use std::mem;
use std::os::raw::c_void;
use crate::renderer::utils::{compile_shader, link_program};
use crate::renderer::ui::Window;
use crate::world::item::ItemType;
use crate::game::player::INVENTORY_SIZE;

const COLS: usize = 3;
const ROWS: usize = 6;

// All sizes in [0,1] normalised screen space (Y down).
const SLOT_SIZE: f32 = 0.075;
const SLOT_GAP:  f32 = 0.010;
const BORDER:    f32 = 0.003;
const PADDING:   f32 = 0.016;
// Count-label glyph dimensions in [0,1] space.
const GLYPH_W:   f32 = 0.0085;
const GLYPH_H:   f32 = 0.0140;
const GLYPH_GAP: f32 = 0.0010;

// ── Digit atlas ───────────────────────────────────────────────────────────────
// 11 glyphs in order: x, 0–9.  Each glyph: 3 wide × 5 tall pixels.
const GLYPHS: [[u8; 5]; 11] = [
    [0b101_00000, 0b101_00000, 0b010_00000, 0b101_00000, 0b101_00000], // x
    [0b111_00000, 0b101_00000, 0b101_00000, 0b101_00000, 0b111_00000], // 0
    [0b010_00000, 0b110_00000, 0b010_00000, 0b010_00000, 0b111_00000], // 1
    [0b111_00000, 0b001_00000, 0b111_00000, 0b100_00000, 0b111_00000], // 2
    [0b111_00000, 0b001_00000, 0b111_00000, 0b001_00000, 0b111_00000], // 3
    [0b101_00000, 0b101_00000, 0b111_00000, 0b001_00000, 0b001_00000], // 4
    [0b111_00000, 0b100_00000, 0b111_00000, 0b001_00000, 0b111_00000], // 5
    [0b111_00000, 0b100_00000, 0b111_00000, 0b101_00000, 0b111_00000], // 6
    [0b111_00000, 0b001_00000, 0b001_00000, 0b001_00000, 0b001_00000], // 7
    [0b111_00000, 0b101_00000, 0b111_00000, 0b101_00000, 0b111_00000], // 8
    [0b111_00000, 0b101_00000, 0b111_00000, 0b001_00000, 0b111_00000], // 9
];
const ATLAS_W: i32 = 44;
const ATLAS_H: i32 = 5;

fn create_digit_texture() -> u32 {
    let mut px = vec![0u8; (ATLAS_W * ATLAS_H) as usize];
    for (g, glyph) in GLYPHS.iter().enumerate() {
        for (row, &bits) in glyph.iter().enumerate() {
            for col in 0..3usize {
                if (bits >> (7 - col)) & 1 == 1 {
                    px[row * ATLAS_W as usize + g * 4 + col] = 255;
                }
            }
        }
    }
    unsafe {
        let mut id = 0u32;
        gl::GenTextures(1, &mut id);
        gl::BindTexture(gl::TEXTURE_2D, id);
        gl::TexImage2D(gl::TEXTURE_2D, 0, gl::R8 as i32,
            ATLAS_W, ATLAS_H, 0,
            gl::RED, gl::UNSIGNED_BYTE, px.as_ptr() as *const _);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as i32);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as i32);
        gl::BindTexture(gl::TEXTURE_2D, 0);
        id
    }
}

pub struct BagRenderer {
    // Window provides flat rect drawing via its UiRenderer.
    window:         Window,
    // Shared VAO/VBO for the two bag-specific shaders (rotated rect + glyph).
    vao:            u32,
    vbo:            u32,
    // Rotated rect shader (stick icon).
    rot_shader:     u32,
    rot_center_loc: i32,
    rot_half_loc:   i32,
    rot_angle_loc:  i32,
    rot_color_loc:  i32,
    // Digit glyph shader.
    glyph_shader:   u32,
    glyph_rect_loc: i32,
    glyph_idx_loc:  i32,
    digit_tex:      u32,
}

impl BagRenderer {
    pub fn new() -> Self {
        #[rustfmt::skip]
        let verts: [f32; 12] = [
            0.0, 0.0,  1.0, 0.0,  1.0, 1.0,
            0.0, 0.0,  1.0, 1.0,  0.0, 1.0,
        ];

        // ── rotated rect shader ─────────────────────────────────────────────
        let rot_vert = compile_shader(gl::VERTEX_SHADER, r#"#version 330 core
            layout(location = 0) in vec2 aPos;
            uniform vec2  u_center; // center in [0,1]
            uniform vec2  u_half;   // half-size in [0,1]
            uniform float u_angle;
            void main() {
                vec2 local = (aPos - 0.5) * 2.0; // [-1,1]
                float c = cos(u_angle); float s = sin(u_angle);
                vec2 rot = vec2(c * local.x - s * local.y,
                                s * local.x + c * local.y);
                vec2 p = u_center + rot * u_half;
                gl_Position = vec4(p.x * 2.0 - 1.0, -(p.y * 2.0 - 1.0), 0.0, 1.0);
            }"#).unwrap();
        let rot_frag = compile_shader(gl::FRAGMENT_SHADER, r#"#version 330 core
            uniform vec4 u_color;
            out vec4 FragColor;
            void main() { FragColor = u_color; }"#).unwrap();
        let rot_shader     = link_program(rot_vert, rot_frag).unwrap();
        let rot_center_loc = unsafe { gl::GetUniformLocation(rot_shader, c"u_center".as_ptr()) };
        let rot_half_loc   = unsafe { gl::GetUniformLocation(rot_shader, c"u_half".as_ptr())   };
        let rot_angle_loc  = unsafe { gl::GetUniformLocation(rot_shader, c"u_angle".as_ptr())  };
        let rot_color_loc  = unsafe { gl::GetUniformLocation(rot_shader, c"u_color".as_ptr())  };

        // ── digit glyph shader ──────────────────────────────────────────────
        let glyph_vert = compile_shader(gl::VERTEX_SHADER, r#"#version 330 core
            layout(location = 0) in vec2 aPos;
            uniform vec4 u_rect;  // glyph cell in [0,1] screen space (Y down)
            uniform int  u_glyph;
            out vec2 v_uv;
            void main() {
                float gx = float(u_glyph) * 4.0;
                v_uv = vec2((gx + aPos.x * 3.0) / 44.0, aPos.y);
                vec2 p = vec2(mix(u_rect.x, u_rect.z, aPos.x),
                              mix(u_rect.y, u_rect.w, aPos.y));
                gl_Position = vec4(p.x * 2.0 - 1.0, -(p.y * 2.0 - 1.0), 0.0, 1.0);
            }"#).unwrap();
        let glyph_frag = compile_shader(gl::FRAGMENT_SHADER, r#"#version 330 core
            in vec2 v_uv;
            uniform sampler2D u_atlas;
            out vec4 FragColor;
            void main() {
                if (texture(u_atlas, v_uv).r < 0.5) discard;
                FragColor = vec4(1.0, 1.0, 1.0, 1.0);
            }"#).unwrap();
        let glyph_shader   = link_program(glyph_vert, glyph_frag).unwrap();
        let glyph_rect_loc = unsafe { gl::GetUniformLocation(glyph_shader, c"u_rect".as_ptr())  };
        let glyph_idx_loc  = unsafe { gl::GetUniformLocation(glyph_shader, c"u_glyph".as_ptr()) };

        let (vao, vbo, digit_tex) = unsafe {
            let mut vao = 0u32;
            let mut vbo = 0u32;
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
            gl::VertexAttribPointer(0, 2, gl::FLOAT, gl::FALSE,
                (2 * mem::size_of::<f32>()) as i32, std::ptr::null());
            gl::EnableVertexAttribArray(0);
            gl::BindBuffer(gl::ARRAY_BUFFER, 0);
            gl::BindVertexArray(0);
            (vao, vbo, create_digit_texture())
        };

        BagRenderer {
            window: Window::new(),
            vao, vbo,
            rot_shader, rot_center_loc, rot_half_loc, rot_angle_loc, rot_color_loc,
            glyph_shader, glyph_rect_loc, glyph_idx_loc,
            digit_tex,
        }
    }

    // Center and half-extents in [0,1] space.
    fn draw_rect_rotated(&self, cx: f32, cy: f32, hw: f32, hh: f32, angle: f32, color: [f32; 4]) {
        unsafe {
            gl::UseProgram(self.rot_shader);
            gl::BindVertexArray(self.vao);
            gl::Uniform2f(self.rot_center_loc, cx, cy);
            gl::Uniform2f(self.rot_half_loc,   hw, hh);
            gl::Uniform1f(self.rot_angle_loc,  angle);
            gl::Uniform4f(self.rot_color_loc,  color[0], color[1], color[2], color[3]);
            gl::DrawArrays(gl::TRIANGLES, 0, 6);
        }
    }

    fn draw_label(&self, text: &str, x0: f32, y0: f32) {
        unsafe {
            gl::UseProgram(self.glyph_shader);
            gl::BindVertexArray(self.vao);
            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, self.digit_tex);

            let mut cx = x0;
            for ch in text.chars() {
                let glyph: i32 = match ch {
                    'x' => 0,
                    '0' => 1, '1' => 2, '2' => 3, '3' => 4, '4' => 5,
                    '5' => 6, '6' => 7, '7' => 8, '8' => 9, '9' => 10,
                    _ => continue,
                };
                gl::Uniform4f(self.glyph_rect_loc, cx, y0, cx + GLYPH_W, y0 + GLYPH_H);
                gl::Uniform1i(self.glyph_idx_loc,  glyph);
                gl::DrawArrays(gl::TRIANGLES, 0, 6);
                cx += GLYPH_W + GLYPH_GAP;
            }
        }
    }

    fn draw_item_icon(&self, item: ItemType, count: u32, sx: f32, sy: f32) {
        match item {
            ItemType::Stick => {
                let cx = sx + SLOT_SIZE * 0.5;
                let cy = sy + SLOT_SIZE * 0.5;
                let hw = SLOT_SIZE * 0.06;
                let hh = SLOT_SIZE * 0.38;
                self.draw_rect_rotated(cx, cy, hw, hh,
                    std::f32::consts::FRAC_PI_4, [0.55, 0.35, 0.17, 1.0]);
            }
            _ => {
                let [r, g, b] = item.color();
                let pad = SLOT_SIZE * 0.12;
                self.window.draw_rect(sx + pad, sy + pad,
                    sx + SLOT_SIZE - pad, sy + SLOT_SIZE - pad,
                    r, g, b, 1.0);
            }
        }

        if count > 1 {
            let label = format!("x{}", count);
            let margin = SLOT_SIZE * 0.04;
            let text_w = label.len() as f32 * (GLYPH_W + GLYPH_GAP) - GLYPH_GAP;
            let lx = sx + SLOT_SIZE - text_w - margin;
            let ly = sy + SLOT_SIZE - GLYPH_H - margin;
            self.draw_label(&label, lx, ly);
        }
    }

    /// Draws the bag inventory panel. Layout is defined in [0,1] normalised
    /// screen space and scales automatically with window resolution.
    pub fn draw(&self, inventory: &[Option<(ItemType, u32)>; INVENTORY_SIZE]) {
        let grid_w  = COLS as f32 * SLOT_SIZE + (COLS - 1) as f32 * SLOT_GAP;
        let grid_h  = ROWS as f32 * SLOT_SIZE + (ROWS - 1) as f32 * SLOT_GAP;
        let panel_w = grid_w + PADDING * 2.0;
        let panel_h = grid_h + PADDING * 2.0;
        let panel_x = (1.0 - panel_w) * 0.5;
        let panel_y = (1.0 - panel_h) * 0.5;

        unsafe {
            gl::Disable(gl::DEPTH_TEST);
            gl::Disable(gl::CULL_FACE);
            gl::Enable(gl::BLEND);
            gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);
        }

        // Dark overlay + panel chrome — flat rects via Window
        self.window.draw_rect(0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.45);
        self.window.draw_rect(
            panel_x - BORDER, panel_y - BORDER,
            panel_x + panel_w + BORDER, panel_y + panel_h + BORDER,
            0.60, 0.60, 0.60, 1.0,
        );
        self.window.draw_rect(panel_x, panel_y, panel_x + panel_w, panel_y + panel_h,
            0.15, 0.15, 0.15, 0.95);

        let grid_x = panel_x + PADDING;
        let grid_y = panel_y + PADDING;

        for row in 0..ROWS {
            for col in 0..COLS {
                let sx = grid_x + col as f32 * (SLOT_SIZE + SLOT_GAP);
                let sy = grid_y + row as f32 * (SLOT_SIZE + SLOT_GAP);

                self.window.draw_rect(
                    sx - BORDER, sy - BORDER,
                    sx + SLOT_SIZE + BORDER, sy + SLOT_SIZE + BORDER,
                    0.50, 0.50, 0.50, 1.0,
                );
                self.window.draw_rect(sx, sy, sx + SLOT_SIZE, sy + SLOT_SIZE,
                    0.22, 0.22, 0.22, 1.0);

                let idx = row * COLS + col;
                if let Some((item, count)) = inventory[idx] {
                    self.draw_item_icon(item, count, sx, sy);
                }
            }
        }

        unsafe {
            gl::Disable(gl::BLEND);
            gl::Enable(gl::CULL_FACE);
            gl::Enable(gl::DEPTH_TEST);
        }
    }
}

impl Drop for BagRenderer {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteVertexArrays(1, &self.vao);
            gl::DeleteBuffers(1, &self.vbo);
            gl::DeleteTextures(1, &self.digit_tex);
            gl::DeleteProgram(self.rot_shader);
            gl::DeleteProgram(self.glyph_shader);
        }
    }
}
