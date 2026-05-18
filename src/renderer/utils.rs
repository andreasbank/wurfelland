use std::ffi::CString;
use gl::types::*;
use std::ptr;

pub fn compile_shader(shader_type: GLenum, source: &str) -> Result<GLuint, String> {
    unsafe {
        let shader = gl::CreateShader(shader_type);
        let c_str = CString::new(source).unwrap();
        gl::ShaderSource(shader, 1, &c_str.as_ptr(), ptr::null());
        gl::CompileShader(shader);
        
        let mut success = 0;
        gl::GetShaderiv(shader, gl::COMPILE_STATUS, &mut success);
        if success == 0 {
            let mut len = 0;
            gl::GetShaderiv(shader, gl::INFO_LOG_LENGTH, &mut len);
            let mut buffer = vec![0u8; len as usize];
            gl::GetShaderInfoLog(shader, len, ptr::null_mut(), buffer.as_mut_ptr() as *mut GLchar);
            return Err(format!("Shader compilation failed: {}", String::from_utf8_lossy(&buffer)));
        }
        
        Ok(shader)
    }
}

pub fn link_program(vertex_shader: GLuint, fragment_shader: GLuint) -> Result<GLuint, String> {
    unsafe {
        let program = gl::CreateProgram();
        gl::AttachShader(program, vertex_shader);
        gl::AttachShader(program, fragment_shader);
        gl::LinkProgram(program);
        
        let mut success = 0;
        gl::GetProgramiv(program, gl::LINK_STATUS, &mut success);
        if success == 0 {
            let mut len = 0;
            gl::GetProgramiv(program, gl::INFO_LOG_LENGTH, &mut len);
            let mut buffer = vec![0u8; len as usize];
            gl::GetProgramInfoLog(program, len, ptr::null_mut(), buffer.as_mut_ptr() as *mut GLchar);
            return Err(format!("Program linking failed: {}", String::from_utf8_lossy(&buffer)));
        }
        
        Ok(program)
    }
}

// ── Item atlas ────────────────────────────────────────────────────────────────
// 256×256 RGBA texture, 16×16 tiles, 16 tiles per row.
// Each tile is 16×16 pixels = 256 [R,G,B,A] values, row-major, top-to-bottom.
//
// HOW TO ADD YOUR OWN PIXEL ART FOR A TILE
// ─────────────────────────────────────────
// 1. Find the tile's section below (e.g. "=== TILE 0: Stick ===").
// 2. Replace the `fill_tile_placeholder(...)` call with `write_tile(&mut pixels, ..., &YOUR_DATA)`.
// 3. Define YOUR_DATA as a `[[u8;4]; 256]` — 256 pixels in reading order (left→right, top→bottom).
//    Each pixel is [R, G, B, A]. Use A=0 for transparent background.
//
// PIXEL FORMAT REMINDER
//   [255, 255, 255, 255] = solid white
//   [140,  89,  43, 255] = brown (stick color)
//   [  0,   0,   0,   0] = fully transparent
//
// TILE MAP (col, row inside the 256×256 atlas)
//   Tile 0 = Stick       (col 0, row 0) — pixels 0..256
//   Tile 1 = LogBlock    (col 1, row 0)
//   Tile 2 = DirtClump   (col 2, row 0)
//   Tile 3 = StoneChunk  (col 3, row 0)
//   Tile 4 = Seeds       (col 4, row 0)

#[allow(dead_code)]
pub fn write_tile(pixels: &mut [u8], tile_idx: usize, data: &[[u8; 4]; 256]) {
    const ATLAS: usize = 256;
    const TILE:  usize = 16;
    const TPR:   usize = ATLAS / TILE; // tiles per row = 16
    let tc = tile_idx % TPR;
    let tr = tile_idx / TPR;
    for py in 0..TILE {
        for px in 0..TILE {
            let ax = tc * TILE + px;
            let ay = tr * TILE + py;
            let dst = (ay * ATLAS + ax) * 4;
            let src = &data[py * TILE + px];
            pixels[dst..dst + 4].copy_from_slice(src);
        }
    }
}

/// Fills a tile with a solid opaque color and a 1px black border.
/// Used as a placeholder until real pixel art is provided.
fn fill_tile_placeholder(pixels: &mut [u8], tile_idx: usize, r: u8, g: u8, b: u8) {
    const ATLAS: usize = 256;
    const TILE:  usize = 16;
    const TPR:   usize = ATLAS / TILE;
    let tc = tile_idx % TPR;
    let tr = tile_idx / TPR;
    for py in 0..TILE {
        for px in 0..TILE {
            let ax = tc * TILE + px;
            let ay = tr * TILE + py;
            let dst = (ay * ATLAS + ax) * 4;
            let border = px == 0 || py == 0 || px == TILE - 1 || py == TILE - 1;
            pixels[dst]     = if border { 40 } else { r };
            pixels[dst + 1] = if border { 20 } else { g };
            pixels[dst + 2] = if border { 10 } else { b };
            pixels[dst + 3] = 255;
        }
    }
}

