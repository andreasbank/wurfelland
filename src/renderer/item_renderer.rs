use std::mem;
use std::os::raw::c_void;
use crate::renderer::utils::{compile_shader, link_program};
use crate::world::item::ItemType;
use crate::world::ItemEntity;

// Vertex format: [x, y, z, r, g, b] — 6 floats, colors baked per vertex.
const STRIDE: usize = 6;

fn push_vert(v: &mut Vec<f32>, x: f32, y: f32, z: f32, r: f32, g: f32, b: f32) {
    v.extend_from_slice(&[x, y, z, r, g, b]);
}

// Emit a CCW quad as two triangles with a flat color.
fn push_quad(v: &mut Vec<f32>, p: [[f32; 3]; 4], r: f32, g: f32, b: f32) {
    for &i in &[0usize, 1, 2, 0, 2, 3] {
        push_vert(v, p[i][0], p[i][1], p[i][2], r, g, b);
    }
}

// ── Stick: a 0.3×0.3 flat quad in the XY plane, centered on X, base at Y=0 ──
fn build_stick_mesh() -> Vec<f32> {
    let mut v = Vec::new();
    let [r, g, b] = ItemType::Stick.color();
    // Front face
    push_quad(&mut v, [[-0.15, 0.0, 0.0], [0.15, 0.0, 0.0], [0.15, 0.3, 0.0], [-0.15, 0.3, 0.0]], r, g, b);
    // Back face (reversed winding so it's visible from behind)
    push_quad(&mut v, [[-0.15, 0.3, 0.0], [0.15, 0.3, 0.0], [0.15, 0.0, 0.0], [-0.15, 0.0, 0.0]], r, g, b);
    v
}

// ── Generic small cube (0.35³, base at Y=0) with brightness per face ──
fn build_cube_mesh(base: [f32; 3]) -> Vec<f32> {
    let mut v = Vec::new();
    let [r, g, b] = base;
    const H: f32 = 0.175;
    const CY: f32 = H;
    // Top, Bottom, Front/Back, Left/Right — brightness matches chunk renderer
    push_quad(&mut v, [[-H,CY+H,-H],[H,CY+H,-H],[H,CY+H,H],[-H,CY+H,H]], r,       g,       b      );
    push_quad(&mut v, [[-H,CY-H,H],[H,CY-H,H],[H,CY-H,-H],[-H,CY-H,-H]], r*0.50, g*0.50, b*0.50);
    push_quad(&mut v, [[-H,CY-H,H],[H,CY-H,H],[H,CY+H,H],[-H,CY+H,H]],   r*0.80, g*0.80, b*0.80);
    push_quad(&mut v, [[H,CY-H,-H],[-H,CY-H,-H],[-H,CY+H,-H],[H,CY+H,-H]],r*0.80,g*0.80, b*0.80);
    push_quad(&mut v, [[-H,CY-H,-H],[-H,CY-H,H],[-H,CY+H,H],[-H,CY+H,-H]],r*0.65,g*0.65,b*0.65);
    push_quad(&mut v, [[H,CY-H,H],[H,CY-H,-H],[H,CY+H,-H],[H,CY+H,H]],    r*0.65,g*0.65, b*0.65);
    v
}

// ── Log cube: same shape but top uses tan ring color and sides use bark color ──
fn build_log_mesh() -> Vec<f32> {
    let mut v = Vec::new();
    const H: f32 = 0.175;
    const CY: f32 = H;
    let [sr, sg, sb] = [0.55_f32, 0.35, 0.17]; // bark base
    // Top — lighter tan rings
    push_quad(&mut v, [[-H,CY+H,-H],[H,CY+H,-H],[H,CY+H,H],[-H,CY+H,H]], 0.73, 0.59, 0.37);
    // Bottom
    push_quad(&mut v, [[-H,CY-H,H],[H,CY-H,H],[H,CY-H,-H],[-H,CY-H,-H]], sr*0.50, sg*0.50, sb*0.50);
    // Front/Back
    push_quad(&mut v, [[-H,CY-H,H],[H,CY-H,H],[H,CY+H,H],[-H,CY+H,H]],   sr*0.80, sg*0.80, sb*0.80);
    push_quad(&mut v, [[H,CY-H,-H],[-H,CY-H,-H],[-H,CY+H,-H],[H,CY+H,-H]],sr*0.80,sg*0.80, sb*0.80);
    // Left/Right
    push_quad(&mut v, [[-H,CY-H,-H],[-H,CY-H,H],[-H,CY+H,H],[-H,CY+H,-H]],sr*0.65,sg*0.65,sb*0.65);
    push_quad(&mut v, [[H,CY-H,H],[H,CY-H,-H],[H,CY+H,-H],[H,CY+H,H]],    sr*0.65,sg*0.65, sb*0.65);
    v
}

