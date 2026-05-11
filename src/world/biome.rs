use crate::world::block::BlockType;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Biome {
    Plains,
    Forest,
    Desert,
    Mountains,
    SnowyTundra,
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
                base_height: 8.0, amplitude: 4.0, scale: 0.03,
                surface_block:     BlockType::Grass,
                sub_surface_block: BlockType::Dirt,
                tree_freq: 220, grass_freq: 3,
            },
            Biome::Forest => BiomeParams {
                base_height: 8.0, amplitude: 5.0, scale: 0.05,
                surface_block:     BlockType::Grass,
                sub_surface_block: BlockType::Dirt,
                tree_freq: 22, grass_freq: 2,
            },
            Biome::Desert => BiomeParams {
                base_height: 8.0, amplitude: 2.5, scale: 0.02,
                surface_block:     BlockType::Sand,
                sub_surface_block: BlockType::Sand,
                tree_freq: 0, grass_freq: 0,
            },
            Biome::Mountains => BiomeParams {
                base_height: 8.0, amplitude: 7.0, scale: 0.08,
                surface_block:     BlockType::Stone,
                sub_surface_block: BlockType::Stone,
                tree_freq: 0, grass_freq: 0,
            },
            Biome::SnowyTundra => BiomeParams {
                base_height: 8.0, amplitude: 2.0, scale: 0.02,
                surface_block:     BlockType::Snow,
                sub_surface_block: BlockType::Dirt,
                tree_freq: 0, grass_freq: 0,
            },
        }
    }

    /// Map pre-sampled noise values (each in [-1, 1]) to a biome.
    pub fn from_noise(temperature: f64, moisture: f64, continentalness: f64) -> Self {
        let t = (temperature    + 1.0) / 2.0; // normalise to [0, 1]
        let m = (moisture       + 1.0) / 2.0;
        let c = (continentalness+ 1.0) / 2.0;

        if c > 0.80 {
            Biome::Mountains
        } else if t < 0.30 {
            Biome::SnowyTundra
        } else if t > 0.65 && m < 0.40 {
            Biome::Desert
        } else if m > 0.55 {
            Biome::Forest
        } else {
            Biome::Plains
        }
    }

    pub fn allows_chickens(&self) -> bool {
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
