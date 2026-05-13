use crate::world::block::BlockType;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Biome {
    Plains,
    Forest,
    Desert,
    Mountains,
    SnowyTundra,
    Ocean,
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
                base_height: 64.0, amplitude: 8.0, scale: 0.03,
                surface_block:     BlockType::Grass,
                sub_surface_block: BlockType::Dirt,
                tree_freq: 220, grass_freq: 3,
            },
            Biome::Forest => BiomeParams {
                base_height: 65.0, amplitude: 14.0, scale: 0.04,
                surface_block:     BlockType::Grass,
                sub_surface_block: BlockType::Dirt,
                tree_freq: 22, grass_freq: 2,
            },
            Biome::Desert => BiomeParams {
                base_height: 63.0, amplitude: 6.0, scale: 0.025,
                surface_block:     BlockType::Sand,
                sub_surface_block: BlockType::Sand,
                tree_freq: 0, grass_freq: 0,
            },
            Biome::Mountains => BiomeParams {
                // base near plains (66) so biome edges aren't cliffs; amplitude 52
                // gives typical peaks at Y=88–102, rare highs to Y=118 — matching
                // classic Minecraft Extreme Hills.  Low scale = broad, gradual slopes.
                base_height: 66.0, amplitude: 52.0, scale: 0.018,
                surface_block:     BlockType::Stone,
                sub_surface_block: BlockType::Stone,
                tree_freq: 0, grass_freq: 0,
            },
            Biome::SnowyTundra => BiomeParams {
                base_height: 64.0, amplitude: 5.0, scale: 0.025,
                surface_block:     BlockType::Snow,
                sub_surface_block: BlockType::Dirt,
                tree_freq: 0, grass_freq: 0,
            },
            Biome::Ocean => BiomeParams {
                base_height: 40.0, amplitude: 10.0, scale: 0.02,
                surface_block:     BlockType::Sand,
                sub_surface_block: BlockType::Stone,
                tree_freq: 0, grass_freq: 0,
            },
        }
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
