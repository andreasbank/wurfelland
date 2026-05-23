use std::mem;
use std::os::raw::c_void;
use crate::renderer::utils::{compile_shader, link_program, create_item_atlas, load_png_texture};
use crate::renderer::geo_model::GeoModel;
use crate::world::item::ItemType;
use crate::world::ItemEntity;

// Vertex format for atlas items: [x, y, z, u, v] — 5 floats.
const STRIDE: usize = 5;

// Returns (u_min, u_max, v_top, v_bot) for a tile in the 256×256 item atlas.
// The atlas stores pixel data top-to-bottom, but OpenGL treats row 0 of TexImage2D
// data as the BOTTOM of the texture. So visual-top of a tile = lower GL v value.
fn tile_uv(tile_idx: usize) -> (f32, f32, f32, f32) {
    let col = tile_idx % 16;
    let row = tile_idx / 16;
    let u_min = col as f32 / 16.0;
    let u_max = (col + 1) as f32 / 16.0;
    let v_top = row as f32 / 16.0;         // top of tile image = low GL v
    let v_bot = (row + 1) as f32 / 16.0;   // bottom of tile image = high GL v
    (u_min, u_max, v_top, v_bot)
}

fn push_vert(v: &mut Vec<f32>, x: f32, y: f32, z: f32, u: f32, vt: f32) {
    v.extend_from_slice(&[x, y, z, u, vt]);
}

// Emit a CCW quad as two triangles.
// p: 4 corners [bottom-left, bottom-right, top-right, top-left]
// uv: matching UV per corner
fn push_quad(v: &mut Vec<f32>, p: [[f32; 3]; 4], uv: [[f32; 2]; 4]) {
    for &i in &[0usize, 1, 2, 0, 2, 3] {
        push_vert(v, p[i][0], p[i][1], p[i][2], uv[i][0], uv[i][1]);
    }
}

// ── Stick: flat double-sided quad, 0.3×0.3, centered on X, base at Y=0 ──
fn build_stick_mesh() -> Vec<f32> {
    let (u0, u1, vt, vb) = tile_uv(ItemType::Stick.tile_index());
    let mut v = Vec::new();
    // Front face: bottom-left, bottom-right, top-right, top-left
    push_quad(&mut v,
        [[-0.15, 0.0, 0.0], [0.15, 0.0, 0.0], [0.15, 0.3, 0.0], [-0.15, 0.3, 0.0]],
        [[u0, vb], [u1, vb], [u1, vt], [u0, vt]]);
    // Back face (reversed winding — same UV, mirrored will look fine for a stick)
    push_quad(&mut v,
        [[-0.15, 0.3, 0.0], [0.15, 0.3, 0.0], [0.15, 0.0, 0.0], [-0.15, 0.0, 0.0]],
        [[u0, vt], [u1, vt], [u1, vb], [u0, vb]]);
    v
}

