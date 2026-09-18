# Cinematic planet art — Dry, Ice, Metallic, and Blue

Generated with the built-in `image_gen` tool on 2026-09-18, one separate call per asset. All eight outputs were copied unchanged into `assets/images/cinematic/`; no cropping, resizing, alpha cleanup, or other pixel edits were performed. The original generated outputs remain in the generator directory.

Each prompt requested 2048 × 2048. The built-in tool returned **1254 × 1254 RGBA** for every asset. Read-only System.Drawing inspection confirmed `Format32bppArgb`, alpha 0 at the upper-left corner and alpha 252 at the center for all eight. This is over three times the 400-pixel map planet resolution and substantially larger than the 109-pixel Blue moon sprite.

## References inspected

These project images were inspected with `view_image` as style and planetary-kind references, not supplied as edit targets. Prompts describe their identity and palette; all generated outputs are new assets.

- Dry: `assets/images/planets/planet2.png`, `planet9.png`, and `dry large.png`.
- Ice: `assets/images/planets/planet1.png`, `planet22.png`, and `ice large.png`.
- Metallic: `assets/images/planets/planet53.png`, `planet61.png`, and `metallic large.png`.
- Blue: `assets/images/planets/moon1.png` and `blue large.png`.

## Visual inspection

All outputs show complete circular worlds, readable terrain across the face, light from the upper left, and transparent surroundings. Dry variants use golden sand and terracotta canyons; Ice variants use white frozen terrain and a blue frozen basin; Metallic variants use pewter/bronze and graphite/gold minerals; Blue variants use slate-cyan and indigo-violet cratered rock. No rings, buildings, stars, background scenes, or text are present. Natural shading remains part of the sprite.

## Exact prompts and saved outputs

Read-only center-line silhouette measurements at alpha ≥128:

| Asset | Horizontal bounds | Vertical bounds | Diameter / canvas (x, y) |
| --- | --- | --- | --- |
| Dry 1 | 19–1235 | 20–1228 | 97.05%, 96.41% |
| Dry 2 | 7–1246 | 7–1237 | 98.88%, 98.17% |
| Ice 1 | 29–1223 | 34–1219 | 95.30%, 94.58% |
| Ice 2 | 13–1240 | 14–1224 | 97.93%, 96.57% |
| Metallic 1 | 32–1221 | 25–1215 | 94.90%, 94.98% |
| Metallic 2 | 15–1240 | 11–1228 | 97.77%, 97.13% |
| Blue 1 | 28–1227 | 27–1223 | 95.69%, 95.45% |
| Blue 2 | 20–1234 | 18–1228 | 96.89%, 96.57% |

Each source and workspace file was SHA-256 compared after copying; all eight match.

### planet dry 1

- Workspace asset: `assets/images/cinematic/planet dry 1.png`
- Generated source: `C:\Users\Mavs\.codex\generated_images\01a0b416-84c7-71d0-a804-9931324bde86\exec-e9ad3b48-3771-432e-b969-7ffe47c13c99.png`
- Dimensions: 1254 × 1254; RGBA.

```text
Use case: stylized-concept. Asset type: high-resolution transparent globe sprite for Stellarion cinematic combat. Create ONE 2048x2048 square PNG, a single complete physically round planet centered and occupying 94% of the frame diameter, with genuinely transparent alpha outside its rim and a small even clear margin. Richly detailed realistic painted 3D strategy-game illustration; crisp clean terrain that holds up at a large scale, not noise or pixelation. Lighting is broad and soft from upper-left, showing detail across almost the entire globe, shading gradually to a darker lower-right limb; do not make a night crescent. No stars, space background, rings, moons, buildings, icons, labels, text, watermark, or other objects. No checkerboard painted into image. No halo outside the narrow natural rim. Silhouette must remain a full circle, not an ellipse. Subject: DRY rocky desert planet, variant 1. Resemble the game's existing golden ochre desert globes: warm sand-gold continents, broad sienna desert basins, weathered tan rocky highlands, ancient circular impact basins and long naturally branching dry canyons. Several broad soft dune belts give interesting large forms. No oceans, vegetation, cloud blanket, or glowing lava. Subtle fine detailed rock and sand surface, thin warm rim only. Natural geological world, not a smooth gas planet.
```

