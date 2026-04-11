#[derive(Copy, Clone, Debug)]
pub enum Face {
    Right,   // +X
    Left,    // -X  
    Up,      // +Y (Top)
    Down,    // -Y (Bottom)
    Front,   // +Z
    Back,    // -Z
}

// Usage:
impl Face {
    pub fn texture_coords(&self, texture_index: u32, atlas_size: u32) -> [[f32; 2]; 4] {
        // Calculate texture coordinates in atlas
        // atlas_size = how many textures per row/column (e.g., 16 for 16x16 atlas)
        
        let tex_size = 1.0 / atlas_size as f32;
        let col = (texture_index % atlas_size) as f32;
        let row = (texture_index / atlas_size) as f32;
        
        // Base coordinates for this texture in atlas
        let left = col * tex_size;
        let right = (col + 1.0) * tex_size;
        let top = row * tex_size;
        let bottom = (row + 1.0) * tex_size;
        
        // Return 4 vertices in correct order for this face
        match self {
            // Standard orientation (adjust if your textures are rotated)
            Face::Right => [[right, top], [right, bottom], [left, bottom], [left, top]],
            Face::Left => [[left, top], [left, bottom], [right, bottom], [right, top]],
            Face::Up => [[left, bottom], [right, bottom], [right, top], [left, top]],
            Face::Down => [[left, top], [right, top], [right, bottom], [left, bottom]],
            Face::Front => [[right, top], [right, bottom], [left, bottom], [left, top]],
            Face::Back => [[left, top], [left, bottom], [right, bottom], [right, top]],
        }
    }
    
    // For each vertex [0..3], returns [side1, side2, corner] block offsets for AO sampling.
    // Offsets are relative to the block being rendered.
    pub fn ao_neighbors(&self) -> [[(i32, i32, i32); 3]; 4] {
        match self {
            Face::Up => [
                [(-1,1,0), (0,1,1),  (-1,1,1)],  // v0
                [(1,1,0),  (0,1,1),  (1,1,1)],   // v1
                [(1,1,0),  (0,1,-1), (1,1,-1)],  // v2
                [(-1,1,0), (0,1,-1), (-1,1,-1)], // v3
            ],
            Face::Down => [
                [(-1,-1,0), (0,-1,-1), (-1,-1,-1)],
                [(1,-1,0),  (0,-1,-1), (1,-1,-1)],
                [(1,-1,0),  (0,-1,1),  (1,-1,1)],
                [(-1,-1,0), (0,-1,1),  (-1,-1,1)],
            ],
            Face::Right => [
                [(1,-1,0), (1,0,-1), (1,-1,-1)],
                [(1,1,0),  (1,0,-1), (1,1,-1)],
                [(1,1,0),  (1,0,1),  (1,1,1)],
                [(1,-1,0), (1,0,1),  (1,-1,1)],
            ],
            Face::Left => [
                [(-1,-1,0), (-1,0,1),  (-1,-1,1)],
                [(-1,1,0),  (-1,0,1),  (-1,1,1)],
                [(-1,1,0),  (-1,0,-1), (-1,1,-1)],
                [(-1,-1,0), (-1,0,-1), (-1,-1,-1)],
            ],
            Face::Front => [
                [(1,0,1),  (0,-1,1), (1,-1,1)],
                [(1,0,1),  (0,1,1),  (1,1,1)],
                [(-1,0,1), (0,1,1),  (-1,1,1)],
                [(-1,0,1), (0,-1,1), (-1,-1,1)],
            ],
            Face::Back => [
                [(-1,0,-1), (0,-1,-1), (-1,-1,-1)],
                [(-1,0,-1), (0,1,-1),  (-1,1,-1)],
                [(1,0,-1),  (0,1,-1),  (1,1,-1)],
                [(1,0,-1),  (0,-1,-1), (1,-1,-1)],
            ],
        }
    }

    pub fn positions(&self, x: f32, y: f32, z: f32) -> [[f32; 3]; 4] {
        match self {
            Face::Right => [
                [x + 1.0, y,       z],      // Bottom-left
                [x + 1.0, y + 1.0, z],      // Top-left
                [x + 1.0, y + 1.0, z + 1.0], // Top-right
                [x + 1.0, y,       z + 1.0], // Bottom-right
            ],
            Face::Left => [
                [x, y,       z + 1.0], // Bottom-right
                [x, y + 1.0, z + 1.0], // Top-right
                [x, y + 1.0, z],       // Top-left
                [x, y,       z],       // Bottom-left
            ],
            Face::Up => [
                [x,      y + 1.0, z + 1.0], // Front-left
                [x + 1.0, y + 1.0, z + 1.0], // Front-right
                [x + 1.0, y + 1.0, z],      // Back-right
                [x,      y + 1.0, z],      // Back-left
            ],
            Face::Down => [
                [x,      y, z],      // Back-left
                [x + 1.0, y, z],      // Back-right
                [x + 1.0, y, z + 1.0], // Front-right
                [x,      y, z + 1.0], // Front-left
            ],
            Face::Front => [
                [x + 1.0, y,      z + 1.0], // Right-top
                [x + 1.0, y + 1.0, z + 1.0], // Right-bottom
                [x,      y + 1.0, z + 1.0], // Left-bottom
                [x,      y,      z + 1.0], // Left-top
            ],
            Face::Back => [
                [x,      y,      z], // Left-top
                [x,      y + 1.0, z], // Left-bottom
                [x + 1.0, y + 1.0, z], // Right-bottom
                [x + 1.0, y,      z], // Right-top
            ],
        }
    }
}