pub fn load_png_texture(path: &str) -> u32 {
    let img = image::open(path)
        .unwrap_or_else(|e| panic!("Failed to load '{}': {}", path, e))
        .into_rgba8();
    let (w, h) = img.dimensions();
    let pixels = img.into_raw();
    unsafe {
        let mut id = 0u32;
        gl::GenTextures(1, &mut id);
        gl::BindTexture(gl::TEXTURE_2D, id);
        gl::TexImage2D(gl::TEXTURE_2D, 0, gl::RGBA as i32,
            w as i32, h as i32, 0,
            gl::RGBA, gl::UNSIGNED_BYTE, pixels.as_ptr() as *const _);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as i32);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as i32);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as i32);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as i32);
        gl::BindTexture(gl::TEXTURE_2D, 0);
        id
    }
}

pub fn create_item_atlas() -> u32 {
    const ATLAS_SIZE: usize = 256;
    let mut pixels = vec![0u8; ATLAS_SIZE * ATLAS_SIZE * 4];

    // === TILE 0: Stick ===
    // Replace fill_tile_placeholder with write_tile(&mut pixels, 0, &YOUR_DATA)
    // when you have your pixel art ready.
    fill_tile_placeholder(&mut pixels, 0, 140, 89, 43);

    // === TILE 1: LogBlock ===
    fill_tile_placeholder(&mut pixels, 1, 139, 90, 43);

    // === TILE 2: DirtClump ===
    fill_tile_placeholder(&mut pixels, 2, 156, 112, 57);

    // === TILE 3: StoneChunk ===
    fill_tile_placeholder(&mut pixels, 3, 128, 128, 128);

    // === TILE 4: Seeds ===
    fill_tile_placeholder(&mut pixels, 4, 204, 192, 51);

    // === TILE 11: Bed ===
    fill_tile_placeholder(&mut pixels, 11, 204, 90, 64);

    // === TILE 15: Furnace ===
    fill_tile_placeholder(&mut pixels, 15, 100, 100, 100);

    unsafe {
        let mut id = 0u32;
        gl::GenTextures(1, &mut id);
        gl::BindTexture(gl::TEXTURE_2D, id);
        gl::TexImage2D(gl::TEXTURE_2D, 0, gl::RGBA as i32,
            ATLAS_SIZE as i32, ATLAS_SIZE as i32, 0,
            gl::RGBA, gl::UNSIGNED_BYTE, pixels.as_ptr() as *const _);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as i32);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as i32);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as i32);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as i32);
        gl::BindTexture(gl::TEXTURE_2D, 0);
        id
    }
}

