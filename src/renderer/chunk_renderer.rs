use crate::renderer::utils::{compile_shader, create_block_atlas};
use gl::types::{GLchar, GLuint};
use std::ptr;
use crate::world::chunk::Chunk;
use crate::camera::Camera;
use glam::Vec3;

struct UniformLocations {
    model: i32,
    view: i32,
    projection: i32,
    use_textures: i32,
    fog_start: i32,
    fog_end: i32,
    transparent_pass: i32,
}

// Main ChunkRenderer struct
pub struct ChunkRenderer {
    shader: u32,
    texture_atlas: u32,
    uniforms: UniformLocations,  // Cache uniform locations
}

impl ChunkRenderer {
    pub fn new() -> Result<Self, String> {
        unsafe {
            // Compile and link shaders
            let vertex_shader = compile_shader(
                gl::VERTEX_SHADER,
                r#"#version 330 core
                layout(location = 0) in vec3 aPos;
                layout(location = 1) in vec3 aColor;
                layout(location = 2) in vec2 aTexCoord;

                out vec3 ourColor;
                out vec2 TexCoord;
                out float fragDist;

                uniform mat4 model;
                uniform mat4 view;
                uniform mat4 projection;

                void main() {
                    vec4 worldPos = model * vec4(aPos, 1.0);
                    vec4 viewPos = view * worldPos;
                    gl_Position = projection * viewPos;
                    ourColor = aColor;
                    TexCoord = aTexCoord;
                    fragDist = abs(viewPos.z);
                }"#
            )?;

            let fragment_shader = compile_shader(
                gl::FRAGMENT_SHADER,
                r#"#version 330 core
                in vec3 ourColor;
                in vec2 TexCoord;
                in float fragDist;
                out vec4 FragColor;

                uniform sampler2D texture_atlas;
                uniform bool use_textures;
                uniform float fog_start;
                uniform float fog_end;
                uniform bool transparent_pass;

                const vec3 FOG_COLOR = vec3(0.53, 0.81, 0.92);

                void main() {
                    vec4 texSample;
                    if (use_textures) {
                        texSample = texture(texture_atlas, TexCoord);
                        if (texSample.a < 0.1) discard;
                    } else {
                        texSample = vec4(1.0);
                    }
                    // Opaque pass: skip semi-transparent fragments.
                    // Transparent pass: skip fully opaque fragments.
                    if (!transparent_pass && texSample.a < 0.99) discard;
                    if ( transparent_pass && texSample.a >= 0.99) discard;
                    vec3 color = texSample.rgb * ourColor;
                    float fog_factor = clamp((fragDist - fog_start) / (fog_end - fog_start), 0.0, 1.0);
                    float alpha = mix(texSample.a, 1.0, fog_factor);
                    FragColor = vec4(mix(color, FOG_COLOR, fog_factor), alpha);
                }"#
            )?;
            
            let shader = gl::CreateProgram();
            gl::AttachShader(shader, vertex_shader);
            gl::AttachShader(shader, fragment_shader);
            gl::LinkProgram(shader);
            
            // Check linking errors
            let mut success = 0;
            gl::GetProgramiv(shader, gl::LINK_STATUS, &mut success);
            if success == 0 {
                let mut len = 0;
                gl::GetProgramiv(shader, gl::INFO_LOG_LENGTH, &mut len);
                let mut buffer = vec![0u8; len as usize];
                gl::GetProgramInfoLog(shader, len, ptr::null_mut(), buffer.as_mut_ptr() as *mut GLchar);
                return Err(format!("Shader linking failed: {}", String::from_utf8_lossy(&buffer)));
            }
            
            // Clean up shaders
            gl::DeleteShader(vertex_shader);
            gl::DeleteShader(fragment_shader);
            
            // 2. Get uniform locations
            let model_loc        = gl::GetUniformLocation(shader, c"model".as_ptr());
            let view_loc         = gl::GetUniformLocation(shader, c"view".as_ptr());
            let projection_loc   = gl::GetUniformLocation(shader, c"projection".as_ptr());
            let use_textures_loc = gl::GetUniformLocation(shader, c"use_textures".as_ptr());
            let fog_start_loc        = gl::GetUniformLocation(shader, c"fog_start".as_ptr());
            let fog_end_loc          = gl::GetUniformLocation(shader, c"fog_end".as_ptr());
            let transparent_pass_loc = gl::GetUniformLocation(shader, c"transparent_pass".as_ptr());

            let texture_atlas = create_block_atlas();

            Ok(ChunkRenderer {
                shader,
                texture_atlas,
                uniforms: UniformLocations {
                    model: model_loc,
                    view: view_loc,
                    projection: projection_loc,
                    use_textures: use_textures_loc,
                    fog_start: fog_start_loc,
                    fog_end: fog_end_loc,
                    transparent_pass: transparent_pass_loc,
                },
            })
        }
    }

