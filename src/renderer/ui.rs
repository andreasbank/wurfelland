use std::mem;
use std::os::raw::c_void;
use std::ptr;
use crate::renderer::utils::{compile_shader, link_program};

// 5x7 pixel bitmaps — each u8 is one row, MSB = leftmost pixel
pub fn char_bitmap(c: char) -> [u8; 7] {
    if c.is_ascii_lowercase() {
        return char_bitmap(c.to_ascii_uppercase());
    }
    match c {
        'A' => [0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
        'B' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110],
        'C' => [0b01111, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b01111],
        'D' => [0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110],
        'E' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111],
        'F' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000],
        'G' => [0b01111, 0b10000, 0b10000, 0b10111, 0b10001, 0b10001, 0b01111],
        'H' => [0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
        'I' => [0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b11111],
        'J' => [0b00111, 0b00001, 0b00001, 0b00001, 0b10001, 0b10001, 0b01110],
        'K' => [0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001],
        'L' => [0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111],
        'M' => [0b10001, 0b11011, 0b10101, 0b10001, 0b10001, 0b10001, 0b10001],
        'N' => [0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001],
        'O' => [0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        'P' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000],
        'Q' => [0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101],
        'R' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001],
        'S' => [0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110],
        'T' => [0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100],
        'U' => [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        'V' => [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100],
        'W' => [0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b11011, 0b10001],
        'X' => [0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001],
        'Y' => [0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100],
        'Z' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111],
        '0' => [0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110],
        '1' => [0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b11111],
        '2' => [0b01110, 0b10001, 0b00001, 0b00110, 0b01000, 0b10000, 0b11111],
        '3' => [0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110],
        '4' => [0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010],
        '5' => [0b11111, 0b10000, 0b10000, 0b11110, 0b00001, 0b00001, 0b11110],
        '6' => [0b01110, 0b10000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110],
        '7' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000],
        '8' => [0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110],
        '9' => [0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00001, 0b01110],
        ':' => [0b00000, 0b00100, 0b00000, 0b00000, 0b00100, 0b00000, 0b00000],
        '.' => [0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00100, 0b00100],
        '!' => [0b00100, 0b00100, 0b00100, 0b00100, 0b00000, 0b00000, 0b00100],
        ' ' => [0b00000; 7],
        '-' => [0b00000, 0b00000, 0b00000, 0b11111, 0b00000, 0b00000, 0b00000],
        '>' => [0b10000, 0b01000, 0b00100, 0b00010, 0b00100, 0b01000, 0b10000],
        '/' => [0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b00000, 0b00000],
        '(' => [0b00110, 0b01000, 0b10000, 0b10000, 0b10000, 0b01000, 0b00110],
        ')' => [0b11000, 0b00100, 0b00010, 0b00010, 0b00010, 0b00100, 0b11000],
        _   => [0b00000; 7],
    }
}

pub struct TextTexture {
    pub id: u32,
    pub uv_max: (f32, f32),
    pub pixel_width: u32,
    pub pixel_height: u32,
}

impl Drop for TextTexture {
    fn drop(&mut self) {
        unsafe { gl::DeleteTextures(1, &self.id); }
    }
}

