# Wurfelland — Rendering Pipeline Explanation

## Overview

```
╔══════════════════════════════════════════════════════════════════════════════════╗
║                          EVERY FRAME  (main.rs loop)                             ║
╚══════════════════════════════════════════════════════════════════════════════════╝

 Player input → player.walk() / player.jump() / process_mouse_movement()
      │
      ▼
 camera.update_pitch_yaw()  +  camera.move_to_abs()          [camera/camera.rs]
      │
      ├── world.update(player.position)      ← chunk pipeline (see below)
      ├── world.tick_water(dt)               ← water flow (see below)
      │
      ▼
 DRAW:  view = camera.view_matrix()          Mat4::look_at_rh(pos, pos+front, up)
        proj = camera.projection_matrix()    Mat4::perspective_rh(fov, aspect, near, far)
```

---

## 1 — Chunk Pipeline

```
world.update()                                              [world/world.rs]
│
├─ load_chunks_around()
│    For every position within loaded_radius (4 chunks):
│    If not already in chunks/terrain_queue/pending_blocks:
│         terrain_queue.insert(pos)          ← just queued, no thread yet
│
├─ dispatch_terrain_threads()                cap: MAX_TERRAIN_THREADS = 4
│    slots = 4 - pending_blocks.len()
│    For each slot:  pop pos from terrain_queue
│                    pending_blocks.insert(pos)
│                    thread::spawn ──────────────────────────────────┐
│                        Chunk::generate(pos)   [world/chunk.rs]     │
│                        │  Perlin noise → surface heights           │
│                        │  Fill Stone / Dirt / Grass / Water        │
│                        │  plant_tree() / TallGrass                 │
│                        └── block_tx.send(BlockReady { pos, chunk })│
│                                                                    ◄┘
├─ finalize_blocks(max=2)          drain up to 2 from block_rx per frame
│    For each arrived chunk:
│    │  chunks.insert(pos, chunk)
│    │  Scan chunk for Water blocks → water_levels.entry(wx,wy,wz).or_insert(8)
│    │  refresh_active_water() + refresh_active_spread()   (water sets)
│    │  mark 4 horizontal neighbour chunks for rebuild
│    │
│    Dispatch mesh threads:  cap: MAX_MESH_THREADS = 4
│    slots = 4 - pending_meshes.len()
│    For each dirty chunk with a free slot:
│         blocks_snapshot()        ← copy of [[[BlockType;16];16];16]
│         water_levels_snapshot()  ← copy of [[[u8;16];16];16]
│         NeighborEdges snapshot   ← 4 border slices + 4 water-level border slices
│         pending_meshes.insert(pos)
│         thread::spawn ──────────────────────────────────────────────┐
│              Chunk::build_vertices(&blocks, &edges, &water_levels)  │
│              (see Vertex Generation below)                          │
│              mesh_tx.send(MeshReady { pos, vertices })              │
│                                                                    ◄┘
└─ finalize_meshes(max=4)          drain up to 4 from mesh_rx per frame
     For each arrived mesh:
          pending_meshes.remove(pos)
          chunk.finalize_mesh(vertices)
               ChunkMesh::from_vertices()    [renderer/chunk_mesh.rs]
               │  gl::GenVertexArrays / GenBuffers
               │  gl::BufferData  (vertices → GPU)
               │  Attrib layout: [x,y,z | r,g,b | u,v]  8 floats/vertex
               └── chunk.mesh = Some(ChunkMesh { vao, vbo, vertex_count })

  unload_distant_chunks()
       chunks/terrain_queue/water_levels/active_spread retain within radius
```

---

## 2 — Vertex Generation

