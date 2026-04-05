use std::collections::HashMap;
use std::ffi::CString;
use std::mem;
use std::os::raw::c_void;
use std::string;
use gl::types::*;
use std::ptr;

use crate::renderer::utils::compile_shader;
use crate::world::Chunk;
use crate::world::BlockType;

pub struct BlockRenderer {
    pub vao: u32,          // ONE vertex array (cube geometry)
    pub vbo: u32,          // ONE vertex buffer  
    pub shader: u32,       // ONE shader
    pub view_loc: i32,
    pub projection_loc: i32,
    pub textures: HashMap<BlockType, u32>, // Multiple textures
}

impl BlockRenderer {
    pub fn new() -> Self {
        let mut vao = 0;
        let mut vbo = 0;

        let vertices: [f32; 216] = [
            // Positions          // Colors
            -0.5, -0.5, -0.5,    1.0, 0.0, 0.0,
             0.5, -0.5, -0.5,    1.0, 0.0, 0.0,
             0.5,  0.5, -0.5,    1.0, 0.0, 0.0,
             0.5,  0.5, -0.5,    1.0, 0.0, 0.0,
            -0.5,  0.5, -0.5,    1.0, 0.0, 0.0,
            -0.5, -0.5, -0.5,    1.0, 0.0, 0.0,
            
            -0.5, -0.5,  0.5,    0.0, 1.0, 0.0,
             0.5, -0.5,  0.5,    0.0, 1.0, 0.0,
             0.5,  0.5,  0.5,    0.0, 1.0, 0.0,
             0.5,  0.5,  0.5,    0.0, 1.0, 0.0,
            -0.5,  0.5,  0.5,    0.0, 1.0, 0.0,
            -0.5, -0.5,  0.5,    0.0, 1.0, 0.0,
            
            -0.5,  0.5,  0.5,    0.0, 0.0, 1.0,
            -0.5,  0.5, -0.5,    0.0, 0.0, 1.0,
            -0.5, -0.5, -0.5,    0.0, 0.0, 1.0,
            -0.5, -0.5, -0.5,    0.0, 0.0, 1.0,
            -0.5, -0.5,  0.5,    0.0, 0.0, 1.0,
            -0.5,  0.5,  0.5,    0.0, 0.0, 1.0,
            
             0.5,  0.5,  0.5,    1.0, 1.0, 0.0,
             0.5,  0.5, -0.5,    1.0, 1.0, 0.0,
             0.5, -0.5, -0.5,    1.0, 1.0, 0.0,
             0.5, -0.5, -0.5,    1.0, 1.0, 0.0,
             0.5, -0.5,  0.5,    1.0, 1.0, 0.0,
             0.5,  0.5,  0.5,    1.0, 1.0, 0.0,
            
            -0.5, -0.5, -0.5,    1.0, 0.0, 1.0,
             0.5, -0.5, -0.5,    1.0, 0.0, 1.0,
             0.5, -0.5,  0.5,    1.0, 0.0, 1.0,
             0.5, -0.5,  0.5,    1.0, 0.0, 1.0,
            -0.5, -0.5,  0.5,    1.0, 0.0, 1.0,
            -0.5, -0.5, -0.5,    1.0, 0.0, 1.0,
            
            -0.5,  0.5, -0.5,    0.0, 1.0, 1.0,
             0.5,  0.5, -0.5,    0.0, 1.0, 1.0,
             0.5,  0.5,  0.5,    0.0, 1.0, 1.0,
             0.5,  0.5,  0.5,    0.0, 1.0, 1.0,
            -0.5,  0.5,  0.5,    0.0, 1.0, 1.0,
            -0.5,  0.5, -0.5,    0.0, 1.0, 1.0,
        ];

        unsafe {
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

            gl::VertexAttribPointer(0,
                                    3,
                                    gl::FLOAT,
                                    gl::FALSE,
                                    (6 * mem::size_of::<f32>()) as GLsizei,
                                    ptr::null());
            gl::EnableVertexAttribArray(0);
            
            gl::VertexAttribPointer(
                1,
                3,
                gl::FLOAT,
                gl::FALSE,
                (6 * mem::size_of::<f32>()) as GLsizei,
                (3 * mem::size_of::<f32>()) as *const c_void,
            );
            gl::EnableVertexAttribArray(1);

            // Unbind
            gl::BindBuffer(gl::ARRAY_BUFFER, 0);
            gl::BindVertexArray(0);

            // Shaders
            let vert_src = String::from( r#"
                #version 330 core
                layout(location = 0) in vec3 aPos;
                layout(location = 1) in vec3 aColor;

                out vec3 ourColor;

                uniform mat4 model;
                uniform mat4 view;
                uniform mat4 projection;

                void main()
                {
                    gl_Position = projection * view * model * vec4(aPos, 1.0);
                    ourColor = aColor;
                }
            "#);
        
            let frag_src = String::from(r#"
                #version 330 core
                in vec3 ourColor;
                out vec4 FragColor;

                void main()
                {
                    FragColor = vec4(ourColor, 1.0);
                }
            "#);
        
            // Compile shaders
            let vert_shader = compile_shader(gl::VERTEX_SHADER, vert_src.as_str()).unwrap();
            let frag_shader = compile_shader(gl::FRAGMENT_SHADER, frag_src.as_str()).unwrap();

            // Create program
            let program = gl::CreateProgram();
            gl::AttachShader(program, vert_shader);
            gl::AttachShader(program, frag_shader);
            gl::LinkProgram(program);

            // Check program linkin
            let mut success = 0;
            gl::GetProgramiv(program, gl::LINK_STATUS, &mut success);
            if success == 0 {
                let mut len = 0;
                gl::GetProgramiv(program, gl::INFO_LOG_LENGTH, &mut len);
                let mut buffer = vec![0u8; len as usize];
                gl::GetProgramInfoLog(program, len, ptr::null_mut(), buffer.as_mut_ptr() as *mut GLchar);
                panic!("Program linking error: {}", String::from_utf8_lossy(&buffer));
            }        

            gl::DeleteShader(vert_shader);
            gl::DeleteShader(frag_shader);

            let textures = HashMap::new();
            // we'll skip texts for now

            // Get uniform locations
            gl::UseProgram(program);
            let view_loc = gl::GetUniformLocation(program, CString::new("view").unwrap().as_ptr());
            let projection_loc = gl::GetUniformLocation(program, CString::new("projection").unwrap().as_ptr()); 

            BlockRenderer { vao, vbo, shader: program, view_loc, projection_loc, textures }
        }
    }

    pub fn set_view_projection(&self, view: &glam::Mat4, projection: &glam::Mat4) {
        unsafe {
            gl::UseProgram(self.shader);
            gl::UniformMatrix4fv(self.view_loc, 1, gl::FALSE, view.as_ref().as_ptr());
            gl::UniformMatrix4fv(self.projection_loc, 1, gl::FALSE, projection.as_ref().as_ptr());
        }
    }
        
    pub fn draw_block(&self, x: f32, y: f32, z: f32) {
        unsafe {
            gl::UseProgram(self.shader);
            let model_loc = gl::GetUniformLocation(self.shader, CString::new("model").unwrap().as_ptr());
            gl::BindVertexArray(self.vao);
            let model_tmp = glam::Mat4::from_translation(glam::Vec3::new(x, y, z));
            gl::UniformMatrix4fv(model_loc, 1, gl::FALSE, model_tmp.as_ref().as_ptr());
            gl::DrawArrays(gl::TRIANGLES, 0, 36);
        }
    }
}

impl Drop for BlockRenderer {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteVertexArrays(1, &self.vao);
            gl::DeleteBuffers(1, &self.vbo);
            gl::DeleteProgram(self.shader);
            // Also delete textures in the future
        }
    }
}