# Cinematic industrial building artwork

Generated on 18 September 2026 with the built-in `image_gen.imagegen` tool, one
reference-based request per building. The existing shop PNGs and cinematic Rocket
Launcher were visually inspected before generation. Image 1 supplied the building
identity; Image 2 supplied only the established cinematic metal-rendering style.
No third-party artwork or API/CLI fallback was used.

The three outputs were visually inspected, then copied byte-for-byte into
`assets/images/cinematic/`. No cropping, background removal, recoloring, alpha
modification or other pixel editing was applied. SHA-256 hashes of each generated
original and saved project PNG matched. All three are 1254 × 1254 RGBA PNGs with
actual transparent pixels, not painted black backgrounds. Shop references remain
unchanged. Runtime lighting, smoke and damage animate these clean source sprites.

| Building | Shop reference | Saved sprite | Fully transparent pixels |
| --- | --- | --- | ---: |
| Shipyard | `assets/images/buildings/shipyard.png` | `assets/images/cinematic/cinematic shipyard.png` | 836950 |
| Factory | `assets/images/buildings/factory.png` | `assets/images/cinematic/cinematic factory.png` | 839518 |
| Missile Silo | `assets/images/buildings/missile silo.png` | `assets/images/cinematic/cinematic missile silo.png` | 941878 |

The shared style reference was
`assets/images/cinematic/cinematic rocket launcher.png`.
Generated originals were retained under
`C:/Users/Mavs/.codex/generated_images/01a0b375-e4c7-7a12-9b86-c4f41d557a21/`.

## Exact prompts and provenance

### Shipyard

Generated original: `exec-751b7920-3f33-472a-86a2-b7205166c433.png`.

SHA-256: `2BC2F127AE6C678BABD26FB00988D396234BB1583A034A22340EFE4131F75907`.

> Use case: stylized-concept. Asset type: one production-ready transparent isometric planetary building sprite for the Stellarion cinematic combat renderer. Input image 1 is the SHIPYARD shop-art visual reference; preserve its recognizable industrial ship-construction hangar identity: huge ribbed arched gantry roof, long central assembly dock, layered service decks and narrow metal catwalks, cranes/work platforms, charcoal steel with restrained warm amber work lights and small cool cyan equipment lights. Input image 2 is style reference ONLY: match its detailed realistic painterly sci-fi metal rendering, readable armored edges and clean isolated cutout quality; do not add its weapons. Turn the shop's interior view into ONE complete compact exterior planetary shipyard facility, viewed from about 30 degrees above in three-quarter isometric perspective. Show a partially open ribbed barrel-vault roof so the dock and industrial machinery can be read from above. Entire building and compact manufactured foundation fully visible, centered, clear 10% transparent margin on all sides. One terrain-free compact industrial foundation, no landscape or extra detached objects. Detailed gunmetal steel with subtly weathered panels; crisp silhouette legible at small game size. TRUE TRANSPARENT ALPHA background, empty alpha outside the building; no black/white/color backdrop, no scenery, stars, planet, floor, checkerboard artwork, labels, text, UI, watermark or frame. NO ships, no weapons or missiles, no explosions, no fire, no smoke, no exhaust trails or big glows. Keep lights small and restrained because the game adds animated lights, smoke and damage at runtime. A single clean unanimated source sprite, not a spritesheet. Deliver one square PNG with actual transparency.

### Factory

Generated original: `exec-dfb0c0f7-4615-4445-9ed6-f042ac3113d0.png`.

SHA-256: `F5EB3FBB0FC6A7A8A8D63F97B0310F863B98D77DCA6423FAE303B8806448449E`.

> Use case: stylized-concept. Asset type: one production-ready transparent isometric planetary building sprite for the Stellarion cinematic combat renderer. Input image 1 is the FACTORY shop-art visual reference; retain its recognizable factory identity and distinctive massing: layered long industrial production halls with rows of rounded vent hoods and broad cylindrical ducts, stacked warm-gray and weathered bronze metal facades, large dark workshop openings, dense compact pipe manifolds and repeated small processing stacks. Input image 2 is style reference ONLY: match its detailed realistic painterly sci-fi metal rendering, crisp readable armored edges and clean isolated cutout quality; do not add its weapons. Reinterpret the scenic shop picture as ONE entire compact planetary factory building viewed from about 30 degrees above in three-quarter isometric perspective. Keep two interconnected low workshop blocks with rounded ventilation housings, a few short distinctive exhaust stacks, exposed pipework and a compact manufactured industrial foundation. Fully visible centered building, entirely inside the frame with at least 8% clear transparent margin. Terrain-free compact foundation, no landscape or extra detached objects. Recognizable rounded ductwork from the shop image, subtly weathered gunmetal, dull bronze-tan roof panels, small amber work lamps and restrained cyan equipment lights. TRUE TRANSPARENT ALPHA background, empty alpha outside the building; no black/white/color backdrop, scenery, stars, planet, floor, checkerboard artwork, labels, text, UI, watermark or frame. No city skyline, ships, vehicles, weapons, missiles, fire, explosions, smoke, steam or busy glowing effects. The game adds animated lights, smoke and damage at runtime. A single clean unanimated source sprite, not a spritesheet. Deliver one square PNG with actual transparency.

### Missile Silo

Generated original: `exec-c22bf131-4d3d-4bc2-97b5-6856a6c54185.png`.

SHA-256: `49FDBC5B4F1223A32C9CD8C589B4B0D20D3CD73E2B7559EBC41628789C51EDD0`.

> Use case: stylized-concept. Asset type: one production-ready transparent isometric planetary building sprite for the Stellarion cinematic combat renderer. Input image 1 is the MISSILE SILO shop-art visual reference. Preserve its distinctive short broad cylindrical central bunker, warm bronze gunmetal armor, evenly spaced vertical amber lighting strips, concentric segmented circular service platform, low radial armored buttresses and maintenance walkways. Input image 2 is style reference ONLY: match its detailed realistic painterly sci-fi metal rendering, crisp readable armored edges and clean isolated cutout quality; do not add its guns or rocket launchers. Reinterpret the scenic shop picture as ONE complete compact missile-storage silo BUILDING at rest, viewed from about 30 degrees above in three-quarter isometric perspective. A chunky round central bunker with a clearly readable closed segmented circular launch hatch in its roof, surrounded by a single compact integrated mechanical platform and low buttresses. NO missiles projecting out, no vehicles, no weapons firing, no background launch sites. Entire building and compact manufactured foundation fully visible, centered, with at least 8% clear transparent margin on every side. Recognizable circular silhouette from the shop art, muted bronze-gray panels, restrained small amber working lights and very few cyan utility indicators. TRUE TRANSPARENT ALPHA background, empty alpha outside the building; no black/white/color backdrop, scenery, stars, planet, rocky terrain, floor, checkerboard artwork, labels, text, UI, watermark or frame. No projectiles, missile plume, explosions, smoke, steam, fire, cast floor shadow or busy baked effects. Game adds animated lights, smoke and damage at runtime. A single clean unanimated source sprite, not a spritesheet. Deliver one square PNG with actual transparency.
