use crate::world::block::BlockType;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ItemType {
    Stick,
    LogBlock,
    DirtClump,
    StoneChunk,
    Seeds,
    Feather,
    Egg,
    ChickenMeat,
    PorkChop,
    StoneAxe,
    Torch,
    Bed,
    RawCopper,
    Coal,
    RawIron,
    Furnace,
    PumpkinSeeds,
}

impl ItemType {
    /// Atlas tile index in the item atlas (256×256, 16 tiles/row, 16×16 px each).
    pub fn tile_index(&self) -> usize {
        match self {
            ItemType::Stick        => 0,
            ItemType::LogBlock     => 1,
            ItemType::DirtClump    => 2,
            ItemType::StoneChunk   => 3,
            ItemType::Seeds        => 4,
            ItemType::Feather      => 5,
            ItemType::Egg          => 6,
            ItemType::ChickenMeat  => 7,
            ItemType::PorkChop     => 8,
            ItemType::StoneAxe     => 9,
            ItemType::Torch        => 10,
            ItemType::Bed          => 11,
            ItemType::RawCopper    => 12,
            ItemType::Coal         => 13,
            ItemType::RawIron      => 14,
            ItemType::Furnace       => 15,
            ItemType::PumpkinSeeds  => 16,
        }
    }

    pub fn from_tile_index(i: usize) -> Option<Self> {
        match i {
            0  => Some(Self::Stick),
            1  => Some(Self::LogBlock),
            2  => Some(Self::DirtClump),
            3  => Some(Self::StoneChunk),
            4  => Some(Self::Seeds),
            5  => Some(Self::Feather),
            6  => Some(Self::Egg),
            7  => Some(Self::ChickenMeat),
            8  => Some(Self::PorkChop),
            9  => Some(Self::StoneAxe),
            10 => Some(Self::Torch),
            11 => Some(Self::Bed),
            12 => Some(Self::RawCopper),
            13 => Some(Self::Coal),
            14 => Some(Self::RawIron),
            15 => Some(Self::Furnace),
            16 => Some(Self::PumpkinSeeds),
            _  => None,
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "stick"        => Some(Self::Stick),
            "log_block"    => Some(Self::LogBlock),
            "dirt_clump"   => Some(Self::DirtClump),
            "stone_chunk"  => Some(Self::StoneChunk),
            "seeds"        => Some(Self::Seeds),
            "feather"      => Some(Self::Feather),
            "egg"          => Some(Self::Egg),
            "chicken_meat" => Some(Self::ChickenMeat),
            "pork_chop"    => Some(Self::PorkChop),
            "stone_axe"    => Some(Self::StoneAxe),
            "torch"        => Some(Self::Torch),
            "bed"          => Some(Self::Bed),
            "raw_copper"   => Some(Self::RawCopper),
            "coal"         => Some(Self::Coal),
            "raw_iron"     => Some(Self::RawIron),
            "furnace"        => Some(Self::Furnace),
            "pumpkin_seeds"  => Some(Self::PumpkinSeeds),
            _                => None,
        }
    }

    pub fn color(&self) -> [f32; 3] {
        match self {
            ItemType::Stick       => [0.55, 0.35, 0.17],
            ItemType::LogBlock    => [0.55, 0.35, 0.17],
            ItemType::DirtClump   => [0.61, 0.44, 0.22],
            ItemType::StoneChunk  => [0.50, 0.50, 0.50],
            ItemType::Seeds       => [0.80, 0.75, 0.20],
            ItemType::Feather     => [0.95, 0.95, 0.90],
            ItemType::Egg         => [0.95, 0.90, 0.75],
            ItemType::ChickenMeat => [0.85, 0.40, 0.30],
            ItemType::PorkChop    => [0.90, 0.45, 0.32],
            ItemType::StoneAxe    => [0.58, 0.58, 0.63],
            ItemType::Torch       => [1.00, 0.47, 0.00],
            ItemType::Bed         => [0.80, 0.35, 0.25],
            ItemType::RawCopper   => [0.72, 0.45, 0.20],
            ItemType::Coal        => [0.15, 0.15, 0.15],
            ItemType::RawIron     => [0.62, 0.42, 0.30],
            ItemType::Furnace      => [0.50, 0.50, 0.50],
            ItemType::PumpkinSeeds => [0.85, 0.65, 0.15],
        }
    }
}

pub struct ItemEntity {
    pub position: [f32; 3], // block-space origin (block's min corner)
    pub item: ItemType,
    pub age: f32,
    vy: f32,
    on_ground: bool,
}

impl ItemEntity {
    pub fn new(x: f32, y: f32, z: f32, item: ItemType) -> Self {
        ItemEntity { position: [x, y, z], item, age: 0.0, vy: 0.0, on_ground: false }
    }

    pub fn update(&mut self, dt: f32, get_block: impl Fn(i32, i32, i32) -> BlockType) {
        self.age += dt;

        if self.on_ground {
            // Recheck support — resume falling if block underneath was removed
            let bx = self.position[0].floor() as i32;
            let by = (self.position[1] - 0.01).floor() as i32; // just below feet
            let bz = self.position[2].floor() as i32;
            let below = get_block(bx, by, bz);
            if !below.is_solid() && below != BlockType::Water {
                self.on_ground = false;
            } else {
                return;
            }
        }

        const GRAVITY: f32 = -20.0;
        self.vy = (self.vy + GRAVITY * dt).max(-50.0);
        let new_y = self.position[1] + self.vy * dt;

        let bx = self.position[0].floor() as i32;
        let bz = self.position[2].floor() as i32;
        let foot_block = get_block(bx, new_y.floor() as i32, bz);

        if foot_block.is_solid() {
            // Land on top of solid block
            self.position[1] = new_y.floor() + 1.0;
            self.vy = 0.0;
            self.on_ground = true;
        } else if foot_block == BlockType::Water {
            // Float just above water surface
            let water_y = new_y.floor() + 1.0;
            if new_y <= water_y {
                self.position[1] = water_y;
                self.vy = 0.0;
                self.on_ground = true;
            } else {
                self.position[1] = new_y;
            }
        } else {
            self.position[1] = new_y;
        }
    }

    /// Y with gentle bob animation (only when resting, not while falling)
    pub fn visual_y(&self) -> f32 {
        let bob = if self.on_ground { (self.age * 2.0).sin() * 0.06 } else { 0.0 };
        self.position[1] + 0.25 + bob
    }
}
