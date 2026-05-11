use crate::renderer::utils::{compile_shader, link_program};

pub struct UnderwaterRenderer {
    shader: u32,
    loc_time: i32,
    loc_screen: i32,
    vao: u32,
}

impl UnderwaterRenderer {
    pub fn new() -> Self {
        unsafe {
            let vert = compile_shader(gl::VERTEX_SHADER, r#"#version 330 core
                void main() {
                    // Full-screen triangle — no VBO needed.
                    vec2 pos[3] = vec2[3](vec2(-1,-1), vec2(3,-1), vec2(-1,3));
                    gl_Position = vec4(pos[gl_VertexID], 0.0, 1.0);
                }"#).unwrap();

            let frag = compile_shader(gl::FRAGMENT_SHADER, r#"#version 330 core
                out vec4 FragColor;
                uniform float u_time;
                uniform vec2  u_screen_size;

                void main() {
                    vec2  uv = gl_FragCoord.xy / u_screen_size;
                    float t  = u_time * 1.2;

                    // Animated caustic shimmer using 3 interfering wave centres.
                    float c1 = sin(length(uv * 8.0 - vec2(sin(t*0.6)*1.5, cos(t*0.5)*1.5)) * 6.0 - t*2.0);
                    float c2 = sin(length(uv * 8.0 + vec2(cos(t*0.7)*1.5, sin(t*0.8)*1.5)) * 6.0 - t*1.7);
                    float c3 = sin(length(uv * 8.0 - vec2(cos(t*0.5)*1.2, sin(t*0.9)*1.2)) * 6.0 - t*2.3);
                    float caustic = max(0.0, (c1 + c2 + c3) / 3.0) * 0.3;

                    vec3 tint = vec3(0.05, 0.20, 0.60)
                              + vec3(caustic * 0.30, caustic * 0.45, caustic * 0.15);
                    FragColor = vec4(tint, 0.42);
                }"#).unwrap();

            let shader = link_program(vert, frag).unwrap();
            gl::DeleteShader(vert);
            gl::DeleteShader(frag);

            let loc_time   = gl::GetUniformLocation(shader, c"u_time".as_ptr());
            let loc_screen = gl::GetUniformLocation(shader, c"u_screen_size".as_ptr());

            // Empty VAO — the vertex shader generates geometry from gl_VertexID.
            let mut vao = 0u32;
            gl::GenVertexArrays(1, &mut vao);

            Self { shader, loc_time, loc_screen, vao }
        }
    }

    pub fn draw(&self, time: f32, screen_width: f32, screen_height: f32) {
        unsafe {
            gl::Disable(gl::DEPTH_TEST);
            gl::Enable(gl::BLEND);
            gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);

            gl::UseProgram(self.shader);
            gl::Uniform1f(self.loc_time, time);
            gl::Uniform2f(self.loc_screen, screen_width, screen_height);

            gl::BindVertexArray(self.vao);
            gl::DrawArrays(gl::TRIANGLES, 0, 3);
            gl::BindVertexArray(0);

            gl::UseProgram(0);
            gl::Enable(gl::DEPTH_TEST);
        }
    }
}

impl Drop for UnderwaterRenderer {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteProgram(self.shader);
            gl::DeleteVertexArrays(1, &self.vao);
        }
    }
}
