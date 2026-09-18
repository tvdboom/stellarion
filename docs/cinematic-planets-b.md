# Cinematic planet art: gas, water, brown, and gray

Generated on 2026-09-18 with the built-in `image_gen` tool, one generation call per asset. Existing game images were inspected and used as visual references. No CLI/API fallback, resizing, cropping, recoloring, alpha replacement, or other pixel edits were used. Each selected generated PNG was copied unchanged into `assets/images/cinematic/`.

The prompts requested 2048×2048 output; the built-in tool returned **1254×1254 RGBA** for every asset. These native generated dimensions are retained. Existing planet references are 400px across, and the two moon references are about 110px across.

Visual inspection confirmed full detailed circular globes, readable illumination, reference-like palettes, no surrounding scene or text, and distinct variants. All PNGs use 32-bit alpha, transparent corners, and nearly opaque globe centers (alpha 251–253). Approximate globe bounds below were measured using alpha >128, sampling every second pixel; render framing can account for these transparent margins without modifying source art.

| Asset | Approximate globe bounds, pixels |
| --- | --- |
| `assets/images/cinematic/planet gas 1.png` | `20, 18, 1232, 1228` |
| `assets/images/cinematic/planet gas 2.png` | `20, 26, 1232, 1220` |
| `assets/images/cinematic/planet water 1.png` | `18, 16, 1234, 1220` |
| `assets/images/cinematic/planet water 2.png` | `16, 14, 1238, 1222` |
| `assets/images/cinematic/planet brown 1.png` | `86, 78, 1168, 1162` |
| `assets/images/cinematic/planet brown 2.png` | `90, 84, 1164, 1164` |
| `assets/images/cinematic/planet gray 1.png` | `64, 64, 1190, 1188` |
| `assets/images/cinematic/planet gray 2.png` | `94, 94, 1160, 1156` |

## planet gas 1

- Source references: `assets/images/planets/planet7.png`, `assets/images/planets/planet10.png`.
- Generated file: `C:\Users\Mavs\.codex\generated_images\01a0b416-b14f-7761-9eb4-eca7ccf475ec\exec-8cdfcbd3-de00-4df8-a2ee-47b40803e9aa.png`.
- Repository file: `assets/images/cinematic/planet gas 1.png`.
- Dimensions: 1254×1254 RGBA.

Exact prompt:

```text
Use case: stylized-concept. Asset type: high-resolution transparent planet sprite for Stellarion cinematic space combat. Generate one 2048x2048 square PNG with genuine alpha transparency, one complete circular globe centered and occupying 92% of the canvas width, with a modest transparent margin on every side. Highly detailed realistic premium strategy-game planetary rendering, crisp fine structure that remains convincing when displayed large. Upper-left key light, gradual darkening toward lower-right limb, most of the disk remains well lit and its surface fully readable. All surrounding pixels genuinely transparent, including corners. No background, stars, rings, moons, structures, spacecraft, text, watermark, shadow cast onto a backdrop, or checkerboard painted into image. Input images are visual references for the existing game's planet kind, not edit targets. Subject: a cream, ivory and ochre Jupiter-like gas giant resembling the reference globes. Rich layered horizontal atmospheric belts curve naturally with the spherical globe, with fine turbulent cloud filaments, one restrained oval amber storm in the lower-right-middle, subtle pale icy-blue belts between warm brown bands. Entire planet consists of opaque atmospheric clouds, with no ground, continents, rocks or hard surface. No exaggerated glowing rim. Clean realistic globe silhouette.
```

## planet gas 2

- Source references: `assets/images/planets/planet18.png`.
- Generated file: `C:\Users\Mavs\.codex\generated_images\01a0b416-b14f-7761-9eb4-eca7ccf475ec\exec-ad38ff00-a215-4cb0-9a95-3ed5f092351a.png`.
- Repository file: `assets/images/cinematic/planet gas 2.png`.
- Dimensions: 1254×1254 RGBA.

Exact prompt:

```text
Use case: stylized-concept. Asset type: high-resolution transparent planet sprite for Stellarion cinematic space combat. Generate one 2048x2048 square PNG with genuine alpha transparency, one complete circular globe centered and occupying 92% of the canvas width, with a modest transparent margin on every side. Highly detailed realistic premium strategy-game planetary rendering, crisp fine structure that remains convincing when displayed large. Upper-left key light, gradual darkening toward lower-right limb, most of the disk remains well lit and its surface fully readable. All surrounding pixels genuinely transparent, including corners. No background, stars, rings, moons, structures, spacecraft, text, watermark, shadow cast onto a backdrop, or checkerboard painted into image. Input image is a visual reference for the existing game's planet kind, not an edit target. Subject: a blue-green turquoise gas giant resembling the reference globe, clearly distinct from a cream-colored gas giant. Fine horizontal bands in cyan, muted teal and deep blue with soft sweeping atmospheric turbulence and a few subtle lighter oval storms. Entire visible planet consists of opaque atmospheric cloud layers, no water ocean, ground, continents, rocks or hard surface. Subtle thin cyan atmospheric rim, clean realistic globe silhouette.
```

