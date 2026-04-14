# Wurfelland

A voxel sandbox game written in Rust, inspired by Minecraft. Built from scratch using OpenGL for rendering and a custom chunk-based world engine.

![Rust](https://img.shields.io/badge/Rust-1.75+-orange)
![OpenGL](https://img.shields.io/badge/OpenGL-3.3-blue)

## Features

- **Procedurally generated terrain** — Perlin noise with surface variation, oceans, beaches, stone layers
- **Block types** — Grass, Dirt, Stone, Water, Log, Leaves, Tall Grass
- **Trees** — randomly placed, deterministic per seed
- **Water simulation** — fills downward and spreads horizontally up to 6 blocks; tilted surface mesh matching water depth
- **Block interaction** — dig blocks with hand (with break animation and crack overlay), blocks drop items
- **Item drops** — sticks from leaves, log blocks from logs, dirt/stone clumps, seeds from tall grass
- **Inventory system** — 18-slot inventory, same-type items stack with count badge
- **Hotbar** — 9 slots mapped to keys 1–9
- **Bag** — press I to open a 3×6 inventory grid
- **Player model** — rendered torso and arms; right arm swings toward the targeted block while digging
- **Health bar** — displayed as hearts in the HUD
- **Ambient occlusion** — per-vertex soft shadows at block corners
- **Fog** — distance fog blending to sky colour (starts at 32, full at 64 world units)
- **Underwater tint** — blue overlay when the player's eye is inside a water block
- **Frustum culling** — chunks outside the camera view are skipped entirely
- **Two-pass rendering** — opaque geometry first, then transparent (water, leaves) with depth-mask off

## Controls

| Key / Input | Action |
|---|---|
| W A S D | Move |
| Space | Jump |
| Mouse | Look around |
| Left click | Dig block |
| 1 – 9 | Select hotbar slot |
| I | Open / close bag |
| ESC | Pause menu |

## Building

Requires Rust (stable) and a GPU supporting OpenGL 3.3.

```bash
git clone <repo>
cd wurfelland
cargo run --release
```

Dependencies are managed via Cargo and pulled automatically. No external setup needed.

## Architecture

The engine is split into three main areas:

**World** (`src/world/`)
- `chunk.rs` — 16×16×16 block chunks, mesh vertex generation, water mesh
- `world.rs` — chunk loading/unloading, water flow simulation, block mutation
- `block.rs` — block types, hardness, drop tables, texture IDs
- `face.rs` — face positions, texture UVs, ambient occlusion neighbour offsets
- `item.rs` — item entities, gravity, bobbing, auto-pickup

**Renderer** (`src/renderer/`)
- `chunk_renderer.rs` — two-pass OpenGL shader for terrain and water
- `chunk_mesh.rs` — VAO/VBO management
- `player_renderer.rs` — player model and arm swing
- `item_renderer.rs` — spinning dropped items with texture atlas
- `hotbar_renderer.rs` / `bag_renderer.rs` — 2D HUD panels
- `crack_renderer.rs` — block break progress overlay
- `utils.rs` — block texture atlas, item texture atlas

**Camera** (`src/camera/`)
- View and projection matrices, yaw/pitch mouse look, frustum extraction

For a detailed walkthrough of the full rendering pipeline — chunk generation, vertex building, water mesh, water flow, and draw order — see [EXPLANATION.md](EXPLANATION.md).

## Project Structure

```
src/
├── main.rs              — game loop, input, draw order
├── camera/
│   ├── camera.rs        — view/projection matrices, frustum
│   └── frustum.rs       — AABB frustum culling
├── world/
│   ├── world.rs         — chunk pipeline, water simulation
│   ├── chunk.rs         — terrain generation, mesh building
│   ├── block.rs         — block definitions
│   ├── face.rs          — face geometry helpers
│   └── item.rs          — dropped item entities
├── renderer/
│   ├── chunk_renderer.rs
│   ├── chunk_mesh.rs
│   ├── player_renderer.rs
│   ├── item_renderer.rs
│   ├── hotbar_renderer.rs
│   ├── bag_renderer.rs
│   ├── crack_renderer.rs
│   ├── block_outline_renderer.rs
│   └── utils.rs
└── game/
    └── player.rs        — movement, inventory, dig logic
```
