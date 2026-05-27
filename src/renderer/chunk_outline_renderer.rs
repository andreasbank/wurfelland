use std::mem;
use std::os::raw::c_void;
use crate::renderer::utils::{compile_shader, link_program};

pub struct ChunkOutlineRenderer {
    vao:       u32,  // 16×16×16 chunk box
    vbo:       u32,
    unit_vao:  u32,  // [0,1]³ unit box, scaled per entity
    unit_vbo:  u32,
    dir_vao:   u32,  // single unit line along +Z, for direction indicator
    dir_vbo:   u32,
    shader:    u32,
    mvp_loc:   i32,
    color_loc: i32,
}

impl ChunkOutlineRenderer {
    pub fn new() -> Self {
        const S: f32 = 16.0;
        #[rustfmt::skip]
        let verts: [f32; 72] = [
            // Bottom face
            0.0, 0.0, 0.0,   S,   0.0, 0.0,
            S,   0.0, 0.0,   S,   0.0, S,
            S,   0.0, S,     0.0, 0.0, S,
            0.0, 0.0, S,     0.0, 0.0, 0.0,
            // Top face
            0.0, S,   0.0,   S,   S,   0.0,
            S,   S,   0.0,   S,   S,   S,
            S,   S,   S,     0.0, S,   S,
            0.0, S,   S,     0.0, S,   0.0,
            // Vertical edges
            0.0, 0.0, 0.0,   0.0, S,   0.0,
            S,   0.0, 0.0,   S,   S,   0.0,
            S,   0.0, S,     S,   S,   S,
            0.0, 0.0, S,     0.0, S,   S,
        ];

        unsafe {
            let mut vao = 0u32;
            let mut vbo = 0u32;
            gl::GenVertexArrays(1, &mut vao);
            gl::GenBuffers(1, &mut vbo);
            gl::BindVertexArray(vao);
            gl::BindBuffer(gl::ARRAY_BUFFER, vbo);
            gl::BufferData(
                gl::ARRAY_BUFFER,
                (verts.len() * mem::size_of::<f32>()) as isize,
                verts.as_ptr() as *const c_void,
                gl::STATIC_DRAW,
            );
            gl::VertexAttribPointer(0, 3, gl::FLOAT, gl::FALSE,
                (3 * mem::size_of::<f32>()) as i32, std::ptr::null());
            gl::EnableVertexAttribArray(0);
            gl::BindBuffer(gl::ARRAY_BUFFER, 0);
            gl::BindVertexArray(0);

            let vert = compile_shader(gl::VERTEX_SHADER, r#"#version 330 core
                layout(location = 0) in vec3 aPos;
                uniform mat4 mvp;
                void main() { gl_Position = mvp * vec4(aPos, 1.0); }
            "#).unwrap();

            let frag = compile_shader(gl::FRAGMENT_SHADER, r#"#version 330 core
                out vec4 FragColor;
                uniform vec4 u_color;
                void main() { FragColor = u_color; }
            "#).unwrap();

            let shader    = link_program(vert, frag).unwrap();
            let mvp_loc   = gl::GetUniformLocation(shader, c"mvp".as_ptr());
            let color_loc = gl::GetUniformLocation(shader, c"u_color".as_ptr());

            // Unit box [0,1]³ — 12 edges = 24 line vertices
            #[rustfmt::skip]
            let unit_verts: [f32; 72] = [
                0.0,0.0,0.0,  1.0,0.0,0.0,
                1.0,0.0,0.0,  1.0,0.0,1.0,
                1.0,0.0,1.0,  0.0,0.0,1.0,
                0.0,0.0,1.0,  0.0,0.0,0.0,
                0.0,1.0,0.0,  1.0,1.0,0.0,
                1.0,1.0,0.0,  1.0,1.0,1.0,
                1.0,1.0,1.0,  0.0,1.0,1.0,
                0.0,1.0,1.0,  0.0,1.0,0.0,
                0.0,0.0,0.0,  0.0,1.0,0.0,
                1.0,0.0,0.0,  1.0,1.0,0.0,
                1.0,0.0,1.0,  1.0,1.0,1.0,
                0.0,0.0,1.0,  0.0,1.0,1.0,
            ];
            let (mut unit_vao, mut unit_vbo) = (0u32, 0u32);
            gl::GenVertexArrays(1, &mut unit_vao);
            gl::GenBuffers(1, &mut unit_vbo);
            gl::BindVertexArray(unit_vao);
            gl::BindBuffer(gl::ARRAY_BUFFER, unit_vbo);
            gl::BufferData(gl::ARRAY_BUFFER,
                (unit_verts.len() * mem::size_of::<f32>()) as isize,
                unit_verts.as_ptr() as *const c_void, gl::STATIC_DRAW);
            gl::VertexAttribPointer(0, 3, gl::FLOAT, gl::FALSE,
                (3 * mem::size_of::<f32>()) as i32, std::ptr::null());
            gl::EnableVertexAttribArray(0);
            gl::BindBuffer(gl::ARRAY_BUFFER, 0);
            gl::BindVertexArray(0);

            // Unit forward line: origin → +Z (1 unit)
            let dir_verts: [f32; 6] = [0.0, 0.0, 0.0,  0.0, 0.0, 1.0];
            let (mut dir_vao, mut dir_vbo) = (0u32, 0u32);
            gl::GenVertexArrays(1, &mut dir_vao);
            gl::GenBuffers(1, &mut dir_vbo);
            gl::BindVertexArray(dir_vao);
            gl::BindBuffer(gl::ARRAY_BUFFER, dir_vbo);
            gl::BufferData(gl::ARRAY_BUFFER,
                (dir_verts.len() * mem::size_of::<f32>()) as isize,
                dir_verts.as_ptr() as *const c_void, gl::STATIC_DRAW);
            gl::VertexAttribPointer(0, 3, gl::FLOAT, gl::FALSE,
                (3 * mem::size_of::<f32>()) as i32, std::ptr::null());
            gl::EnableVertexAttribArray(0);
            gl::BindBuffer(gl::ARRAY_BUFFER, 0);
            gl::BindVertexArray(0);

            ChunkOutlineRenderer { vao, vbo, unit_vao, unit_vbo, dir_vao, dir_vbo, shader, mvp_loc, color_loc }
        }
    }

    fn begin_overlay(&self) {
        unsafe {
            gl::UseProgram(self.shader);
            gl::Disable(gl::DEPTH_TEST);
            gl::Disable(gl::CULL_FACE);
            gl::Enable(gl::BLEND);
            gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);
        }
    }

    fn end_overlay(&self) {
        unsafe {
            gl::BindVertexArray(0);
            gl::Enable(gl::DEPTH_TEST);
            gl::Enable(gl::CULL_FACE);
            gl::Disable(gl::BLEND);
        }
    }

    pub fn draw_chunks(&self, positions: &[[i32; 3]], view: &glam::Mat4, projection: &glam::Mat4) {
        self.begin_overlay();
        unsafe {
            gl::LineWidth(3.0);
            gl::Uniform4f(self.color_loc, 1.0, 0.0, 1.0, 0.6); // magenta
            gl::BindVertexArray(self.vao);
            for &[cx, cy, cz] in positions {
                let t = glam::Mat4::from_translation(glam::Vec3::new(
                    cx as f32 * 16.0, cy as f32 * 16.0, cz as f32 * 16.0,
                ));
                let mvp = *projection * *view * t;
                gl::UniformMatrix4fv(self.mvp_loc, 1, gl::FALSE, mvp.to_cols_array().as_ptr());
                gl::DrawArrays(gl::LINES, 0, 24);
            }
            gl::LineWidth(1.0);
        }
        self.end_overlay();
    }

    /// Draw a cyan wireframe box around each entity with a yellow direction arrow.
    /// `boxes`: (pos_feet, width, height, depth, box_yaw, dir_yaw)
    ///   box_yaw — matches the model's render transform
    ///   dir_yaw — actual movement direction (π/2 − entity.yaw_radians for all entity types)
    pub fn draw_entity_boxes(
        &self,
        boxes:      &[([f32; 3], f32, f32, f32, f32, f32)],
        view:       &glam::Mat4,
        projection: &glam::Mat4,
    ) {
        if boxes.is_empty() { return; }
        self.begin_overlay();
        unsafe {
            // Boxes in cyan
            gl::Uniform4f(self.color_loc, 0.0, 1.0, 1.0, 0.85);
            gl::BindVertexArray(self.unit_vao);
            for &(pos, w, h, d, box_yaw, _) in boxes {
                let model = glam::Mat4::from_translation(glam::Vec3::new(pos[0], pos[1], pos[2]))
                    * glam::Mat4::from_rotation_y(box_yaw)
                    * glam::Mat4::from_translation(glam::Vec3::new(-w * 0.5, 0.0, -d * 0.5))
                    * glam::Mat4::from_scale(glam::Vec3::new(w, h, d));
                let mvp = *projection * *view * model;
                gl::UniformMatrix4fv(self.mvp_loc, 1, gl::FALSE, mvp.to_cols_array().as_ptr());
                gl::DrawArrays(gl::LINES, 0, 24);
            }
            // Direction lines in yellow, 1.5 units forward from box centre
            gl::Uniform4f(self.color_loc, 1.0, 1.0, 0.0, 1.0);
            gl::BindVertexArray(self.dir_vao);
            for &(pos, _w, h, _d, _, dir_yaw) in boxes {
                let model = glam::Mat4::from_translation(glam::Vec3::new(pos[0], pos[1] + h * 0.5, pos[2]))
                    * glam::Mat4::from_rotation_y(dir_yaw)
                    * glam::Mat4::from_scale(glam::Vec3::new(1.0, 1.0, 1.5));
                let mvp = *projection * *view * model;
                gl::UniformMatrix4fv(self.mvp_loc, 1, gl::FALSE, mvp.to_cols_array().as_ptr());
                gl::DrawArrays(gl::LINES, 0, 2);
            }
        }
        self.end_overlay();
    }
}

impl Drop for ChunkOutlineRenderer {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteVertexArrays(1, &self.vao);
            gl::DeleteBuffers(1, &self.vbo);
            gl::DeleteVertexArrays(1, &self.unit_vao);
            gl::DeleteBuffers(1, &self.unit_vbo);
            gl::DeleteVertexArrays(1, &self.dir_vao);
            gl::DeleteBuffers(1, &self.dir_vbo);
            gl::DeleteProgram(self.shader);
        }
    }
}
