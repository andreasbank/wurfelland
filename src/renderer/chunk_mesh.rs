use std::mem;
use std::os::raw::c_void;
use std::ptr;
use gl::types::*;

pub struct ChunkMesh {
    pub vao: u32,           // Vertex Array Object
    pub vbo: u32,           // Vertex Buffer Object
    pub vertex_count: i32,  // How many vertices to draw
}

impl ChunkMesh {
    pub fn from_vertices(vertices: &[f32]) -> Self {
        unsafe {
            let mut vao = 0;
            let mut vbo = 0;
            gl::GenVertexArrays(1, &mut vao);
            gl::GenBuffers(1, &mut vbo);
            
            gl::BindVertexArray(vao);
            gl::BindBuffer(gl::ARRAY_BUFFER, vbo);
            gl::BufferData(
                gl::ARRAY_BUFFER,
                (vertices.len() * mem::size_of::<f32>()) as isize,
                vertices.as_ptr() as *const _,
                gl::STATIC_DRAW,
            );
            
            // Layout: [x, y, z, r, g, b, u, v] = 8 floats per vertex
            let stride = (8 * mem::size_of::<f32>()) as GLsizei;

            // location 0: position (3 floats)
            gl::VertexAttribPointer(0, 3, gl::FLOAT, gl::FALSE, stride, ptr::null());
            gl::EnableVertexAttribArray(0);

            // location 1: color (3 floats, offset 3)
            gl::VertexAttribPointer(1, 3, gl::FLOAT, gl::FALSE, stride, (3 * mem::size_of::<f32>()) as *const c_void);
            gl::EnableVertexAttribArray(1);

            // location 2: tex coords (2 floats, offset 6)
            gl::VertexAttribPointer(2, 2, gl::FLOAT, gl::FALSE, stride, (6 * mem::size_of::<f32>()) as *const c_void);
            gl::EnableVertexAttribArray(2);

            gl::BindBuffer(gl::ARRAY_BUFFER, 0);
            gl::BindVertexArray(0);

            ChunkMesh {
                vao,
                vbo,
                vertex_count: vertices.len() as i32 / 8, // 8 floats per vertex
            }
        }
    }
    
    pub fn cleanup(&self) {
        unsafe {
            gl::DeleteVertexArrays(1, &self.vao);
            gl::DeleteBuffers(1, &self.vbo);
        }
    }
}

impl Drop for ChunkMesh {
    fn drop(&mut self) {
        self.cleanup();
    }
}