use std::ptr;
use crate::renderer::utils::compile_shader;
use crate::world::chunk::Chunk;

/// Cascaded Shadow Maps: split the view frustum into N slices, give each its
/// own shadow map. Near cascades are tight (sharp shadows), far cascades are
/// large (cover the horizon at lower effective resolution).
pub const NUM_CASCADES: usize = 3;

/// View-space far distance for each cascade. Geometry between two cascade
/// boundaries gets sampled from the corresponding shadow map.
///   cascade 0: 0  → 12  (sharp close-up shadows)
///   cascade 1: 12 → 32  (mid-range)
///   cascade 2: 32 → 80  (distant — beyond the fog falloff)
pub const CASCADE_ENDS: [f32; NUM_CASCADES] = [12.0, 32.0, 80.0];

const MAP_SIZE: i32 = 2048;

pub struct ShadowPass {
    fbo: u32,
    /// One GL_TEXTURE_2D_ARRAY with NUM_CASCADES layers — each layer is the
    /// depth map for one cascade.
    depth_array: u32,
    shader: u32,
    map_size: i32,
    light_space_loc: i32,
    model_loc: i32,
    atlas_loc: i32,
    /// Light-space (view * ortho) matrix for each cascade, recomputed every frame
    /// to fit the current camera frustum slice tightly.
    light_space_matrices: [glam::Mat4; NUM_CASCADES],
    /// World-space size of one shadow texel for each cascade. The main shader
    /// uses these for receiver-side normal offset (closes the contact gap).
    texel_world_sizes: [f32; NUM_CASCADES],
}