### planet dry 2

- Workspace asset: `assets/images/cinematic/planet dry 2.png`
- Generated source: `C:\Users\Mavs\.codex\generated_images\01a0b416-84c7-71d0-a804-9931324bde86\exec-9f216b96-70bb-4ceb-90e4-ef9a88b6dd53.png`
- Dimensions: 1254 × 1254; RGBA.

```text
Use case: stylized-concept. Asset type: high-resolution transparent globe sprite for Stellarion cinematic combat. Create ONE 2048x2048 square PNG, a single complete physically round planet centered and occupying 94% of the frame diameter, with genuinely transparent alpha outside its rim and a small even clear margin. Richly detailed realistic painted 3D strategy-game illustration; crisp clean terrain that holds up at a large scale, not noise or pixelation. Lighting is broad and soft from upper-left, showing detail across almost the entire globe, shading gradually to a darker lower-right limb; do not make a night crescent. No stars, space background, rings, moons, buildings, icons, labels, text, watermark, or other objects. No checkerboard painted into image. No halo outside the narrow natural rim. Silhouette must remain a full circle, not an ellipse. Subject: DRY rocky desert planet, variant 2, a different geography from variant 1. Resemble the game's rust-and-tan desert globes: weathered terracotta plains, pale sandstone crater rims, dark copper-brown exposed rock plates, a large old off-center crater with eroded ejecta and several smaller impacts, branching canyon scars. Low delicate windblown dust streaks across the surface without obscuring terrain. No oceans, vegetation, cloud blanket, or glowing lava. Natural geological world, not a smooth gas planet.
```

### planet ice 1

- Workspace asset: `assets/images/cinematic/planet ice 1.png`
- Generated source: `C:\Users\Mavs\.codex\generated_images\01a0b416-84c7-71d0-a804-9931324bde86\exec-9e4ee1ac-711f-4565-bbaa-29a0f6ba34f9.png`
- Dimensions: 1254 × 1254; RGBA.

```text
Use case: stylized-concept. Asset type: high-resolution transparent globe sprite for Stellarion cinematic combat. Create ONE 2048x2048 square PNG, a single complete physically round planet centered and occupying 92% of the frame diameter, with genuinely transparent alpha outside its rim and a small even clear margin. Richly detailed realistic painted 3D strategy-game illustration; crisp clean terrain that holds up at a large scale, not noise or pixelation. Lighting is broad and soft from upper-left, showing detail across almost the entire globe, shading gradually to a darker lower-right limb; do not make a night crescent. No stars, space background, rings, moons, buildings, icons, labels, text, watermark, or other objects. No checkerboard painted into image. No halo outside the narrow natural rim. Silhouette must remain a full circle, not an ellipse. Subject: ICE planet, variant 1. Preserve the game's pale white and icy gray-blue frozen planetary appearance. Entire surface covered in ivory-white glaciers and powder-blue frozen basins, intricate but restrained natural ice fractures, several wide ancient impact basins with snowy rims, long windswept glacial ridges. Calm broad frozen plains balance fine terrain texture. Crystalline pale cyan highlights in deep glacial fissures, no liquid ocean and no green or brown continents. Subtle blue atmospheric rim, no giant storm clouds. Distinct from a desert and a water world.
```

### planet ice 2

- Workspace asset: `assets/images/cinematic/planet ice 2.png`
- Generated source: `C:\Users\Mavs\.codex\generated_images\01a0b416-84c7-71d0-a804-9931324bde86\exec-83cad71a-778a-4528-befe-a5175eaa1d45.png`
- Dimensions: 1254 × 1254; RGBA.