    pub fn begin_frame(&self, view: &glam::Mat4, projection: &glam::Mat4) {
        unsafe {
            gl::UseProgram(self.shader);
            gl::UniformMatrix4fv(self.uniforms.view, 1, gl::FALSE, view.as_ref().as_ptr());
            gl::UniformMatrix4fv(self.uniforms.projection, 1, gl::FALSE, projection.as_ref().as_ptr());
            gl::Uniform1i(self.uniforms.use_textures, 1);
            gl::Uniform1f(self.uniforms.fog_start, 32.0);
            gl::Uniform1f(self.uniforms.fog_end,   64.0);
            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, self.texture_atlas);
            gl::Enable(gl::DEPTH_TEST);
            gl::Enable(gl::CULL_FACE);
            gl::CullFace(gl::BACK);
            gl::Enable(gl::BLEND);
            gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);
        }
    }
    
    pub fn set_transparent_pass(&self, enabled: bool) {
        unsafe {
            gl::Uniform1i(self.uniforms.transparent_pass, enabled as i32);
            if enabled {
                gl::DepthMask(gl::FALSE);   // don't write depth for water
                gl::Disable(gl::CULL_FACE); // show water faces from inside too
            } else {
                gl::DepthMask(gl::TRUE);
                gl::Enable(gl::CULL_FACE);
                gl::CullFace(gl::BACK);
            }
        }
    }

    pub fn end_frame(&self) {
        unsafe {
            gl::DepthMask(gl::TRUE);
            gl::Disable(gl::BLEND);
            gl::BindVertexArray(0);
            gl::UseProgram(0);
        }
    }

    pub fn draw_chunk(&self, chunk: &Chunk) {
        // Skip if chunk has no mesh (not built yet)
        let mesh = match &chunk.mesh {
            Some(m) => m,
            None => return,
        };
        
        unsafe {
            // 1. Set model matrix (chunk's world position)
            let model = chunk.model_matrix();
            gl::UniformMatrix4fv(self.uniforms.model, 1, gl::FALSE, model.as_ref().as_ptr());
            
            // 2. Bind chunk's VAO (mesh data)
            gl::BindVertexArray(mesh.vao);
            
            // 3. Draw
            gl::DrawArrays(gl::TRIANGLES, 0, mesh.vertex_count);
            
            // Optional: unbind VAO (not strictly needed)
            // gl::BindVertexArray(0);
        }
    }
    
    // If chunk position is separate:
    pub fn draw_chunk_at(&self, chunk: &Chunk, position: &glam::Mat4) {
        let mesh = match &chunk.mesh {
            Some(m) => m,
            None => return,
        };
        
        unsafe {
            gl::UniformMatrix4fv(self.uniforms.model, 1, gl::FALSE, position.as_ref().as_ptr());
            gl::BindVertexArray(mesh.vao);
            gl::DrawArrays(gl::TRIANGLES, 0, mesh.vertex_count);
        }
    }

    pub fn draw_chunks(&self, chunks: &[&Chunk]) {
        unsafe {
            // Count draw calls for debugging
            let mut draw_calls = 0;
            
            for chunk in chunks {
                // Skip chunks without mesh
                let mesh = match &chunk.mesh {
                    Some(m) => m,
                    None => continue,
                };
                
                // Set model matrix for this chunk
                let model = chunk.model_matrix();
                gl::UniformMatrix4fv(self.uniforms.model, 1, gl::FALSE, model.as_ref().as_ptr());
                
                // Bind and draw
                gl::BindVertexArray(mesh.vao);
                gl::DrawArrays(gl::TRIANGLES, 0, mesh.vertex_count);
                
                draw_calls += 1;
            }
            
            // Optional: print draw call count (debug)
            // println!("Drew {} chunks in {} draw calls", chunks.len(), draw_calls);
        }
    }
    
    // Alternative: With frustum culling
    pub fn draw_chunks_with_culling(&self, chunks: &[&Chunk], camera: &Camera) {
        let frustum = camera.frustum();
        
        for chunk in chunks {
            // Skip if chunk not visible
            if !chunk.is_in_frustum(&frustum) {
                continue;
            }
            
            // Draw as before...
            if let Some(mesh) = &chunk.mesh {
                unsafe {
                    let model = chunk.model_matrix();
                    gl::UniformMatrix4fv(self.uniforms.model, 1, gl::FALSE, model.as_ref().as_ptr());
                    gl::BindVertexArray(mesh.vao);
                    gl::DrawArrays(gl::TRIANGLES, 0, mesh.vertex_count);
                }
            }
        }
    }
    
    // Alternative: Batch by distance (near to far for transparency)
    pub fn draw_chunks_sorted(&self, chunks: &mut [&Chunk], camera_pos: Vec3) {
        // Sort chunks by distance from camera (front to back for opaque)
        chunks.sort_by_key(|chunk| {
            let chunk_center = chunk.center();
            (camera_pos.distance_squared(chunk_center) * 1000.0) as i32
        });
        
        self.draw_chunks(chunks);
    }
}

impl Drop for ChunkRenderer {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteProgram(self.shader);
            gl::DeleteTextures(1, &self.texture_atlas);
        }
    }
}