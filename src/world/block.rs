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
    GrassShort,
    Sand,
    Snow,
    Bed,
}

impl BlockType {
    pub fn color(&self) -> [f32; 3] {
        match self {
            BlockType::Air        => [0.0,  0.0,  0.0 ],
            BlockType::Grass      => [0.47, 0.67, 0.19],
            BlockType::Dirt       => [0.61, 0.44, 0.22],
            BlockType::Stone      => [0.5,  0.5,  0.5 ],
            BlockType::Water      => [0.247, 0.463, 0.894],
            BlockType::Log        => [0.55, 0.35, 0.17],
            BlockType::Leaves     => [0.25, 0.51, 0.19],
            BlockType::TallGrass  => [0.38, 0.70, 0.20],
            BlockType::GrassShort => [0.38, 0.70, 0.20],
            BlockType::Sand       => [0.96, 0.87, 0.60],
            BlockType::Snow       => [0.90, 0.94, 1.00],
            BlockType::Bed        => [0.80, 0.35, 0.25],
        }
    }
    
    pub fn is_opaque(&self) -> bool {
        match self {
            BlockType::Air | BlockType::Water | BlockType::Leaves
            | BlockType::TallGrass | BlockType::GrassShort => false,
            BlockType::Bed => true,
            _ => true,
        }
    }

    pub fn is_solid(&self) -> bool {
        match self {
            BlockType::Air | BlockType::Water | BlockType::TallGrass | BlockType::GrassShort => false,
            _ => true,
        }
    }

    pub fn is_targetable(&self) -> bool {
        match self {
            BlockType::Air | BlockType::Water => false,
            _ => true,
        }
    }

    pub fn is_fluid(&self) -> bool {
        matches!(self, BlockType::Water)
    }

    /// Base dig time in seconds with bare hands (None = cannot be dug).
    /// Multiply delta_time by a tool speed factor before accumulating progress
    /// to support faster tools in the future.
    pub fn hardness(&self) -> Option<f32> {
        match self {
            BlockType::Air        => None,
            BlockType::Water      => None,
            BlockType::TallGrass  => Some(0.05),
            BlockType::GrassShort => Some(0.05),
            BlockType::Snow       => Some(0.2),
            BlockType::Leaves     => Some(0.2),
            BlockType::Grass      => Some(0.5),
            BlockType::Dirt       => Some(0.5),
            BlockType::Sand       => Some(0.5),
            BlockType::Log        => Some(1.5),
            BlockType::Stone      => Some(3.0),
            BlockType::Bed        => Some(0.5),
        }
    }

    /// Items dropped when this block is broken.
    /// Uses a position hash for deterministic ~50% drop rate on leaves.
    pub fn drops(&self, wx: i32, wy: i32, wz: i32) -> Vec<crate::world::item::ItemType> {
        use crate::world::item::ItemType;
        let hash = (wx.wrapping_mul(374761393)
            .wrapping_add(wy.wrapping_mul(668265263))
            .wrapping_add(wz.wrapping_mul(2147483647))) as u32;
        match self {
            BlockType::Leaves => {
                if hash % 2 == 0 { vec![ItemType::Stick] } else { vec![] }
            }
            BlockType::Log                              => vec![ItemType::LogBlock],
            BlockType::Grass | BlockType::Dirt          => vec![ItemType::DirtClump],
            BlockType::Sand                             => vec![ItemType::DirtClump],
            BlockType::TallGrass | BlockType::GrassShort => {
                if hash % 20 == 0 { vec![ItemType::Seeds] } else { vec![] }
            }
            BlockType::Stone => vec![ItemType::StoneChunk],
            BlockType::Bed   => vec![ItemType::Bed],
            _ => vec![],
        }
    }

    pub fn break_sound(&self) -> Option<&'static str> {
        match self {
            BlockType::Dirt | BlockType::Grass => Some("assets/sounds/UI_Quirky_53.mp3"),
            _ => None,
        }
    }

    #[allow(dead_code)]
    pub fn to_net_id(self) -> u8 {
        match self {
            BlockType::Air        => 0,
            BlockType::Grass      => 1,
            BlockType::Dirt       => 2,
            BlockType::Stone      => 3,
            BlockType::Water      => 4,
            BlockType::Log        => 5,
            BlockType::Leaves     => 6,
            BlockType::TallGrass  => 7,
            BlockType::Sand       => 8,
            BlockType::Snow       => 9,
            BlockType::GrassShort => 10,
            BlockType::Bed        => 11,
        }
    }

    pub fn from_net_id(id: u8) -> Self {
        match id {
            1  => Self::Grass,
            2  => Self::Dirt,
            3  => Self::Stone,
            4  => Self::Water,
            5  => Self::Log,
            6  => Self::Leaves,
            7  => Self::TallGrass,
            8  => Self::Sand,
            9  => Self::Snow,
            10 => Self::GrassShort,
            11 => Self::Bed,
            _  => Self::Air,
        }
    }

    pub fn texture_id(&self, face: crate::world::Face) -> u32 {
        match self {
            BlockType::Air        => 0,
            BlockType::Grass      => match face {
                crate::world::Face::Up   => 0,
                crate::world::Face::Down => 1,
                _                        => 4,
            },
            BlockType::Dirt       => 1,
            BlockType::Stone      => 2,
            BlockType::Water      => 3,
            BlockType::Log        => match face {
                crate::world::Face::Up | crate::world::Face::Down => 7,
                _                                                  => 5,
            },
            BlockType::Leaves     => 6,
            BlockType::TallGrass  => 8,
            BlockType::GrassShort => 8, // reuses the same atlas tile, rendered shorter
            BlockType::Sand       => 14,
            BlockType::Snow       => 15,
            BlockType::Bed        => 5,  // reuse log-side texture (brown)
        }
    }

    
}