```text
Use case: stylized-concept. Asset type: high-resolution transparent globe sprite for Stellarion cinematic combat. Create ONE 2048x2048 square PNG, a single complete physically round planet centered and occupying 92% of the frame diameter, with genuinely transparent alpha outside its rim and a small even clear margin. Richly detailed realistic painted 3D strategy-game illustration; crisp clean terrain that holds up at a large scale, not noise or pixelation. Lighting is broad and soft from upper-left, showing detail across almost the entire globe, shading gradually to a darker lower-right limb; do not make a night crescent. No stars, space background, rings, moons, buildings, icons, labels, text, watermark, or other objects. No checkerboard painted into image. No halo outside the narrow natural rim. Silhouette must remain a full circle, not an ellipse. Subject: ICE planet, variant 2, distinct geography and more blue tone. Preserve the game's turquoise frozen-world appearance: a huge smooth turquoise-blue frozen central basin bounded by jagged white glacial highlands, frost-covered irregular continental ice plates, intersecting delicate white cracks and scattered frosted craters. Frozen surface has depth and crystalline texture, visible pale aquamarine beneath solid ice sheets. Clean blue-white palette. No liquid ocean, forests, city lights, cloud blanket, or exaggerated glowing cracks. Very thin cold blue atmospheric rim.
```

### planet metallic 1

- Workspace asset: `assets/images/cinematic/planet metallic 1.png`
- Generated source: `C:\Users\Mavs\.codex\generated_images\01a0b416-84c7-71d0-a804-9931324bde86\exec-f8e5caca-7b61-400b-b8f8-f7ef8f7ff85d.png`
- Dimensions: 1254 × 1254; RGBA.

```text
Use case: stylized-concept. Asset type: high-resolution transparent globe sprite for Stellarion cinematic combat. Create ONE 2048x2048 square PNG, a single complete physically round planet centered and occupying 90% of the frame diameter, with genuinely transparent alpha outside its rim and a small even clear margin. Richly detailed realistic painted 3D strategy-game illustration; crisp clean terrain that holds up at a large scale, not noise or pixelation. Lighting is broad and soft from upper-left, showing detail across almost the entire globe, shading gradually to a darker lower-right limb; do not make a night crescent. No stars, space background, rings, moons, buildings, icons, labels, text, watermark, or other objects. No checkerboard painted into image. No halo outside the narrow natural rim. Silhouette must remain a full circle, not an ellipse. Subject: METALLIC mineral-rich rocky planet, variant 1. Preserve the game's pewter, warm bronze and stony mineral-rich globe identity. Natural bare planetary crust with silver-gray iron-rich highlands, muted bronze-tan dry basins, dark charcoal impact scars and long exposed metallic mineral veins. The metallic character is subtle mineral sheen on ridges under the light, not a chrome ball, artificial machine, plated shell, grid, or manufactured object. Broad weathered crater basins and rugged folded ridges, high geological realism, no clouds, water, forests or lava.
```

### planet metallic 2

- Workspace asset: `assets/images/cinematic/planet metallic 2.png`
- Generated source: `C:\Users\Mavs\.codex\generated_images\01a0b416-84c7-71d0-a804-9931324bde86\exec-05cf71f6-a580-462e-88f9-003825f583a0.png`
- Dimensions: 1254 × 1254; RGBA.

```text
Use case: stylized-concept. Asset type: high-resolution transparent globe sprite for Stellarion cinematic combat. Create ONE 2048x2048 square PNG, a single complete physically round planet centered and occupying 90% of the frame diameter, with genuinely transparent alpha outside its rim and a small even clear margin. Richly detailed realistic painted 3D strategy-game illustration; crisp clean terrain that holds up at a large scale, not noise or pixelation. Lighting is broad and soft from upper-left, showing detail across almost the entire globe, shading gradually to a darker lower-right limb; do not make a night crescent. No stars, space background, rings, moons, buildings, icons, labels, text, watermark, or other objects. No checkerboard painted into image. No halo outside the narrow natural rim. Silhouette must remain a full circle, not an ellipse. Subject: METALLIC mineral-rich rocky planet, variant 2, clearly different geography and contrast. Preserve the game's dark black-and-gold mineral world appearance: broad irregular obsidian and graphite rocky plains divided by weathered antique-gold and copper mineral-rich ridges and impact rims. Some dark circular ancient crater basins, fractured natural ochre plateau regions. Restrained realistic mineral reflection, not glowing gold. Natural geological world, not artificial plates, manufactured shell, city, circuit board, polished chrome or mechanical planet. No clouds, water, forests or lava.
```

