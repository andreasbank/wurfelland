use gl::types::*;
use std::mem;
use std::os::raw::c_void;
use std::ptr;
use crate::renderer::utils::{compile_shader, link_program};

// 5x7 pixel bitmaps — each u8 is one row, MSB = leftmost pixel
fn char_bitmap(c: char) -> [u8; 7] {
    match c {
        'P' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000],
        'A' => [0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
        'U' => [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        'S' => [0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110],
        'E' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111],
        'D' => [0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110],
        'X' => [0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001],
        'I' => [0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b11111],
        'T' => [0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100],
        'L' => [0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111],
        'N' => [0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001],
        'O' => [0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        'F' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000],
        ':' => [0b00000, 0b00100, 0b00000, 0b00000, 0b00100, 0b00000, 0b00000],
        'R' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001],
        'H' => [0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
        _ => [0u8; 7],
    }
}

struct TextTexture {
    id: u32,
    uv_max: (f32, f32),
}

fn create_text_texture(text: &str) -> TextTexture {
    const CHAR_W: usize = 5;
    const CHAR_H: usize = 7;
    const GAP: usize = 2;
    const SCALE: usize = 4;

    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    let content_w = n * CHAR_W + n.saturating_sub(1) * GAP;
    let scaled_w = content_w * SCALE;
    let scaled_h = CHAR_H * SCALE;
    let tex_w = scaled_w.next_power_of_two();
    let tex_h = scaled_h.next_power_of_two();

    let mut pixels = vec![0u8; tex_w * tex_h * 4];

    for (ci, &ch) in chars.iter().enumerate() {
        let bitmap = char_bitmap(ch);
        let char_x = ci * (CHAR_W + GAP);

        for row in 0..CHAR_H {
            for col in 0..CHAR_W {
                if (bitmap[row] >> (CHAR_W - 1 - col)) & 1 == 0 {
                    continue;
                }
                for sy in 0..SCALE {
                    for sx in 0..SCALE {
                        let px = (char_x + col) * SCALE + sx;
                        let py = row * SCALE + sy;
                        if px < tex_w && py < tex_h {
                            let idx = (py * tex_w + px) * 4;
                            pixels[idx]     = 255;
                            pixels[idx + 1] = 255;
                            pixels[idx + 2] = 255;
                            pixels[idx + 3] = 255;
                        }
                    }
                }
            }
        }
    }

    let uv_max = (scaled_w as f32 / tex_w as f32, scaled_h as f32 / tex_h as f32);

    unsafe {
        let mut id = 0;
        gl::GenTextures(1, &mut id);
        gl::BindTexture(gl::TEXTURE_2D, id);
        gl::TexImage2D(
            gl::TEXTURE_2D, 0, gl::RGBA as i32,
            tex_w as i32, tex_h as i32, 0,
            gl::RGBA, gl::UNSIGNED_BYTE, pixels.as_ptr() as *const _,
        );
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as i32);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as i32);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as i32);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as i32);
        gl::BindTexture(gl::TEXTURE_2D, 0);
        TextTexture { id, uv_max }
    }
}

pub struct MenuRenderer {
    quad_vao: u32,
    quad_vbo: u32,
    flat_shader: u32,
    flat_color_loc: i32,
    flat_rect_loc: i32,
    tex_shader: u32,
    tex_rect_loc: i32,
    tex_uv_max_loc: i32,
    paused_tex: TextTexture,
    exit_tex: TextTexture,
    outline_on_tex: TextTexture,
    outline_off_tex: TextTexture,
    res_lo_tex: TextTexture,
    res_hi_tex: TextTexture,
    pub exit_bounds: (f32, f32, f32, f32),    // x0, y0, x1, y1 in screen [0,1] space (Y down)
    pub outline_bounds: (f32, f32, f32, f32), // same
    pub res_bounds: (f32, f32, f32, f32),     // same
}

