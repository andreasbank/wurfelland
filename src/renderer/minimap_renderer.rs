use std::collections::{HashSet, VecDeque};
use crate::renderer::utils::compile_shader;
use crate::world::World;
use crate::world::BlockType;

const MAP_SIZE: usize = 512;         // toroidal texture side (must be power-of-2, 32 * CHUNK_SIZE)
const VIEW_RADIUS: f32 = 80.0;       // blocks shown from player to disc edge
const SCAN_CHUNK_RADIUS: i32 = 20;   // scan up to this many chunks from player
const CHUNKS_PER_FRAME: usize = 16;  // max chunks painted per update() call
const MAX_ENTITIES: usize = 32;      // max entity dots rendered in shader

pub struct MinimapRenderer {
    shader: u32,
    vao: u32,
    texture: u32,
    pixels: Vec<u8>,     // MAP_SIZE * MAP_SIZE * 4  RGBA CPU buffer
    heights: Vec<i32>,   // MAP_SIZE * MAP_SIZE  surface Y per texel (slope shading)
    known_chunks: HashSet<(i32, i32)>,
    in_queue: HashSet<(i32, i32)>,
    scan_queue: VecDeque<(i32, i32)>,
    last_player_chunk: (i32, i32),
    last_chunk_count: usize,
    // uniform locations
    u_center: i32,
    u_half_size: i32,
    u_player_uv: i32,
    u_view_scale: i32,
    u_player_dir: i32,
    u_entity_count: i32,
    u_entity_offsets: i32,
    u_entity_colors: i32,
}

impl MinimapRenderer {
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

