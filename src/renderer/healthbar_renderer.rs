use std::mem;
use std::os::raw::c_void;
use std::ptr;

use crate::renderer::utils::{compile_shader, link_program};

pub struct HealthBar {
    vao: u32,
    vbo: u32,
    shader: u32,
    color_loc: i32,
    health_loc: i32,
}

impl HealthBar {
    pub fn new() -> Self {
        unsafe {
            // Background quad + fill quad share the same geometry.
            // A full-width bar from x=0.02 to x=0.22, y=0.92 to y=0.96 (screen [0,1] space).
            // We draw it twice: once full-width for the background, once scaled by health for the fill.
            let vertices: [f32; 8] = [
                0.02, 0.96, // top-left
                0.22, 0.96, // top-right
                0.22, 0.92, // bottom-right
                0.02, 0.92, // bottom-left
            ];

            let mut vao = 0;
            let mut vbo = 0;
            gl::GenVertexArrays(1, &mut vao);
            gl::GenBuffers(1, &mut vbo);

            gl::BindVertexArray(vao);
            gl::BindBuffer(gl::ARRAY_BUFFER, vbo);
            gl::BufferData(
                gl::ARRAY_BUFFER,
                (vertices.len() * mem::size_of::<f32>()) as isize,
                vertices.as_ptr() as *const c_void,
                gl::STATIC_DRAW,
            );

            gl::VertexAttribPointer(0, 2, gl::FLOAT, gl::FALSE, 0, ptr::null());
            gl::EnableVertexAttribArray(0);

            gl::BindBuffer(gl::ARRAY_BUFFER, 0);
            gl::BindVertexArray(0);

            let vert = compile_shader(gl::VERTEX_SHADER, r#"#version 330 core
                layout(location = 0) in vec2 aPos;
                uniform float health; // 0.0 to 1.0 — scales fill width around left edge
                uniform bool is_fill;
                void main() {
                    vec2 p = aPos;
                    if (is_fill) {
                        // Scale x distance from left edge (0.02) by health
                        p.x = 0.02 + (aPos.x - 0.02) * health;
                    }
                    gl_Position = vec4(p.x * 2.0 - 1.0, -(p.y * 2.0 - 1.0), 0.0, 1.0);
                }"#).unwrap();

            let frag = compile_shader(gl::FRAGMENT_SHADER, r#"#version 330 core
                out vec4 FragColor;
                uniform vec3 color;
                void main() {
                    FragColor = vec4(color, 1.0);
                }"#).unwrap();

            let shader = link_program(vert, frag).unwrap();

            let color_loc  = gl::GetUniformLocation(shader, c"color".as_ptr());
            let health_loc = gl::GetUniformLocation(shader, c"health".as_ptr());

            HealthBar { vao, vbo, shader, color_loc, health_loc }
        }
    }

    // health_fraction: 0.0 (dead) to 1.0 (full)
    pub fn draw(&self, health_fraction: f32) {
        unsafe {
            gl::Disable(gl::DEPTH_TEST);
            gl::UseProgram(self.shader);
            gl::BindVertexArray(self.vao);

            let is_fill_loc = gl::GetUniformLocation(self.shader, c"is_fill".as_ptr());

            // Draw dark background (full width)
            gl::Uniform1i(is_fill_loc, 0);
            gl::Uniform3f(self.color_loc, 0.2, 0.2, 0.2);
            gl::Uniform1f(self.health_loc, 1.0);
            gl::DrawArrays(gl::TRIANGLE_FAN, 0, 4);

            // Draw colored fill (scaled by health)
            gl::Uniform1i(is_fill_loc, 1);
            gl::Uniform1f(self.health_loc, health_fraction.clamp(0.0, 1.0));
            // Green -> yellow -> red based on health
            let r = (1.0 - health_fraction) * 2.0;
            let g = health_fraction * 2.0;
            gl::Uniform3f(self.color_loc, r.min(1.0), g.min(1.0), 0.0);
            gl::DrawArrays(gl::TRIANGLE_FAN, 0, 4);

            gl::Enable(gl::DEPTH_TEST);
        }
    }
}

impl Drop for HealthBar {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteVertexArrays(1, &self.vao);
            gl::DeleteBuffers(1, &self.vbo);
            gl::DeleteProgram(self.shader);
        }
    }
}
