use crate::renderer::utils::compile_shader;

pub struct ClockRenderer {
    shader: u32,
    vao: u32,
    center_loc: i32,
    half_size_loc: i32,
    d0_loc: i32,
    d1_loc: i32,
    d2_loc: i32,
    d3_loc: i32,
    d4_loc: i32,
    d5_loc: i32,
}

impl ClockRenderer {
    pub fn new() -> Self {
        let vs = compile_shader(gl::VERTEX_SHADER, r#"#version 330 core
            uniform vec2 center;
            uniform vec2 half_size;
            out vec2 uv;
            void main() {
                vec2 corners[6] = vec2[6](
                    vec2(-1,-1), vec2(1,-1), vec2(1,1),
                    vec2(-1,-1), vec2(1,1),  vec2(-1,1)
                );
                vec2 c = corners[gl_VertexID];
                uv = c * 0.5 + 0.5;
                gl_Position = vec4(center + c * half_size, 0.0, 1.0);
            }"#).unwrap();

        // 3×5 bitmap font. Bit layout per digit: bit = row*3 + col,
        // row 0 = top, col 0 = left. Each row is 3 bits (L,C,R).
        //
        //  0→31599  1→29842  2→29671  3→31143  4→18925
        //  5→31183  6→31695  7→9383   8→31727  9→31215
        let fs = compile_shader(gl::FRAGMENT_SHADER, r#"#version 330 core
            in  vec2 uv;
            out vec4 FragColor;

            // Hours tens/ones, minutes tens/ones, seconds tens/ones
            uniform int d0, d1, d2, d3, d4, d5;

            const int FONT[10] = int[10](
                31599, 29842, 29671, 31143, 18925,
                31183, 31695,  9383, 31727, 31215
            );

            bool font_px(int d, int col, int row) {
                return ((FONT[d] >> (row * 3 + col)) & 1) != 0;
            }

            void main() {
                // Rounded-corner clip.
                const float CR = 0.08;
                vec2 q = max(vec2(0.0), abs(uv - 0.5) - (0.5 - CR));
                if (length(q) > CR) discard;

                // Dark background.
                FragColor = vec4(0.06, 0.06, 0.08, 0.88);

                // 8 equal columns: H0 H1 : M0 M1 : S0 S1
                float col_f = uv.x * 8.0;
                int   cell  = int(col_f);
                float lx    = fract(col_f);  // [0,1) within cell
                float ly    = uv.y;           // [0,1), 0=bottom 1=top

                bool colon_cell = (cell == 2 || cell == 5);

                if (colon_cell) {
                    // Two small round dots at 1/3 and 2/3 height.
                    float dx = lx - 0.5;
                    float dy1 = ly - 0.67;
                    float dy2 = ly - 0.33;
                    if (dx*dx + dy1*dy1 < 0.04 || dx*dx + dy2*dy2 < 0.04) {
                        FragColor = vec4(0.85, 0.85, 0.90, 1.0);
                    }
                    return;
                }

                // Map cell index to digit value.
                int di = cell > 5 ? cell - 2 : cell > 2 ? cell - 1 : cell;
                int d  = (di == 0) ? d0 : (di == 1) ? d1 : (di == 2) ? d2
                       : (di == 3) ? d3 : (di == 4) ? d4 : d5;

                // Map lx/ly into the 3×5 bitmap grid with padding.
                // Horizontal: 15% padding each side → inner 70% maps to [0,3).
                // Vertical:   10% padding each side → inner 80% maps to [0,5).
                float gx = (lx - 0.15) / 0.70 * 3.0;
                float gy = (1.0 - ly - 0.10) / 0.80 * 5.0;  // flip: row 0 = top

                if (gx >= 0.0 && gx < 3.0 && gy >= 0.0 && gy < 5.0) {
                    if (font_px(d, int(gx), int(gy))) {
                        FragColor = vec4(0.88, 0.88, 0.93, 1.0);
                    }
                }
            }"#).unwrap();

        unsafe {
            let shader = gl::CreateProgram();
            gl::AttachShader(shader, vs);
            gl::AttachShader(shader, fs);
            gl::LinkProgram(shader);
            gl::DeleteShader(vs);
            gl::DeleteShader(fs);

            let mut vao = 0u32;
            gl::GenVertexArrays(1, &mut vao);

            ClockRenderer {
                shader,
                vao,
                center_loc:    gl::GetUniformLocation(shader, c"center".as_ptr()),
                half_size_loc: gl::GetUniformLocation(shader, c"half_size".as_ptr()),
                d0_loc:        gl::GetUniformLocation(shader, c"d0".as_ptr()),
                d1_loc:        gl::GetUniformLocation(shader, c"d1".as_ptr()),
                d2_loc:        gl::GetUniformLocation(shader, c"d2".as_ptr()),
                d3_loc:        gl::GetUniformLocation(shader, c"d3".as_ptr()),
                d4_loc:        gl::GetUniformLocation(shader, c"d4".as_ptr()),
                d5_loc:        gl::GetUniformLocation(shader, c"d5".as_ptr()),
            }
        }
    }

    /// Draw the clock. `hours` 0–23, `minutes` 0–59, `seconds` 0–59.
    pub fn draw(&self, hours: u32, minutes: u32, seconds: u32, win_w: f32, win_h: f32) {
        // Horizontally: same center as the minimap (20 px right margin + 80 px radius).
        // Vertically: 15 px from top, 40 px tall.
        // Keep these in sync with minimap_renderer.rs positioning constants.
        const RIGHT_MARGIN_PX: f32 = 20.0;
        const MINIMAP_RADIUS_PX: f32 = 80.0;
        const CLOCK_TOP_PX: f32 = 15.0;
        const CLOCK_HALF_H_PX: f32 = 20.0;

        let cx     = 1.0 - (RIGHT_MARGIN_PX + MINIMAP_RADIUS_PX) * 2.0 / win_w;
        let cy     = 1.0 - (CLOCK_TOP_PX + CLOCK_HALF_H_PX) * 2.0 / win_h;
        let half_w = MINIMAP_RADIUS_PX * 2.0 / win_w;
        let half_h = CLOCK_HALF_H_PX * 2.0 / win_h;

        unsafe {
            gl::UseProgram(self.shader);
            gl::Uniform2f(self.center_loc,    cx, cy);
            gl::Uniform2f(self.half_size_loc, half_w, half_h);
            gl::Uniform1i(self.d0_loc, (hours   / 10) as i32);
            gl::Uniform1i(self.d1_loc, (hours   % 10) as i32);
            gl::Uniform1i(self.d2_loc, (minutes / 10) as i32);
            gl::Uniform1i(self.d3_loc, (minutes % 10) as i32);
            gl::Uniform1i(self.d4_loc, (seconds / 10) as i32);
            gl::Uniform1i(self.d5_loc, (seconds % 10) as i32);

            gl::Disable(gl::DEPTH_TEST);
            gl::Enable(gl::BLEND);
            gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);

            gl::BindVertexArray(self.vao);
            gl::DrawArrays(gl::TRIANGLES, 0, 6);

            gl::Enable(gl::DEPTH_TEST);
            gl::Disable(gl::BLEND);
        }
    }
}

impl Drop for ClockRenderer {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteProgram(self.shader);
            gl::DeleteVertexArrays(1, &self.vao);
        }
    }
}
