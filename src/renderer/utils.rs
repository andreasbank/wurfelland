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
        [ 64, 105, 225], // 3: Water
        [156, 112,  57], // 4: Grass side (base dirt, stripe added below)
        [139,  90,  43], // 5: Log side (brown bark)
        [ 58, 120,  42], // 6: Leaves (dark green)
        [191, 152,  96], // 7: Log top (lighter tan rings)
        [102, 179,  51], // 8: Tall grass (bright green stems)
    ];

    let mut pixels = vec![0u8; ATLAS_SIZE * ATLAS_SIZE * 4];

    for tile_idx in 0..tile_colors.len() {
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
                    170  // water: ~67% opaque
                } else {
                    255
                };

                let color = match tile_idx {
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