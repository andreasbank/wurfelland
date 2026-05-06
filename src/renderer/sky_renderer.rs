use crate::renderer::utils::compile_shader;

/// Renders an equirectangular HDR panorama as a sky background.
///
/// Draw this first (after gl::Clear) so all terrain and billboards naturally
/// occlude it. Uses a full-screen triangle with depth writes disabled.
pub struct SkyRenderer {
    shader: u32,
    vao: u32,
    texture: u32,
    view_loc: i32,
    proj_loc: i32,
    tint_loc: i32,
    exposure_loc: i32,
}

impl SkyRenderer {
    /// `hdr_path` — path to an equirectangular .hdr (Radiance RGBE) image.
    pub fn new(hdr_path: &str) -> Result<Self, String> {
        let img = image::open(hdr_path)
            .map_err(|e| format!("sky HDR load failed: {e}"))?
            .into_rgb32f();
        let (w, h) = (img.width(), img.height());

        let vs = compile_shader(gl::VERTEX_SHADER, r#"#version 330 core
            out vec2 ndc_pos;
            void main() {
                // Full-screen triangle: three vertices that cover all of NDC [-1,1]^2.
                // VertexID 0 → (-1,-1), 1 → (3,-1), 2 → (-1,3)
                float x = float((gl_VertexID & 1) != 0) * 4.0 - 1.0;
                float y = float((gl_VertexID & 2) != 0) * 4.0 - 1.0;
                ndc_pos = vec2(x, y);
                gl_Position = vec4(x, y, 1.0, 1.0);
            }"#)?;

        let fs = compile_shader(gl::FRAGMENT_SHADER, r#"#version 330 core
            in  vec2 ndc_pos;
            out vec4 FragColor;

            uniform sampler2D sky_tex;
            uniform mat4      view;
            uniform mat4      projection;
            uniform vec3      sky_tint;   // day/dusk/night blend color from main loop
            uniform float     exposure;   // linear exposure multiplier (tune to taste)

            const float PI = 3.14159265359;

            void main() {
                // --- Reconstruct world-space ray direction ---
                //
                // For a standard perspective matrix P:
                //   ndc.x = vx * P[0][0] / (-vz)  →  vx = ndc.x / P[0][0]  (at vz = -1)
                //   ndc.y = vy * P[1][1] / (-vz)  →  vy = ndc.y / P[1][1]
                // P[col][row] in GLSL column-major: P[0][0]=f/aspect, P[1][1]=f.
                vec3 view_dir = normalize(vec3(
                    ndc_pos.x / projection[0][0],
                    ndc_pos.y / projection[1][1],
                    -1.0
                ));

                // Rotate from view space to world space.
                // mat3(view) is the pure-rotation part of the view matrix;
                // its transpose is its inverse (orthogonal matrix property).
                vec3 world_dir = normalize(transpose(mat3(view)) * view_dir);

                // --- Equirectangular (lat-long) UV mapping ---
                float u = 0.5 + atan(world_dir.z, world_dir.x) / (2.0 * PI);
                float v = 0.5 - asin(clamp(world_dir.y, -1.0, 1.0)) / PI;

                vec3 hdr_color = texture(sky_tex, vec2(u, v)).rgb * sky_tint;

                // Scale by exposure then apply Reinhard tone mapping to compress
                // the HDR range into [0,1].  Finish with gamma correction so the
                // linear HDR values display correctly on an sRGB monitor.
                vec3 exposed = hdr_color * exposure;
                vec3 mapped  = exposed / (exposed + vec3(1.0));   // Reinhard
                vec3 gamma   = pow(mapped, vec3(1.0 / 2.2));
                FragColor = vec4(gamma, 1.0);
            }"#)?;

        unsafe {
            let mut texture = 0u32;
            gl::GenTextures(1, &mut texture);
            gl::BindTexture(gl::TEXTURE_2D, texture);
            gl::TexImage2D(
                gl::TEXTURE_2D, 0, gl::RGB16F as i32,
                w as i32, h as i32, 0,
                gl::RGB, gl::FLOAT,
                img.as_raw().as_ptr() as *const _,
            );
            // Wrap horizontally (seamless 360°), clamp vertically (avoid pole seam).
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::REPEAT as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR as i32);
            gl::BindTexture(gl::TEXTURE_2D, 0);

            let shader = gl::CreateProgram();
            gl::AttachShader(shader, vs);
            gl::AttachShader(shader, fs);
            gl::LinkProgram(shader);
            gl::DeleteShader(vs);
            gl::DeleteShader(fs);

            let mut vao = 0u32;
            gl::GenVertexArrays(1, &mut vao);

            Ok(SkyRenderer {
                shader,
                vao,
                texture,
                view_loc:     gl::GetUniformLocation(shader, c"view".as_ptr()),
                proj_loc:     gl::GetUniformLocation(shader, c"projection".as_ptr()),
                tint_loc:     gl::GetUniformLocation(shader, c"sky_tint".as_ptr()),
                exposure_loc: gl::GetUniformLocation(shader, c"exposure".as_ptr()),
            })
        }
    }

    /// Draw the sky. Call after `gl::Clear` and before any 3-D geometry.
    ///
    /// `sky_tint` is the procedural day/dusk/night color already computed in
    /// main — multiplying it against the HDR pixels gives correct day/night
    /// darkening without a separate brightness uniform.
    /// `exposure` — linear multiplier applied before tone mapping.
    /// Start around 0.5–1.0 and tune up/down: higher = brighter overall sky,
    /// lower = darker with more visible sun bloom.
    pub fn draw(
        &self,
        view: &glam::Mat4,
        projection: &glam::Mat4,
        sky_tint: glam::Vec3,
        exposure: f32,
    ) {
        unsafe {
            gl::UseProgram(self.shader);
            gl::UniformMatrix4fv(self.view_loc, 1, gl::FALSE, view.as_ref().as_ptr());
            gl::UniformMatrix4fv(self.proj_loc, 1, gl::FALSE, projection.as_ref().as_ptr());
            gl::Uniform3f(self.tint_loc, sky_tint.x, sky_tint.y, sky_tint.z);
            gl::Uniform1f(self.exposure_loc, exposure);

            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, self.texture);

            gl::Disable(gl::DEPTH_TEST);
            gl::DepthMask(gl::FALSE);
            gl::Disable(gl::BLEND);

            gl::BindVertexArray(self.vao);
            gl::DrawArrays(gl::TRIANGLES, 0, 3);

            gl::Enable(gl::DEPTH_TEST);
            gl::DepthMask(gl::TRUE);
        }
    }
}

impl Drop for SkyRenderer {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteProgram(self.shader);
            gl::DeleteVertexArrays(1, &self.vao);
            gl::DeleteTextures(1, &self.texture);
        }
    }
}
