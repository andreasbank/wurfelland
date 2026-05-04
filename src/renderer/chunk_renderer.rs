use crate::renderer::shadow_pass::NUM_CASCADES;
use crate::renderer::utils::{compile_shader, create_block_atlas};
use gl::types::GLchar;
use std::ptr;
use crate::world::chunk::Chunk;

struct UniformLocations {
    model: i32,
    view: i32,
    projection: i32,
    use_textures: i32,
    fog_start: i32,
    fog_end: i32,
    transparent_pass: i32,
    /// Base location of `lightSpaceMatrices[0]`; the array is contiguous so we
    /// set all NUM_CASCADES with a single UniformMatrix4fv call.
    light_space_matrices: i32,
    shadow_maps: i32,        // sampler2DArray (one layer per cascade)
    cascade_ends: i32,       // base of float array
    shadow_texel_sizes: i32, // base of float array
    light_dir: i32,
    fog_color: i32,
    ambient_light: i32,
    directional_light: i32,
}

pub struct ChunkRenderer {
    shader: u32,
    texture_atlas: u32,
    uniforms: UniformLocations,
}

impl ChunkRenderer {
    pub fn new() -> Result<Self, String> {
        unsafe {
            // Vertex shader: pass world position and normal through to the
            // fragment shader, which picks the cascade per fragment.
            let vertex_shader = compile_shader(
                gl::VERTEX_SHADER,
                r#"#version 330 core
                layout(location = 0) in vec3 aPos;
                layout(location = 1) in vec3 aColor;
                layout(location = 2) in vec2 aTexCoord;
                layout(location = 3) in vec3 aNormal;

                out vec3 ourColor;
                out vec2 TexCoord;
                out float fragDist;
                out vec3 vWorldPos;
                out vec3 vNormal;

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
                    vWorldPos = worldPos.xyz;
                    // Chunks are translation-only, so mat3(model) == identity.
                    vNormal = mat3(model) * aNormal;
                }"#
            )?;

            let fragment_shader = compile_shader(
                gl::FRAGMENT_SHADER,
                r#"#version 330 core
                #define NUM_CASCADES 3

                in vec3 ourColor;
                in vec2 TexCoord;
                in float fragDist;
                in vec3 vWorldPos;
                in vec3 vNormal;
                out vec4 FragColor;

                uniform sampler2D texture_atlas;
                uniform sampler2DArray shadowMaps;
                uniform mat4  lightSpaceMatrices[NUM_CASCADES];
                uniform float cascadeEnds[NUM_CASCADES];
                uniform float shadowTexelSizes[NUM_CASCADES];
                uniform bool  use_textures;
                uniform float fog_start;
                uniform float fog_end;
                uniform vec3  fog_color;
                uniform bool  transparent_pass;
                uniform vec3  lightDir;
                uniform float ambientLight;     // base illumination, day/night dependent
                uniform float directionalLight; // sun/moon contribution, gated by shadow

                float calcShadow(vec3 worldPos, vec3 normal, float viewDist) {
                    // Back-faces are already in shadow — no map lookup needed.
                    if (dot(normalize(normal), -normalize(lightDir)) <= 0.0) return 1.0;

                    // Pick cascade by view-space distance: closer cascades are
                    // sharper, farther ones cover more ground.
                    int cascade = NUM_CASCADES - 1;
                    for (int i = 0; i < NUM_CASCADES - 1; i++) {
                        if (viewDist < cascadeEnds[i]) { cascade = i; break; }
                    }

                    // Receiver-side normal offset matched to this cascade's
                    // texel size — closes contact gaps without depth bias.
                    vec3 offsetWorld = worldPos + normalize(normal) * shadowTexelSizes[cascade];
                    vec4 fragPosLS = lightSpaceMatrices[cascade] * vec4(offsetWorld, 1.0);

                    vec3 projCoords = fragPosLS.xyz / fragPosLS.w;
                    projCoords = projCoords * 0.5 + 0.5;

                    if (projCoords.z > 1.0) return 0.0;
                    float currentDepth = projCoords.z;

                    // 3x3 PCF in this cascade's depth layer.
                    float shadow = 0.0;
                    vec2 texelSize = 1.0 / vec2(textureSize(shadowMaps, 0).xy);
                    for (int x = -1; x <= 1; ++x) {
                        for (int y = -1; y <= 1; ++y) {
                            float pcfDepth = texture(
                                shadowMaps,
                                vec3(projCoords.xy + vec2(x, y) * texelSize, cascade)
                            ).r;
                            shadow += currentDepth > pcfDepth ? 1.0 : 0.0;
                        }
                    }
                    return shadow / 9.0;
                }

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

                    float shadow = calcShadow(vWorldPos, vNormal, fragDist);
                    // Light = ambient + directional * (1 - shadow). At night the
                    // directional term is small, so shadows naturally fade out.
                    float light = ambientLight + directionalLight * (1.0 - shadow);
                    vec3 color = texSample.rgb * ourColor * light;

                    float fog_factor = clamp((fragDist - fog_start) / (fog_end - fog_start), 0.0, 1.0);
                    float alpha = mix(texSample.a, 1.0, fog_factor);
                    FragColor = vec4(mix(color, fog_color, fog_factor), alpha);
                }"#
            )?;

            let shader = gl::CreateProgram();
            gl::AttachShader(shader, vertex_shader);
            gl::AttachShader(shader, fragment_shader);
            gl::LinkProgram(shader);

