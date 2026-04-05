use gl::types::*;
use std::mem;
use std::os::raw::c_void;
use crate::renderer::utils::{compile_shader, link_program};

pub struct BlockOutlineRenderer {
    vao: u32,
    vbo: u32,
    shader: u32,
    mvp_loc: i32,
}

impl BlockOutlineRenderer {
    pub fn new() -> Self {
        // 12 edges of a unit cube, each as 2 vertices = 24 positions
        // Slightly expanded by 0.005 to avoid z-fighting
        const E: f32 = 0.005;
        #[rustfmt::skip]
        let verts: [f32; 72] = [
            // Bottom face edges
            -E, -E, -E,   1.0+E, -E, -E,
            1.0+E, -E, -E,   1.0+E, -E, 1.0+E,
            1.0+E, -E, 1.0+E,   -E, -E, 1.0+E,
            -E, -E, 1.0+E,   -E, -E, -E,
            // Top face edges
            -E, 1.0+E, -E,   1.0+E, 1.0+E, -E,
            1.0+E, 1.0+E, -E,   1.0+E, 1.0+E, 1.0+E,
            1.0+E, 1.0+E, 1.0+E,   -E, 1.0+E, 1.0+E,
            -E, 1.0+E, 1.0+E,   -E, 1.0+E, -E,
            // Vertical edges
            -E, -E, -E,   -E, 1.0+E, -E,
            1.0+E, -E, -E,   1.0+E, 1.0+E, -E,
            1.0+E, -E, 1.0+E,   1.0+E, 1.0+E, 1.0+E,
            -E, -E, 1.0+E,   -E, 1.0+E, 1.0+E,
        ];

        unsafe {
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
            gl::VertexAttribPointer(0, 3, gl::FLOAT, gl::FALSE,
                (3 * mem::size_of::<f32>()) as i32, std::ptr::null());
            gl::EnableVertexAttribArray(0);
            gl::BindBuffer(gl::ARRAY_BUFFER, 0);
            gl::BindVertexArray(0);

            let vert = compile_shader(gl::VERTEX_SHADER, r#"#version 330 core
                layout(location = 0) in vec3 aPos;
                uniform mat4 mvp;
                void main() {
                    gl_Position = mvp * vec4(aPos, 1.0);
                }"#).unwrap();

            let frag = compile_shader(gl::FRAGMENT_SHADER, r#"#version 330 core
                out vec4 FragColor;
                void main() {
                    FragColor = vec4(0.75, 0.75, 0.75, 1.0);
                }"#).unwrap();

            let shader = link_program(vert, frag).unwrap();
            let mvp_loc = gl::GetUniformLocation(shader, c"mvp".as_ptr());

            BlockOutlineRenderer { vao, vbo, shader, mvp_loc }
        }
    }

    pub fn draw(&self, block_pos: [i32; 3], view: &glam::Mat4, projection: &glam::Mat4) {
        let translation = glam::Mat4::from_translation(glam::Vec3::new(
            block_pos[0] as f32,
            block_pos[1] as f32,
            block_pos[2] as f32,
        ));
        let mvp = *projection * *view * translation;

        unsafe {
            gl::Disable(gl::DEPTH_TEST);
            gl::UseProgram(self.shader);
            gl::UniformMatrix4fv(self.mvp_loc, 1, gl::FALSE, mvp.to_cols_array().as_ptr());
            gl::BindVertexArray(self.vao);
            gl::DrawArrays(gl::LINES, 0, 24);
            gl::BindVertexArray(0);
            gl::Enable(gl::DEPTH_TEST);
        }
    }
}

impl Drop for BlockOutlineRenderer {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteVertexArrays(1, &self.vao);
            gl::DeleteBuffers(1, &self.vbo);
            gl::DeleteProgram(self.shader);
        }
    }
}
