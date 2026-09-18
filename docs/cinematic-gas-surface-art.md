# Cinematic gas facilities and red/yellow globe art

Generated 2026-09-18 with the built-in `image_gen` tool, one generation per asset. No CLI/API fallback was used. The original outputs were copied byte-for-byte into `assets/images/cinematic/`; no cropping, alpha replacement, resizing, or pixel edits were applied.

## Visual reference review

Reviewed the actual map `assets/images/buildings/gas-development.png` atlas and its use in `src/core/map/details.rs`, as well as the `facilities.png` atlas, all six normal cinematic building sprites, and their six original shop images. Gas buildings retain the recognizable shop silhouettes while adding the game's floating armored platforms, mechanical undersides, and cyan lift jets. The red and yellow moons use `moon4.png` / `moon5.png` and their corresponding small and large detail images as references. Globe lighting comes from upper-left and the full silhouette remains available for renderer framing.

## Output inspection

All ten PNG files are **1254 × 1254**, RGBA, with alpha extrema **0–255**. Building prompts requested 1280 square; planet prompts requested 2048 square, but the built-in tool returned 1254 square for all outputs. No artificial enlargement was applied. Each output was visually inspected for identity, entire visible silhouette, transparent exterior, no baked text/background, and appropriate floating construction or rocky planetary material.

The planet bounds below contain pixels with alpha > 128, using exclusive right/bottom coordinates. They let the renderer frame the same apparent globe diameter without editing the artwork.

| Asset | Opaque bounds (left, top, right, bottom) | Width / canvas | Height / canvas |
| --- | --- | --- | --- |
| planet red 1.png | 50, 52, 1204, 1199 | 0.920255 | 0.914673 |
| planet red 2.png | 38, 42, 1217, 1214 | 0.940191 | 0.934609 |
| planet yellow 1.png | 46, 48, 1208, 1198 | 0.926635 | 0.917065 |
| planet yellow 2.png | 52, 54, 1205, 1194 | 0.919458 | 0.909091 |

## Final prompts and provenance

### cinematic gas metal mine.png

- Final asset: `assets/images/cinematic/cinematic gas metal mine.png`
- Original generated output: `C:/Users/Mavs/.codex/generated_images/01a0b416-eb96-7ce2-ab95-ba39415bfe06/exec-59f3eaed-147a-4f08-8627-4b956833bced.png`
- Referenced images, in order: `assets/images/buildings/gas-development.png`, `assets/images/cinematic/cinematic metal mine.png`.

```text
Use case: stylized-concept. Asset type: transparent isometric science-fiction strategy game building sprite, 1280x1280. Create ONE gas-planet metal mine on a floating industrial platform. References: image1 is the game's gas-development atlas; borrow its hovering armored platform, underside cyan anti-gravity thrusters, worn grey steel and warm amber windows. Image2 is the existing normal metal mine: preserve its recognizable pair of stepped pyramidal processing towers, cyan slatted panels, connecting ore conveyor. Convert the whole facility into a compact hovering platform with hanging intake machinery below and four short cyan lift flames. Camera: isometric three-quarter view from above, matching the reference ground sprite exactly. Clean legible silhouette, a few bold forms, crisp detailed game render, upper-left key light, metallic grey with cyan and small amber lights. Entire object fits inside frame with 6% margin, no cropping. Truly transparent alpha background, including between parts. No ground, no dirt, no concrete land slab, no sky, no planet, no stars, no label, no text, no border, no other buildings. Single asset, not a collage.
```

### cinematic gas crystal mine.png

- Final asset: `assets/images/cinematic/cinematic gas crystal mine.png`
- Original generated output: `C:/Users/Mavs/.codex/generated_images/01a0b416-eb96-7ce2-ab95-ba39415bfe06/exec-ca62ceec-3ed7-4ea7-b8d1-6c8344f50769.png`
- Referenced images, in order: `assets/images/buildings/gas-development.png`, `assets/images/cinematic/cinematic crystal mine.png`, `assets/images/buildings/crystal mine.png`.