impl MenuRenderer {
    pub fn new() -> Self {
        unsafe {
            // Unit quad in [0,1]x[0,1]
            let verts: [f32; 8] = [
                0.0, 0.0,
                1.0, 0.0,
                1.0, 1.0,
                0.0, 1.0,
            ];
            let mut vao = 0;
            let mut vbo = 0;
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
            gl::BindBuffer(gl::ARRAY_BUFFER, 0);
            gl::BindVertexArray(0);

            // Flat colored quad shader
            let flat_vert = compile_shader(gl::VERTEX_SHADER, r#"#version 330 core
                layout(location = 0) in vec2 aPos;
                uniform vec4 rect; // x0,y0,x1,y1 in screen [0,1] space
                void main() {
                    vec2 p = vec2(mix(rect.x, rect.z, aPos.x), mix(rect.y, rect.w, aPos.y));
                    gl_Position = vec4(p.x * 2.0 - 1.0, -(p.y * 2.0 - 1.0), 0.0, 1.0);
                }"#).unwrap();
            let flat_frag = compile_shader(gl::FRAGMENT_SHADER, r#"#version 330 core
                out vec4 FragColor;
                uniform vec4 color;
                void main() { FragColor = color; }"#).unwrap();
            let flat_shader = link_program(flat_vert, flat_frag).unwrap();
            let flat_color_loc = gl::GetUniformLocation(flat_shader, c"color".as_ptr());
            let flat_rect_loc  = gl::GetUniformLocation(flat_shader, c"rect".as_ptr());

            // Textured quad shader
            let tex_vert = compile_shader(gl::VERTEX_SHADER, r#"#version 330 core
                layout(location = 0) in vec2 aPos;
                uniform vec4 rect;
                uniform vec2 uv_max;
                out vec2 TexCoord;
                void main() {
                    vec2 p = vec2(mix(rect.x, rect.z, aPos.x), mix(rect.y, rect.w, aPos.y));
                    gl_Position = vec4(p.x * 2.0 - 1.0, -(p.y * 2.0 - 1.0), 0.0, 1.0);
                    TexCoord = aPos * uv_max;
                }"#).unwrap();
            let tex_frag = compile_shader(gl::FRAGMENT_SHADER, r#"#version 330 core
                in vec2 TexCoord;
                out vec4 FragColor;
                uniform sampler2D tex;
                void main() { FragColor = texture(tex, TexCoord); }"#).unwrap();
            let tex_shader = link_program(tex_vert, tex_frag).unwrap();
            let tex_rect_loc   = gl::GetUniformLocation(tex_shader, c"rect".as_ptr());
            let tex_uv_max_loc = gl::GetUniformLocation(tex_shader, c"uv_max".as_ptr());

            let paused_tex      = create_text_texture("PAUSED");
            let exit_tex        = create_text_texture("EXIT");
            let outline_on_tex  = create_text_texture("OUTLINE:ON");
            let outline_off_tex = create_text_texture("OUTLINE:OFF");
            let res_lo_tex      = create_text_texture("RES:LO");
            let res_hi_tex      = create_text_texture("RES:HI");

            MenuRenderer {
                quad_vao: vao, quad_vbo: vbo,
                flat_shader, flat_color_loc, flat_rect_loc,
                tex_shader, tex_rect_loc, tex_uv_max_loc,
                paused_tex, exit_tex, outline_on_tex, outline_off_tex,
                res_lo_tex, res_hi_tex,
                outline_bounds: (0.30, 0.44, 0.70, 0.52),
                res_bounds:     (0.30, 0.56, 0.70, 0.64),
                exit_bounds:    (0.38, 0.68, 0.62, 0.76),
            }
        }
    }

    fn draw_rect(&self, x0: f32, y0: f32, x1: f32, y1: f32, r: f32, g: f32, b: f32, a: f32) {
        unsafe {
            gl::UseProgram(self.flat_shader);
            gl::Uniform4f(self.flat_rect_loc, x0, y0, x1, y1);
            gl::Uniform4f(self.flat_color_loc, r, g, b, a);
            gl::BindVertexArray(self.quad_vao);
            gl::DrawArrays(gl::TRIANGLE_FAN, 0, 4);
        }
    }

    fn draw_text(&self, tex: &TextTexture, x0: f32, y0: f32, x1: f32, y1: f32) {
        unsafe {
            gl::UseProgram(self.tex_shader);
            gl::Uniform4f(self.tex_rect_loc, x0, y0, x1, y1);
            gl::Uniform2f(self.tex_uv_max_loc, tex.uv_max.0, tex.uv_max.1);
            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, tex.id);
            gl::BindVertexArray(self.quad_vao);
            gl::DrawArrays(gl::TRIANGLE_FAN, 0, 4);
        }
    }

    pub fn draw(&self, outline_enabled: bool, hi_res: bool) {
        unsafe {
            gl::Disable(gl::DEPTH_TEST);
            gl::Disable(gl::CULL_FACE);
            gl::Enable(gl::BLEND);
            gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);

            // Semi-transparent dark overlay
            self.draw_rect(0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.6);

            // "PAUSED" title
            self.draw_text(&self.paused_tex, 0.30, 0.28, 0.70, 0.38);

            // OUTLINE toggle button
            {
                let (x0, y0, x1, y1) = self.outline_bounds;
                self.draw_rect(x0 - 0.01, y0 - 0.01, x1 + 0.01, y1 + 0.01, 0.8, 0.8, 0.8, 1.0);
                self.draw_rect(x0, y0, x1, y1, 0.2, 0.2, 0.2, 1.0);
                let label = if outline_enabled { &self.outline_on_tex } else { &self.outline_off_tex };
                self.draw_text(label, x0 + 0.02, y0 + 0.01, x1 - 0.02, y1 - 0.01);
            }

            // RES toggle button
            {
                let (x0, y0, x1, y1) = self.res_bounds;
                self.draw_rect(x0 - 0.01, y0 - 0.01, x1 + 0.01, y1 + 0.01, 0.8, 0.8, 0.8, 1.0);
                self.draw_rect(x0, y0, x1, y1, 0.2, 0.2, 0.2, 1.0);
                let label = if hi_res { &self.res_hi_tex } else { &self.res_lo_tex };
                self.draw_text(label, x0 + 0.04, y0 + 0.01, x1 - 0.04, y1 - 0.01);
            }

            // EXIT button
            {
                let (x0, y0, x1, y1) = self.exit_bounds;
                self.draw_rect(x0 - 0.01, y0 - 0.01, x1 + 0.01, y1 + 0.01, 0.8, 0.8, 0.8, 1.0);
                self.draw_rect(x0, y0, x1, y1, 0.2, 0.2, 0.2, 1.0);
                self.draw_text(&self.exit_tex, x0 + 0.04, y0 + 0.01, x1 - 0.04, y1 - 0.01);
            }

            gl::Disable(gl::BLEND);
            gl::Enable(gl::CULL_FACE);
            gl::Enable(gl::DEPTH_TEST);
        }
    }

    // mouse_x/y are raw pixel coordinates from GLFW
    pub fn is_exit_clicked(&self, mouse_x: f32, mouse_y: f32, win_w: f32, win_h: f32) -> bool {
        let nx = mouse_x / win_w;
        let ny = mouse_y / win_h;
        let (x0, y0, x1, y1) = self.exit_bounds;
        nx >= x0 && nx <= x1 && ny >= y0 && ny <= y1
    }

    pub fn is_outline_clicked(&self, mouse_x: f32, mouse_y: f32, win_w: f32, win_h: f32) -> bool {
        let nx = mouse_x / win_w;
        let ny = mouse_y / win_h;
        let (x0, y0, x1, y1) = self.outline_bounds;
        nx >= x0 && nx <= x1 && ny >= y0 && ny <= y1
    }

    pub fn is_res_clicked(&self, mouse_x: f32, mouse_y: f32, win_w: f32, win_h: f32) -> bool {
        let nx = mouse_x / win_w;
        let ny = mouse_y / win_h;
        let (x0, y0, x1, y1) = self.res_bounds;
        nx >= x0 && nx <= x1 && ny >= y0 && ny <= y1
    }
}

impl Drop for MenuRenderer {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteVertexArrays(1, &self.quad_vao);
            gl::DeleteBuffers(1, &self.quad_vbo);
            gl::DeleteProgram(self.flat_shader);
            gl::DeleteProgram(self.tex_shader);
            gl::DeleteTextures(1, &self.paused_tex.id);
            gl::DeleteTextures(1, &self.exit_tex.id);
            gl::DeleteTextures(1, &self.outline_on_tex.id);
            gl::DeleteTextures(1, &self.outline_off_tex.id);
            gl::DeleteTextures(1, &self.res_lo_tex.id);
            gl::DeleteTextures(1, &self.res_hi_tex.id);
        }
    }
}
