#[derive(Copy, Clone, Debug, PartialEq)]
pub enum BlockType {
    Air,
    Grass,
    Dirt,
    Stone,
    Water,
    Log,
    Leaves,
    TallGrass,
}

impl BlockType {
    pub fn color(&self) -> [f32; 3] {
        match self {
            BlockType::Air       => [0.0,  0.0,  0.0 ],
            BlockType::Grass     => [0.47, 0.67, 0.19],
            BlockType::Dirt      => [0.61, 0.44, 0.22],
            BlockType::Stone     => [0.5,  0.5,  0.5 ],
            BlockType::Water     => [0.25, 0.41, 0.88],
            BlockType::Log       => [0.55, 0.35, 0.17],
            BlockType::Leaves    => [0.25, 0.51, 0.19],
            BlockType::TallGrass => [0.38, 0.70, 0.20],
        }
    }
    
    pub fn is_opaque(&self) -> bool {
        match self {
            BlockType::Air | BlockType::Water | BlockType::Leaves | BlockType::TallGrass => false,
            _ => true,
        }
    }

    pub fn is_solid(&self) -> bool {
        match self {
            BlockType::Air | BlockType::Water | BlockType::TallGrass => false,
            _ => true,
        }
    }

    /// True if the block can be selected/outlined by the crosshair raycast.
    /// Differs from is_solid: tall grass is targetable but not a collision obstacle.
    pub fn is_targetable(&self) -> bool {
        match self {
            BlockType::Air | BlockType::Water => false,
            _ => true,
        }
    }

    pub fn is_fluid(&self) -> bool {
        matches!(self, BlockType::Water)
    }

    pub fn texture_id(&self, face: crate::world::Face) -> u32 {
        match self {
            BlockType::Air       => 0,
            BlockType::Grass     => match face {
                crate::world::Face::Up   => 0,
                crate::world::Face::Down => 1,
                _                        => 4,
            },
            BlockType::Dirt      => 1,
            BlockType::Stone     => 2,
            BlockType::Water     => 3,
            BlockType::Log       => match face {
                crate::world::Face::Up | crate::world::Face::Down => 7,
                _                                                  => 5,
            },
            BlockType::Leaves    => 6,
            BlockType::TallGrass => 8,
        }
    }

    
}