// ── Flat sprite: double-sided square quad, 0.35×0.35, base at Y=0 ──
// UVs cover the full texture [0,1]×[0,1] so any PNG can be used directly.
fn build_sprite_mesh() -> Vec<f32> {
    const S: f32 = 0.175;
    let mut v = Vec::new();
    // Front face
    push_quad(&mut v,
        [[-S, 0.0, 0.0], [S, 0.0, 0.0], [S, S * 2.0, 0.0], [-S, S * 2.0, 0.0]],
        [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]]);
    // Back face
    push_quad(&mut v,
        [[-S, S * 2.0, 0.0], [S, S * 2.0, 0.0], [S, 0.0, 0.0], [-S, 0.0, 0.0]],
        [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
    v
}

// ── Flat bed slab (1.0×0.25×0.5, base at Y=0) ──
fn build_bed_mesh() -> Vec<f32> {
    let (u0, u1, vt, vb) = tile_uv(ItemType::Bed.tile_index());
    let uv = [[u0,vb],[u1,vb],[u1,vt],[u0,vt]];
    let mut v = Vec::new();
    const W: f32 = 0.5;   // half-width  (X)
    const H: f32 = 0.06;  // half-height (Y)
    const D: f32 = 0.25;  // half-depth  (Z)
    const CY: f32 = H;
    push_quad(&mut v, [[-W,CY+H,-D],[W,CY+H,-D],[W,CY+H,D],[-W,CY+H,D]],   uv); // top
    push_quad(&mut v, [[-W,CY-H,D],[W,CY-H,D],[W,CY-H,-D],[-W,CY-H,-D]],   uv); // bottom
    push_quad(&mut v, [[-W,CY-H,D],[W,CY-H,D],[W,CY+H,D],[-W,CY+H,D]],     uv); // front
    push_quad(&mut v, [[W,CY-H,-D],[-W,CY-H,-D],[-W,CY+H,-D],[W,CY+H,-D]], uv); // back
    push_quad(&mut v, [[-W,CY-H,-D],[-W,CY-H,D],[-W,CY+H,D],[-W,CY+H,-D]], uv); // left
    push_quad(&mut v, [[W,CY-H,D],[W,CY-H,-D],[W,CY+H,-D],[W,CY+H,D]],     uv); // right
    v
}

// ── Small cube (0.35³, base at Y=0): all 6 faces map the same tile ──
fn build_cube_mesh(tile_idx: usize) -> Vec<f32> {
    let (u0, u1, vt, vb) = tile_uv(tile_idx);
    // Standard face UV: bottom-left→bottom-right→top-right→top-left
    let uv = [[u0,vb],[u1,vb],[u1,vt],[u0,vt]];
    let mut v = Vec::new();
    const H: f32 = 0.175;
    const CY: f32 = H;
    push_quad(&mut v, [[-H,CY+H,-H],[H,CY+H,-H],[H,CY+H,H],[-H,CY+H,H]],   uv);
    push_quad(&mut v, [[-H,CY-H,H],[H,CY-H,H],[H,CY-H,-H],[-H,CY-H,-H]],   uv);
    push_quad(&mut v, [[-H,CY-H,H],[H,CY-H,H],[H,CY+H,H],[-H,CY+H,H]],     uv);
    push_quad(&mut v, [[H,CY-H,-H],[-H,CY-H,-H],[-H,CY+H,-H],[H,CY+H,-H]], uv);
    push_quad(&mut v, [[-H,CY-H,-H],[-H,CY-H,H],[-H,CY+H,H],[-H,CY+H,-H]], uv);
    push_quad(&mut v, [[H,CY-H,H],[H,CY-H,-H],[H,CY+H,-H],[H,CY+H,H]],     uv);
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
        // attrib 0: position (vec3)
        gl::VertexAttribPointer(0, 3, gl::FLOAT, gl::FALSE, stride, std::ptr::null());
        gl::EnableVertexAttribArray(0);
        // attrib 1: uv (vec2)
        gl::VertexAttribPointer(1, 2, gl::FLOAT, gl::FALSE, stride,
            (3 * mem::size_of::<f32>()) as *const c_void);
        gl::EnableVertexAttribArray(1);
        gl::BindBuffer(gl::ARRAY_BUFFER, 0);
        gl::BindVertexArray(0);
        let _ = vbo;
        vao
    }
}

pub struct ItemRenderer {
    // Atlas-textured items (Stick, cubes)
    vao_stick:         u32,
    vao_log:           u32,
    vao_dirt:          u32,
    vao_stone:         u32,
    vao_seeds:         u32,
    vao_bed:           u32,
    shader:            u32,
    mvp_loc:           i32,
    atlas:             u32,
    stick_vert_count:  i32,
    cube_vert_count:   i32,
    bed_vert_count:    i32,
    // Flat PNG-sprite items (RawCopper, Coal, …)
    vao_sprite:        u32,
    sprite_vert_count: i32,
    raw_copper_tex:    u32,
    coal_tex:          u32,
    // Colored geo items (loaded from JSON)
    colored_shader:    u32,
    colored_mvp_loc:   i32,
    axe_model:         Option<GeoModel>,
    torch_model:       Option<GeoModel>,
}