```text
Use case: stylized-concept. Asset type: transparent isometric science-fiction strategy game building sprite, 1280x1280. Create ONE gas-planet crystal mine on a floating metal industrial platform. References: image1 is the actual game's gas-development atlas; use that weathered metal floating platform vocabulary, angular steel deck and compact cyan antigravity jets underneath. Image2 is the normal cinematic crystal mine: preserve the tall central drilling/separation mast, crossbeam arms and hanging cables, cyan-blue crystal bins in a metal basin, white-grey worn panels with small amber windows. Image3 shop icon supports crystal-mining identity. Convert the facility into a compact airborne platform with hanging harvesting intake units below the deck, no rock foundation. Camera: matching isometric three-quarter from above. One coherent readable silhouette at small scale; detailed crisp painted 3D game-render finish; upper-left lighting. Entire object inside frame with 6% transparent margin. True transparent alpha background including between cables and supports. No ground, no dirt, no concrete land slab, no sky, no planet, no stars, no text, no labels, no border. Exactly one asset, not collage.
```

### cinematic gas deuterium synthesizer.png

- Final asset: `assets/images/cinematic/cinematic gas deuterium synthesizer.png`
- Original generated output: `C:/Users/Mavs/.codex/generated_images/01a0b416-eb96-7ce2-ab95-ba39415bfe06/exec-eadfcf68-e3b2-4e9f-b391-89dd7f04fc86.png`
- Referenced images, in order: `assets/images/buildings/gas-development.png`, `assets/images/cinematic/cinematic deuterium synthesizer.png`, `assets/images/buildings/deuterium synthesizer.png`.

```text
Use case: stylized-concept. Asset type: transparent isometric science-fiction strategy game building sprite, 1280x1280. Generate ONE gas-planet deuterium synthesizer hovering on a compact metal industrial platform. References: image1 is the actual game's gas-development atlas and defines floating heavy grey steel decks and cyan antigravity lift jets beneath; image2 is the normal cinematic deuterium synthesizer, preserve its distinctive tapered bell-like armored reactor tower with turquoise-glowing vertical tubes and piping, image3 shop image supports the same architecture. Gas adaptation has a suspended deep atmospheric intake funnel and hanging gas collection cylinders beneath the platform, compact cyan lift jets at corners. No solid terrain: entire building rests on a self-contained floating metal deck. Isometric three-quarter view from above, matching existing sprite; upper-left lighting, crisp worn silver-grey metal, turquoise deuterium glow and tiny amber utility lights, few bold coherent forms readable at small size. Entire object fully visible with 6% transparent margin. Real transparent alpha background, no ground, no sky, no planet, no stars, no text or label or border. Exactly one sprite, not a grid.
```

### cinematic gas shipyard.png

- Final asset: `assets/images/cinematic/cinematic gas shipyard.png`
- Original generated output: `C:/Users/Mavs/.codex/generated_images/01a0b416-eb96-7ce2-ab95-ba39415bfe06/exec-639fd47b-2682-49d8-9569-7c860fbfd60c.png`
- Referenced images, in order: `assets/images/buildings/gas-development.png`, `assets/images/cinematic/cinematic shipyard.png`, `assets/images/buildings/shipyard.png`.

```text
Use case: stylized-concept. Asset type: transparent isometric sci-fi game building sprite, 1280x1280. Create ONE gas-planet shipyard hovering on a metal platform. Reference1 actual gas-development atlas TOP RIGHT cell is especially important: floating ship construction deck with large gantry, cyan welding lamps and cyan lift jets below. Reference2 normal cinematic shipyard: preserve its two large open arch-shaped steel gantries and central open runway/ship construction bay. Reference3 shop image shows shipyard interior style. Make one compact coherent hovering shipyard with two open vaulted assembly hoops above the deck, amber lit workshops along its side, small yellow crane, subtle cyan welding lights, and mechanical underside with four cyan antigravity jets. The central construction berth should be open and readable. No separate large ship, no free-standing extra buildings. Camera isometric three-quarter from above matching existing sprites; worn silver-grey hard metal, upper-left light, crisp detailed game render. Entire silhouette visible with 6% transparent margin. True transparent alpha background including through open gantries. No ground, land, dirt, concrete land slab, sky, planet, stars, text, labels, border. Single isolated asset.
```

### cinematic gas factory.png

- Final asset: `assets/images/cinematic/cinematic gas factory.png`
- Original generated output: `C:/Users/Mavs/.codex/generated_images/01a0b416-eb96-7ce2-ab95-ba39415bfe06/exec-dd734d61-b2c7-4c9f-b288-4fd139ed772d.png`
- Referenced images, in order: `assets/images/buildings/gas-development.png`, `assets/images/cinematic/cinematic factory.png`, `assets/images/buildings/factory.png`.

