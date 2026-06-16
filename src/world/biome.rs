use crate::world::block::BlockType;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Biome {
    Plains,
    Forest,
    Desert,
    Mountains,
    SnowyTundra,
    Ocean,
    Swamp,
}

pub struct BiomeParams {
    pub base_height: f32,
    pub amplitude:   f32,
    pub scale:       f64,
    pub surface_block:     BlockType,
    pub sub_surface_block: BlockType,
    /// 0 = no trees; N = 1-in-N chance per column
    pub tree_freq: u32,
    /// 0 = no tall grass; N = 1-in-N chance per column
    pub grass_freq: u32,
}

impl Biome {
    pub fn params(&self) -> BiomeParams {
        match self {
            Biome::Plains => BiomeParams {
                base_height: 128.0, amplitude: 8.0, scale: 0.03,
                surface_block:     BlockType::Grass,
                sub_surface_block: BlockType::Dirt,
                tree_freq: 4, grass_freq: 3,  // 1 in 4 cells (≈1 tree per 256 blocks)
            },
            Biome::Forest => BiomeParams {
                base_height: 129.0, amplitude: 14.0, scale: 0.04,
                surface_block:     BlockType::Grass,
                sub_surface_block: BlockType::Dirt,
                tree_freq: 1, grass_freq: 2,  // every cell (≈1 tree per 64 blocks)
            },
            Biome::Desert => BiomeParams {
                base_height: 127.0, amplitude: 6.0, scale: 0.025,
                surface_block:     BlockType::Sand,
                sub_surface_block: BlockType::Sand,
                tree_freq: 0, grass_freq: 0,
            },
            Biome::Mountains => BiomeParams {
                // base near plains (130) so biome edges aren't cliffs; amplitude 52
                // gives typical peaks at Y=152–166, rare highs to Y=182.
                // Low scale = broad, gradual slopes.
                base_height: 130.0, amplitude: 52.0, scale: 0.018,
                surface_block:     BlockType::Stone,
                sub_surface_block: BlockType::Stone,
                tree_freq: 0, grass_freq: 0,
            },
            Biome::SnowyTundra => BiomeParams {
                base_height: 128.0, amplitude: 5.0, scale: 0.025,
                surface_block:     BlockType::Snow,
                sub_surface_block: BlockType::Dirt,
                tree_freq: 0, grass_freq: 0,
            },
            Biome::Ocean => BiomeParams {
                base_height: 104.0, amplitude: 10.0, scale: 0.02,
                surface_block:     BlockType::Sand,
                sub_surface_block: BlockType::Stone,
                tree_freq: 0, grass_freq: 0,
            },
            Biome::Swamp => BiomeParams {
                // Low, flat land hovering around sea level (127): gentle dips fill
                // with water to form murky ponds.  Lush grass, scattered trees.
                base_height: 125.0, amplitude: 3.0, scale: 0.035,
                surface_block:     BlockType::Grass,
                sub_surface_block: BlockType::Dirt,
                tree_freq: 8, grass_freq: 2,
            },
        }
    }

    /// Swamp tints water and grass with a murky colour (Minecraft Bedrock style).
    pub fn is_swamp(&self) -> bool {
        matches!(self, Biome::Swamp)
    }

    /// Map pre-sampled noise values (each in [-1, 1]) to a biome.
    pub fn from_noise(temperature: f64, moisture: f64, continentalness: f64) -> Self {
        let t = (temperature    + 1.0) / 2.0; // normalise to [0, 1]
        let m = (moisture       + 1.0) / 2.0;
        let c = (continentalness+ 1.0) / 2.0;

        if c < 0.28 {
            Biome::Ocean
        } else if c > 0.80 {
            Biome::Mountains
        } else if t < 0.30 {
            Biome::SnowyTundra
        } else if t > 0.65 && m < 0.40 {
            Biome::Desert
        } else if m > 0.72 && (0.45..=0.78).contains(&t) {
            // Warm and very wet → murky swampland.
            Biome::Swamp
        } else if m > 0.55 {
            Biome::Forest
        } else {
            Biome::Plains
        }
    }

    pub fn allows_chickens(&self) -> bool {
        matches!(self, Biome::Plains | Biome::Forest)
    }

    pub fn allows_pigs(&self) -> bool {
        matches!(self, Biome::Plains | Biome::Forest)
    }

    pub fn allows_penguins(&self) -> bool {
        matches!(self, Biome::SnowyTundra)
    }

    pub fn allows_cows(&self) -> bool {
        matches!(self, Biome::Plains | Biome::Forest)
    }
}

/// Convenience: derive the biome at a world-space coordinate.
/// Creates its own noise instances — cheap enough for startup/spawning use.
pub fn biome_at_world(wx: f64, wz: f64) -> Biome {
    use noise::{NoiseFn, Perlin};
    let temp  = Perlin::new(100);
    let moist = Perlin::new(200);
    let cont  = Perlin::new(300);
    Biome::from_noise(
        temp .get([wx * 0.003, wz * 0.003]),
        moist.get([wx * 0.003, wz * 0.003]),
        cont .get([wx * 0.005, wz * 0.005]),
    )
}