// Generates a 256x256 atlas (16x16 tiles, each tile 16x16 px).
// Tile layout matches BlockType::texture_id():
//   0 = Grass top (green)
//   1 = Dirt (brown)
//   2 = Stone (gray)
//   3 = Water (blue)
//   4 = Grass side (green+brown stripe)
pub fn create_block_atlas() -> u32 {
    const ATLAS_SIZE: usize = 256;
    const TILE_SIZE: usize  = 16;
    const TILES_PER_ROW: usize = ATLAS_SIZE / TILE_SIZE;

    // [r, g, b] base colors per tile index
    let tile_colors: &[[u8; 3]] = &[
        [120, 172,  48], // 0: Grass top
        [156, 112,  57], // 1: Dirt
        [128, 128, 128], // 2: Stone
        [255, 255, 255], // 3: Water (neutral — vertex color provides the tint)
        [156, 112,  57], // 4: Grass side (base dirt, stripe added below)
        [139,  90,  43], // 5: Log side (brown bark)
        [ 58, 120,  42], // 6: Leaves (dark green)
        [191, 152,  96], // 7: Log top (lighter tan rings)
        [102, 179,  51], // 8: Tall grass (bright green stems)
        [  0,   0,   0], // 9–13: crack overlays — overwritten below; placeholders only
        [  0,   0,   0],
        [  0,   0,   0],
        [  0,   0,   0],
        [  0,   0,   0],
        [245, 222, 153], // 14: Sand
        [230, 240, 255], // 15: Snow
        [128, 128, 128], // 16: Copper ore (stone base, copper specks added below)
        [128, 128, 128], // 17: Coal ore   (stone base, dark specks added below)
        [128, 128, 128], // 18: Iron ore   (stone base, rust-tan specks added below)
        [100, 100, 100], // 19: Furnace sides/top (dark cobblestone bricks)
        [ 60,  60,  60], // 20: Furnace front (dark chamber + glow, generated below)
        [200,  70,  10], // 21: Lava (bright orange base, bright spots + dark cracks)
        [112, 112, 112], // 22: Cobblestone (stone base with irregular dark cracks)
    ];

    let mut pixels = vec![0u8; ATLAS_SIZE * ATLAS_SIZE * 4];

    for tile_idx in 0..tile_colors.len() {
        // Crack overlay tiles (9–13) are fully generated in the second pass below;
        // skipping here keeps their background transparent (pixels stay zero).
        if (9..=13).contains(&tile_idx) { continue; }

        let tile_col = tile_idx % TILES_PER_ROW;
        let tile_row = tile_idx / TILES_PER_ROW;
        let base_color = tile_colors[tile_idx];

        for py in 0..TILE_SIZE {
            for px in 0..TILE_SIZE {
                let atlas_x = tile_col * TILE_SIZE + px;
                let atlas_y = tile_row * TILE_SIZE + py;
                let idx = (atlas_y * ATLAS_SIZE + atlas_x) * 4;

                // Tall grass: three leaning blades with transparent background.
                // Blades lean in slightly different directions and are 2 px wide.
                // t=0 at top of tile (py=0), t=1 at bottom (py=TILE_SIZE-1).
                let t = py as f32 / (TILE_SIZE - 1) as f32;
                let blade_cx = [
                    3.5  - t * 2.5,  // left blade leans left as it grows up
                    8.0  + t * 0.5,  // middle blade almost straight
                    12.5 + t * 2.0,  // right blade leans right
                ];
                let in_blade = blade_cx.iter().any(|&cx| (px as f32 - cx).abs() < 1.1);

                // Alpha per tile: grass blades have cutout gaps, water is semi-transparent.
                let alpha: u8 = if tile_idx == 8 {
                    if in_blade { 255 } else { 0 }  // tall grass: cutout
                } else if tile_idx == 3 {
                    160  // water: ~63% opaque
                } else {
                    255
                };

                let color = match tile_idx {
                    // Copper ore: stone base with scattered orange-copper blobs
                    16 => {
                        let h = (px.wrapping_mul(7) ^ py.wrapping_mul(13) ^ px.wrapping_mul(py).wrapping_mul(3)) % 16;
                        if h < 3 { [184u8, 115, 51] } else { base_color }
                    }
                    // Coal ore: stone base with dark coal blobs
                    17 => {
                        let h = (px.wrapping_mul(11) ^ py.wrapping_mul(7) ^ px.wrapping_mul(py).wrapping_mul(5)) % 16;
                        if h < 4 { [30u8, 30, 30] } else { base_color }
                    }
                    // Iron ore: stone base with rust-tan blobs
                    18 => {
                        let h = (px.wrapping_mul(9) ^ py.wrapping_mul(17) ^ px.wrapping_mul(py).wrapping_mul(7)) % 16;
                        if h < 4 { [160u8, 107, 75] } else { base_color }
                    }
                    // Furnace sides/top: cobblestone brick pattern — dark mortar lines on a grid
                    19 => {
                        // Mortar grid: 1-px lines every 5 px (horizontal) and every 8 px offset
                        // by 4 px alternating rows (running-bond brick layout).
                        let row       = py / 5;
                        let col_off   = if row % 2 == 0 { 0usize } else { 4 };
                        let local_x   = (px + col_off) % 8;
                        let local_y   = py % 5;
                        let is_mortar = local_x == 0 || local_y == 0;
                        if is_mortar { [55u8, 55, 55] } else { base_color }
                    }
                    // Furnace front: dark chamber with orange glow around the mouth opening
                    20 => {
                        // Chamber opening: a rounded rectangle centred in the tile
                        let (cx, cy) = (px as i32 - 8, py as i32 - 10);
                        let in_chamber = cx.abs() <= 4 && cy >= -2 && cy <= 4;
                        // Thin mortar border (same as tile 19 for consistency)
                        let row     = py / 5;
                        let col_off = if row % 2 == 0 { 0usize } else { 4 };
                        let is_mortar = (px + col_off) % 8 == 0 || py % 5 == 0;
                        if in_chamber {
                            // Glow: bright orange-yellow at centre, fades to dark
                            let dist = (cx.abs() + cy.abs()) as u8;
                            if dist <= 2 { [230u8, 140, 30] } else { [140, 70, 10] }
                        } else if is_mortar {
                            [55u8, 55, 55]
                        } else {
                            base_color
                        }
                    }
                    // Cobblestone: stone base with irregular dark cracks at rounded-rect boundaries
                    22 => {
                        let cx = (px as i32 % 8 - 4).abs();
                        let cy = (py as i32 % 6 - 3).abs();
                        let crack = (px.wrapping_mul(5) ^ py.wrapping_mul(3)
                            ^ (px / 8).wrapping_mul(17) ^ (py / 6).wrapping_mul(13)) % 6;
                        if cx >= 3 || cy >= 2 || crack == 0 { [70u8, 70, 70] } else { base_color }
                    }
                    // Lava: orange-red base with bright glowing blobs and dark cracks
                    21 => {
                        let h = (px.wrapping_mul(7) ^ py.wrapping_mul(13)
                            ^ px.wrapping_mul(py).wrapping_mul(11)) % 32;
                        let crack = (px.wrapping_mul(3) ^ py.wrapping_mul(5)
                            ^ px.wrapping_add(py).wrapping_mul(7)) % 12;
                        if crack == 0       { [60u8, 20, 5] }      // dark crack
                        else if h < 4      { [255u8, 200, 60] }    // bright hot spot
                        else if h < 9      { [240u8, 120, 20] }    // warm orange
                        else               { base_color }           // base red-orange
                    }
                    // Grass side: green stripe at top
                    4 if py >= TILE_SIZE - 4 => [120u8, 172, 48],
                    // Log side: darker vertical streaks on edges
                    5 if px == 0 || px == TILE_SIZE - 1 => [100u8, 60, 25],
                    // Log top: concentric ring pattern
                    7 => {
                        let cx = px as i32 - TILE_SIZE as i32 / 2;
                        let cy = py as i32 - TILE_SIZE as i32 / 2;
                        let ring = ((cx * cx + cy * cy) as f32).sqrt() as usize;
                        if ring % 3 == 0 { [140u8, 100, 55] } else { base_color }
                    }
                    // Tall grass: yellow-green stem at base, bright green toward tips
                    8 if py > TILE_SIZE * 2 / 3 => {
                        [(base_color[0] as u16 * 13 / 10).min(255) as u8,
                         base_color[1],
                         (base_color[2] as u16 * 6 / 10) as u8]
                    }
                    _ => base_color,
                };

                // Slight per-pixel variation so tiles don't look flat
                let variation = ((px ^ py) % 3) as i16 * 4 - 4;
                pixels[idx]     = (color[0] as i16 + variation).clamp(0, 255) as u8;
                pixels[idx + 1] = (color[1] as i16 + variation).clamp(0, 255) as u8;
                pixels[idx + 2] = (color[2] as i16 + variation).clamp(0, 255) as u8;
                pixels[idx + 3] = alpha;
            }
        }
    }

    // Crack overlay tiles 9–13 (5 stages, transparent background + dark crack lines).
    // Uses three families of crossing sine waves; higher stages have a lower threshold
    // so more pixels become visible cracks.
    let crack_thresholds = [0.10f32, 0.18, 0.28, 0.40, 0.55];
    for stage in 0..5usize {
        let tile_idx  = 9 + stage;
        let tile_col  = tile_idx % TILES_PER_ROW;
        let tile_row  = tile_idx / TILES_PER_ROW;
        let threshold = crack_thresholds[stage];
        for py in 0..TILE_SIZE {
            for px in 0..TILE_SIZE {
                let atlas_x = tile_col * TILE_SIZE + px;
                let atlas_y = tile_row * TILE_SIZE + py;
                let idx = (atlas_y * ATLAS_SIZE + atlas_x) * 4;
                let x = px as f32 / TILE_SIZE as f32 * std::f32::consts::TAU;
                let y = py as f32 / TILE_SIZE as f32 * std::f32::consts::TAU;
                let v1 = (x * 2.3 + y * 1.7).sin().abs();
                let v2 = (x * 0.9 - y * 2.1).sin().abs();
                let v3 = (x * 3.1 + y * 0.5).sin().abs();
                if v1.min(v2).min(v3) < threshold {
                    pixels[idx]     = 20;
                    pixels[idx + 1] = 20;
                    pixels[idx + 2] = 20;
                    pixels[idx + 3] = 200;
                }
                // else: already zeroed (transparent)
            }
        }
    }

    unsafe {
        let mut texture_id = 0;
        gl::GenTextures(1, &mut texture_id);
        gl::BindTexture(gl::TEXTURE_2D, texture_id);
        gl::TexImage2D(
            gl::TEXTURE_2D, 0, gl::RGBA as i32,
            ATLAS_SIZE as i32, ATLAS_SIZE as i32, 0,
            gl::RGBA, gl::UNSIGNED_BYTE,
            pixels.as_ptr() as *const _,
        );
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::REPEAT as i32);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::REPEAT as i32);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as i32);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as i32);
        gl::BindTexture(gl::TEXTURE_2D, 0);
        texture_id
    }
}