```
Chunk::build_vertices(blocks, edges, water_levels)         [world/chunk.rs]

  For every block position [x, y, z] in 0..16³:

  ┌─ Air?         → skip
  ├─ TallGrass?   → cross_vertices()    two diagonal quads, cutout texture
  ├─ Water?       → water path (see Water Mesh below)
  └─ Everything else:
       For each of 6 faces [Right, Left, Up, Down, Front, Back]:
           is_face_visible()?
           │  Neighbour block inside chunk  → blocks[ix][iy][iz]
           │  Neighbour outside chunk       → NeighborEdges (pre-snapshotted border)
           │  Hide face if neighbour is opaque
           │  Hide face between two identical fluids
           │
           compute_ao()   [ambient occlusion]
           │  For each of 4 face corners, sample 2 side + 1 diagonal block
           │  ao[i] = 1.0 - (side1 + side2 + corner) * 0.2
           │
           face_vertices_real_()
                positions  = face.positions(x, y, z)     [world/face.rs]
                tex_coords = face.texture_coords(texture_id, atlas_size=16)
                brightness = Up:1.0  Down:0.5  Front/Back:0.8  Left/Right:0.65
                Emit 6 vertices (2 triangles) per face:
                [x,y,z, r*light,g*light,b*light, u,v]   8 floats each

Vertex layout in memory (per chunk VBO):
  ┌──────────────────────────────────────────────┐
  │ f32 f32 f32 │ f32 f32 f32 │ f32 f32          │
  │   x   y   z │   r   g   b │  u   v           │
  │  position   │ lit color   │ atlas UV         │
  └──────────────────────────────────────────────┘
  attrib 0 ──────┘             └── attrib 1       └── attrib 2
```

---

## 3 — Texture Atlas

```
create_block_atlas()                                       [renderer/utils.rs]

  256×256 RGBA texture, 16 tiles per row, each tile 16×16 px

  Tile index → block face:
  ┌─────┬────┬─────┬─────┬─────────┬───────┬──────┬──────┬─────────┬──────┐
  │ 0   │ 1  │ 2   │ 3   │ 4       │ 5     │ 6    │ 7    │ 8       │ 9-13 │
  │Grass│Dirt│Stone│Water│GrassSide│LogSide│Leaves│LogTop│TallGrass│Cracks│
  └─────┴────┴─────┴─────┴─────────┴───────┴──────┴──────┴─────────┴──────┘

  UV mapping:  col = tile_id % 16,  row = tile_id / 16
               u = [col/16, (col+1)/16]
               v = [row/16, (row+1)/16]

  BlockType::texture_id(face) selects which tile:      [world/block.rs]
    Grass:  Up→0  Down→1  Sides→4
    Log:    Up/Down→7  Sides→5
    Water:  3 (all faces, semi-transparent α=170)
    Stone:  2,  Dirt: 1,  Leaves: 6,  TallGrass: 8
```

---

## 4 — Water Mesh (Variable Height)

```
Water block at [x, y, z] with water_level L (1–8):

  Corner height = water_corner_h(cx, cy, cz)             [world/chunk.rs]
  │  The 4 blocks sharing corner (cx,cz) at height cy:
  │  (cx-1,cz-1)  (cx,cz-1)  (cx-1,cz)  (cx,cz)
  │  For each: look up water level from water_levels or NeighborEdges
  │            if block above is also water → treat as 8 (full)
  │  corner_h = max(all 4 levels) / 8.0        range [0.0 … 1.0]

  Top face (Face::Up) — 4 corners all vary:
  ┌──────────────────────────────────────────────┐
  │ v3 (x,  z  ) h=hBL    v2 (x+1,z  ) h=hBR     │
  │                                              │
  │ v0 (x,  z+1) h=hFL    v1 (x+1,z+1) h=hFR     │
  └──────────────────────────────────────────────┘
  Y of each corner = y + corner_h  →  tilted surface toward flow direction

  Side faces (Right/Left/Front/Back):
       Bottom 2 corners stay at y (block base)
       Top    2 corners lifted to water_corner_h values at that edge
       → sides are shorter where water is shallower

  water_face_vertices() emits same [x,y,z,r,g,b,u,v] format as solid blocks
  Water level data crosses chunk borders via wl_right/left/front/back in NeighborEdges
```

---

## 5 — Water Flow (tick_water)

