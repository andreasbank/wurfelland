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

    // === TILE 20: Beef ===
    fill_tile_placeholder(&mut pixels, 20, 191, 71, 46);

    // === TILE 21: Leather ===
    fill_tile_placeholder(&mut pixels, 21, 158, 107, 56);

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

/// Loads the block face atlas from `assets/textures/blocks_atlas.png`.
/// Run `cargo run --bin gen_icons` once to generate that file, then edit it freely.
pub fn create_block_atlas() -> u32 {
    const PATH: &str = "assets/textures/blocks_atlas.png";
    let img = image::open(PATH)
        .unwrap_or_else(|e| panic!(
            "Cannot load block atlas '{}': {}\nRun `cargo run --bin gen_icons` to generate it.",
            PATH, e
        ))
        .into_rgba8();
    let (w, h) = img.dimensions();
    let pixels = img.into_raw();
    unsafe {
        let mut texture_id = 0u32;
        gl::GenTextures(1, &mut texture_id);
        gl::BindTexture(gl::TEXTURE_2D, texture_id);
        gl::TexImage2D(
            gl::TEXTURE_2D, 0, gl::RGBA as i32,
            w as i32, h as i32, 0,
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