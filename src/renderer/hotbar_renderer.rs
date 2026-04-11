use std::mem;
use std::os::raw::c_void;
use crate::renderer::utils::{compile_shader, link_program};
use crate::world::item::ItemType;

const SLOT_SIZE:   f32 = 40.0;
const SLOT_GAP:    f32 =  4.0;
const BORDER:      f32 =  2.0;
const SEL_BORDER:  f32 =  3.0;
const MARGIN_BOT:  f32 = 10.0;
const NUM_SLOTS:   usize = 9;

pub struct HotbarRenderer {
    vao: u32,
    shader: u32,
    pos_loc: i32,
    size_loc: i32,
    screen_loc: i32,
    color_loc: i32,
}

impl HotbarRenderer {
    pub fn new() -> Self {
        // Unit quad [0,0]→[1,1] in two triangles
        #[rustfmt::skip]
        let verts: [f32; 12] = [
            0.0, 0.0,  1.0, 0.0,  1.0, 1.0,
            0.0, 0.0,  1.0, 1.0,  0.0, 1.0,
        ];

        let vert = compile_shader(gl::VERTEX_SHADER, r#"#version 330 core
            layout(location = 0) in vec2 aPos;
            uniform vec2 u_pos;
            uniform vec2 u_size;
            uniform vec2 u_screen;
            void main() {
                vec2 px  = u_pos + aPos * u_size;
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

        unsafe {
            let pos_loc    = gl::GetUniformLocation(shader, c"u_pos".as_ptr());
            let size_loc   = gl::GetUniformLocation(shader, c"u_size".as_ptr());
            let screen_loc = gl::GetUniformLocation(shader, c"u_screen".as_ptr());
            let color_loc  = gl::GetUniformLocation(shader, c"u_color".as_ptr());

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

            HotbarRenderer { vao, shader, pos_loc, size_loc, screen_loc, color_loc }
        }
    }

    fn draw_rect(&self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4], screen: [f32; 2]) {
        unsafe {
            gl::Uniform2f(self.pos_loc,    x, y);
            gl::Uniform2f(self.size_loc,   w, h);
            gl::Uniform2f(self.screen_loc, screen[0], screen[1]);
            gl::Uniform4f(self.color_loc,  color[0], color[1], color[2], color[3]);
            gl::DrawArrays(gl::TRIANGLES, 0, 6);
        }
    }

    pub fn draw(
        &self,
        selected: usize,
        slots: &[Option<ItemType>; NUM_SLOTS],
        screen_w: f32,
        screen_h: f32,
    ) {
        let screen = [screen_w, screen_h];

        // Total bar width, centered on screen
        let bar_w = NUM_SLOTS as f32 * SLOT_SIZE + (NUM_SLOTS - 1) as f32 * SLOT_GAP;
        let start_x = (screen_w - bar_w) * 0.5;
        let start_y = MARGIN_BOT;

        unsafe {
            gl::Disable(gl::DEPTH_TEST);
            gl::Enable(gl::BLEND);
            gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);
            gl::UseProgram(self.shader);
            gl::BindVertexArray(self.vao);

            for i in 0..NUM_SLOTS {
                let sx = start_x + i as f32 * (SLOT_SIZE + SLOT_GAP);
                let sy = start_y;
                let is_sel = i == selected;

                let border = if is_sel { SEL_BORDER } else { BORDER };
                let bdr_color: [f32; 4] = if is_sel {
                    [1.0, 1.0, 1.0, 1.0]
                } else {
                    [0.55, 0.55, 0.55, 0.90]
                };

                // Border (drawn slightly larger than the slot)
                self.draw_rect(
                    sx - border, sy - border,
                    SLOT_SIZE + border * 2.0, SLOT_SIZE + border * 2.0,
                    bdr_color, screen,
                );
                // Background
                self.draw_rect(sx, sy, SLOT_SIZE, SLOT_SIZE,
                    [0.18, 0.18, 0.18, 0.75], screen);

                // Item icon — a smaller colored square centered in the slot
                if let Some(item) = slots[i] {
                    let [r, g, b] = item.color();
                    let pad = 7.0;
                    self.draw_rect(
                        sx + pad, sy + pad,
                        SLOT_SIZE - pad * 2.0, SLOT_SIZE - pad * 2.0,
                        [r, g, b, 1.0], screen,
                    );
                }
            }

            gl::BindVertexArray(0);
            gl::Disable(gl::BLEND);
            gl::Enable(gl::DEPTH_TEST);
        }
    }
}

impl Drop for HotbarRenderer {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteVertexArrays(1, &self.vao);
            gl::DeleteProgram(self.shader);
        }
    }
}
