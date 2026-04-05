use std::ffi::CString;
use std::mem;
use std::os::raw::c_void;
use std::string;
use gl::types::*;
use std::ptr;

use crate::renderer::utils::compile_shader;
use crate::renderer::utils::link_program;

pub struct Crosshair {
    vao: u32,
    vbo: u32,
    shader: u32,
}

impl Crosshair {
    pub fn new() -> Self {
        unsafe {
            // Simple vertex: center of screen in normalized [0,1] coordinates
            let vertices: [f32; 2] = [0.5, 0.5]; // Exactly center
            
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
            
            gl::VertexAttribPointer(0, 2, gl::FLOAT, gl::FALSE, 0, ptr::null());
            gl::EnableVertexAttribArray(0);
            
            gl::BindBuffer(gl::ARRAY_BUFFER, 0);
            gl::BindVertexArray(0);
            
            // 2D shader (converts [0,1] to clip space [-1,1])
            let shader = compile_shader(gl::VERTEX_SHADER,
                r#"#version 330 core
                layout(location = 0) in vec2 aPos;
                void main() {
                    // Convert from [0,1] to [-1,1] with Y flipped
                    gl_Position = vec4(aPos.x * 2.0 - 1.0, 
                                      -(aPos.y * 2.0 - 1.0), 
                                      0.0, 1.0);
                }"#).unwrap();
            let frag_shader = compile_shader(gl::FRAGMENT_SHADER,
                r#"#version 330 core
                out vec4 FragColor;
                uniform vec3 color;
                void main() {
                    FragColor = vec4(color, 1.0);
                }"#).unwrap();
            
            let shader = link_program(shader, frag_shader).unwrap();
            
            Crosshair { vao, vbo, shader }
        }
    }
    
    pub fn draw(&self) {
        unsafe {
            // Disable depth test so crosshair draws on top
            gl::Disable(gl::DEPTH_TEST);
            
            gl::UseProgram(self.shader);
            
            // Set color (red crosshair)
            let color_loc = gl::GetUniformLocation(self.shader, c"color".as_ptr());
            gl::Uniform3f(color_loc, 1.0, 0.0, 0.0);
            
            // Draw point
            gl::BindVertexArray(self.vao);
            gl::PointSize(4.0); // Make it visible
            gl::DrawArrays(gl::POINTS, 0, 1);
            
            // Re-enable depth test for 3D
            gl::Enable(gl::DEPTH_TEST);
        }
    }
}