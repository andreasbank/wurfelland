# generate_assets.py — pip install Pillow
# Generates all hand-authored game assets:
#   assets/skins/default.png        — 64x32 player skin atlas
#   assets/textures/blocks_atlas.png — 256x256 block texture atlas

import math
import os
from PIL import Image, ImageDraw

# ── Player skin ───────────────────────────────────────────────────────────────
# Layout summary (all pixel rects are [x0, y0, x1-1, y1-1] in PIL terms):
#
#   HEAD (8px per face, top-left origin at 0,0):
#     top    (8,0)→(16,8)     bottom (16,0)→(24,8)
#     right  (0,8)→(8,16)     front  (8,8)→(16,16)   ← face / eyes
#     left   (16,8)→(24,16)   back   (24,8)→(32,16)
#
#   TORSO (4W×7H×2D, origin at 32,0):
#     top    (34,0)→(38,2)    bottom (38,0)→(42,2)
#     left   (32,2)→(34,9)    front  (34,2)→(38,9)
#     right  (38,2)→(40,9)    back   (40,2)→(44,9)
#
#   LEFT ARM (2W×7H×2D, origin at 44,0):
#     top    (46,0)→(48,2)    bottom (48,0)→(50,2)
#     left   (44,2)→(46,9)    front  (46,2)→(48,9)
#     right  (48,2)→(50,9)    back   (50,2)→(52,9)
#
#   RIGHT ARM (2W×7H×2D, origin at 52,0):
#     top    (54,0)→(56,2)    bottom (56,0)→(58,2)
#     left   (52,2)→(54,9)    front  (54,2)→(56,9)
#     right  (56,2)→(58,9)    back   (58,2)→(60,9)
#
#   LEFT LEG (4W×8H×2D, origin at 0,16):
#     top    (2,16)→(6,18)    bottom (6,16)→(10,18)
#     left   (0,18)→(2,26)    front  (2,18)→(6,26)
#     right  (6,18)→(8,26)    back   (8,18)→(12,26)
#
#   RIGHT LEG (4W×8H×2D, origin at 12,16):
#     top    (14,16)→(18,18)  bottom (18,16)→(22,18)
#     left   (12,18)→(14,26)  front  (14,18)→(18,26)
#     right  (18,18)→(20,26)  back   (20,18)→(24,26)

def generate_skin():
    W, H = 64, 32
    img = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)

    def rect(x0, y0, x1, y1, fill):
        d.rectangle([x0, y0, x1 - 1, y1 - 1], fill=fill)

    SKIN  = (210, 170, 120, 255)
    HAIR  = ( 80,  50,  30, 255)
    EYE   = ( 40,  40, 120, 255)
    SHIRT = ( 60,  90, 160, 255)
    PANT  = ( 40,  40,  80, 255)
    BOOT  = ( 50,  35,  20, 255)
    ARM   = (200, 155, 110, 255)

    # HEAD
    rect( 8,  0, 16,  8, HAIR)
    rect(16,  0, 24,  8, SKIN)
    rect( 0,  8,  8, 16, SKIN)
    rect(16,  8, 24, 16, SKIN)
    rect(24,  8, 32, 16, HAIR)
    rect( 8,  8, 16, 16, SKIN)
    d.point([ 9, 11], fill=EYE)
    d.point([14, 11], fill=EYE)

    # TORSO
    rect(34,  0, 38,  2, SHIRT)
    rect(38,  0, 42,  2, SHIRT)
    rect(32,  2, 34,  9, SHIRT)
    rect(34,  2, 38,  9, SHIRT)
    rect(38,  2, 40,  9, SHIRT)
    rect(40,  2, 44,  9, SHIRT)

    # LEFT ARM
    rect(46,  0, 48,  2, ARM)
    rect(48,  0, 50,  2, ARM)
    rect(44,  2, 46,  9, ARM)
    rect(46,  2, 48,  9, ARM)
    rect(48,  2, 50,  9, ARM)
    rect(50,  2, 52,  9, ARM)

    # RIGHT ARM
    rect(54,  0, 56,  2, ARM)
    rect(56,  0, 58,  2, ARM)
    rect(52,  2, 54,  9, ARM)
    rect(54,  2, 56,  9, ARM)
    rect(56,  2, 58,  9, ARM)
    rect(58,  2, 60,  9, ARM)

    # LEFT LEG
    rect( 2, 16,  6, 18, PANT)
    rect( 6, 16, 10, 18, PANT)
    rect( 0, 18,  2, 26, PANT)
    rect( 2, 18,  6, 26, PANT)
    rect( 6, 18,  8, 26, PANT)
    rect( 8, 18, 12, 26, PANT)
    rect( 2, 24,  6, 26, BOOT)
    rect( 6, 24,  8, 26, BOOT)

    # RIGHT LEG
    rect(14, 16, 18, 18, PANT)
    rect(18, 16, 22, 18, PANT)
    rect(12, 18, 14, 26, PANT)
    rect(14, 18, 18, 26, PANT)
    rect(18, 18, 20, 26, PANT)
    rect(20, 18, 24, 26, PANT)
    rect(14, 24, 18, 26, BOOT)
    rect(12, 24, 14, 26, BOOT)

    os.makedirs("assets/skins", exist_ok=True)
    img.save("assets/skins/default.png")
    print(f"Saved assets/skins/default.png  ({W}x{H} px)")