```text
Use case: stylized-concept. Asset type: transparent isometric sci-fi strategy game building sprite, 1280x1280. Generate ONE gas-planet factory as a compact industrial facility supported by a hovering steel platform. Reference1 actual game gas-development atlas provides the floating grey armored deck, small amber-lit windows, cyan lift jets and underhanging machinery vocabulary. Reference2 existing normal cinematic factory provides the building's identity: two parallel rounded industrial halls with repeated ribbed vented roofs, thick linking pipes, central production line, several slim chimneys. Reference3 shop image supports that heavy factory architecture. Keep those recognizable forms, perched on a compact angular hovering platform with four short cyan antigravity jets and hanging pipes/tanks below. No terrain or foundation slab. Isometric three-quarter view from above, crisp detailed game render matching existing assets, silver-grey weathered steel, warm amber production lights and restrained cyan utility lights, upper-left lighting. Whole single object visible with 6% transparent margins. Truly transparent alpha background including gaps. No sky, planet, stars, dirt, land, concrete ground, label, letters, text, border, collage.
```

### cinematic gas missile silo.png

- Final asset: `assets/images/cinematic/cinematic gas missile silo.png`
- Original generated output: `C:/Users/Mavs/.codex/generated_images/01a0b416-eb96-7ce2-ab95-ba39415bfe06/exec-d38f3ea3-57af-4514-9465-171b2a3390fd.png`
- Referenced images, in order: `assets/images/buildings/gas-development.png`, `assets/images/cinematic/cinematic missile silo.png`, `assets/images/buildings/missile silo.png`.

```text
Use case: stylized-concept. Asset type: transparent isometric science-fiction strategy game building sprite, 1280x1280. Generate ONE gas-planet missile silo on a hovering military-industrial metal platform. Reference1 actual gas-development atlas: especially bottom-left cell, a floating launch platform with missile tubes, heavy armored underside and lift jets. Reference2 normal cinematic missile silo: preserve the circular stepped armored tower and large round launch hatch with amber vertical lights, reference3 shop supports a protruding upright missile. Combine recognizable circular armored silo into a compact floating platform with ONE visible raised red-white missile through a partially open central hatch, two closed smaller hatches, cyan antigravity jets below and hanging machinery. No missile launching, no exhaust trail above. Match isometric three-quarter view from above and worn silver-grey steel material, warm amber windows plus cyan lift glow, upper-left lighting, crisp detailed game sprite. One coherent silhouette, entire object in frame with 6% transparent margin. True transparent alpha background. No ground, dirt, land, concrete terrain, sky, planet, stars, text, label, border, collage.
```

### planet red 1.png

- Final asset: `assets/images/cinematic/planet red 1.png`
- Original generated output: `C:/Users/Mavs/.codex/generated_images/01a0b416-eb96-7ce2-ab95-ba39415bfe06/exec-4f7c6627-91de-434d-85d4-40e84db56461.png`
- Referenced images, in order: `assets/images/planets/moon4.png`, `assets/images/planets/red large.png`.

```text
Use case: stylized-concept. Asset type: high-resolution cinematic planet sprite for a science-fiction strategy game, requested 2048x2048. Generate ONE full perfectly circular red rocky moon/planet on a truly transparent background. Reference1 moon4 is the existing tiny map globe: preserve its recognizable muted rusty red-brown and dark plum mineral coloration with pale grey/lavender cratered highlands. Reference2 red large provides the detailed dry pitted rock material. Build a substantially more detailed crisp globe, with large old impact basins, finer craters, rough mineral ridges and subtle dusty dark red plains. Upper-left hemisphere lit by soft white sunlight, smoothly shaded lower-right limb, keep night-side rock discernible. Nearly full disc, entire clean spherical outline visible with modest 5% transparent margin. Full globe occupies about90% of square frame, centered, no clipping. Realistic detailed space-game painting, no exaggerated lava, no atmosphere glow, no continents resembling Earth. No stars, galaxy, sky, black background, ring, moons, ships, structures, labels, text or border. Alpha transparent everywhere outside sphere. Variant1 has one broad old dark basin lower-left and clustered crater highlands upper-right.
```

### planet red 2.png