## planet water 1

- Source references: `assets/images/planets/planet25.png`.
- Generated file: `C:\Users\Mavs\.codex\generated_images\01a0b416-b14f-7761-9eb4-eca7ccf475ec\exec-17ed0b68-9c24-435e-80d5-8b9d9a876c56.png`.
- Repository file: `assets/images/cinematic/planet water 1.png`.
- Dimensions: 1254×1254 RGBA.

Exact prompt:

```text
Use case: stylized-concept. Asset type: high-resolution transparent planet sprite for Stellarion cinematic space combat. Generate one 2048x2048 square PNG with genuine alpha transparency, one complete circular globe centered and occupying 92% of the canvas width, with a modest transparent margin on every side. Highly detailed realistic premium strategy-game planetary rendering, crisp fine structure that remains convincing when displayed large. Upper-left key light, gradual darkening toward lower-right limb, most of the disk remains well lit and its surface fully readable. All surrounding pixels genuinely transparent, including corners. No background, stars, rings, moons, structures, spacecraft, text, watermark, shadow cast onto a backdrop, or checkerboard painted into image. Input image is a visual reference for the existing game's water-planet kind, not an edit target. Subject: an ocean world with deep cobalt blue seas almost entirely covering the globe, delicate white cloud spirals and fragmented thin cloud bands following atmospheric circulation. A few small muted green island archipelagos with turquoise shallows are visible through clear regions of the clouds, but no large continents. Rich realistic fine cloud detail, subtle ocean highlights, thin restrained blue atmospheric limb. Keep the world predominantly deep blue and white, matching the reference.
```

## planet water 2

- Source references: `assets/images/planets/planet32.png`, `assets/images/planets/planet52.png`.
- Generated file: `C:\Users\Mavs\.codex\generated_images\01a0b416-b14f-7761-9eb4-eca7ccf475ec\exec-99c7f198-fe86-46b5-bcc0-216048b06a3f.png`.
- Repository file: `assets/images/cinematic/planet water 2.png`.
- Dimensions: 1254×1254 RGBA.

Exact prompt:

```text
Use case: stylized-concept. Asset type: high-resolution transparent planet sprite for Stellarion cinematic space combat. Generate one 2048x2048 square PNG with genuine alpha transparency, one complete circular globe centered and occupying 92% of the canvas width, with a modest transparent margin on every side. Highly detailed realistic premium strategy-game planetary rendering, crisp fine structure that remains convincing when displayed large. Upper-left key light, gradual darkening toward lower-right limb, most of the disk remains well lit and its surface fully readable. All surrounding pixels genuinely transparent, including corners. No background, stars, rings, moons, structures, spacecraft, text, watermark, shadow cast onto a backdrop, or checkerboard painted into image. Input images are visual references for the existing game's water-planet kind, not edit targets. Subject: a distinct inhabited-looking but entirely natural ocean world, sapphire oceans covering around 80% of the visible globe and small irregular sandy-gold and muted olive green islands and narrow separated landmasses around a long turquoise shallow sea. Wispy white clouds partly spiral across the globe with ample clear gaps showing water and detailed rugged coastlines. Do not copy Earth's real continents. Thin restrained blue atmospheric limb, realistic detailed geography and weather, reference-like blue-white-and-muted-tan palette. No city lights.
```

## planet brown 1

- Source references: `assets/images/planets/moon2.png`.
- Generated file: `C:\Users\Mavs\.codex\generated_images\01a0b416-b14f-7761-9eb4-eca7ccf475ec\exec-17d94f7c-958e-4c6a-8ff7-315bd41e5e1c.png`.
- Repository file: `assets/images/cinematic/planet brown 1.png`.
- Dimensions: 1254×1254 RGBA.

Exact prompt:

```text
Use case: stylized-concept. Asset type: high-resolution transparent planet sprite for Stellarion cinematic space combat. Generate one 2048x2048 square PNG with genuine alpha transparency. One complete circular globe, perfectly centered and occupying only 90% of the canvas width so there is clearly visible transparent padding all around. Highly detailed realistic premium strategy-game planetary rendering, crisp fine structure that remains convincing when displayed large. Upper-left key light, gradual darkening toward lower-right limb, most of the disk remains well lit and its surface fully readable. All surrounding pixels genuinely transparent, including corners. No background, stars, rings, moons, structures, spacecraft, text, watermark, shadow cast onto a backdrop, or checkerboard painted into image. Input image is a visual reference for the existing game's brown rocky moon kind, not an edit target. Subject: a large barren brown rocky moon in warm dark umber and desaturated sandstone tones matching the reference, with an intricate ancient cratered crust, overlapping circular impact basins, broad darker brown smooth lava plains and narrow fractured mountain ridges. A restrained patch of pale sunlit chalk-colored ejecta on the upper-left surface. Detailed mineral texture, entirely solid rock, no atmosphere or colored glow, no water, vegetation or clouds. Realistic perfectly spherical globe with clean edge.
```

## planet brown 2

