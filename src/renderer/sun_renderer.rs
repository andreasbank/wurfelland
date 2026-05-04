use crate::renderer::utils::compile_shader;

/// Renders a billboarded sun disk in the sky. The sun is placed far along the
/// world-space sun direction from the camera and oriented to face the camera,
/// so it always appears as a circle regardless of view angle.
pub struct SunRenderer {
    shader: u32,
    vao: u32,
    view_loc: i32,
    projection_loc: i32,
    cam_pos_loc: i32,
    sun_dir_loc: i32,
    size_loc: i32,
    color_loc: i32,
}

impl SunRenderer {
    pub fn new() -> Result<Self, String> {
        unsafe {
            // Vertex shader generates the quad procedurally from gl_VertexID,
            // so no VBO is needed — we draw with a bound (but empty) VAO.
            let vs = compile_shader(gl::VERTEX_SHADER, r#"#version 330 core
                uniform mat4  view;
                uniform mat4  projection;
                uniform vec3  camPos;
                uniform vec3  sunDirToward;  // unit vector from camera toward sun
                uniform float size;          // half-extent in world units

                out vec2 uv;

                void main() {
                    vec2 corners[6] = vec2[6](
                        vec2(-1.0, -1.0), vec2( 1.0, -1.0), vec2( 1.0,  1.0),
                        vec2(-1.0, -1.0), vec2( 1.0,  1.0), vec2(-1.0,  1.0)
                    );
                    vec2 c = corners[gl_VertexID];
                    uv = c * 0.5 + 0.5;

                    // Camera basis from the view matrix rows: row 0 = right,
                    // row 1 = up. Reading row r, column c is view[c][r].
                    vec3 right = vec3(view[0][0], view[1][0], view[2][0]);
                    vec3 up    = vec3(view[0][1], view[1][1], view[2][1]);

                    // Place sun far away along its direction; offset corners in
                    // camera-aligned axes so it always faces the viewer.
                    vec3 sunCenter = camPos + normalize(sunDirToward) * 500.0;
                    vec3 worldPos  = sunCenter + right * c.x * size + up * c.y * size;

                    gl_Position = projection * view * vec4(worldPos, 1.0);
                }"#)?;

            let fs = compile_shader(gl::FRAGMENT_SHADER, r#"#version 330 core
                in  vec2 uv;
                out vec4 FragColor;
                uniform vec3 sunColor;

                void main() {
                    vec2  d = uv - 0.5;
                    float r = length(d);
                    if (r > 0.5) discard;
                    // Bright core fading into a soft halo.
                    float core = 1.0 - smoothstep(0.0,  0.30, r);
                    float halo = 1.0 - smoothstep(0.30, 0.50, r);
                    float a = clamp(core + halo * 0.5, 0.0, 1.0);
                    FragColor = vec4(sunColor, a);
                }"#)?;

            let shader = gl::CreateProgram();
            gl::AttachShader(shader, vs);
            gl::AttachShader(shader, fs);
            gl::LinkProgram(shader);
            gl::DeleteShader(vs);
            gl::DeleteShader(fs);

            let mut vao = 0u32;
            gl::GenVertexArrays(1, &mut vao);

            Ok(SunRenderer {
                shader,
                vao,
                view_loc:       gl::GetUniformLocation(shader, c"view".as_ptr()),
                projection_loc: gl::GetUniformLocation(shader, c"projection".as_ptr()),
                cam_pos_loc:    gl::GetUniformLocation(shader, c"camPos".as_ptr()),
                sun_dir_loc:    gl::GetUniformLocation(shader, c"sunDirToward".as_ptr()),
                size_loc:       gl::GetUniformLocation(shader, c"size".as_ptr()),
                color_loc:      gl::GetUniformLocation(shader, c"sunColor".as_ptr()),
            })
        }
    }

    /// Draw a celestial disc (sun or moon). `dir_toward` is a unit vector from
    /// the camera toward the body. Should be called after the framebuffer
    /// clear and before opaque terrain renders, so terrain occludes it where
    /// hills and trees rise into the sky.
    pub fn draw(
        &self,
        view: &glam::Mat4,
        projection: &glam::Mat4,
        cam_pos: glam::Vec3,
        dir_toward: glam::Vec3,
        size: f32,
        color: glam::Vec3,
    ) {
        unsafe {
            gl::UseProgram(self.shader);
            gl::UniformMatrix4fv(self.view_loc, 1, gl::FALSE, view.as_ref().as_ptr());
            gl::UniformMatrix4fv(self.projection_loc, 1, gl::FALSE, projection.as_ref().as_ptr());
            gl::Uniform3f(self.cam_pos_loc, cam_pos.x, cam_pos.y, cam_pos.z);
            gl::Uniform3f(self.sun_dir_loc, dir_toward.x, dir_toward.y, dir_toward.z);
            gl::Uniform1f(self.size_loc, size);
            gl::Uniform3f(self.color_loc, color.x, color.y, color.z);

            // No depth test/write — terrain drawn afterwards will naturally
            // occlude the sun where geometry rises in front of it.
            gl::Disable(gl::DEPTH_TEST);
            gl::DepthMask(gl::FALSE);
            gl::Enable(gl::BLEND);
            gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);
            gl::BindVertexArray(self.vao);
            gl::DrawArrays(gl::TRIANGLES, 0, 6);
            gl::Enable(gl::DEPTH_TEST);
            gl::DepthMask(gl::TRUE);
        }
    }
}

impl Drop for SunRenderer {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteProgram(self.shader);
            gl::DeleteVertexArrays(1, &self.vao);
        }
    }
}
