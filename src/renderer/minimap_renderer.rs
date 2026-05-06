use crate::renderer::utils::compile_shader;
use crate::world::World;

const MAP_SIZE: usize = 256;
const MAP_RADIUS: i32 = (MAP_SIZE / 2) as i32;
const MIN_REBUILD_FRAMES: u32 = 30; // ~0.5 s at 60 fps

pub struct MinimapRenderer {
    shader: u32,
    vao: u32,
    texture: u32,
    pixels: Vec<u8>,
    center_loc: i32,
    half_size_loc: i32,
    player_dir_loc: i32,
    uv_offset_loc: i32,
    last_chunk_x: i32,
    last_chunk_z: i32,
    cooldown: u32,
    rebuild_cx: i32, // player block position when the texture was last built
    rebuild_cz: i32,
    last_world_chunk_count: usize,
}

impl MinimapRenderer {
    pub fn new() -> Self {
        let vs = compile_shader(gl::VERTEX_SHADER, r#"#version 330 core
            uniform vec2 center;     // NDC center of the minimap disc
            uniform vec2 half_size;  // NDC half-extents of the bounding quad

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

        let fs = compile_shader(gl::FRAGMENT_SHADER, r#"#version 330 core
            in  vec2 uv;
            out vec4 FragColor;

            uniform sampler2D map_tex;
            uniform vec2 player_dir;  // (front.x, front.z) — world-space heading
            uniform vec2 uv_offset;   // shift in UV space from rebuild center to player

            void main() {
                vec2  d = uv - 0.5;
                float r = length(d);

                if (r > 0.5) discard;

                // Dark border ring.
                if (r > 0.46) {
                    FragColor = vec4(0.05, 0.05, 0.05, 0.85);
                    return;
                }

                // Player dot at center.
                if (r < 0.035) {
                    FragColor = vec4(1.0, 1.0, 1.0, 1.0);
                    return;
                }

                // Direction indicator: red wedge pointing where the player faces.
                if (r < 0.12) {
                    vec2  pdir = normalize(player_dir);
                    float alignment = dot(normalize(d), pdir);
                    if (alignment > 0.72) {
                        FragColor = vec4(0.95, 0.25, 0.25, 1.0);
                        return;
                    }
                }

                // Sample terrain, offset so the disc scrolls as the player moves.
                vec4 color = texture(map_tex, uv + uv_offset);
                FragColor = vec4(color.rgb, 0.88);
            }"#).unwrap();

        unsafe {
            let shader = gl::CreateProgram();
            gl::AttachShader(shader, vs);
            gl::AttachShader(shader, fs);
            gl::LinkProgram(shader);
            gl::DeleteShader(vs);
            gl::DeleteShader(fs);

            let mut texture = 0u32;
            gl::GenTextures(1, &mut texture);
            gl::BindTexture(gl::TEXTURE_2D, texture);
            let blank = vec![0u8; MAP_SIZE * MAP_SIZE * 4];
            gl::TexImage2D(
                gl::TEXTURE_2D, 0, gl::RGBA as i32,
                MAP_SIZE as i32, MAP_SIZE as i32, 0,
                gl::RGBA, gl::UNSIGNED_BYTE,
                blank.as_ptr() as *const _,
            );
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as i32);
            gl::BindTexture(gl::TEXTURE_2D, 0);

            let mut vao = 0u32;
            gl::GenVertexArrays(1, &mut vao);

            MinimapRenderer {
                shader,
                vao,
                texture,
                pixels: vec![0u8; MAP_SIZE * MAP_SIZE * 4],
                center_loc:     gl::GetUniformLocation(shader, c"center".as_ptr()),
                half_size_loc:  gl::GetUniformLocation(shader, c"half_size".as_ptr()),
                player_dir_loc: gl::GetUniformLocation(shader, c"player_dir".as_ptr()),
                uv_offset_loc:  gl::GetUniformLocation(shader, c"uv_offset".as_ptr()),
                last_chunk_x: i32::MAX,
                last_chunk_z: i32::MAX,
                cooldown: 0,
                rebuild_cx: 0,
                rebuild_cz: 0,
                last_world_chunk_count: 0,
            }
        }
    }

    /// Rebuilds the terrain texture only when the player crosses a chunk border
    /// (and the cooldown has expired), so most frames are free.
    pub fn update(&mut self, world: &World, player_x: f32, player_z: f32) {
        let cx = player_x as i32;
        let cz = player_z as i32;
        let chunk_x = cx.div_euclid(16);
        let chunk_z = cz.div_euclid(16);

        if self.cooldown > 0 { self.cooldown -= 1; }

        let new_chunks_arrived = world.chunk_count() != self.last_world_chunk_count;
        let player_moved_chunk = chunk_x != self.last_chunk_x || chunk_z != self.last_chunk_z;

        if !new_chunks_arrived && !player_moved_chunk {
            return;
        }
        if self.cooldown > 0 {
            return;
        }
        self.last_chunk_x = chunk_x;
        self.last_chunk_z = chunk_z;
        self.last_world_chunk_count = world.chunk_count();
        self.cooldown = MIN_REBUILD_FRAMES;

        // Record the exact player block position this texture is centered on.
        self.rebuild_cx = cx;
        self.rebuild_cz = cz;

        let chunk_min_x = (cx - MAP_RADIUS).div_euclid(16);
        let chunk_max_x = (cx + MAP_RADIUS - 1).div_euclid(16);
        let chunk_min_z = (cz - MAP_RADIUS).div_euclid(16);
        let chunk_max_z = (cz + MAP_RADIUS - 1).div_euclid(16);

        for chunk_z in chunk_min_z..=chunk_max_z {
            for chunk_x in chunk_min_x..=chunk_max_x {
                let chunk = world.chunk_at(chunk_x, chunk_z);

                let chunk_wx = chunk_x * 16;
                let chunk_wz = chunk_z * 16;

                for lz in 0..16usize {
                    let wz = chunk_wz + lz as i32;
                    let py = (wz - (cz - MAP_RADIUS)) as usize;
                    if py >= MAP_SIZE { continue; }

                    for lx in 0..16usize {
                        let wx = chunk_wx + lx as i32;
                        let px = (wx - (cx - MAP_RADIUS)) as usize;
                        if px >= MAP_SIZE { continue; }

                        let (block, surface_y) = if let Some(c) = chunk {
                            let mut blk = crate::world::BlockType::Air;
                            let mut sy  = 0i32;
                            for y in (0..16usize).rev() {
                                let b = c.get_block(lx, y, lz);
                                if b != crate::world::BlockType::Air {
                                    blk = b;
                                    sy  = y as i32;
                                    break;
                                }
                            }
                            (blk, sy)
                        } else {
                            (crate::world::BlockType::Air, 0)
                        };

                        let base  = block.color();
                        let shade = 0.55 + (surface_y as f32 / 15.0).clamp(0.0, 1.0) * 0.45;

                        let i = (py * MAP_SIZE + px) * 4;
                        self.pixels[i]     = (base[0] * shade * 255.0) as u8;
                        self.pixels[i + 1] = (base[1] * shade * 255.0) as u8;
                        self.pixels[i + 2] = (base[2] * shade * 255.0) as u8;
                        self.pixels[i + 3] = 255;
                    }
                }
            }
        }

        unsafe {
            gl::BindTexture(gl::TEXTURE_2D, self.texture);
            gl::TexSubImage2D(
                gl::TEXTURE_2D, 0, 0, 0,
                MAP_SIZE as i32, MAP_SIZE as i32,
                gl::RGBA, gl::UNSIGNED_BYTE,
                self.pixels.as_ptr() as *const _,
            );
            gl::BindTexture(gl::TEXTURE_2D, 0);
        }
    }

    /// Render the minimap disc. Call every frame — cheap, just a draw call.
    /// `player_x/z` is the current player position, used to scroll the map
    /// smoothly between texture rebuilds.
    pub fn draw(
        &self,
        player_x: f32,
        player_z: f32,
        player_front_x: f32,
        player_front_z: f32,
        win_w: f32,
        win_h: f32,
    ) {
        const RIGHT_MARGIN_PX: f32 = 20.0;
        const RADIUS_PX: f32 = 80.0;
        // Sits below the clock rect (15 top + 40 height + 8 gap = 63 px from top).
        const MINIMAP_TOP_MARGIN_PX: f32 = 63.0;

        let half_w = RADIUS_PX * 2.0 / win_w;
        let half_h = RADIUS_PX * 2.0 / win_h;
        let cx =  1.0 - (RIGHT_MARGIN_PX + RADIUS_PX) * 2.0 / win_w;
        let cy =  1.0 - (MINIMAP_TOP_MARGIN_PX + RADIUS_PX) * 2.0 / win_h;

        // How far the player has moved from the last rebuild center, in UV units.
        // 1 block = 1/MAP_SIZE of the texture.
        let uv_dx = (player_x - self.rebuild_cx as f32) / MAP_SIZE as f32;
        let uv_dz = (player_z - self.rebuild_cz as f32) / MAP_SIZE as f32;

        unsafe {
            gl::UseProgram(self.shader);
            gl::Uniform2f(self.center_loc, cx, cy);
            gl::Uniform2f(self.half_size_loc, half_w, half_h);
            gl::Uniform2f(self.player_dir_loc, player_front_x, player_front_z);
            gl::Uniform2f(self.uv_offset_loc, uv_dx, uv_dz);

            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, self.texture);

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

impl Drop for MinimapRenderer {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteProgram(self.shader);
            gl::DeleteVertexArrays(1, &self.vao);
            gl::DeleteTextures(1, &self.texture);
        }
    }
}
