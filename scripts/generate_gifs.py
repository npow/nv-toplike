#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0

import json
import os
import glob
from PIL import Image, ImageDraw, ImageFont

FONT_REG_PATH = "/usr/share/fonts/truetype/jetbrains-mono/JetBrainsMonoNL-Regular.ttf"
FONT_BOLD_PATH = "/usr/share/fonts/truetype/jetbrains-mono/JetBrainsMonoNL-Bold.ttf"
FONT_SIZE = 15

font_reg = ImageFont.truetype(FONT_REG_PATH, FONT_SIZE)
font_bold = ImageFont.truetype(FONT_BOLD_PATH, FONT_SIZE)

# Calculate character dimensions
bbox = font_reg.getbbox("M")
char_w = 9
char_h = 18
pad_x = 16
pad_y = 16

views = ["overview", "constellation", "memory", "fabric", "fleet"]

os.makedirs("assets", exist_ok=True)

for view in views:
    frame_files = sorted(glob.glob(f"target/frames/{view}/frame_*.json"))
    if not frame_files:
        print(f"No frames found for {view}")
        continue

    images = []
    print(f"Generating GIF and PNG for {view} ({len(frame_files)} frames)...")

    for i, frame_file in enumerate(frame_files):
        with open(frame_file, "r") as f:
            data = json.load(f)

        cols = data["width"]
        rows = data["height"]

        img_w = cols * char_w + pad_x * 2
        img_h = rows * char_h + pad_y * 2

        img = Image.new("RGB", (img_w, img_h), (13, 17, 23)) # Sleek GitHub dark background
        draw = ImageDraw.Draw(img)

        # Draw cells
        for r_idx, row in enumerate(data["cells"]):
            y = pad_y + r_idx * char_h
            for c_idx, cell in enumerate(row):
                x = pad_x + c_idx * char_w
                sym = cell["symbol"]
                if not sym or sym == " ":
                    continue

                fg = tuple(cell["fg"])
                font = font_bold if cell.get("bold", False) else font_reg
                draw.text((x, y), sym, fill=fg, font=font)

        images.append(img)
        if i == 0:
            # Save first frame as static PNG screenshot
            png_path = f"assets/{view}.png"
            img.save(png_path, "PNG", optimize=True)
            print(f"  Saved screenshot -> {png_path}")

    # Save animated GIF
    gif_path = f"assets/{view}.gif"
    images[0].save(
        gif_path,
        save_all=True,
        append_images=images[1:],
        duration=100, # 10 FPS
        loop=0,
        optimize=True,
    )
    print(f"  Saved animated GIF -> {gif_path}")

print("\nAll assets generated successfully in assets/!")
