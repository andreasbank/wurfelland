use crate::world::item::ItemType;

pub struct CropDef {
    pub id:            u8,
    pub stages:        u8,
    pub stage_heights: &'static [f32],
    pub final_solid:   bool,
    pub grow_secs:     f32,
    pub seed_item:     ItemType,
    pub tile_stem:     u32,
    pub tile_side:     u32,
    pub tile_top:      u32,
    pub wild_chance:   u32,
    pub wild_final:    bool,
}

static WHEAT_HEIGHTS:   [f32; 5] = [0.15, 0.25, 0.40, 0.55, 0.75];
static PUMPKIN_HEIGHTS: [f32; 4] = [0.15, 0.30, 0.50, 0.70];

pub static CROPS: &[CropDef] = &[
    CropDef {
        id: 0, stages: 5, stage_heights: &WHEAT_HEIGHTS,
        final_solid: false, grow_secs: 30.0,
        seed_item: ItemType::Seeds,
        tile_stem: 23, tile_side: 0, tile_top: 0,
        wild_chance: 80, wild_final: false,
    },
    CropDef {
        id: 1, stages: 4, stage_heights: &PUMPKIN_HEIGHTS,
        final_solid: true, grow_secs: 30.0,
        seed_item: ItemType::PumpkinSeeds,
        tile_stem: 28, tile_side: 32, tile_top: 33,
        wild_chance: 200, wild_final: true,
    },
];

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum BlockMaterial { Grass, Dirt, Stone, Wood, Leaves, Sand, Snow }