pub fn create_text_texture(text: &str) -> TextTexture {
    const CHAR_W: usize = 5;
    const CHAR_H: usize = 7;
    const GAP:    usize = 2;
    const SCALE:  usize = 4;

    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    let content_w = n * CHAR_W + n.saturating_sub(1) * GAP;
    let scaled_w  = content_w * SCALE;
    let scaled_h  = CHAR_H * SCALE;
    let tex_w = scaled_w.next_power_of_two();
    let tex_h = scaled_h.next_power_of_two();

    let mut pixels = vec![0u8; tex_w * tex_h * 4];

    for (ci, &ch) in chars.iter().enumerate() {
        let bitmap  = char_bitmap(ch);
        let char_x  = ci * (CHAR_W + GAP);
        for row in 0..CHAR_H {
            for col in 0..CHAR_W {
                if (bitmap[row] >> (CHAR_W - 1 - col)) & 1 == 0 { continue; }
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
        TextTexture { id, uv_max, pixel_width: scaled_w as u32, pixel_height: scaled_h as u32 }
    }
}

// ─── UiRenderer ──────────────────────────────────────────────────────────────
// Holds the shared OpenGL resources for 2D UI drawing (flat and textured quads).

pub struct UiRenderer {
    quad_vao:       u32,
    quad_vbo:       u32,
    flat_shader:    u32,
    flat_color_loc: i32,
    flat_rect_loc:  i32,
    tex_shader:     u32,
    tex_rect_loc:   i32,
    tex_uv_max_loc: i32,
}

impl UiRenderer {
    pub fn new() -> Self {
        unsafe {
            let verts: [f32; 8] = [0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0];
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
            gl::VertexAttribPointer(0, 2, gl::FLOAT, gl::FALSE, 0, ptr::null());
            gl::EnableVertexAttribArray(0);
            gl::BindBuffer(gl::ARRAY_BUFFER, 0);
            gl::BindVertexArray(0);

            let flat_vert = compile_shader(gl::VERTEX_SHADER, r#"#version 330 core
                layout(location = 0) in vec2 aPos;
                uniform vec4 rect;
                void main() {
                    vec2 p = vec2(mix(rect.x, rect.z, aPos.x), mix(rect.y, rect.w, aPos.y));
                    gl_Position = vec4(p.x * 2.0 - 1.0, -(p.y * 2.0 - 1.0), 0.0, 1.0);
                }"#).unwrap();
            let flat_frag = compile_shader(gl::FRAGMENT_SHADER, r#"#version 330 core
                out vec4 FragColor;
                uniform vec4 color;
                void main() { FragColor = color; }"#).unwrap();
            let flat_shader    = link_program(flat_vert, flat_frag).unwrap();
            let flat_color_loc = gl::GetUniformLocation(flat_shader, c"color".as_ptr());
            let flat_rect_loc  = gl::GetUniformLocation(flat_shader, c"rect".as_ptr());

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
            let tex_shader     = link_program(tex_vert, tex_frag).unwrap();
            let tex_rect_loc   = gl::GetUniformLocation(tex_shader, c"rect".as_ptr());
            let tex_uv_max_loc = gl::GetUniformLocation(tex_shader, c"uv_max".as_ptr());

            UiRenderer {
                quad_vao: vao, quad_vbo: vbo,
                flat_shader, flat_color_loc, flat_rect_loc,
                tex_shader,  tex_rect_loc,   tex_uv_max_loc,
            }
        }
    }

    pub fn draw_rect(&self, x0: f32, y0: f32, x1: f32, y1: f32, r: f32, g: f32, b: f32, a: f32) {
        unsafe {
            gl::UseProgram(self.flat_shader);
            gl::Uniform4f(self.flat_rect_loc,  x0, y0, x1, y1);
            gl::Uniform4f(self.flat_color_loc, r, g, b, a);
            gl::BindVertexArray(self.quad_vao);
            gl::DrawArrays(gl::TRIANGLE_FAN, 0, 4);
        }
    }

    pub fn draw_text(&self, tex: &TextTexture, x0: f32, y0: f32, x1: f32, y1: f32) {
        unsafe {
            gl::UseProgram(self.tex_shader);
            gl::Uniform4f(self.tex_rect_loc,   x0, y0, x1, y1);
            gl::Uniform2f(self.tex_uv_max_loc, tex.uv_max.0, tex.uv_max.1);
            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, tex.id);
            gl::BindVertexArray(self.quad_vao);
            gl::DrawArrays(gl::TRIANGLE_FAN, 0, 4);
        }
    }
}

impl Drop for UiRenderer {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteVertexArrays(1, &self.quad_vao);
            gl::DeleteBuffers(1, &self.quad_vbo);
            gl::DeleteProgram(self.flat_shader);
            gl::DeleteProgram(self.tex_shader);
        }
    }
}

// ─── TextButton ───────────────────────────────────────────────────────────────
// A clickable button with one or more label textures (use set_label to switch
// between them, e.g. for toggle buttons with ON/OFF states).

pub struct TextButton {
    pub id:     String,
    pub bounds: (f32, f32, f32, f32),  // x0, y0, x1, y1 in [0,1] screen space (Y down)
    labels:     Vec<TextTexture>,
    current:    usize,
}

impl TextButton {
    pub fn new(id: &str, label: &str, bounds: (f32, f32, f32, f32)) -> Self {
        TextButton {
            id: id.to_string(),
            bounds,
            labels:  vec![create_text_texture(label)],
            current: 0,
        }
    }

    /// Creates a toggle button with multiple labels; switch with `set_label(idx)`.
    pub fn new_toggle(id: &str, labels: &[&str], bounds: (f32, f32, f32, f32)) -> Self {
        TextButton {
            id: id.to_string(),
            bounds,
            labels:  labels.iter().map(|s| create_text_texture(s)).collect(),
            current: 0,
        }
    }

    /// Selects which label texture is shown (0-based index).
    pub fn set_label(&mut self, idx: usize) {
        if idx < self.labels.len() { self.current = idx; }
    }

    pub fn is_hit(&self, nx: f32, ny: f32) -> bool {
        let (x0, y0, x1, y1) = self.bounds;
        nx >= x0 && nx <= x1 && ny >= y0 && ny <= y1
    }

    pub fn draw(&self, r: &UiRenderer) {
        let (x0, y0, x1, y1) = self.bounds;
        let pad = 0.01;
        r.draw_rect(x0 - pad, y0 - pad, x1 + pad, y1 + pad, 0.8, 0.8, 0.8, 1.0);
        r.draw_rect(x0, y0, x1, y1, 0.2, 0.2, 0.2, 1.0);
        r.draw_text(&self.labels[self.current], x0 + pad * 2.0, y0 + pad, x1 - pad * 2.0, y1 - pad);
    }
}

// ─── Window ───────────────────────────────────────────────────────────────────
// A 2D UI panel: optional full-screen overlay, optional title, and a list of
// TextButtons. Call `handle_click` to get the id of the clicked button.

pub struct Window {
    renderer:      UiRenderer,
    overlay_alpha: f32,                                       // 0 = none, >0 = dark overlay
    title:         Option<(TextTexture, (f32, f32, f32, f32))>,  // texture + draw bounds
    pub buttons:   Vec<TextButton>,
}

impl Window {
    pub fn new() -> Self {
        Window {
            renderer:      UiRenderer::new(),
            overlay_alpha: 0.0,
            title:         None,
            buttons:       Vec::new(),
        }
    }

    /// Adds a semi-transparent full-screen dark overlay behind the window.
    pub fn with_overlay(mut self, alpha: f32) -> Self {
        self.overlay_alpha = alpha;
        self
    }

    /// Adds a text title drawn at the given [0,1] screen-space bounds.
    pub fn with_title(mut self, text: &str, bounds: (f32, f32, f32, f32)) -> Self {
        self.title = Some((create_text_texture(text), bounds));
        self
    }

    pub fn add_button(&mut self, btn: TextButton) {
        self.buttons.push(btn);
    }

    /// Returns the `id` of the first button hit by the click, or `None`.
    /// `nx` and `ny` are normalised screen coords in [0,1] (Y down).
    pub fn handle_click(&self, nx: f32, ny: f32) -> Option<&str> {
        self.buttons.iter()
            .find(|b| b.is_hit(nx, ny))
            .map(|b| b.id.as_str())
    }

    /// Finds a button by id so its label can be updated before drawing.
    pub fn button_mut(&mut self, id: &str) -> Option<&mut TextButton> {
        self.buttons.iter_mut().find(|b| b.id == id)
    }

    /// Draw a flat coloured rectangle in [0,1] screen space (Y down).
    /// Caller is responsible for enabling BLEND if transparency is needed.
    pub fn draw_rect(&self, x0: f32, y0: f32, x1: f32, y1: f32, r: f32, g: f32, b: f32, a: f32) {
        self.renderer.draw_rect(x0, y0, x1, y1, r, g, b, a);
    }

    /// Draw a text texture in [0,1] screen space (Y down).
    /// Caller is responsible for enabling BLEND.
    pub fn draw_text(&self, tex: &TextTexture, x0: f32, y0: f32, x1: f32, y1: f32) {
        self.renderer.draw_text(tex, x0, y0, x1, y1);
    }

    pub fn draw(&self) {
        unsafe {
            gl::Disable(gl::DEPTH_TEST);
            gl::Disable(gl::CULL_FACE);
            gl::Enable(gl::BLEND);
            gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);
        }

        if self.overlay_alpha > 0.0 {
            self.renderer.draw_rect(0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 0.0, self.overlay_alpha);
        }

        if let Some((ref tex, (x0, y0, x1, y1))) = self.title {
            self.renderer.draw_text(tex, x0, y0, x1, y1);
        }

        for btn in &self.buttons {
            btn.draw(&self.renderer);
        }

        unsafe {
            gl::Disable(gl::BLEND);
            gl::Enable(gl::CULL_FACE);
            gl::Enable(gl::DEPTH_TEST);
        }
    }
}