impl ShadowPass {
    pub fn new() -> Result<Self, String> {
        unsafe {
            // Allocate the depth array — one texture, NUM_CASCADES layers.
            let mut depth_array = 0u32;
            gl::GenTextures(1, &mut depth_array);
            gl::BindTexture(gl::TEXTURE_2D_ARRAY, depth_array);
            gl::TexImage3D(
                gl::TEXTURE_2D_ARRAY, 0, gl::DEPTH_COMPONENT as i32,
                MAP_SIZE, MAP_SIZE, NUM_CASCADES as i32, 0,
                gl::DEPTH_COMPONENT, gl::FLOAT, ptr::null(),
            );
            gl::TexParameteri(gl::TEXTURE_2D_ARRAY, gl::TEXTURE_MIN_FILTER, gl::NEAREST as i32);
            gl::TexParameteri(gl::TEXTURE_2D_ARRAY, gl::TEXTURE_MAG_FILTER, gl::NEAREST as i32);
            // Outside the cascade box: depth=1.0 (fully lit, not in shadow).
            gl::TexParameteri(gl::TEXTURE_2D_ARRAY, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_BORDER as i32);
            gl::TexParameteri(gl::TEXTURE_2D_ARRAY, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_BORDER as i32);
            let border = [1.0f32, 1.0, 1.0, 1.0];
            gl::TexParameterfv(gl::TEXTURE_2D_ARRAY, gl::TEXTURE_BORDER_COLOR, border.as_ptr());

            // FBO has no permanent depth attachment — begin_cascade() rebinds the
            // appropriate array layer each time.
            let mut fbo = 0u32;
            gl::GenFramebuffers(1, &mut fbo);
            gl::BindFramebuffer(gl::FRAMEBUFFER, fbo);
            gl::DrawBuffer(gl::NONE);
            gl::ReadBuffer(gl::NONE);
            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);

            // Depth-only shader — same as before. The lightSpaceMatrix uniform
            // is updated per cascade by begin_cascade().
            let vs = compile_shader(gl::VERTEX_SHADER, r#"#version 330 core
                layout(location = 0) in vec3 aPos;
                layout(location = 2) in vec2 aTexCoord;
                out vec2 TexCoord;
                uniform mat4 lightSpaceMatrix;
                uniform mat4 model;
                void main() {
                    gl_Position = lightSpaceMatrix * model * vec4(aPos, 1.0);
                    TexCoord = aTexCoord;
                }"#)?;

            let fs = compile_shader(gl::FRAGMENT_SHADER, r#"#version 330 core
                in vec2 TexCoord;
                uniform sampler2D atlas;
                void main() {
                    if (texture(atlas, TexCoord).a < 0.5) discard;
                }"#)?;

            let shader = gl::CreateProgram();
            gl::AttachShader(shader, vs);
            gl::AttachShader(shader, fs);
            gl::LinkProgram(shader);
            gl::DeleteShader(vs);
            gl::DeleteShader(fs);

            let light_space_loc = gl::GetUniformLocation(shader, c"lightSpaceMatrix".as_ptr());
            let model_loc       = gl::GetUniformLocation(shader, c"model".as_ptr());
            let atlas_loc       = gl::GetUniformLocation(shader, c"atlas".as_ptr());

            Ok(ShadowPass {
                fbo,
                depth_array,
                shader,
                map_size: MAP_SIZE,
                light_space_loc,
                model_loc,
                atlas_loc,
                light_space_matrices: [glam::Mat4::IDENTITY; NUM_CASCADES],
                texel_world_sizes: [1.0; NUM_CASCADES],
            })
        }
    }

    /// Build per-cascade light-space matrices by fitting an ortho box around
    /// each slice of the camera's view frustum, transformed into the sun's
    /// view space. Also returns the texel world size per cascade so the main
    /// shader can apply a matching normal offset.
    fn compute_cascades(
        sun_dir: glam::Vec3,
        cam_pos: glam::Vec3,
        cam_forward: glam::Vec3,
        cam_world_up: glam::Vec3,
        fov_y_rad: f32,
        aspect: f32,
    ) -> ([glam::Mat4; NUM_CASCADES], [f32; NUM_CASCADES]) {
        let sun_dir = sun_dir.normalize();
        let cam_forward = cam_forward.normalize();
        let cam_right = cam_forward.cross(cam_world_up).normalize();
        let cam_up    = cam_right.cross(cam_forward).normalize();
        let tan_half  = (fov_y_rad * 0.5).tan();

        // Each cascade's near = previous cascade's far. Cascade 0 starts at the
        // camera near plane (small offset).
        let near_starts: [f32; NUM_CASCADES] = [0.1, CASCADE_ENDS[0], CASCADE_ENDS[1]];

        let mut matrices    = [glam::Mat4::IDENTITY; NUM_CASCADES];
        let mut texel_sizes = [0.0f32; NUM_CASCADES];

        for i in 0..NUM_CASCADES {
            let near = near_starts[i];
            let far  = CASCADE_ENDS[i];

            // Build the 8 corners of this frustum slice in world space.
            let near_h = tan_half * near;
            let near_w = near_h * aspect;
            let far_h  = tan_half * far;
            let far_w  = far_h * aspect;
            let near_c = cam_pos + cam_forward * near;
            let far_c  = cam_pos + cam_forward * far;
            let corners = [
                near_c - cam_right * near_w - cam_up * near_h,
                near_c + cam_right * near_w - cam_up * near_h,
                near_c - cam_right * near_w + cam_up * near_h,
                near_c + cam_right * near_w + cam_up * near_h,
                far_c  - cam_right * far_w  - cam_up * far_h,
                far_c  + cam_right * far_w  - cam_up * far_h,
                far_c  - cam_right * far_w  + cam_up * far_h,
                far_c  + cam_right * far_w  + cam_up * far_h,
            ];

            // Centroid: where the light camera looks.
            let mut center = glam::Vec3::ZERO;
            for c in &corners { center += *c; }
            center /= 8.0;

            // Light view matrix: place the eye well behind the centroid along
            // the sun direction so all corners stay in front of it.
            let up = if sun_dir.y.abs() > 0.99 { glam::Vec3::Z } else { glam::Vec3::Y };
            let eye_distance = 200.0;
            let eye = center - sun_dir * eye_distance;
            let light_view = glam::Mat4::look_at_rh(eye, center, up);

            // Transform corners into light view space; AABB gives the ortho bounds.
            let mut min = glam::Vec3::splat(f32::INFINITY);
            let mut max = glam::Vec3::splat(f32::NEG_INFINITY);
            for c in &corners {
                let p = light_view.transform_point3(*c);
                min = min.min(p);
                max = max.max(p);
            }

            // In RH look_at view space, objects in front of the eye have negative
            // Z. Ortho near/far are positive distances, so we negate.
            // Pull near plane closer to the light to capture casters between the
            // sun and the frustum slice (e.g. tall trees overhead).
            let pullback = 50.0;
            let ortho_near = (-max.z - pullback).max(0.1);
            let ortho_far  = -min.z;

            let proj = glam::Mat4::orthographic_rh(
                min.x, max.x, min.y, max.y, ortho_near, ortho_far,
            );

            matrices[i] = proj * light_view;

            // Texel world size = ortho extent / map resolution. Use the larger
            // axis so the offset is conservative on stretched cascades.
            let world_extent = (max.x - min.x).max(max.y - min.y);
            texel_sizes[i] = world_extent / MAP_SIZE as f32;
        }

        (matrices, texel_sizes)
    }

    /// Bind the shadow FBO, recompute cascades, set common state. After this,
    /// call `begin_cascade(i)` then draw geometry, for each cascade.
    pub fn begin(
        &mut self,
        sun_dir: glam::Vec3,
        cam_pos: glam::Vec3,
        cam_forward: glam::Vec3,
        cam_world_up: glam::Vec3,
        fov_y_rad: f32,
        aspect: f32,
        atlas_texture: u32,
    ) {
        let (matrices, texel_sizes) = Self::compute_cascades(
            sun_dir, cam_pos, cam_forward, cam_world_up, fov_y_rad, aspect,
        );
        self.light_space_matrices = matrices;
        self.texel_world_sizes    = texel_sizes;
        unsafe {
            gl::BindFramebuffer(gl::FRAMEBUFFER, self.fbo);
            gl::Viewport(0, 0, self.map_size, self.map_size);
            gl::UseProgram(self.shader);
            // Atlas (unit 0) for alpha-testing transparent geometry.
            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, atlas_texture);
            gl::Uniform1i(self.atlas_loc, 0);
            gl::Enable(gl::DEPTH_TEST);
            gl::Enable(gl::CULL_FACE);
            gl::CullFace(gl::BACK);
            // Slope-scale depth bias on the caster side.
            gl::Enable(gl::POLYGON_OFFSET_FILL);
            gl::PolygonOffset(2.0, 4.0);
        }
    }

