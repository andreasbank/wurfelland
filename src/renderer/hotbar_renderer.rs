use std::mem;
use std::os::raw::c_void;
use crate::renderer::utils::{compile_shader, link_program, create_block_atlas};
use crate::renderer::geo_atlas::GeoAtlas;
use crate::world::item::ItemType;

const SLOT_SIZE:   f32 = 40.0;
const SLOT_GAP:    f32 =  4.0;
const BORDER:      f32 =  2.0;
const SEL_BORDER:  f32 =  3.0;
const MARGIN_BOT:  f32 = 10.0;
const NUM_SLOTS:   usize = 9;

pub struct HotbarRenderer {
    vao: u32,
    shader: u32,
    pos_loc: i32,
    size_loc: i32,
    screen_loc: i32,
    color_loc: i32,
    // Textured shader for block/geo item icons
    tex_shader:     u32,
    tex_pos_loc:    i32,
    tex_size_loc:   i32,
    tex_screen_loc: i32,
    tex_uv0_loc:    i32,
    tex_uv1_loc:    i32,
    block_atlas:    u32,
    geo_atlas:      GeoAtlas,
}

impl HotbarRenderer {
    pub fn new() -> Self {
        // Unit quad [0,0]→[1,1] in two triangles
        #[rustfmt::skip]
        let verts: [f32; 12] = [
            0.0, 0.0,  1.0, 0.0,  1.0, 1.0,
            0.0, 0.0,  1.0, 1.0,  0.0, 1.0,
        ];

        let vert = compile_shader(gl::VERTEX_SHADER, r#"#version 330 core
            layout(location = 0) in vec2 aPos;
            uniform vec2 u_pos;
            uniform vec2 u_size;
            uniform vec2 u_screen;
            void main() {
                vec2 px  = u_pos + aPos * u_size;
                vec2 ndc = (px / u_screen) * 2.0 - 1.0;
                gl_Position = vec4(ndc, 0.0, 1.0);
            }
        "#).unwrap();

        let frag = compile_shader(gl::FRAGMENT_SHADER, r#"#version 330 core
            uniform vec4 u_color;
            out vec4 FragColor;
            void main() { FragColor = u_color; }
        "#).unwrap();

        let shader = link_program(vert, frag).unwrap();

        let tv = compile_shader(gl::VERTEX_SHADER, r#"#version 330 core
            layout(location = 0) in vec2 aPos;
            uniform vec2 u_pos;
            uniform vec2 u_size;
            uniform vec2 u_screen;
            uniform vec2 u_uv0;
            uniform vec2 u_uv1;
            out vec2 vUV;
            void main() {
                vec2 px  = u_pos + aPos * u_size;
                vec2 ndc = (px / u_screen) * 2.0 - 1.0;
                gl_Position = vec4(ndc, 0.0, 1.0);
                vUV = u_uv0 + aPos * (u_uv1 - u_uv0);
            }
        "#).unwrap();
        let tf = compile_shader(gl::FRAGMENT_SHADER, r#"#version 330 core
            in vec2 vUV;
            uniform sampler2D u_tex;
            out vec4 FragColor;
            void main() { FragColor = texture(u_tex, vUV); }
        "#).unwrap();
        let tex_shader = link_program(tv, tf).unwrap();

        unsafe {
            let pos_loc    = gl::GetUniformLocation(shader, c"u_pos".as_ptr());
            let size_loc   = gl::GetUniformLocation(shader, c"u_size".as_ptr());
            let screen_loc = gl::GetUniformLocation(shader, c"u_screen".as_ptr());
            let color_loc  = gl::GetUniformLocation(shader, c"u_color".as_ptr());

            let tex_pos_loc    = gl::GetUniformLocation(tex_shader, c"u_pos".as_ptr());
            let tex_size_loc   = gl::GetUniformLocation(tex_shader, c"u_size".as_ptr());
            let tex_screen_loc = gl::GetUniformLocation(tex_shader, c"u_screen".as_ptr());
            let tex_uv0_loc    = gl::GetUniformLocation(tex_shader, c"u_uv0".as_ptr());
            let tex_uv1_loc    = gl::GetUniformLocation(tex_shader, c"u_uv1".as_ptr());

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
            gl::VertexAttribPointer(0, 2, gl::FLOAT, gl::FALSE,
                (2 * mem::size_of::<f32>()) as i32, std::ptr::null());
            gl::EnableVertexAttribArray(0);
            gl::BindBuffer(gl::ARRAY_BUFFER, 0);
            gl::BindVertexArray(0);
            let _ = vbo;

            let block_atlas = create_block_atlas();
            let geo_atlas   = GeoAtlas::build("assets/models");

            HotbarRenderer {
                vao, shader, pos_loc, size_loc, screen_loc, color_loc,
                tex_shader, tex_pos_loc, tex_size_loc, tex_screen_loc,
                tex_uv0_loc, tex_uv1_loc, block_atlas, geo_atlas,
            }
        }
    }

    fn draw_tex(&self, x: f32, y: f32, size: f32,
                tex: u32, u0: f32, v0: f32, u1: f32, v1: f32, screen: [f32; 2]) {
        unsafe {
            gl::UseProgram(self.tex_shader);
            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, tex);
            gl::Uniform2f(self.tex_pos_loc,    x, y);
            gl::Uniform2f(self.tex_size_loc,   size, size);
            gl::Uniform2f(self.tex_screen_loc, screen[0], screen[1]);
            gl::Uniform2f(self.tex_uv0_loc,    u0, v0);
            gl::Uniform2f(self.tex_uv1_loc,    u1, v1);
            gl::DrawArrays(gl::TRIANGLES, 0, 6);
            gl::BindTexture(gl::TEXTURE_2D, 0);
            gl::UseProgram(self.shader);
        }
    }

    fn draw_rect(&self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4], screen: [f32; 2]) {
        unsafe {
            gl::Uniform2f(self.pos_loc,    x, y);
            gl::Uniform2f(self.size_loc,   w, h);
            gl::Uniform2f(self.screen_loc, screen[0], screen[1]);
            gl::Uniform4f(self.color_loc,  color[0], color[1], color[2], color[3]);
            gl::DrawArrays(gl::TRIANGLES, 0, 6);
        }
    }

    /// Draw a recognisable icon for `item` inside the slot whose bottom-left
    /// pixel corner is `(sx, sy)`.  The hotbar coordinate system has y=0 at the
    /// bottom of the screen (OpenGL NDC), so all vertical positions are mirrored
    /// compared to the bag renderer which uses y=0 at the top.
    fn draw_item_icon(&self, item: ItemType, sx: f32, sy: f32, screen: [f32; 2]) {
        let pad = SLOT_SIZE * 0.09; // matches bag_renderer proportions

        // Block items: sample the block atlas tile for the side face.
        // Block atlas: 256×256, 16×16 px per tile, 16 tiles per row.
        // v_top < v_bot because pixel data is stored top-first (OpenGL row 0 = bottom).
        let block_tile: Option<u32> = match item {
            ItemType::LogBlock   => Some(5),
            ItemType::DirtClump  => Some(1),
            ItemType::StoneChunk => Some(22),
            ItemType::WoodBlock  => Some(34),
            ItemType::Seeds      => Some(8),
            _                    => None,
        };
        if let Some(tile_idx) = block_tile {
            let col   = (tile_idx % 16) as f32;
            let row   = (tile_idx / 16) as f32;
            let u0    = col / 16.0;
            let u1    = (col + 1.0) / 16.0;
            let v_top = row / 16.0;
            let v_bot = (row + 1.0) / 16.0;
            self.draw_tex(sx + pad, sy + pad, SLOT_SIZE - pad * 2.0,
                self.block_atlas, u0, v_bot, u1, v_top, screen);
            return;
        }

        // Geo-model items: draw icon from the geo atlas.
        if let Some([u0, v_bot, u1, v_top]) = self.geo_atlas.uv_for_item(item) {
            self.draw_tex(sx + pad, sy + pad, SLOT_SIZE - pad * 2.0,
                self.geo_atlas.texture_id, u0, v_bot, u1, v_top, screen);
            return;
        }

        // Helper: convert bag-style (top_frac, bot_frac) to hotbar (y, h).
        // bag uses y-from-top; hotbar uses y-from-bottom.
        //   y = sy + (1 - bot_frac) * SLOT_SIZE
        //   h =      (bot_frac - top_frac) * SLOT_SIZE
        match item {
            ItemType::Stick => {
                // Vertical stick (bag draws it rotated 45°; hotbar can't rotate)
                self.draw_rect(
                    sx + SLOT_SIZE * 0.44,
                    sy + SLOT_SIZE * 0.10,
                    SLOT_SIZE * 0.12,
                    SLOT_SIZE * 0.80,
                    [0.55, 0.35, 0.17, 1.0], screen,
                );
            }
            _ => {
                let [r, g, b] = item.color();
                self.draw_rect(
                    sx + pad, sy + pad,
                    SLOT_SIZE - pad * 2.0, SLOT_SIZE - pad * 2.0,
                    [r, g, b, 1.0], screen,
                );
            }
        }
    }

    /// Returns which hotbar slot index was hit, or None.
    /// `mx/my` are raw GLFW pixel coords (Y from top).
    pub fn slot_at_pos(&self, mx: f32, my: f32, screen_w: f32, screen_h: f32) -> Option<usize> {
        let bar_w   = NUM_SLOTS as f32 * SLOT_SIZE + (NUM_SLOTS - 1) as f32 * SLOT_GAP;
        let start_x = (screen_w - bar_w) * 0.5;
        // Hotbar lives at GL-pixel-y = MARGIN_BOT..MARGIN_BOT+SLOT_SIZE (from bottom).
        // GLFW y is from top → convert: glfw_y = screen_h - gl_y.
        let glfw_top = screen_h - (MARGIN_BOT + SLOT_SIZE);
        let glfw_bot = screen_h - MARGIN_BOT;
        if my < glfw_top || my > glfw_bot { return None; }
        let lx = mx - start_x;
        if lx < 0.0 { return None; }
        let slot = (lx / (SLOT_SIZE + SLOT_GAP)) as usize;
        if slot >= NUM_SLOTS { return None; }
        if lx - slot as f32 * (SLOT_SIZE + SLOT_GAP) > SLOT_SIZE { return None; }
        Some(slot)
    }

    pub fn draw(
        &self,
        selected: usize,
        slots: &[Option<(ItemType, u32)>; NUM_SLOTS],
        screen_w: f32,
        screen_h: f32,
    ) {
        let screen = [screen_w, screen_h];

        // Total bar width, centered on screen
        let bar_w = NUM_SLOTS as f32 * SLOT_SIZE + (NUM_SLOTS - 1) as f32 * SLOT_GAP;
        let start_x = (screen_w - bar_w) * 0.5;
        let start_y = MARGIN_BOT;

        unsafe {
            gl::Disable(gl::DEPTH_TEST);
            gl::Enable(gl::BLEND);
            gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);
            gl::UseProgram(self.shader);
            gl::BindVertexArray(self.vao);

            for i in 0..NUM_SLOTS {
                let sx = start_x + i as f32 * (SLOT_SIZE + SLOT_GAP);
                let sy = start_y;
                let is_sel = i == selected;

                let border = if is_sel { SEL_BORDER } else { BORDER };
                let bdr_color: [f32; 4] = if is_sel {
                    [1.0, 1.0, 1.0, 1.0]
                } else {
                    [0.55, 0.55, 0.55, 0.90]
                };

                // Border (drawn slightly larger than the slot)
                self.draw_rect(
                    sx - border, sy - border,
                    SLOT_SIZE + border * 2.0, SLOT_SIZE + border * 2.0,
                    bdr_color, screen,
                );
                // Background
                self.draw_rect(sx, sy, SLOT_SIZE, SLOT_SIZE,
                    [0.18, 0.18, 0.18, 0.75], screen);

                // Item icon
                if let Some((item, _count)) = slots[i] {
                    self.draw_item_icon(item, sx, sy, screen);
                }
            }

            gl::BindVertexArray(0);
            gl::Disable(gl::BLEND);
            gl::Enable(gl::DEPTH_TEST);
        }
    }
}

impl Drop for HotbarRenderer {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteVertexArrays(1, &self.vao);
            gl::DeleteProgram(self.shader);
            gl::DeleteProgram(self.tex_shader);
            gl::DeleteTextures(1, &self.block_atlas);
            // geo_atlas has its own Drop impl
        }
    }
}