# ── Block texture atlas ───────────────────────────────────────────────────────
# 256x256 RGBA, 16x16 tiles, 16 tiles per row.
# Tile indices match BlockType::texture_id() in src/world/block.rs:
#
#   0  Grass top        9-13  Crack overlays (transparent bg)
#   1  Dirt             14    Sand
#   2  Stone            15    Snow
#   3  Water            16    Copper ore
#   4  Grass side       17    Coal ore
#   5  Log side         18    Iron ore
#   6  Leaves           19    Furnace sides/top
#   7  Log top          20    Furnace front
#   8  Tall grass       21    Lava
#                       22    Cobblestone
#
# Edit this file freely — run it again to regenerate assets/textures/blocks_atlas.png.

def generate_block_atlas():
    ATLAS  = 256
    TILE   = 16
    TPR    = ATLAS // TILE  # tiles per row = 16
    TAU    = math.tau

    # Base [R, G, B] per tile index (used as fill colour before per-tile overrides)
    base_colors = {
         0: (120, 172,  48),   # Grass top
         1: (156, 112,  57),   # Dirt
         2: (128, 128, 128),   # Stone
         3: (255, 255, 255),   # Water (vertex color tints it in-engine)
         4: (156, 112,  57),   # Grass side (dirt base, green stripe added below)
         5: (139,  90,  43),   # Log side
         6: ( 58, 120,  42),   # Leaves
         7: (191, 152,  96),   # Log top
         8: (102, 179,  51),   # Tall grass
        14: (245, 222, 153),   # Sand
        15: (230, 240, 255),   # Snow
        16: (128, 128, 128),   # Copper ore (stone base)
        17: (128, 128, 128),   # Coal ore
        18: (128, 128, 128),   # Iron ore
        19: (100, 100, 100),   # Furnace sides/top
        20: ( 60,  60,  60),   # Furnace front
        21: (200,  60,   0),   # Lava
        22: (112, 112, 112),   # Cobblestone
    }

    pixels = Image.new("RGBA", (ATLAS, ATLAS), (0, 0, 0, 0))
    px_data = pixels.load()

    def put(tile_idx, px, py, r, g, b, a=255):
        tc = tile_idx % TPR
        tr = tile_idx // TPR
        ax = tc * TILE + px
        ay = tr * TILE + py
        px_data[ax, ay] = (r, g, b, a)

    def variation(px, py):
        return ((px ^ py) % 3) * 4 - 4

    def clamp(v):
        return max(0, min(255, v))

    for tile_idx, bc in base_colors.items():
        for py in range(TILE):
            for px_ in range(TILE):
                r, g, b = bc
                a = 255

                t = py / (TILE - 1)

                # ── Tile-specific overrides ───────────────────────────────
                if tile_idx == 3:      # Water: semi-transparent
                    a = 160

                elif tile_idx == 4:    # Grass side: green stripe at top 4 rows
                    if py >= TILE - 4:
                        r, g, b = 120, 172, 48

                elif tile_idx == 5:    # Log side: dark bark streaks on edges
                    if px_ == 0 or px_ == TILE - 1:
                        r, g, b = 100, 60, 25

                elif tile_idx == 7:    # Log top: concentric rings
                    cx = px_ - TILE // 2
                    cy = py  - TILE // 2
                    ring = int(math.sqrt(cx * cx + cy * cy))
                    if ring % 3 == 0:
                        r, g, b = 140, 100, 55

                elif tile_idx == 8:    # Tall grass: blades with transparent gaps
                    blade_cx = [
                        3.5 - t * 2.5,
                        8.0 + t * 0.5,
                        12.5 + t * 2.0,
                    ]
                    in_blade = any(abs(px_ - cx) < 1.1 for cx in blade_cx)
                    if not in_blade:
                        a = 0
                    elif py > TILE * 2 // 3:  # yellowy stem at base
                        r = clamp(int(r * 13 / 10))
                        b = int(b * 6 / 10)

                elif tile_idx == 16:   # Copper ore
                    h = (px_ * 7 ^ py * 13 ^ px_ * py * 3) % 16
                    if h < 3:
                        r, g, b = 184, 115, 51

                elif tile_idx == 17:   # Coal ore
                    h = (px_ * 11 ^ py * 7 ^ px_ * py * 5) % 16
                    if h < 4:
                        r, g, b = 30, 30, 30

                elif tile_idx == 18:   # Iron ore
                    h = (px_ * 9 ^ py * 17 ^ px_ * py * 7) % 16
                    if h < 4:
                        r, g, b = 160, 107, 75

                elif tile_idx == 19:   # Furnace sides: brick mortar pattern
                    row     = py // 5
                    col_off = 0 if row % 2 == 0 else 4
                    lx      = (px_ + col_off) % 8
                    ly      = py % 5
                    if lx == 0 or ly == 0:
                        r, g, b = 55, 55, 55

                elif tile_idx == 20:   # Furnace front: chamber with glow
                    cx = px_ - 8
                    cy = py  - 10
                    in_chamber = abs(cx) <= 4 and -2 <= cy <= 4
                    row     = py // 5
                    col_off = 0 if row % 2 == 0 else 4
                    is_mortar = (px_ + col_off) % 8 == 0 or py % 5 == 0
                    if in_chamber:
                        dist = abs(cx) + abs(cy)
                        r, g, b = (230, 140, 30) if dist <= 2 else (140, 70, 10)
                    elif is_mortar:
                        r, g, b = 55, 55, 55

                elif tile_idx == 21:   # Lava
                    blob = (px_ * 3 + py * 5 ^ px_ * py * 7 ^ (px_ // 3) * 11 ^ (py // 3) * 13) % 24
                    hot  = (px_ * 17 ^ py * 23 ^ (px_ + py) * 31) % 8
                    rock = (px_ * 5 ^ py * 7 ^ (px_ // 4) * 3 ^ (py // 4) * 9) % 10
                    if blob < 6:
                        r, g, b = (140, 20, 0) if rock == 0 else (180, 40, 0)
                    elif blob < 10:
                        r, g, b = 220, 80, 0
                    elif hot == 0:
                        r, g, b = 255, 248, 120
                    elif hot < 3:
                        r, g, b = 255, 210, 30

                elif tile_idx == 22:   # Cobblestone
                    cx    = abs(px_ % 8 - 4)
                    cy    = abs(py  % 6 - 3)
                    crack = (px_ * 5 ^ py * 3 ^ (px_ // 8) * 17 ^ (py // 6) * 13) % 6
                    if cx >= 3 or cy >= 2 or crack == 0:
                        r, g, b = 70, 70, 70

                # ── Per-pixel brightness variation ────────────────────────
                if a > 0:
                    v = variation(px_, py)
                    r = clamp(r + v)
                    g = clamp(g + v)
                    b = clamp(b + v)

                put(tile_idx, px_, py, r, g, b, a)

    # ── Crack overlays (tiles 9–13) ───────────────────────────────────────────
    # Transparent background with dark sine-wave crack lines.
    # Higher stage index = more cracks visible.
    thresholds = [0.10, 0.18, 0.28, 0.40, 0.55]
    for stage, threshold in enumerate(thresholds):
        tile_idx = 9 + stage
        for py in range(TILE):
            for px_ in range(TILE):
                x = px_ / TILE * TAU
                y = py  / TILE * TAU
                v1 = abs(math.sin(x * 2.3 + y * 1.7))
                v2 = abs(math.sin(x * 0.9 - y * 2.1))
                v3 = abs(math.sin(x * 3.1 + y * 0.5))
                if min(v1, v2, v3) < threshold:
                    put(tile_idx, px_, py, 20, 20, 20, 200)

    # ── Wheat tiles 23–27 (one per growth stage) ─────────────────────────────
    # Background is transparent. Stalks are green; stage 4 gets a golden grain head.
    # The cross_vertices UV crop shows only the bottom (height * 16) rows of each tile.
    stage_heights = [0.15, 0.25, 0.40, 0.55, 0.75]
    stalk_cols = [4, 8, 12]
    for stage, h in enumerate(stage_heights):
        tile_idx = 23 + stage
        vis_top = int(TILE * (1.0 - h))   # first visible row (from top)
        plant_h = TILE - vis_top           # visible pixel height

        for py in range(vis_top, TILE):
            for px_ in range(TILE):
                in_stalk = any(px_ == sx for sx in stalk_cols)
                in_leaf  = any(abs(px_ - sx) == 1 for sx in stalk_cols)
                local_y  = TILE - 1 - py  # 0 = bottom of tile

                if in_stalk:
                    if stage == 4 and local_y >= max(0, plant_h - 3):
                        r, g, b = 205, 175, 40   # golden grain head
                    else:
                        r, g, b = 100, 155, 40   # green stem
                elif in_leaf:
                    mid = vis_top + plant_h // 2
                    if mid - 1 <= py <= mid + 1:
                        r, g, b = 70, 130, 30    # leaf node
                    else:
                        continue  # transparent
                else:
                    continue  # transparent

                v = ((px_ ^ py) % 3) * 4 - 4
                put(tile_idx, px_, py, clamp(r + v), clamp(g + v), clamp(b + v))

    os.makedirs("assets/textures", exist_ok=True)
    pixels.save("assets/textures/blocks_atlas.png")
    print(f"Saved assets/textures/blocks_atlas.png  ({ATLAS}x{ATLAS} px, {TILE}x{TILE} tiles)")


# ── Entry point ───────────────────────────────────────────────────────────────
generate_skin()
generate_block_atlas()