- Source references: `assets/images/planets/moon2.png`.
- Generated file: `C:\Users\Mavs\.codex\generated_images\01a0b416-b14f-7761-9eb4-eca7ccf475ec\exec-43984550-a5b6-4f93-ac79-8f1437d85e97.png`.
- Repository file: `assets/images/cinematic/planet brown 2.png`.
- Dimensions: 1254×1254 RGBA.

Exact prompt:

```text
Use case: stylized-concept. Asset type: high-resolution transparent planet sprite for Stellarion cinematic space combat. Generate one 2048x2048 square PNG with genuine alpha transparency. One complete circular globe, perfectly centered and occupying only 90% of the canvas width so there is clearly visible transparent padding all around. Highly detailed realistic premium strategy-game planetary rendering, crisp fine structure that remains convincing when displayed large. Upper-left key light, gradual darkening toward lower-right limb, most of the disk remains well lit and its surface fully readable. All surrounding pixels genuinely transparent, including corners. No background, stars, rings, moons, structures, spacecraft, text, watermark, shadow cast onto a backdrop, or checkerboard painted into image. Input image is a visual reference for the existing game's brown rocky moon kind, not an edit target. Subject: a distinct variant of a barren brown rocky moon, muted copper-brown and ochre mineral crust with darker umber ancient crater basins, scattered lighter dusty sandy highlands, a prominent large impact basin just right of center and a rugged diagonal canyon system on the lower-left. Fine sharp crater rims, realistic varied crater sizes, granular stony surface. Match the restrained natural brown palette of the reference. Entirely solid rock, no atmosphere or colored glow, no water, vegetation or clouds. Realistic perfectly spherical globe with clean edge.
```

## planet gray 1

- Source references: `assets/images/planets/moon3.png`.
- Generated file: `C:\Users\Mavs\.codex\generated_images\01a0b416-b14f-7761-9eb4-eca7ccf475ec\exec-037f019b-bad8-4a20-a143-5e802c9e7ccd.png`.
- Repository file: `assets/images/cinematic/planet gray 1.png`.
- Dimensions: 1254×1254 RGBA.

Exact prompt:

```text
Use case: stylized-concept. Asset type: high-resolution transparent planet sprite for Stellarion cinematic space combat. Generate one 2048x2048 square PNG with genuine alpha transparency. One complete circular globe, perfectly centered and occupying only 90% of the canvas width so there is clearly visible transparent padding all around. Highly detailed realistic premium strategy-game planetary rendering, crisp fine structure that remains convincing when displayed large. Upper-left key light, gradual darkening toward lower-right limb, most of the disk remains well lit and its surface fully readable. All surrounding pixels genuinely transparent, including corners. No background, stars, rings, moons, structures, spacecraft, text, watermark, shadow cast onto a backdrop, or checkerboard painted into image. Input image is a visual reference for the existing game's gray moon kind, not an edit target. Subject: a silver-gray rocky moon with a faint cool blue mineral cast, bright pale sunlit highlands, broad dark slate basalt plains and many sharply detailed circular impact craters. A large luminous silvery ejecta pattern crosses the upper-left surface, smaller crater chains and rugged ancient broken ridges throughout. Match the reference's cool gray/silver character without depicting an Earth-like blue ocean. Entirely solid cratered rock, no atmosphere glow, water, vegetation or clouds. Realistic perfectly spherical globe with clean edge.
```

## planet gray 2

- Source references: `assets/images/planets/moon3.png`.
- Generated file: `C:\Users\Mavs\.codex\generated_images\01a0b416-b14f-7761-9eb4-eca7ccf475ec\exec-e23687e0-9137-42a1-9d42-a0714e783648.png`.
- Repository file: `assets/images/cinematic/planet gray 2.png`.
- Dimensions: 1254×1254 RGBA.

Exact prompt:

```text
Use case: stylized-concept. Asset type: high-resolution transparent planet sprite for Stellarion cinematic space combat. Generate one 2048x2048 square PNG with genuine alpha transparency. One complete circular globe, perfectly centered and occupying only 90% of the canvas width so there is clearly visible transparent padding all around. Highly detailed realistic premium strategy-game planetary rendering, crisp fine structure that remains convincing when displayed large. Upper-left key light, gradual darkening toward lower-right limb, most of the disk remains well lit and its surface fully readable. All surrounding pixels genuinely transparent, including corners. No background, stars, rings, moons, structures, spacecraft, text, watermark, shadow cast onto a backdrop, or checkerboard painted into image. Input image is a visual reference for the existing game's gray moon kind, not an edit target. Subject: a distinct silver-gray moon variant with cool charcoal and blue-slate rocky plains, lighter chalk-gray crater rims, one massive ancient concentric impact basin in the lower-left quadrant, dense small bright impact craters around the upper hemisphere, and several dark fractured valleys running across the right hemisphere. Highly realistic fine lunar rock texture and clear differences in elevation, softly lit silver-gray highlands. Match the reference's cool gray/silver palette. Entirely solid cratered rock, no atmosphere glow, water, vegetation or clouds. Realistic perfectly spherical globe with clean edge.
```


