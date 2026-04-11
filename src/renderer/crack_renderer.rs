use std::mem;
use std::os::raw::c_void;
use crate::renderer::utils::{compile_shader, link_program, create_block_atlas};

pub struct CrackRenderer {
    vao: u32,
    shader: u32,
    atlas: u32,
    mvp_loc: i32,
    tile_u_loc: i32,
}

// Slightly-expanded unit cube faces [x,y,z, u,v].
// EPS pushes each face out 0.003 to avoid z-fighting with the block surface.
fn build_cube() -> Vec<f32> {
    const EPS: f32 = 0.003;
    const N: f32 = -EPS;
    const P: f32 = 1.0 + EPS;

    // Helper: emit a quad as two CCW triangles.
    // p = four corners in order [BL, BR, TR, TL], uv matches.
    let mut v = Vec::<f32>::new();
    let mut quad = |pts: [[f32; 3]; 4]| {
        let uvs: [[f32; 2]; 4] = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
        for &i in &[0usize, 1, 2, 0, 2, 3] {
            v.extend_from_slice(&[pts[i][0], pts[i][1], pts[i][2], uvs[i][0], uvs[i][1]]);
        }
    };

    quad([[N,N,P],[P,N,P],[P,P,P],[N,P,P]]); // front  +Z
    quad([[P,N,N],[N,N,N],[N,P,N],[P,P,N]]); // back   -Z
    quad([[P,N,P],[P,N,N],[P,P,N],[P,P,P]]); // right  +X
    quad([[N,N,N],[N,N,P],[N,P,P],[N,P,N]]); // left   -X
    quad([[N,P,P],[P,P,P],[P,P,N],[N,P,N]]); // top    +Y
    quad([[N,N,N],[P,N,N],[P,N,P],[N,N,P]]); // bottom -Y

    v
}

impl CrackRenderer {
    pub fn new() -> Self {
        let cube = build_cube();

        let vert = compile_shader(gl::VERTEX_SHADER, r#"#version 330 core
            layout(location = 0) in vec3 aPos;
            layout(location = 1) in vec2 aUV;
            uniform mat4 mvp;
            out vec2 vUV;
            void main() {
                gl_Position = mvp * vec4(aPos, 1.0);
                vUV = aUV;
            }"#).unwrap();

        let frag = compile_shader(gl::FRAGMENT_SHADER, r#"#version 330 core
            in vec2 vUV;
            out vec4 FragColor;
            uniform sampler2D atlas;
            uniform float tile_u;  // left edge of this tile in atlas [0,1]
            void main() {
                const float tile_size = 1.0 / 16.0;
                vec2 uv = vec2(tile_u + vUV.x * tile_size, vUV.y * tile_size);
                vec4 c = texture(atlas, uv);
                if (c.a < 0.1) discard;
                FragColor = c;
            }"#).unwrap();

        let shader = link_program(vert, frag).unwrap();

        unsafe {
            let mvp_loc    = gl::GetUniformLocation(shader, c"mvp".as_ptr());
            let tile_u_loc = gl::GetUniformLocation(shader, c"tile_u".as_ptr());

            let mut vao = 0u32;
            let mut vbo = 0u32;
            gl::GenVertexArrays(1, &mut vao);
            gl::GenBuffers(1, &mut vbo);
            gl::BindVertexArray(vao);
            gl::BindBuffer(gl::ARRAY_BUFFER, vbo);
            gl::BufferData(
                gl::ARRAY_BUFFER,
                (cube.len() * mem::size_of::<f32>()) as isize,
                cube.as_ptr() as *const c_void,
                gl::STATIC_DRAW,
            );
            let stride = (5 * mem::size_of::<f32>()) as i32;
            gl::VertexAttribPointer(0, 3, gl::FLOAT, gl::FALSE, stride, std::ptr::null());
            gl::EnableVertexAttribArray(0);
            gl::VertexAttribPointer(1, 2, gl::FLOAT, gl::FALSE, stride,
                (3 * mem::size_of::<f32>()) as *const c_void);
            gl::EnableVertexAttribArray(1);
            gl::BindBuffer(gl::ARRAY_BUFFER, 0);
            gl::BindVertexArray(0);
            let _ = vbo;

            let atlas = create_block_atlas();

            CrackRenderer { vao, shader, atlas, mvp_loc, tile_u_loc }
        }
    }

    /// Draw the crack overlay for `block_pos` at the given dig stage (0 = just started, 4 = almost done).
    pub fn draw(&self, block_pos: [i32; 3], stage: usize, view: &glam::Mat4, projection: &glam::Mat4) {
        let translation = glam::Mat4::from_translation(glam::Vec3::new(
            block_pos[0] as f32, block_pos[1] as f32, block_pos[2] as f32,
        ));
        let mvp = *projection * *view * translation;

        // Crack tiles occupy atlas columns 9–13 in row 0.
        let tile_col = (9 + stage.min(4)) as f32;
        let tile_u = tile_col / 16.0;

        unsafe {
            gl::Enable(gl::BLEND);
            gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);
            gl::DepthMask(gl::FALSE);

            gl::UseProgram(self.shader);
            gl::UniformMatrix4fv(self.mvp_loc, 1, gl::FALSE, mvp.as_ref().as_ptr());
            gl::Uniform1f(self.tile_u_loc, tile_u);

            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, self.atlas);

            gl::BindVertexArray(self.vao);
            gl::DrawArrays(gl::TRIANGLES, 0, 36);
            gl::BindVertexArray(0);

            gl::DepthMask(gl::TRUE);
        }
    }
}

impl Drop for CrackRenderer {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteVertexArrays(1, &self.vao);
            gl::DeleteProgram(self.shader);
            gl::DeleteTextures(1, &self.atlas);
        }
    }
}
