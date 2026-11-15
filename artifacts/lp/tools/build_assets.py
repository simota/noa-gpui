"""Builds site/assets from real noa-gpui renders.

Inputs (see artifacts/lp/README.md for how to produce them):
  SNAPS/{catalog,minios}.png  - `noa-demo --scene=X --snapshot=...` (2560x1600)
  THEMES/*.png                         - tools/themeshot.rs output (760x660 each)
Run: python3 build_assets.py SNAPS THEMES REPO
"""
import sys, glob, os
from PIL import Image

snaps, themes, repo = sys.argv[1:4]
out = os.path.join(repo, "site", "assets")

def webp(im, path, width, q=84):
    if im.width != width:
        im = im.resize((width, round(im.height * width / im.width)), Image.LANCZOS)
    im.save(path, "WEBP", quality=q, method=6)
    print(path, im.size, os.path.getsize(path) // 1024, "KB")

minios = Image.open(f"{snaps}/minios.png").convert("RGB")
crop = minios.crop((0, 0, 1500, 1000))      # menu bar, Finder and Notes windows, some desktop
for w in (1500, 760):
    webp(crop, f"{out}/minios-{w}.webp", w)

catalog = Image.open(f"{snaps}/catalog.png").convert("RGB")
cat = catalog.crop((0, 0, 2560, 1500))
for w in (1600, 800):
    webp(cat, f"{out}/catalog-{w}.webp", w)
# Phones: the navigation and the first cards at a readable scale.
webp(catalog.crop((0, 0, 1240, 1240)), f"{out}/catalog-phone-760.webp", 760)

for f in sorted(glob.glob(f"{themes}/*.png")):
    name = os.path.basename(f)[:-4].replace("-+-", "-")
    webp(Image.open(f).convert("RGB"), f"{out}/themes/{name}.webp", 760, q=86)