        let fs = compile_shader(gl::FRAGMENT_SHADER, r#"#version 330 core
            in  vec2 uv;
            out vec4 FragColor;

            uniform sampler2D map_tex;
            uniform vec2  player_uv;       // player's position in [0,1) toroidal UV
            uniform float view_scale;      // 2 * VIEW_RADIUS / MAP_SIZE
            uniform vec2  player_dir;      // (front_x, front_z) world heading
            uniform int   entity_count;
            uniform vec2  entity_offsets[32]; // (ex-px, ez-pz) world blocks
            uniform vec3  entity_colors[32];

            void main() {
                vec2  d = uv - 0.5;
                float r = length(d);
                if (r > 0.5) discard;

                // Dark border ring
                if (r > 0.46) {
                    FragColor = vec4(0.05, 0.05, 0.05, 0.90);
                    return;
                }

                // Heading-up rotation:
                //   angle = atan2(-front_x, -front_z) is 0 when facing north (front_z=-1)
                //   rot rotates a disc coord d into a "north-up" world direction rd
                float angle = atan(-player_dir.x, -player_dir.y);
                vec2 cs  = vec2(cos(angle), sin(angle));
                mat2 rot = mat2(cs.x, cs.y, -cs.y, cs.x); // col-major: (cosθ, sinθ, -sinθ, cosθ)

                vec2 rd = rot * d;

                // Sample toroidal terrain (GL_REPEAT wrapping handles the wrap-around).
                // Z-flip: texture V increases south (+Z), disc Y increases north.
                vec2 tex_uv = player_uv + vec2(rd.x, -rd.y) * view_scale;
                vec4 terrain = texture(map_tex, tex_uv);

                // Player dot (topmost priority)
                if (r < 0.03) {
                    FragColor = vec4(1.0, 1.0, 1.0, 1.0);
                    return;
                }

                // Entity dots
                // rot_inv = transpose of rot = rotation by -angle
                mat2 rot_inv = mat2(cs.x, -cs.y, cs.y, cs.x);
                for (int i = 0; i < entity_count && i < 32; i++) {
                    // Convert world offset to north-up disc coord, then rotate to screen space
                    vec2 ent_rd = vec2(entity_offsets[i].x, -entity_offsets[i].y) * (0.5 / 80.0);
                    vec2 ent_d  = rot_inv * ent_rd;
                    if (length(d - ent_d) < 0.025 && length(ent_d) < 0.44) {
                        FragColor = vec4(entity_colors[i], 1.0);
                        return;
                    }
                }

                FragColor = vec4(terrain.rgb, 0.88);
            }"#).unwrap();

        unsafe {
            let shader = gl::CreateProgram();
            gl::AttachShader(shader, vs);
            gl::AttachShader(shader, fs);
            gl::LinkProgram(shader);
            gl::DeleteShader(vs);
            gl::DeleteShader(fs);

            // Set sampler uniform once (texture unit 0)
            gl::UseProgram(shader);
            let map_tex_loc = gl::GetUniformLocation(shader, c"map_tex".as_ptr());
            gl::Uniform1i(map_tex_loc, 0);
            gl::UseProgram(0);

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
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::REPEAT as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::REPEAT as i32);
            gl::BindTexture(gl::TEXTURE_2D, 0);

            let mut vao = 0u32;
            gl::GenVertexArrays(1, &mut vao);

            let u_center         = gl::GetUniformLocation(shader, c"center".as_ptr());
            let u_half_size      = gl::GetUniformLocation(shader, c"half_size".as_ptr());
            let u_player_uv      = gl::GetUniformLocation(shader, c"player_uv".as_ptr());
            let u_view_scale     = gl::GetUniformLocation(shader, c"view_scale".as_ptr());
            let u_player_dir     = gl::GetUniformLocation(shader, c"player_dir".as_ptr());
            let u_entity_count   = gl::GetUniformLocation(shader, c"entity_count".as_ptr());
            let u_entity_offsets = gl::GetUniformLocation(shader, c"entity_offsets[0]".as_ptr());
            let u_entity_colors  = gl::GetUniformLocation(shader, c"entity_colors[0]".as_ptr());

            MinimapRenderer {
                shader, vao, texture,
                pixels:  vec![0u8;  MAP_SIZE * MAP_SIZE * 4],
                heights: vec![0i32; MAP_SIZE * MAP_SIZE],
                known_chunks: HashSet::new(),
                in_queue:     HashSet::new(),
                scan_queue:   VecDeque::new(),
                last_player_chunk: (i32::MAX, i32::MAX),
                last_chunk_count: 0,
                u_center, u_half_size, u_player_uv, u_view_scale,
                u_player_dir, u_entity_count, u_entity_offsets, u_entity_colors,
            }
        }
    }

    // Clear scan state and enqueue all chunks in SCAN_CHUNK_RADIUS around (pcx, pcz),
    // sorted nearest-first so the visible area paints before far edges.
    fn enqueue_around(&mut self, pcx: i32, pcz: i32) {
        self.known_chunks.clear();
        self.scan_queue.clear();
        self.in_queue.clear();

        let r = SCAN_CHUNK_RADIUS;
        let mut candidates: Vec<(i32, i32, i32)> = Vec::with_capacity(((2*r+1)*(2*r+1)) as usize);
        for dz in -r..=r {
            for dx in -r..=r {
                let dist = dx.abs().max(dz.abs()); // Chebyshev distance
                candidates.push((dist, pcx + dx, pcz + dz));
            }
        }
        candidates.sort_unstable_by_key(|&(d, _, _)| d);
        for (_, cx, cz) in candidates {
            self.scan_queue.push_back((cx, cz));
            self.in_queue.insert((cx, cz));
        }
    }

    // Paint one 16×16 chunk into the toroidal CPU buffer and upload to GPU.
    // Returns false if the chunk is not yet loaded (pixels left unpainted).
    fn scan_chunk(&mut self, cx: i32, cz: i32, world: &World) -> bool {
        let Some(chunk) = world.chunk_at(cx, cz) else { return false };

        let chunk_wx = cx * 16;
        let chunk_wz = cz * 16;

        for lz in 0..16usize {
            for lx in 0..16usize {
                let wx = chunk_wx + lx as i32;
                let wz = chunk_wz + lz as i32;
                let tx = (wx & (MAP_SIZE as i32 - 1)) as usize;
                let tz = (wz & (MAP_SIZE as i32 - 1)) as usize;

                // Find topmost non-air block
                let mut block = BlockType::Air;
                let mut height = 0i32;
                for y in (0..16usize).rev() {
                    let b = chunk.get_block(lx, y, lz);
                    if b != BlockType::Air {
                        block = b;
                        height = y as i32;
                        break;
                    }
                }

                self.heights[tz * MAP_SIZE + tx] = height;

                // Slope shading: brighter when higher than north neighbor
                let north_tz = tz.wrapping_sub(1) & (MAP_SIZE - 1);
                let north_h  = self.heights[north_tz * MAP_SIZE + tx];
                let slope    = (height - north_h).clamp(-3, 3);
                let shade    = 1.0f32 + slope as f32 * 0.08;

                let base = block.color();
                let pi   = (tz * MAP_SIZE + tx) * 4;
                self.pixels[pi]     = ((base[0] * shade).clamp(0.0, 1.0) * 255.0) as u8;
                self.pixels[pi + 1] = ((base[1] * shade).clamp(0.0, 1.0) * 255.0) as u8;
                self.pixels[pi + 2] = ((base[2] * shade).clamp(0.0, 1.0) * 255.0) as u8;
                self.pixels[pi + 3] = 255;
            }
        }

        // Partial GPU upload for this chunk's 16×16 patch.
        // GL_UNPACK_ROW_LENGTH tells OpenGL the source row stride is MAP_SIZE pixels.
        let tx = (chunk_wx & (MAP_SIZE as i32 - 1)) as i32;
        let tz = (chunk_wz & (MAP_SIZE as i32 - 1)) as i32;
        unsafe {
            gl::BindTexture(gl::TEXTURE_2D, self.texture);
            gl::PixelStorei(gl::UNPACK_ROW_LENGTH, MAP_SIZE as i32);
            gl::TexSubImage2D(
                gl::TEXTURE_2D, 0,
                tx, tz, 16, 16,
                gl::RGBA, gl::UNSIGNED_BYTE,
                self.pixels[(tz as usize * MAP_SIZE + tx as usize) * 4..].as_ptr() as *const _,
            );
            gl::PixelStorei(gl::UNPACK_ROW_LENGTH, 0);
            gl::BindTexture(gl::TEXTURE_2D, 0);
        }

        true
    }

    pub fn update(&mut self, world: &World, player_x: f32, player_z: f32) {
        let pcx = (player_x as i32).div_euclid(16);
        let pcz = (player_z as i32).div_euclid(16);
        let player_chunk = (pcx, pcz);

        // Full re-queue on chunk boundary crossing
        if player_chunk != self.last_player_chunk {
            self.last_player_chunk = player_chunk;
            self.enqueue_around(pcx, pcz);
        }

        // Re-enqueue any not-yet-known chunks when the world loads new ones
        let current_chunk_count = world.chunk_count();
        if current_chunk_count != self.last_chunk_count {
            self.last_chunk_count = current_chunk_count;
            let r = SCAN_CHUNK_RADIUS;
            for dz in -r..=r {
                for dx in -r..=r {
                    let cx = pcx + dx;
                    let cz = pcz + dz;
                    if !self.known_chunks.contains(&(cx, cz)) && !self.in_queue.contains(&(cx, cz)) {
                        self.scan_queue.push_back((cx, cz));
                        self.in_queue.insert((cx, cz));
                    }
                }
            }
        }

        // Process up to CHUNKS_PER_FRAME from the queue
        let mut processed = 0;
        while processed < CHUNKS_PER_FRAME {
            let Some((cx, cz)) = self.scan_queue.pop_front() else { break };
            self.in_queue.remove(&(cx, cz));
            if self.known_chunks.contains(&(cx, cz)) { continue; }

            if self.scan_chunk(cx, cz, world) {
                self.known_chunks.insert((cx, cz));
            }
            processed += 1;
        }
    }

    pub fn draw(
        &self,
        player_x: f32,
        player_z: f32,
        player_front_x: f32,
        player_front_z: f32,
        entity_positions: &[(f32, f32)],
        entity_colors: &[(f32, f32, f32)],
        win_w: f32,
        win_h: f32,
    ) {
        const RIGHT_MARGIN_PX: f32 = 20.0;
        const RADIUS_PX: f32 = 80.0;
        // Sits below the clock rect (15 top + 40 height + 8 gap = 63 px from top).
        const MINIMAP_TOP_MARGIN_PX: f32 = 63.0;

        let half_w   = RADIUS_PX * 2.0 / win_w;
        let half_h   = RADIUS_PX * 2.0 / win_h;
        let cx_ndc   =  1.0 - (RIGHT_MARGIN_PX + RADIUS_PX) * 2.0 / win_w;
        let cy_ndc   =  1.0 - (MINIMAP_TOP_MARGIN_PX + RADIUS_PX) * 2.0 / win_h;

        // Player's toroidal UV: world position modulo MAP_SIZE, normalized to [0,1)
        let player_uv_x = player_x.rem_euclid(MAP_SIZE as f32) / MAP_SIZE as f32;
        let player_uv_z = player_z.rem_euclid(MAP_SIZE as f32) / MAP_SIZE as f32;

        // At disc edge (|d|=0.5), sample VIEW_RADIUS blocks away → VIEW_RADIUS/MAP_SIZE UV units
        // tex_uv += vec2(rd.x, -rd.y) * view_scale, and rd = rot*d with |d|=0.5 at edge
        let view_scale = 2.0 * VIEW_RADIUS / MAP_SIZE as f32;

        // Flatten entity data (capped at MAX_ENTITIES)
        let count = entity_positions.len().min(MAX_ENTITIES);
        let mut offsets_flat = vec![0.0f32; MAX_ENTITIES * 2];
        let mut colors_flat  = vec![0.0f32; MAX_ENTITIES * 3];
        for i in 0..count {
            offsets_flat[i * 2]     = entity_positions[i].0 - player_x;
            offsets_flat[i * 2 + 1] = entity_positions[i].1 - player_z;
            colors_flat[i * 3]     = entity_colors[i].0;
            colors_flat[i * 3 + 1] = entity_colors[i].1;
            colors_flat[i * 3 + 2] = entity_colors[i].2;
        }

        unsafe {
            gl::UseProgram(self.shader);
            gl::Uniform2f(self.u_center,     cx_ndc, cy_ndc);
            gl::Uniform2f(self.u_half_size,  half_w, half_h);
            gl::Uniform2f(self.u_player_uv,  player_uv_x, player_uv_z);
            gl::Uniform1f(self.u_view_scale, view_scale);
            gl::Uniform2f(self.u_player_dir, player_front_x, player_front_z);
            gl::Uniform1i(self.u_entity_count, count as i32);
            if count > 0 {
                gl::Uniform2fv(self.u_entity_offsets, count as i32, offsets_flat.as_ptr());
                gl::Uniform3fv(self.u_entity_colors,  count as i32, colors_flat.as_ptr());
            }

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