### planet blue 1

- Workspace asset: `assets/images/cinematic/planet blue 1.png`
- Generated source: `C:\Users\Mavs\.codex\generated_images\01a0b416-84c7-71d0-a804-9931324bde86\exec-aeaf6672-d67f-4a5d-8ec5-bda75169b2ba.png`
- Dimensions: 1254 × 1254; RGBA.

```text
Use case: stylized-concept. Asset type: high-resolution transparent globe sprite for Stellarion cinematic combat. Create ONE 2048x2048 square PNG, a single complete physically round small rocky world centered and occupying 90% of the frame diameter, with genuinely transparent alpha outside its rim and a small even clear margin. Richly detailed realistic painted 3D strategy-game illustration; crisp clean terrain that holds up at a large scale, not noise or pixelation. Lighting is broad and soft from upper-left, showing detail across almost the entire globe, shading gradually to a darker lower-right limb; do not make a night crescent. No stars, space background, rings, other moons, buildings, icons, labels, text, watermark, or other objects. No checkerboard painted into image. No halo outside the narrow natural rim. Silhouette must remain a full circle, not an ellipse. Subject: BLUE mineral-rich cratered moon, variant 1. Preserve the game's existing blue moon identity: muted slate-blue and steel-cyan bare rock, darker navy-violet impact basins and scattered circular craters, bright cool silver-blue crater rims. Long rugged rocky ridges separate smoother basaltic plains, delicate naturally branching mineral fractures. This is an airless blue rocky moon, not a blue ocean planet: no water, ice sheets, vegetation, clouds, glowing fissures, artificial plates or buildings. Terrain shape and lighting should look physically plausible with restrained cool mineral color.
```

### planet blue 2

- Workspace asset: `assets/images/cinematic/planet blue 2.png`
- Generated source: `C:\Users\Mavs\.codex\generated_images\01a0b416-84c7-71d0-a804-9931324bde86\exec-ea2d0065-bc32-44dd-afa7-eef60e539865.png`
- Dimensions: 1254 × 1254; RGBA.

```text
Use case: stylized-concept. Asset type: high-resolution transparent globe sprite for Stellarion cinematic combat. Create ONE 2048x2048 square PNG, a single complete physically round small rocky world centered and occupying 90% of the frame diameter, with genuinely transparent alpha outside its rim and a small even clear margin. Richly detailed realistic painted 3D strategy-game illustration; crisp clean terrain that holds up at a large scale, not noise or pixelation. Lighting is broad and soft from upper-left, showing detail across almost the entire globe, shading gradually to a darker lower-right limb; do not make a night crescent. No stars, space background, rings, other moons, buildings, icons, labels, text, watermark, or other objects. No checkerboard painted into image. No halo outside the narrow natural rim. Silhouette must remain a full circle, not an ellipse. Subject: BLUE mineral-rich cratered moon, variant 2, distinct geography. Preserve the game's blue-violet rocky moon identity: dusky indigo and cool blue-gray basalt, large dark violet ancient crater basin near lower-left, smaller sharply defined pale blue impact craters across the upper hemisphere, long diagonal uplifted ridge and natural jagged fracture canyons. More violet shadows and less cyan than variant 1, but still recognizable blue moon. No oceans, ice sheets, clouds, vegetation, glowing fissures, artificial plates or buildings. Natural airless rocky geology with restrained mineral color.
```