impl BlockMaterial {
    pub fn sound(&self) -> &'static str {
        match self {
            BlockMaterial::Grass  => "assets/sounds/grass.mp3",
            BlockMaterial::Dirt   => "assets/sounds/dirt.mp3",
            BlockMaterial::Stone  => "assets/sounds/stone.mp3",
            BlockMaterial::Wood   => "assets/sounds/wood.mp3",
            BlockMaterial::Leaves => "assets/sounds/leaves.mp3",
            BlockMaterial::Sand   => "assets/sounds/sand.mp3",
            BlockMaterial::Snow   => "assets/sounds/snow.mp3",
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
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
    CopperOre,
    CoalOre,
    IronOre,
    Furnace,
    Lava,
    Cobblestone,
    WoodBlock,
    Workbench,
    Crop(u8, u8),     // (crop_id, stage) — see CROPS table
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
            BlockType::CopperOre  => [0.72, 0.45, 0.20],
            BlockType::CoalOre    => [0.15, 0.15, 0.15],
            BlockType::IronOre    => [0.62, 0.42, 0.30],
            BlockType::Furnace    => [0.50, 0.50, 0.50],
            BlockType::Lava        => [1.00, 0.45, 0.00],
            BlockType::Cobblestone => [0.44, 0.44, 0.44],
            BlockType::WoodBlock   => [0.76, 0.60, 0.35],
            BlockType::Workbench   => [0.55, 0.37, 0.18],
            BlockType::Crop(_, _)  => [1.00, 1.00, 1.00],
        }
    }

    pub fn is_opaque(&self) -> bool {
        match self {
            BlockType::Air | BlockType::Water | BlockType::Leaves
            | BlockType::TallGrass | BlockType::GrassShort | BlockType::Workbench => false,
            BlockType::Crop(id, stage) => {
                let def = &CROPS[*id as usize];
                def.final_solid && *stage == def.stages - 1
            }
            _ => true,
        }
    }

    pub fn is_solid(&self) -> bool {
        match self {
            BlockType::Air | BlockType::Water | BlockType::Lava
            | BlockType::TallGrass | BlockType::GrassShort => false,
            BlockType::Crop(id, stage) => {
                let def = &CROPS[*id as usize];
                def.final_solid && *stage == def.stages - 1
            }
            _ => true,
        }
    }

    pub fn is_targetable(&self) -> bool {
        match self {
            BlockType::Air | BlockType::Water | BlockType::Lava => false,
            _ => true,
        }
    }

    pub fn is_fluid(&self) -> bool {
        matches!(self, BlockType::Water | BlockType::Lava)
    }

    /// Base dig time in seconds with bare hands (None = cannot be dug).
    /// Multiply delta_time by a tool speed factor before accumulating progress
    /// to support faster tools in the future.
    pub fn hardness(&self) -> Option<f32> {
        match self {
            BlockType::Air        => None,
            BlockType::Water      => None,
            BlockType::Lava       => None,
            BlockType::Cobblestone => Some(2.0),
            BlockType::TallGrass  => Some(0.05),
            BlockType::GrassShort => Some(0.05),
            BlockType::Crop(id, stage)  => {
                let def = &CROPS[*id as usize];
                if def.final_solid && *stage == def.stages - 1 { Some(1.0) } else { Some(0.05) }
            }
            BlockType::Snow       => Some(0.2),
            BlockType::CopperOre  => Some(3.0),
            BlockType::CoalOre    => Some(3.0),
            BlockType::IronOre    => Some(3.0),
            BlockType::Furnace    => Some(3.5),
            BlockType::Leaves     => Some(0.2),
            BlockType::Grass      => Some(0.5),
            BlockType::Dirt       => Some(0.5),
            BlockType::Sand       => Some(0.5),
            BlockType::Log        => Some(1.5),
            BlockType::WoodBlock  => Some(1.5),
            BlockType::Workbench  => Some(1.5),
            BlockType::Stone      => Some(3.0),
            BlockType::Bed        => Some(0.5),
        }
    }

    /// Items dropped when this block is broken.
    /// Uses a position hash for deterministic ~50% drop rate on leaves.
    pub fn drops(&self, wx: i32, wy: i32, wz: i32) -> Vec<ItemType> {
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
            BlockType::Crop(id, stage) => {
                let def = &CROPS[*id as usize];
                if def.final_solid && *stage == def.stages - 1 {
                    vec![def.seed_item, def.seed_item]
                } else {
                    vec![def.seed_item]
                }
            }
            BlockType::Stone     => vec![ItemType::StoneChunk],
            BlockType::Bed       => vec![ItemType::Bed],
            BlockType::CopperOre => vec![ItemType::RawCopper],
            BlockType::CoalOre   => vec![ItemType::Coal],
            BlockType::IronOre   => vec![ItemType::RawIron],
            BlockType::Furnace   => vec![ItemType::Furnace],
            BlockType::Lava        => vec![],
            BlockType::Cobblestone => vec![ItemType::StoneChunk],
            BlockType::WoodBlock   => vec![ItemType::WoodBlock],
            BlockType::Workbench   => vec![ItemType::Workbench],
            _ => vec![],
        }
    }

    pub fn material(&self) -> Option<BlockMaterial> {
        match self {
            BlockType::Grass | BlockType::TallGrass | BlockType::GrassShort => Some(BlockMaterial::Grass),
            BlockType::Crop(id, stage) => {
                let def = &CROPS[*id as usize];
                if def.final_solid && *stage == def.stages - 1 {
                    Some(BlockMaterial::Wood)
                } else {
                    Some(BlockMaterial::Grass)
                }
            }
            BlockType::Dirt   => Some(BlockMaterial::Dirt),
            BlockType::Stone  => Some(BlockMaterial::Stone),
            BlockType::Log | BlockType::Bed | BlockType::WoodBlock | BlockType::Workbench => Some(BlockMaterial::Wood),
            BlockType::CopperOre             => Some(BlockMaterial::Stone),
            BlockType::CoalOre               => Some(BlockMaterial::Stone),
            BlockType::IronOre               => Some(BlockMaterial::Stone),
            BlockType::Furnace               => Some(BlockMaterial::Stone),
            BlockType::Leaves => Some(BlockMaterial::Leaves),
            BlockType::Sand   => Some(BlockMaterial::Sand),
            BlockType::Snow   => Some(BlockMaterial::Snow),
            BlockType::Lava        => None,
            BlockType::Cobblestone => Some(BlockMaterial::Stone),
            _                      => None,
        }
    }

    pub fn break_sound(&self) -> Option<&'static str> {
        self.material().map(|m| m.sound())
    }

    pub fn hit_sound(&self) -> Option<&'static str> {
        self.material().map(|m| m.sound())
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
            BlockType::CopperOre  => 12,
            BlockType::CoalOre    => 13,
            BlockType::IronOre    => 14,
            BlockType::Furnace    => 15,
            BlockType::Lava        => 16,
            BlockType::Cobblestone => 17,
            BlockType::WoodBlock   => 28,
            BlockType::Workbench   => 29,
            BlockType::Crop(0, s) => 18 + s,
            BlockType::Crop(1, s) => 23 + s,
            BlockType::Crop(id, s) => 32 + id * 8 + s,
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
            12 => Self::CopperOre,
            13 => Self::CoalOre,
            14 => Self::IronOre,
            15 => Self::Furnace,
            16 => Self::Lava,
            17 => Self::Cobblestone,
            18..=22 => Self::Crop(0, id - 18),   // wheat stages 0–4
            23..=26 => Self::Crop(1, id - 23),   // pumpkin stages 0–3 (26 = solid)
            27      => Self::Crop(1, 3),          // backward compat: old Pumpkin → solid stage
            28 => Self::WoodBlock,
            29 => Self::Workbench,
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
            BlockType::Bed        => 5,   // reuse log-side texture (brown)
            BlockType::CopperOre  => 16,
            BlockType::CoalOre    => 17,
            BlockType::IronOre    => 18,
            BlockType::Furnace    => match face {
                crate::world::Face::Front | crate::world::Face::Back => 20,
                _                                                     => 19,
            },
            BlockType::Lava        => 21,
            BlockType::Cobblestone => 22,
            BlockType::WoodBlock   => 34,
            BlockType::Workbench   => match face {
                crate::world::Face::Up => 36,
                _                      => 34,
            },
            BlockType::Crop(id, stage) => {
                let def = &CROPS[*id as usize];
                if def.final_solid && *stage == def.stages - 1 {
                    match face {
                        crate::world::Face::Up => def.tile_top,
                        _                      => def.tile_side,
                    }
                } else {
                    def.tile_stem + *stage as u32
                }
            }
        }
    }

    pub fn selection_height(&self) -> f32 {
        match self {
            BlockType::Crop(id, stage) => {
                let def = &CROPS[*id as usize];
                if def.final_solid && *stage == def.stages - 1 {
                    1.0
                } else {
                    def.stage_heights[(*stage as usize).min(def.stage_heights.len() - 1)]
                }
            }
            _ => 1.0,
        }
    }
}