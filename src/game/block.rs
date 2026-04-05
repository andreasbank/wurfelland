pub struct Block {
    pub position: (f32, f32, f32),
}

impl Block {
    pub fn new(position: (f32, f32, f32) ) -> Self {
        Block {
            position,
        }
    }

    // Minecraft-like colors:
    // Grass block top: (0.47, 0.67, 0.19)
    // Grass side/dirt: (0.61, 0.44, 0.22)
    // Dirt: (0.61, 0.44, 0.22)
    // Stone: (0.5, 0.5, 0.5)
    // Water: (0.25, 0.41, 0.88)
}