fn upload_vao(mesh: &[f32]) -> u32 {
    unsafe {
        let mut vao = 0u32;
        let mut vbo = 0u32;
        gl::GenVertexArrays(1, &mut vao);
        gl::GenBuffers(1, &mut vbo);
        gl::BindVertexArray(vao);
        gl::BindBuffer(gl::ARRAY_BUFFER, vbo);
        gl::BufferData(
            gl::ARRAY_BUFFER,
            (mesh.len() * mem::size_of::<f32>()) as isize,
            mesh.as_ptr() as *const c_void,
            gl::STATIC_DRAW,
        );
        let stride = (STRIDE * mem::size_of::<f32>()) as i32;
        // attrib 0: position
        gl::VertexAttribPointer(0, 3, gl::FLOAT, gl::FALSE, stride, std::ptr::null());
        gl::EnableVertexAttribArray(0);
        // attrib 1: color
        gl::VertexAttribPointer(1, 3, gl::FLOAT, gl::FALSE, stride,
            (3 * mem::size_of::<f32>()) as *const c_void);
        gl::EnableVertexAttribArray(1);
        gl::BindBuffer(gl::ARRAY_BUFFER, 0);
        gl::BindVertexArray(0);
        let _ = vbo;
        vao
    }
}

pub struct ItemRenderer {
    vao_quad: u32,  // stick
    vao_cube: u32,  // log block
    vao_dirt: u32,  // dirt clump
    vao_stone: u32, // stone chunk
    shader: u32,
    mvp_loc: i32,
    quad_vert_count: i32,
    cube_vert_count: i32,
    dirt_vert_count: i32,
    stone_vert_count: i32,
}

impl ItemRenderer {
    pub fn new() -> Self {
        let vert = compile_shader(gl::VERTEX_SHADER, r#"#version 330 core
            layout(location = 0) in vec3 aPos;
            layout(location = 1) in vec3 aColor;
            uniform mat4 mvp;
            out vec3 vColor;
            void main() {
                gl_Position = mvp * vec4(aPos, 1.0);
                vColor = aColor;
            }
        "#).unwrap();

        let frag = compile_shader(gl::FRAGMENT_SHADER, r#"#version 330 core
            in vec3 vColor;
            out vec4 FragColor;
            void main() { FragColor = vec4(vColor, 1.0); }
        "#).unwrap();

        let shader = link_program(vert, frag).unwrap();
        let mvp_loc = unsafe { gl::GetUniformLocation(shader, c"mvp".as_ptr()) };

        let stick_mesh = build_stick_mesh();
        let log_mesh   = build_log_mesh();
        let dirt_mesh  = build_cube_mesh([0.61, 0.44, 0.22]);
        let stone_mesh = build_cube_mesh([0.50, 0.50, 0.50]);

        let quad_vert_count  = (stick_mesh.len() / STRIDE) as i32;
        let cube_vert_count  = (log_mesh.len()   / STRIDE) as i32;
        let dirt_vert_count  = (dirt_mesh.len()  / STRIDE) as i32;
        let stone_vert_count = (stone_mesh.len() / STRIDE) as i32;

        let vao_quad  = upload_vao(&stick_mesh);
        let vao_cube  = upload_vao(&log_mesh);
        let vao_dirt  = upload_vao(&dirt_mesh);
        let vao_stone = upload_vao(&stone_mesh);

        ItemRenderer { vao_quad, vao_cube, vao_dirt, vao_stone, shader, mvp_loc,
                       quad_vert_count, cube_vert_count, dirt_vert_count, stone_vert_count }
    }

    pub fn draw(&self, items: &[ItemEntity], view: &glam::Mat4, projection: &glam::Mat4) {
        if items.is_empty() { return; }

        unsafe {
            gl::Disable(gl::CULL_FACE);
            gl::UseProgram(self.shader);

            for item in items {
                let (vao, vert_count) = match item.item {
                    ItemType::Stick      => (self.vao_quad,  self.quad_vert_count),
                    ItemType::LogBlock   => (self.vao_cube,  self.cube_vert_count),
                    ItemType::DirtClump  => (self.vao_dirt,  self.dirt_vert_count),
                    ItemType::StoneChunk => (self.vao_stone, self.stone_vert_count),
                };

                let pos = glam::Vec3::new(
                    item.position[0] + 0.5,
                    item.visual_y(),
                    item.position[2] + 0.5,
                );
                let model = glam::Mat4::from_translation(pos)
                    * glam::Mat4::from_rotation_y(item.age * 1.5);
                let mvp = *projection * *view * model;
                gl::UniformMatrix4fv(self.mvp_loc, 1, gl::FALSE, mvp.to_cols_array().as_ptr());

                gl::BindVertexArray(vao);
                gl::DrawArrays(gl::TRIANGLES, 0, vert_count);
            }

            gl::BindVertexArray(0);
            gl::Enable(gl::CULL_FACE);
        }
    }
}

impl Drop for ItemRenderer {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteVertexArrays(1, &self.vao_quad);
            gl::DeleteVertexArrays(1, &self.vao_cube);
            gl::DeleteVertexArrays(1, &self.vao_dirt);
            gl::DeleteVertexArrays(1, &self.vao_stone);
            gl::DeleteProgram(self.shader);
        }
    }
}