            let mut success = 0;
            gl::GetProgramiv(shader, gl::LINK_STATUS, &mut success);
            if success == 0 {
                let mut len = 0;
                gl::GetProgramiv(shader, gl::INFO_LOG_LENGTH, &mut len);
                let mut buffer = vec![0u8; len as usize];
                gl::GetProgramInfoLog(shader, len, ptr::null_mut(), buffer.as_mut_ptr() as *mut GLchar);
                return Err(format!("Shader linking failed: {}", String::from_utf8_lossy(&buffer)));
            }

            gl::DeleteShader(vertex_shader);
            gl::DeleteShader(fragment_shader);

            let model_loc            = gl::GetUniformLocation(shader, c"model".as_ptr());
            let view_loc             = gl::GetUniformLocation(shader, c"view".as_ptr());
            let projection_loc       = gl::GetUniformLocation(shader, c"projection".as_ptr());
            let use_textures_loc     = gl::GetUniformLocation(shader, c"use_textures".as_ptr());
            let fog_start_loc        = gl::GetUniformLocation(shader, c"fog_start".as_ptr());
            let fog_end_loc          = gl::GetUniformLocation(shader, c"fog_end".as_ptr());
            let transparent_pass_loc = gl::GetUniformLocation(shader, c"transparent_pass".as_ptr());
            let light_space_loc      = gl::GetUniformLocation(shader, c"lightSpaceMatrices[0]".as_ptr());
            let shadow_maps_loc      = gl::GetUniformLocation(shader, c"shadowMaps".as_ptr());
            let cascade_ends_loc     = gl::GetUniformLocation(shader, c"cascadeEnds[0]".as_ptr());
            let texel_sizes_loc      = gl::GetUniformLocation(shader, c"shadowTexelSizes[0]".as_ptr());
            let light_dir_loc        = gl::GetUniformLocation(shader, c"lightDir".as_ptr());
            let fog_color_loc        = gl::GetUniformLocation(shader, c"fog_color".as_ptr());
            let ambient_loc          = gl::GetUniformLocation(shader, c"ambientLight".as_ptr());
            let directional_loc      = gl::GetUniformLocation(shader, c"directionalLight".as_ptr());

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
                    light_space_matrices: light_space_loc,
                    shadow_maps: shadow_maps_loc,
                    cascade_ends: cascade_ends_loc,
                    shadow_texel_sizes: texel_sizes_loc,
                    light_dir: light_dir_loc,
                    fog_color: fog_color_loc,
                    ambient_light: ambient_loc,
                    directional_light: directional_loc,
                },
            })
        }
    }

    pub fn begin_frame(
        &self,
        view: &glam::Mat4,
        projection: &glam::Mat4,
        shadow_texture_array: u32,
        light_space_matrices: &[glam::Mat4; NUM_CASCADES],
        cascade_ends: &[f32; NUM_CASCADES],
        shadow_texel_sizes: &[f32; NUM_CASCADES],
        sun_dir: glam::Vec3,
        fog_color: glam::Vec3,
        ambient_light: f32,
        directional_light: f32,
    ) {
        unsafe {
            gl::UseProgram(self.shader);
            gl::UniformMatrix4fv(self.uniforms.view, 1, gl::FALSE, view.as_ref().as_ptr());
            gl::UniformMatrix4fv(self.uniforms.projection, 1, gl::FALSE, projection.as_ref().as_ptr());
            // Mat4 is repr(C) over 16 contiguous f32s, so the array is tightly
            // packed and we can upload all NUM_CASCADES at once.
            gl::UniformMatrix4fv(
                self.uniforms.light_space_matrices,
                NUM_CASCADES as i32,
                gl::FALSE,
                light_space_matrices.as_ptr() as *const f32,
            );
            gl::Uniform1fv(self.uniforms.cascade_ends, NUM_CASCADES as i32, cascade_ends.as_ptr());
            gl::Uniform1fv(self.uniforms.shadow_texel_sizes, NUM_CASCADES as i32, shadow_texel_sizes.as_ptr());
            gl::Uniform1i(self.uniforms.use_textures, 1);
            gl::Uniform1f(self.uniforms.fog_start, 32.0);
            gl::Uniform1f(self.uniforms.fog_end,   64.0);
            gl::Uniform3f(self.uniforms.fog_color, fog_color.x, fog_color.y, fog_color.z);
            gl::Uniform1f(self.uniforms.ambient_light, ambient_light);
            gl::Uniform1f(self.uniforms.directional_light, directional_light);
            // Texture unit 0: block atlas (sampler2D).
            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, self.texture_atlas);
            // Texture unit 1: cascaded shadow maps (sampler2DArray).
            gl::ActiveTexture(gl::TEXTURE1);
            gl::BindTexture(gl::TEXTURE_2D_ARRAY, shadow_texture_array);
            gl::Uniform1i(self.uniforms.shadow_maps, 1);
            gl::Uniform3f(self.uniforms.light_dir, sun_dir.x, sun_dir.y, sun_dir.z);
            gl::ActiveTexture(gl::TEXTURE0);
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

    pub fn texture_atlas(&self) -> u32 {
        self.texture_atlas
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
        let mesh = match &chunk.mesh {
            Some(m) => m,
            None => return,
        };

        unsafe {
            let model = chunk.model_matrix();
            gl::UniformMatrix4fv(self.uniforms.model, 1, gl::FALSE, model.as_ref().as_ptr());
            gl::BindVertexArray(mesh.vao);
            gl::DrawArrays(gl::TRIANGLES, 0, mesh.vertex_count);
        }
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