- Final asset: `assets/images/cinematic/planet red 2.png`
- Original generated output: `C:/Users/Mavs/.codex/generated_images/01a0b416-eb96-7ce2-ab95-ba39415bfe06/exec-f443ac63-1955-4fd2-844d-900d48cbffbd.png`
- Referenced images, in order: `assets/images/planets/moon4.png`, `assets/images/planets/red large.png`.

```text
Use case: stylized-concept. Asset type: high-resolution cinematic planet sprite for science-fiction strategy game, requested 2048x2048. Generate ONE full perfectly circular red rocky moon/planet with truly transparent background. Reference1 moon4 tiny map globe defines a dry red-brown and muted dark plum rocky world with pale grey-lavender mineral highlands; reference2 red large defines fine cratered rock texture. Create variant2, visually distinct terrain arrangement: a diagonal pale lavender-grey fractured highland belt from upper-left to lower-middle, rich rusty red dusty plains to right, a broad overlapping cluster of ancient impact basins in upper-right. Crisp detailed craters of many sizes, subtle fault ridges, dry solid airless rock, no lava or clouds. Sphere centered, entire circular globe visible occupying about90% of square canvas with modest5% transparent margin. Soft white sunlight from upper-left, shaded lower-right limb with discernible terrain. Rich physically plausible game art details suitable for large on-screen planet, clean geometric outline. Transparent alpha outside sphere. No background black or otherwise, no stars, nebula, sky, rings, other celestial bodies, structures, text, labels, border, collage.
```

### planet yellow 1.png

- Final asset: `assets/images/cinematic/planet yellow 1.png`
- Original generated output: `C:/Users/Mavs/.codex/generated_images/01a0b416-eb96-7ce2-ab95-ba39415bfe06/exec-013131cb-4cf7-471f-8493-3a101a6ac650.png`
- Referenced images, in order: `assets/images/planets/moon5.png`, `assets/images/planets/yellow large.png`.

```text
Use case: stylized-concept. Asset type: high-resolution cinematic planet sprite for a science-fiction strategy game, requested2048x2048. Generate ONE full perfectly circular yellow-ochre rocky moon/planet on truly transparent alpha background. Reference1 moon5 is the existing tiny game globe: preserve its muted golden ochre, sandy yellow-brown and pale cream cratered appearance. Reference2 yellow large gives its finely pitted solid rock, old craters, dry mineral surface. Variant1: pale ivory-yellow ancient highlands covering the upper-left half, dark ochre rugged basins across lower-right, several broad circular impacts and many subtle small craters; natural rock, not molten, no oceans or gas cloud bands. Highly detailed crisp but coherent terrain for large-screen cinematic viewing. Entire circular sphere centered occupying roughly90% of square frame, about5% transparent margin, no clipped edges. Soft white upper-left sunlight with lower-right limb darker but discernible. Realistic space-game illustration. Alpha transparent outside sphere. No stars, nebula, background, sky, rings, second moon, structures, ships, text, labels, border or collage.
```

### planet yellow 2.png

- Final asset: `assets/images/cinematic/planet yellow 2.png`
- Original generated output: `C:/Users/Mavs/.codex/generated_images/01a0b416-eb96-7ce2-ab95-ba39415bfe06/exec-14481e14-8e58-4c24-8931-7e9bc451a4d4.png`
- Referenced images, in order: `assets/images/planets/moon5.png`, `assets/images/planets/yellow large.png`.

```text
Use case: stylized-concept. Asset type: high-resolution cinematic planet sprite for sci-fi strategy game, requested2048x2048. Generate ONE whole perfectly circular yellow rocky moon/planet on truly transparent alpha. Reference1 moon5 defines game identity: ochre-gold, dusty yellow-brown and pale cream dry cratered rock. Reference2 yellow large shows material and crater shapes. This is distinct variant2: central warm mustard-gold smoother plain, a pair of broad dark ancient impact basins near the upper-left, pale chalk-yellow scarps sweeping around lower-left to bottom, subtle small crater chains through the right hemisphere. Airless solid rocky sphere, no gas bands, oceans, vegetation, glowing lava or cloud layers. Richly detailed crisp realistic space-game illustration. Soft upper-left sunlight and darker lower-right limb, night-side faintly visible. Clean full circular globe centered, about90% of square with5% transparent margin, no clipped edge. Real alpha transparency outside the sphere. No starfield, galaxy, sky, background, rings, moons, structures, text, labels, border, collage.
```

