use std::mem;
use std::os::raw::c_void;
use crate::renderer::utils::{compile_shader, link_program};
use crate::world::item::ItemType;
use crate::game::player::INVENTORY_SIZE;

const COLS: usize = 3;
const ROWS: usize = 6;
const SLOT_SIZE: f32 = 50.0;
const SLOT_GAP:  f32 =  5.0;
const BORDER:    f32 =  2.0;
const PADDING:   f32 = 16.0;

// ── Bitmap font ───────────────────────────────────────────────────────────────
// 11 glyphs in order: x, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9
// Each glyph: 3 wide × 5 tall pixels. Each row is one u8 (3 MSBs, left→right).
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

// Atlas: 11 glyphs × 4px each (3 content + 1 gap) = 44 wide, 5 tall
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
    vao: u32,
    // Rect shader
    shader: u32,
    pos_loc: i32,
    size_loc: i32,
    screen_loc: i32,
    color_loc: i32,
    angle_loc: i32,
    // Text shader
    text_shader: u32,
    digit_tex: u32,
    t_pos_loc: i32,
    t_screen_loc: i32,
    t_glyph_loc: i32,
    t_scale_loc: i32,
}

impl BagRenderer {
    pub fn new() -> Self {
        #[rustfmt::skip]
        let verts: [f32; 12] = [
            0.0, 0.0,  1.0, 0.0,  1.0, 1.0,
            0.0, 0.0,  1.0, 1.0,  0.0, 1.0,
        ];

        // Rect shader
        let vert = compile_shader(gl::VERTEX_SHADER, r#"#version 330 core
            layout(location = 0) in vec2 aPos;
            uniform vec2 u_pos;
            uniform vec2 u_size;
            uniform vec2 u_screen;
            uniform float u_angle;
            void main() {
                vec2 center  = u_pos + u_size * 0.5;
                vec2 local   = (aPos - 0.5) * u_size;
                float c = cos(u_angle); float s = sin(u_angle);
                vec2 rotated = vec2(c*local.x - s*local.y, s*local.x + c*local.y);
                vec2 px  = center + rotated;
                vec2 ndc = (px / u_screen) * 2.0 - 1.0;
                gl_Position = vec4(ndc, 0.0, 1.0);
            }
        "#).unwrap();

        let frag = compile_shader(gl::FRAGMENT_SHADER, r#"#version 330 core
            uniform vec4 u_color;
            out vec4 FragColor;
            void main() { FragColor = u_color; }
        "#).unwrap();

        let shader = link_program(vert, frag).unwrap();

        // Text shader — samples glyph from digit atlas, renders white pixels
        let t_vert = compile_shader(gl::VERTEX_SHADER, r#"#version 330 core
            layout(location = 0) in vec2 aPos;
            uniform vec2  u_pos;
            uniform vec2  u_screen;
            uniform int   u_glyph;
            uniform float u_scale;
            out vec2 v_uv;
            void main() {
                float gx = float(u_glyph) * 4.0;
                v_uv = vec2((gx + aPos.x * 3.0) / 44.0, 1.0 - aPos.y);
                vec2 px  = u_pos + aPos * vec2(3.0, 5.0) * u_scale;
                vec2 ndc = (px / u_screen) * 2.0 - 1.0;
                gl_Position = vec4(ndc, 0.0, 1.0);
            }
        "#).unwrap();

        let t_frag = compile_shader(gl::FRAGMENT_SHADER, r#"#version 330 core
            in vec2 v_uv;
            uniform sampler2D u_atlas;
            out vec4 FragColor;
            void main() {
                if (texture(u_atlas, v_uv).r < 0.5) discard;
                FragColor = vec4(1.0, 1.0, 1.0, 1.0);
            }
        "#).unwrap();

        let text_shader = link_program(t_vert, t_frag).unwrap();

        unsafe {
            let pos_loc    = gl::GetUniformLocation(shader, c"u_pos".as_ptr());
            let size_loc   = gl::GetUniformLocation(shader, c"u_size".as_ptr());
            let screen_loc = gl::GetUniformLocation(shader, c"u_screen".as_ptr());
            let color_loc  = gl::GetUniformLocation(shader, c"u_color".as_ptr());
            let angle_loc  = gl::GetUniformLocation(shader, c"u_angle".as_ptr());

            let t_pos_loc    = gl::GetUniformLocation(text_shader, c"u_pos".as_ptr());
            let t_screen_loc = gl::GetUniformLocation(text_shader, c"u_screen".as_ptr());
            let t_glyph_loc  = gl::GetUniformLocation(text_shader, c"u_glyph".as_ptr());
            let t_scale_loc  = gl::GetUniformLocation(text_shader, c"u_scale".as_ptr());

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
            let _ = vbo;

            let digit_tex = create_digit_texture();

            BagRenderer {
                vao, shader, pos_loc, size_loc, screen_loc, color_loc, angle_loc,
                text_shader, digit_tex, t_pos_loc, t_screen_loc, t_glyph_loc, t_scale_loc,
            }
        }
    }

    fn draw_rect(&self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4], screen: [f32; 2]) {
        self.draw_rect_rotated(x, y, w, h, 0.0, color, screen);
    }

    fn draw_rect_rotated(&self, x: f32, y: f32, w: f32, h: f32, angle: f32, color: [f32; 4], screen: [f32; 2]) {
        unsafe {
            gl::Uniform2f(self.pos_loc,    x, y);
            gl::Uniform2f(self.size_loc,   w, h);
            gl::Uniform2f(self.screen_loc, screen[0], screen[1]);
            gl::Uniform4f(self.color_loc,  color[0], color[1], color[2], color[3]);
            gl::Uniform1f(self.angle_loc,  angle);
            gl::DrawArrays(gl::TRIANGLES, 0, 6);
        }
    }

    /// Renders "x", "0"–"9" characters at pixel position (x, y).
    /// scale=2 → each font pixel becomes 2×2 screen pixels (chars are 6×10px).
    fn draw_text(&self, text: &str, x: f32, y: f32, scale: f32, screen: [f32; 2]) {
        unsafe {
            gl::UseProgram(self.text_shader);
            gl::Uniform2f(self.t_screen_loc, screen[0], screen[1]);
            gl::Uniform1f(self.t_scale_loc,  scale);
            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, self.digit_tex);

            let mut cx = x;
            for ch in text.chars() {
                let glyph: i32 = match ch {
                    'x' => 0,
                    '0' => 1, '1' => 2, '2' => 3, '3' => 4, '4' => 5,
                    '5' => 6, '6' => 7, '7' => 8, '8' => 9, '9' => 10,
                    _ => continue,
                };
                gl::Uniform2f(self.t_pos_loc, cx, y);
                gl::Uniform1i(self.t_glyph_loc, glyph);
                gl::DrawArrays(gl::TRIANGLES, 0, 6);
                cx += 4.0 * scale; // 3px glyph + 1px gap
            }

            // Restore rect shader for subsequent draw_rect calls
            gl::UseProgram(self.shader);
        }
    }

    fn draw_item_icon(&self, item: ItemType, count: u32, sx: f32, sy: f32, screen: [f32; 2]) {
        match item {
            ItemType::Stick => {
                let cx = sx + SLOT_SIZE * 0.5;
                let cy = sy + SLOT_SIZE * 0.5;
                let (w, h) = (5.0, 30.0);
                self.draw_rect_rotated(cx - w * 0.5, cy - h * 0.5, w, h,
                    std::f32::consts::FRAC_PI_4, [0.55, 0.35, 0.17, 1.0], screen);
            }
            _ => {
                let [r, g, b] = item.color();
                let pad = 8.0;
                self.draw_rect(sx + pad, sy + pad,
                    SLOT_SIZE - pad * 2.0, SLOT_SIZE - pad * 2.0,
                    [r, g, b, 1.0], screen);
            }
        }

        // Stack count label — hidden for single items
        if count > 1 {
            let label = format!("x{}", count);
            let scale = 2.0;
            let text_w = label.len() as f32 * 4.0 * scale;
            let text_h = 5.0 * scale;
            self.draw_text(
                &label,
                sx + SLOT_SIZE - text_w - 2.0,
                sy + SLOT_SIZE - text_h - 2.0,
                scale,
                screen,
            );
        }
    }

    pub fn draw(&self, inventory: &[Option<(ItemType, u32)>; INVENTORY_SIZE], screen_w: f32, screen_h: f32) {
        let screen = [screen_w, screen_h];

        let grid_w = COLS as f32 * SLOT_SIZE + (COLS - 1) as f32 * SLOT_GAP;
        let grid_h = ROWS as f32 * SLOT_SIZE + (ROWS - 1) as f32 * SLOT_GAP;
        let panel_w = grid_w + PADDING * 2.0;
        let panel_h = grid_h + PADDING * 2.0;
        let panel_x = (screen_w - panel_w) * 0.5;
        let panel_y = (screen_h - panel_h) * 0.5;

        unsafe {
            gl::Disable(gl::DEPTH_TEST);
            gl::Enable(gl::BLEND);
            gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);
            gl::UseProgram(self.shader);
            gl::BindVertexArray(self.vao);

            // Full-screen dark overlay
            self.draw_rect(0.0, 0.0, screen_w, screen_h, [0.0, 0.0, 0.0, 0.45], screen);

            // Panel border then background
            let b = BORDER;
            self.draw_rect(panel_x - b, panel_y - b, panel_w + b * 2.0, panel_h + b * 2.0,
                [0.60, 0.60, 0.60, 1.0], screen);
            self.draw_rect(panel_x, panel_y, panel_w, panel_h, [0.15, 0.15, 0.15, 0.95], screen);

            // Slot grid
            let grid_x = panel_x + PADDING;
            let grid_y = panel_y + PADDING;
            for row in 0..ROWS {
                for col in 0..COLS {
                    let sx = grid_x + col as f32 * (SLOT_SIZE + SLOT_GAP);
                    let sy = grid_y + row as f32 * (SLOT_SIZE + SLOT_GAP);

                    self.draw_rect(sx - b, sy - b, SLOT_SIZE + b * 2.0, SLOT_SIZE + b * 2.0,
                        [0.50, 0.50, 0.50, 1.0], screen);
                    self.draw_rect(sx, sy, SLOT_SIZE, SLOT_SIZE,
                        [0.22, 0.22, 0.22, 1.0], screen);

                    let idx = row * COLS + col;
                    if let Some((item, count)) = inventory[idx] {
                        self.draw_item_icon(item, count, sx, sy, screen);
                    }
                }
            }

            gl::BindVertexArray(0);
            gl::Disable(gl::BLEND);
            gl::Enable(gl::DEPTH_TEST);
        }
    }
}

impl Drop for BagRenderer {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteVertexArrays(1, &self.vao);
            gl::DeleteTextures(1, &self.digit_tex);
            gl::DeleteProgram(self.shader);
            gl::DeleteProgram(self.text_shader);
        }
    }
}