    /// Switch the FBO's depth attachment to cascade `index`'s layer, clear it,
    /// and set the corresponding light-space matrix uniform.
    pub fn begin_cascade(&self, index: usize) {
        unsafe {
            gl::FramebufferTextureLayer(
                gl::FRAMEBUFFER, gl::DEPTH_ATTACHMENT,
                self.depth_array, 0, index as i32,
            );
            gl::Clear(gl::DEPTH_BUFFER_BIT);
            gl::UniformMatrix4fv(
                self.light_space_loc, 1, gl::FALSE,
                self.light_space_matrices[index].as_ref().as_ptr(),
            );
        }
    }

    pub fn end(&self, fb_w: i32, fb_h: i32) {
        unsafe {
            gl::Disable(gl::POLYGON_OFFSET_FILL);
            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
            gl::Viewport(0, 0, fb_w, fb_h);
        }
    }

    /// Draw a single chunk into whichever cascade is currently active.
    pub fn draw_chunk(&self, chunk: &Chunk) {
        let mesh = match &chunk.mesh {
            Some(m) => m,
            None => return,
        };
        unsafe {
            let model = chunk.model_matrix();
            gl::UniformMatrix4fv(self.model_loc, 1, gl::FALSE, model.as_ref().as_ptr());
            gl::BindVertexArray(mesh.vao);
            gl::DrawArrays(gl::TRIANGLES, 0, mesh.vertex_count);
        }
    }

    pub fn depth_texture_array(&self) -> u32 {
        self.depth_array
    }

    pub fn light_space_matrices(&self) -> &[glam::Mat4; NUM_CASCADES] {
        &self.light_space_matrices
    }

    pub fn texel_world_sizes(&self) -> &[f32; NUM_CASCADES] {
        &self.texel_world_sizes
    }
}

impl Drop for ShadowPass {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteFramebuffers(1, &self.fbo);
            gl::DeleteTextures(1, &self.depth_array);
            gl::DeleteProgram(self.shader);
        }
    }
}
