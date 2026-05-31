/// Tile index of the purple/black missing-icon checkerboard inside the block atlas.
pub const MISSING_ICON_TILE: usize = 35;

#[allow(dead_code)]
/// Generates the raw RGBA pixels for the 256×256 block face atlas.
/// Layout: 16×16 tiles, each 16×16 px, row-major.
/// Call this to produce the initial `assets/textures/blocks_atlas.png`
/// via `cargo run --bin gen_icons`, then edit that PNG externally.
pub fn build_block_atlas_pixels() -> Vec<u8> {
    const ATLAS_SIZE: usize = 256;
    const TILE_SIZE: usize  = 16;
    const TILES_PER_ROW: usize = ATLAS_SIZE / TILE_SIZE;

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
        [200,  60,   0], // 21: Lava (Minecraft-style: red-orange base, yellow cores, dark rock)
        [112, 112, 112], // 22: Cobblestone (stone base with irregular dark cracks)
    ];

    let mut pixels = vec![0u8; ATLAS_SIZE * ATLAS_SIZE * 4];

    for tile_idx in 0..tile_colors.len() {
        if (9..=13).contains(&tile_idx) { continue; }

        let tile_col = tile_idx % TILES_PER_ROW;
        let tile_row = tile_idx / TILES_PER_ROW;
        let base_color = tile_colors[tile_idx];

        for py in 0..TILE_SIZE {
            for px in 0..TILE_SIZE {
                let atlas_x = tile_col * TILE_SIZE + px;
                let atlas_y = tile_row * TILE_SIZE + py;
                let idx = (atlas_y * ATLAS_SIZE + atlas_x) * 4;

                let t = py as f32 / (TILE_SIZE - 1) as f32;
                let blade_cx = [
                    3.5  - t * 2.5,
                    8.0  + t * 0.5,
                    12.5 + t * 2.0,
                ];
                let in_blade = blade_cx.iter().any(|&cx| (px as f32 - cx).abs() < 1.1);

                let alpha: u8 = if tile_idx == 8 {
                    if in_blade { 255 } else { 0 }
                } else if tile_idx == 3 {
                    160
                } else {
                    255
                };

                let color = match tile_idx {
                    16 => {
                        let h = (px.wrapping_mul(7) ^ py.wrapping_mul(13) ^ px.wrapping_mul(py).wrapping_mul(3)) % 16;
                        if h < 3 { [184u8, 115, 51] } else { base_color }
                    }
                    17 => {
                        let h = (px.wrapping_mul(11) ^ py.wrapping_mul(7) ^ px.wrapping_mul(py).wrapping_mul(5)) % 16;
                        if h < 4 { [30u8, 30, 30] } else { base_color }
                    }
                    18 => {
                        let h = (px.wrapping_mul(9) ^ py.wrapping_mul(17) ^ px.wrapping_mul(py).wrapping_mul(7)) % 16;
                        if h < 4 { [160u8, 107, 75] } else { base_color }
                    }
                    19 => {
                        let row       = py / 5;
                        let col_off   = if row % 2 == 0 { 0usize } else { 4 };
                        let local_x   = (px + col_off) % 8;
                        let local_y   = py % 5;
                        let is_mortar = local_x == 0 || local_y == 0;
                        if is_mortar { [55u8, 55, 55] } else { base_color }
                    }
                    20 => {
                        let (cx, cy) = (px as i32 - 8, py as i32 - 10);
                        let in_chamber = cx.abs() <= 4 && cy >= -2 && cy <= 4;
                        let row     = py / 5;
                        let col_off = if row % 2 == 0 { 0usize } else { 4 };
                        let is_mortar = (px + col_off) % 8 == 0 || py % 5 == 0;
                        if in_chamber {
                            let dist = (cx.abs() + cy.abs()) as u8;
                            if dist <= 2 { [230u8, 140, 30] } else { [140, 70, 10] }
                        } else if is_mortar {
                            [55u8, 55, 55]
                        } else {
                            base_color
                        }
                    }
                    22 => {
                        let cx = (px as i32 % 8 - 4).abs();
                        let cy = (py as i32 % 6 - 3).abs();
                        let crack = (px.wrapping_mul(5) ^ py.wrapping_mul(3)
                            ^ (px / 8).wrapping_mul(17) ^ (py / 6).wrapping_mul(13)) % 6;
                        if cx >= 3 || cy >= 2 || crack == 0 { [70u8, 70, 70] } else { base_color }
                    }
                    21 => {
                        let blob = (px.wrapping_mul(3).wrapping_add(py.wrapping_mul(5))
                            ^ px.wrapping_mul(py).wrapping_mul(7)
                            ^ (px / 3).wrapping_mul(11) ^ (py / 3).wrapping_mul(13)) % 24;
                        let hot = (px.wrapping_mul(17) ^ py.wrapping_mul(23)
                            ^ px.wrapping_add(py).wrapping_mul(31)) % 8;
                        let rock = (px.wrapping_mul(5) ^ py.wrapping_mul(7)
                            ^ (px / 4).wrapping_mul(3) ^ (py / 4).wrapping_mul(9)) % 10;

                        if blob < 6 {
                            if rock == 0 { [140u8, 20, 0] } else { [180u8, 40, 0] }
                        } else if blob < 10 {
                            [220u8, 80, 0]
                        } else if hot == 0 {
                            [255u8, 248, 120]
                        } else if hot < 3 {
                            [255u8, 210, 30]
                        } else {
                            base_color
                        }
                    }
                    4 if py >= TILE_SIZE - 4 => [120u8, 172, 48],
                    5 if px == 0 || px == TILE_SIZE - 1 => [100u8, 60, 25],
                    7 => {
                        let cx = px as i32 - TILE_SIZE as i32 / 2;
                        let cy = py as i32 - TILE_SIZE as i32 / 2;
                        let ring = ((cx * cx + cy * cy) as f32).sqrt() as usize;
                        if ring % 3 == 0 { [140u8, 100, 55] } else { base_color }
                    }
                    8 if py > TILE_SIZE * 2 / 3 => {
                        [(base_color[0] as u16 * 13 / 10).min(255) as u8,
                         base_color[1],
                         (base_color[2] as u16 * 6 / 10) as u8]
                    }
                    _ => base_color,
                };

                let variation = ((px ^ py) % 3) as i16 * 4 - 4;
                pixels[idx]     = (color[0] as i16 + variation).clamp(0, 255) as u8;
                pixels[idx + 1] = (color[1] as i16 + variation).clamp(0, 255) as u8;
                pixels[idx + 2] = (color[2] as i16 + variation).clamp(0, 255) as u8;
                pixels[idx + 3] = alpha;
            }
        }
    }

    // Crack overlay tiles 9–13
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
            }
        }
    }

    // Wheat tiles 23–27
    {
        let stage_heights = [0.15f32, 0.25, 0.40, 0.55, 0.75];
        let stalk_cols = [4usize, 8, 12];
        for stage in 0..5usize {
            let tile_idx = 23 + stage;
            let tile_col = tile_idx % TILES_PER_ROW;
            let tile_row = tile_idx / TILES_PER_ROW;
            let h = stage_heights[stage];
            let vis_top = (TILE_SIZE as f32 * (1.0 - h)) as usize;
            let plant_h = TILE_SIZE - vis_top;

            for py in vis_top..TILE_SIZE {
                for px in 0..TILE_SIZE {
                    let ax = tile_col * TILE_SIZE + px;
                    let ay = tile_row * TILE_SIZE + py;
                    let idx = (ay * ATLAS_SIZE + ax) * 4;

                    let in_stalk = stalk_cols.iter().any(|&sx| px == sx);
                    let in_leaf  = stalk_cols.iter().any(|&sx| px.abs_diff(sx) == 1);
                    let local_y  = TILE_SIZE - 1 - py;

                    let (r, g, b): (i16, i16, i16) = if in_stalk {
                        if stage == 4 && local_y >= plant_h.saturating_sub(3) {
                            (205, 175, 40)
                        } else {
                            (100, 155, 40)
                        }
                    } else if in_leaf {
                        let mid = vis_top + plant_h / 2;
                        if py >= mid.saturating_sub(1) && py <= mid + 1 {
                            (70, 130, 30)
                        } else {
                            continue;
                        }
                    } else {
                        continue;
                    };

                    let vari = ((px ^ py) % 3) as i16 * 4 - 4;
                    pixels[idx]     = (r + vari).clamp(0, 255) as u8;
                    pixels[idx + 1] = (g + vari).clamp(0, 255) as u8;
                    pixels[idx + 2] = (b + vari).clamp(0, 255) as u8;
                    pixels[idx + 3] = 255;
                }
            }
        }
    }

    // Pumpkin stem tiles 28–31
    {
        let stage_heights = [0.15f32, 0.30, 0.50, 0.70];
        let stalk_cols = [5usize, 8, 11];
        for stage in 0..4usize {
            let tile_idx = 28 + stage;
            let tile_col = tile_idx % TILES_PER_ROW;
            let tile_row = tile_idx / TILES_PER_ROW;
            let h = stage_heights[stage];
            let vis_top = (TILE_SIZE as f32 * (1.0 - h)) as usize;
            let plant_h = TILE_SIZE - vis_top;

            for py in vis_top..TILE_SIZE {
                for px in 0..TILE_SIZE {
                    let ax = tile_col * TILE_SIZE + px;
                    let ay = tile_row * TILE_SIZE + py;
                    let idx = (ay * ATLAS_SIZE + ax) * 4;

                    let in_stalk = stalk_cols.iter().any(|&sx| px == sx);
                    let in_tendril = stalk_cols.iter().any(|&sx| px.abs_diff(sx) == 1);
                    let local_y = TILE_SIZE - 1 - py;

                    let (r, g, b): (i16, i16, i16) = if in_stalk {
                        if stage >= 2 && local_y >= plant_h.saturating_sub(2) {
                            (80, 160, 40)
                        } else {
                            (60, 130, 30)
                        }
                    } else if in_tendril {
                        let mid = vis_top + plant_h / 2;
                        if py >= mid.saturating_sub(1) && py <= mid + 1 {
                            (50, 110, 25)
                        } else {
                            continue;
                        }
                    } else {
                        continue;
                    };

                    let vari = ((px ^ py) % 3) as i16 * 4 - 4;
                    pixels[idx]     = (r + vari).clamp(0, 255) as u8;
                    pixels[idx + 1] = (g + vari).clamp(0, 255) as u8;
                    pixels[idx + 2] = (b + vari).clamp(0, 255) as u8;
                    pixels[idx + 3] = 255;
                }
            }
        }
    }

    // Pumpkin block tiles: 32 = side face, 33 = top face
    {
        for tile_idx in [32usize, 33usize] {
            let is_top = tile_idx == 33;
            let tile_col = tile_idx % TILES_PER_ROW;
            let tile_row = tile_idx / TILES_PER_ROW;
            for py in 0..TILE_SIZE {
                for px in 0..TILE_SIZE {
                    let ax = tile_col * TILE_SIZE + px;
                    let ay = tile_row * TILE_SIZE + py;
                    let idx = (ay * ATLAS_SIZE + ax) * 4;

                    let (r, g, b): (i16, i16, i16) = if is_top {
                        let rib = (px as i16 % 4 == 0) || (py as i16 % 4 == 0);
                        if rib { (170, 95, 10) } else { (220, 130, 20) }
                    } else {
                        let rib = px % 4 == 0;
                        let eye = (py >= 7 && py <= 10)
                            && ((px >= 3 && px <= 4) || (px >= 11 && px <= 12));
                        let mouth = (py == 3 || py == 4) && (px >= 4 && px <= 11);
                        if eye || mouth {
                            (30, 20, 5)
                        } else if rib {
                            (170, 95, 10)
                        } else {
                            (220, 130, 20)
                        }
                    };

                    let vari = ((px ^ py) % 3) as i16 * 4 - 4;
                    pixels[idx]     = (r + vari).clamp(0, 255) as u8;
                    pixels[idx + 1] = (g + vari).clamp(0, 255) as u8;
                    pixels[idx + 2] = (b + vari).clamp(0, 255) as u8;
                    pixels[idx + 3] = 255;
                }
            }
        }
    }

    // Tile 35: Missing-icon fallback — purple/black 2×2 checkerboard.
    // Read by geo_atlas::load_broken_icon() when a model has no icon.png.
    // Index is exported as MISSING_ICON_TILE so geo_atlas can find it.
    {
        const TILE_IDX: usize = MISSING_ICON_TILE;
        let tile_col = TILE_IDX % TILES_PER_ROW;
        let tile_row = TILE_IDX / TILES_PER_ROW;
        for py in 0..TILE_SIZE {
            for px in 0..TILE_SIZE {
                let ax = tile_col * TILE_SIZE + px;
                let ay = tile_row * TILE_SIZE + py;
                let idx = (ay * ATLAS_SIZE + ax) * 4;
                pixels[idx + 3] = 255;
                if (px / 2 + py / 2) % 2 == 0 {
                    pixels[idx]     = 148; // purple
                    pixels[idx + 2] = 211;
                }
            }
        }
    }

    // Tile 34: WoodBlock — horizontal planks with staggered vertical seams
    {
        const TILE_IDX: usize = 34;
        let tile_col = TILE_IDX % TILES_PER_ROW;
        let tile_row = TILE_IDX / TILES_PER_ROW;
        for py in 0..TILE_SIZE {
            let plank_row = py / 4;
            let (base_r, base_g, base_b): (i16, i16, i16) = if plank_row % 2 == 0 {
                (194, 153, 89)
            } else {
                (174, 133, 73)
            };
            let is_seam_h = py % 4 == 0;
            for px in 0..TILE_SIZE {
                let ax = tile_col * TILE_SIZE + px;
                let ay = tile_row * TILE_SIZE + py;
                let idx = (ay * ATLAS_SIZE + ax) * 4;

                let seam_x = if plank_row % 2 == 0 { 8usize } else { 0usize };
                let is_seam_v = px == seam_x;
                let is_seam = is_seam_h || is_seam_v;

                let (r, g, b) = if is_seam {
                    ((base_r - 30).max(0), (base_g - 25).max(0), (base_b - 18).max(0))
                } else {
                    let grain = if px % 3 == 0 { 6i16 } else if px % 3 == 2 { -4 } else { 0 };
                    (base_r + grain, base_g + grain, base_b + grain)
                };

                pixels[idx]     = r.clamp(0, 255) as u8;
                pixels[idx + 1] = g.clamp(0, 255) as u8;
                pixels[idx + 2] = b.clamp(0, 255) as u8;
                pixels[idx + 3] = 255;
            }
        }
    }

    pixels
}