impl ItemRenderer {
    pub fn new() -> Self {
        // ── Atlas shader ──────────────────────────────────────────────────────
        let vert = compile_shader(gl::VERTEX_SHADER, r#"#version 330 core
            layout(location = 0) in vec3 aPos;
            layout(location = 1) in vec2 aUV;
            uniform mat4 mvp;
            out vec2 vUV;
            void main() {
                gl_Position = mvp * vec4(aPos, 1.0);
                vUV = aUV;
            }
        "#).unwrap();

        let frag = compile_shader(gl::FRAGMENT_SHADER, r#"#version 330 core
            in vec2 vUV;
            uniform sampler2D u_atlas;
            out vec4 FragColor;
            void main() {
                vec4 col = texture(u_atlas, vUV);
                if (col.a < 0.1) discard;
                FragColor = col;
            }
        "#).unwrap();

        let shader = link_program(vert, frag).unwrap();
        let mvp_loc = unsafe { gl::GetUniformLocation(shader, c"mvp".as_ptr()) };

        // ── Colored (vertex-color) shader for geo models ───────────────────────
        let cv = compile_shader(gl::VERTEX_SHADER, r#"#version 330 core
            layout(location = 0) in vec3 aPos;
            layout(location = 1) in vec3 aColor;
            uniform mat4 mvp;
            out vec3 vColor;
            void main() {
                gl_Position = mvp * vec4(aPos, 1.0);
                vColor = aColor;
            }
        "#).unwrap();

        let cf = compile_shader(gl::FRAGMENT_SHADER, r#"#version 330 core
            in vec3 vColor;
            out vec4 FragColor;
            void main() {
                FragColor = vec4(vColor, 1.0);
            }
        "#).unwrap();

        let colored_shader = link_program(cv, cf).unwrap();
        let colored_mvp_loc = unsafe { gl::GetUniformLocation(colored_shader, c"mvp".as_ptr()) };

        // ── Atlas meshes ───────────────────────────────────────────────────────
        let stick_mesh = build_stick_mesh();
        let log_mesh   = build_cube_mesh(ItemType::LogBlock.tile_index());
        let dirt_mesh  = build_cube_mesh(ItemType::DirtClump.tile_index());
        let stone_mesh = build_cube_mesh(ItemType::StoneChunk.tile_index());
        let seeds_mesh = build_cube_mesh(ItemType::Seeds.tile_index());
        let bed_mesh   = build_bed_mesh();

        let sprite_mesh = build_sprite_mesh();

        let stick_vert_count  = (stick_mesh.len()  / STRIDE) as i32;
        let cube_vert_count   = (log_mesh.len()    / STRIDE) as i32;
        let bed_vert_count    = (bed_mesh.len()    / STRIDE) as i32;
        let sprite_vert_count = (sprite_mesh.len() / STRIDE) as i32;

        let vao_stick  = upload_vao(&stick_mesh);
        let vao_log    = upload_vao(&log_mesh);
        let vao_dirt   = upload_vao(&dirt_mesh);
        let vao_stone  = upload_vao(&stone_mesh);
        let vao_seeds  = upload_vao(&seeds_mesh);
        let vao_bed    = upload_vao(&bed_mesh);
        let vao_sprite = upload_vao(&sprite_mesh);

        let atlas          = create_item_atlas();
        let raw_copper_tex = load_png_texture("assets/ui/raw_copper.png");
        let coal_tex       = load_png_texture("assets/ui/coal.png");

        // ── Geo models ────────────────────────────────────────────────────────
        let axe_model = match GeoModel::load("assets/models/stone_axe.geo.json") {
            Ok(m)  => Some(m),
            Err(e) => { eprintln!("[item_renderer] stone_axe.geo.json: {e}"); None }
        };
        let torch_model = match GeoModel::load("assets/models/torch.geo.json") {
            Ok(m)  => Some(m),
            Err(e) => { eprintln!("[item_renderer] torch.geo.json: {e}"); None }
        };

        ItemRenderer {
            vao_stick, vao_log, vao_dirt, vao_stone, vao_seeds, vao_bed,
            shader, mvp_loc, atlas,
            stick_vert_count, cube_vert_count, bed_vert_count,
            vao_sprite, sprite_vert_count, raw_copper_tex, coal_tex,
            colored_shader, colored_mvp_loc,
            axe_model,
            torch_model,
        }
    }

    pub fn draw(&self, items: &[ItemEntity], view: &glam::Mat4, projection: &glam::Mat4) {
        if items.is_empty() { return; }

        unsafe {
            gl::Disable(gl::CULL_FACE);
            gl::Enable(gl::BLEND);
            gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);

            // ── Atlas-textured items ───────────────────────────────────────────
            gl::UseProgram(self.shader);
            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, self.atlas);

            // ── Flat PNG sprites (RawCopper, Coal, …) ────────────────────────
            gl::BindVertexArray(self.vao_sprite);
            for item in items {
                let tex = match item.item {
                    ItemType::RawCopper => self.raw_copper_tex,
                    ItemType::Coal      => self.coal_tex,
                    _                   => continue,
                };
                gl::BindTexture(gl::TEXTURE_2D, tex);
                let pos = glam::Vec3::new(
                    item.position[0] + 0.5,
                    item.visual_y(),
                    item.position[2] + 0.5,
                );
                let model = glam::Mat4::from_translation(pos)
                    * glam::Mat4::from_rotation_y(item.age * 1.5);
                let mvp = *projection * *view * model;
                gl::UniformMatrix4fv(self.mvp_loc, 1, gl::FALSE, mvp.to_cols_array().as_ptr());
                gl::DrawArrays(gl::TRIANGLES, 0, self.sprite_vert_count);
            }
            gl::BindVertexArray(0);
            gl::BindTexture(gl::TEXTURE_2D, self.atlas);

            for item in items {
                let (vao, vert_count) = match item.item {
                    ItemType::Stick       => (self.vao_stick, self.stick_vert_count),
                    ItemType::LogBlock    => (self.vao_log,   self.cube_vert_count),
                    ItemType::DirtClump   => (self.vao_dirt,  self.cube_vert_count),
                    ItemType::StoneChunk  => (self.vao_stone, self.cube_vert_count),
                    ItemType::Seeds       => (self.vao_seeds, self.cube_vert_count),
                    ItemType::Bed         => (self.vao_bed,   self.bed_vert_count),
                    ItemType::Feather     => (self.vao_stone, self.cube_vert_count),
                    ItemType::Egg         => (self.vao_stone, self.cube_vert_count),
                    ItemType::ChickenMeat => (self.vao_stone, self.cube_vert_count),
                    ItemType::PorkChop    => (self.vao_stone, self.cube_vert_count),
                    ItemType::Furnace     => (self.vao_stone, self.cube_vert_count),
                    ItemType::RawCopper   => continue, // handled by sprite pass above
                    ItemType::Coal        => continue, // handled by sprite pass above
                    ItemType::RawIron     => continue, // handled by sprite pass above
                    ItemType::StoneAxe    => continue, // handled by geo pass below
                    ItemType::Torch       => continue, // handled by geo pass below
                    ItemType::PumpkinSeeds => (self.vao_seeds, self.cube_vert_count),
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
            gl::BindTexture(gl::TEXTURE_2D, 0);

            // ── Colored geo items (StoneAxe, Torch, …) ────────────────────────
            gl::UseProgram(self.colored_shader);
            let geo_pairs: &[(&Option<GeoModel>, ItemType)] = &[
                (&self.axe_model,   ItemType::StoneAxe),
                (&self.torch_model, ItemType::Torch),
            ];
            for (model_opt, kind) in geo_pairs {
                if let Some(geo) = model_opt {
                    for item in items {
                        if item.item != *kind { continue; }
                        let pos = glam::Vec3::new(
                            item.position[0] + 0.5,
                            item.visual_y(),
                            item.position[2] + 0.5,
                        );
                        let model = glam::Mat4::from_translation(pos)
                            * glam::Mat4::from_rotation_y(item.age * 1.5);
                        let mvp = *projection * *view * model;
                        gl::UniformMatrix4fv(self.colored_mvp_loc, 1, gl::FALSE, mvp.to_cols_array().as_ptr());
                        gl::BindVertexArray(geo.vao);
                        gl::DrawArrays(gl::TRIANGLES, 0, geo.vert_count);
                    }
                    gl::BindVertexArray(0);
                }
            }

            gl::Disable(gl::BLEND);
            gl::Enable(gl::CULL_FACE);
        }
    }

    /// Render a single item held in a player's hand.
    /// `mvp` comes from `PlayerRenderer::hand_item_mvp`.
    pub fn draw_held(&self, item: ItemType, mvp: &glam::Mat4) {
        unsafe {
            gl::Disable(gl::CULL_FACE);
            gl::Enable(gl::BLEND);
            gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);

            // Geo items (vertex-colored models)
            let geo: Option<&GeoModel> = match item {
                ItemType::StoneAxe => self.axe_model.as_ref(),
                ItemType::Torch    => self.torch_model.as_ref(),
                _                  => None,
            };

            if let Some(geo) = geo {
                gl::UseProgram(self.colored_shader);
                gl::UniformMatrix4fv(self.colored_mvp_loc, 1, gl::FALSE, mvp.to_cols_array().as_ptr());
                gl::BindVertexArray(geo.vao);
                gl::DrawArrays(gl::TRIANGLES, 0, geo.vert_count);
            } else {
                // Atlas items — render using the appropriate VAO
                let (vao, count) = match item {
                    ItemType::Stick => (self.vao_stick, self.stick_vert_count),
                    ItemType::Bed   => (self.vao_bed,   self.bed_vert_count),
                    _               => (self.vao_stone,  self.cube_vert_count),
                };
                gl::UseProgram(self.shader);
                gl::ActiveTexture(gl::TEXTURE0);
                gl::BindTexture(gl::TEXTURE_2D, self.atlas);
                gl::UniformMatrix4fv(self.mvp_loc, 1, gl::FALSE, mvp.to_cols_array().as_ptr());
                gl::BindVertexArray(vao);
                gl::DrawArrays(gl::TRIANGLES, 0, count);
                gl::BindTexture(gl::TEXTURE_2D, 0);
            }

            gl::BindVertexArray(0);
            gl::Disable(gl::BLEND);
            gl::Enable(gl::CULL_FACE);
        }
    }
}

impl Drop for ItemRenderer {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteVertexArrays(1, &self.vao_stick);
            gl::DeleteVertexArrays(1, &self.vao_log);
            gl::DeleteVertexArrays(1, &self.vao_dirt);
            gl::DeleteVertexArrays(1, &self.vao_stone);
            gl::DeleteVertexArrays(1, &self.vao_seeds);
            gl::DeleteVertexArrays(1, &self.vao_bed);
            gl::DeleteVertexArrays(1, &self.vao_sprite);
            gl::DeleteTextures(1, &self.atlas);
            gl::DeleteTextures(1, &self.raw_copper_tex);
            gl::DeleteTextures(1, &self.coal_tex);
            gl::DeleteProgram(self.shader);
            gl::DeleteProgram(self.colored_shader);
        }
    }
}