```
world.tick_water(dt)                                       [world/world.rs]

Three sets maintained incrementally:

  water_levels    HashMap<[i32;3], u8>     8=source  7..3=flowing  (stops at 2)
  active_water    HashSet<[i32;3]>         water blocks with Air directly below
  active_spread   HashSet<[i32;3]>         water blocks (level>2) with Air beside them

  Each are updated by refresh_active_water() + refresh_active_spread()
  called from set_block() whenever any block changes.

Step 1 — Downward flow
  active_water → find Water where block below is Air, not already pending
  Queue [wx, wy-1, wz] in pending_water with (timer=1.0s, level=8)
  ← fallen water always resets to source strength (level 8)

Step 2 — Horizontal flow
  active_spread → find Water with level > 2
  For each of 4 horizontal neighbours that is Air and not pending:
       queue in pending_water with (timer=1.0s, level = this.level - 1)

  Spread chain:  source(8) → 7 → 6 → 5 → 4 → 3 → 2   (6 blocks max)
                                                    ↑ stops here (not > 2)

Step 3 — Tick timers
  pending_water.retain: decrement timers, collect expired

Step 4 — Fill
  For expired positions still Air:
       water_levels.insert(pos, lvl)     ← BEFORE set_block so level is known
       set_block(wx,wy,wz, Water)
            → chunk.set_block + mark_for_rebuild
            → mark 4 border neighbours dirty
            → refresh_active_water + refresh_active_spread
```

---

## 6 — Draw (per frame)

```
 ┌──────────────────────────────────────────────────────────────────────────┐
 │  gl::Clear(COLOR | DEPTH)                                                │
 │                                                                          │
 │  PASS 1 — Opaque geometry          chunk_renderer.set_transparent(false) │
 │  DepthMask ON,  CullFace BACK                                            │
 │  Fragment shader: discard if α < 0.99  (skips water / leaves / glass)    │
 │  world.draw() → frustum cull → draw_chunk() per visible chunk            │
 │       chunk.model_matrix() → translate to chunk's world position         │
 │       gl::UniformMatrix4fv(model, view, projection)                      │
 │       gl::BindVertexArray(mesh.vao)                                      │
 │       gl::DrawArrays(TRIANGLES, 0, vertex_count)                         │
 │                                                                          │
 │  PASS 2 — Transparent geometry     chunk_renderer.set_transparent(true)  │
 │  DepthMask OFF (water doesn't occlude things behind it)                  │
 │  CullFace OFF  (see water from below)                                    │
 │  Fragment shader: discard if α >= 0.99  (skips opaque blocks)            │
 │  Same draw_chunk() loop — water faces now pass the filter                │
 │       Fog applied: mix(block_color, sky_color, fog_factor)               │
 │       fog_start=32  fog_end=64  (world units)                            │
 │                                                                          │
 │  3D overlays (depth test ON):                                            │
 │     player_renderer.draw()   ← arms with swing_angle rotation            │
 │     outline_renderer.draw()  ← slightly expanded cube wireframe          │
 │     item_renderer.draw()     ← spinning dropped items                    │
 │     crack_renderer.draw()    ← crack overlay tiles 9-13 from atlas       │
 │                                                                          │
 │  HUD (depth test OFF, blend ON):                                         │
 │     underwater tint  → hotbar_renderer.draw_fullscreen_tint()            │
 │     crosshair        → crosshair_renderer.draw()                         │
 │     health bar       → health_bar.draw(health/100)                       │
 │     hotbar           → hotbar_renderer.draw(selected, slots)             │
 │     bag              → bag_renderer.draw(&inventory)     (if open)       │
 │     pause menu       → menu_renderer.draw()              (if paused)     │
 └──────────────────────────────────────────────────────────────────────────┘

Frustum culling (world.draw):
  camera.frustum() → 6 planes from view*projection matrix
  chunk.is_in_frustum() → AABB test (chunk min/max corners vs planes)
  Chunks behind the camera or off-screen are skipped entirely
```

---

## 7 — Thread Model Summary

```
Main thread                Worker threads (capped)
──────────────────────     ──────────────────────────────────
terrain_queue              ≤4 terrain threads
  → spawn → pending_blocks    Chunk::generate()  (Perlin, trees)
              ↓ block_rx          → block_tx
finalize_blocks (2/frame)

pending_meshes             ≤4 mesh threads
  → spawn ← dirty chunks      Chunk::build_vertices()
              ↓ mesh_rx           → mesh_tx
finalize_meshes (4/frame)
  → ChunkMesh::from_vertices()
  → VAO/VBO upload to GPU
```

The caps (4+4) exist so worker threads don't compete with the main thread for CPU
cores. Each batch is spread across multiple frames so no single frame does a large
spike of work.
