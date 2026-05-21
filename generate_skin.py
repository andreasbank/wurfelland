# generate_skin.py — pip install Pillow
# Generates assets/skins/default.png matching the 64x32 skin atlas layout.
#
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

import os
from PIL import Image, ImageDraw

W, H = 64, 32
img = Image.new("RGBA", (W, H), (0, 0, 0, 0))
d = ImageDraw.Draw(img)

# PIL rectangle [x0, y0, x1, y1] draws pixels from (x0,y0) to (x1,y1) INCLUSIVE.
# Atlas rects above use exclusive end coords, so subtract 1 for PIL.
def rect(x0, y0, x1, y1, fill):
    d.rectangle([x0, y0, x1 - 1, y1 - 1], fill=fill)

# Colours
SKIN   = (210, 170, 120, 255)
HAIR   = ( 80,  50,  30, 255)
EYE    = ( 40,  40, 120, 255)
SHIRT  = ( 60,  90, 160, 255)
PANT   = ( 40,  40,  80, 255)
BOOT   = ( 50,  35,  20, 255)
ARM    = (200, 155, 110, 255)

# HEAD
rect( 8,  0, 16,  8, HAIR)   # top
rect(16,  0, 24,  8, SKIN)   # bottom
rect( 0,  8,  8, 16, SKIN)   # right side
rect(16,  8, 24, 16, SKIN)   # left side
rect(24,  8, 32, 16, HAIR)   # back
rect( 8,  8, 16, 16, SKIN)   # front face
d.point([ 9, 11], fill=EYE)  # left eye
d.point([14, 11], fill=EYE)  # right eye

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
rect( 2, 24,  6, 26, BOOT)  # boot stripe front
rect( 6, 24,  8, 26, BOOT)  # boot stripe right

# RIGHT LEG
rect(14, 16, 18, 18, PANT)
rect(18, 16, 22, 18, PANT)
rect(12, 18, 14, 26, PANT)
rect(14, 18, 18, 26, PANT)
rect(18, 18, 20, 26, PANT)
rect(20, 18, 24, 26, PANT)
rect(14, 24, 18, 26, BOOT)  # boot stripe front
rect(12, 24, 14, 26, BOOT)  # boot stripe left

os.makedirs("assets/skins", exist_ok=True)
img.save("assets/skins/default.png")
print(f"Saved assets/skins/default.png  ({W}x{H} px)")
