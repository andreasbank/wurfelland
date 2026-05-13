use std::mem;
use std::os::raw::c_void;
use crate::renderer::utils::{compile_shader, link_program};

pub struct ChunkOutlineRenderer {
    vao: u32,
    vbo: u32,
    shader: u32,
    mvp_loc: i32,
}

impl ChunkOutlineRenderer {
    pub fn new() -> Self {
        const S: f32 = 16.0;
        #[rustfmt::skip]
        let verts: [f32; 72] = [
            // Bottom face
            0.0, 0.0, 0.0,   S,   0.0, 0.0,
            S,   0.0, 0.0,   S,   0.0, S,
            S,   0.0, S,     0.0, 0.0, S,
            0.0, 0.0, S,     0.0, 0.0, 0.0,
            // Top face
            0.0, S,   0.0,   S,   S,   0.0,
            S,   S,   0.0,   S,   S,   S,
            S,   S,   S,     0.0, S,   S,
            0.0, S,   S,     0.0, S,   0.0,
            // Vertical edges
            0.0, 0.0, 0.0,   0.0, S,   0.0,
            S,   0.0, 0.0,   S,   S,   0.0,
            S,   0.0, S,     S,   S,   S,
            0.0, 0.0, S,     0.0, S,   S,
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
                void main() { gl_Position = mvp * vec4(aPos, 1.0); }
            "#).unwrap();

            let frag = compile_shader(gl::FRAGMENT_SHADER, r#"#version 330 core
                out vec4 FragColor;
                void main() { FragColor = vec4(1.0, 0.0, 1.0, 0.6); }
            "#).unwrap();

            let shader = link_program(vert, frag).unwrap();
            let mvp_loc = gl::GetUniformLocation(shader, c"mvp".as_ptr());

            ChunkOutlineRenderer { vao, vbo, shader, mvp_loc }
        }
    }

    pub fn draw_chunks(&self, positions: &[[i32; 3]], view: &glam::Mat4, projection: &glam::Mat4) {
        unsafe {
            gl::UseProgram(self.shader);
            gl::Disable(gl::DEPTH_TEST);
            gl::Disable(gl::CULL_FACE);
            gl::Enable(gl::BLEND);
            gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);
            gl::BindVertexArray(self.vao);

            for &[cx, cy, cz] in positions {
                let t = glam::Mat4::from_translation(glam::Vec3::new(
                    cx as f32 * 16.0,
                    cy as f32 * 16.0,
                    cz as f32 * 16.0,
                ));
                let mvp = *projection * *view * t;
                gl::UniformMatrix4fv(self.mvp_loc, 1, gl::FALSE, mvp.to_cols_array().as_ptr());
                gl::DrawArrays(gl::LINES, 0, 24);
            }

            gl::BindVertexArray(0);
            gl::Enable(gl::DEPTH_TEST);
            gl::Enable(gl::CULL_FACE);
        }
    }
}

impl Drop for ChunkOutlineRenderer {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteVertexArrays(1, &self.vao);
            gl::DeleteBuffers(1, &self.vbo);
            gl::DeleteProgram(self.shader);
        }
    }
}
