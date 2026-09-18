# Cinematic economic-building artwork

These three building sprites were generated in September 2026 with the built-in
`image_gen.imagegen` tool, one distinct reference-based request per building. The
existing 200 × 200 shop PNGs were inspected with `view_image` before generation. No CLI/API
fallback, Python processing, source-shop replacement, or image editing was used.

Each request used its corresponding `assets/images/buildings/<unit>.png` as
**Image 1: subject reference**, and the inspected
`assets/images/cinematic/cinematic plasma turret.png` as **Image 2: rendering-style
reference only**. The turret's weapon geometry was explicitly excluded.

The selected outputs were visually inspected and copied byte-for-byte into
`assets/images/cinematic/`. All are 1254 × 1254, 8-bit RGBA PNGs (PNG color type 6); sampled pixels include
alpha 0 and 255, and both opposite corners have alpha 0. SHA-256 checks confirmed
that every saved source is identical to the selected generated bitmap. The normal
cinematic asset pipeline performs runtime resizing and mip generation separately.

## Final assets

| Output | Shop features preserved | SHA-256 |
| --- | --- | --- |
| `cinematic metal mine.png` | Angular processing halls, cyan roof strips, amber windows and connected ore conveyors on a compact engineered base. | `6026cb70d0599b7f8a82b64b1afa6da6c4478b075a2a654068c1d9f6e2c1c461` |
| `cinematic crystal mine.png` | Pale extraction column, round upper drum, cross-arms and stabilizing cables, red status light, blue crystals contained in the base hopper. | `20865068d0e2e7fdd32652f0c544b00505c62febc09d5ac9ce04cb992f678c0f` |
| `cinematic deuterium synthesizer.png` | Tapered armored tower, dark annular machinery, vertical buttresses and teal processing tubes; no ocean or boats. | `9325eb7b12468de08e2b57f9a23a1730076900a8af7b4b4c08bc0698e760179b` |

All three are clean intact single-frame building cutouts. Status-light pulses,
smoke, damage and destruction belong to the runtime renderer. No terrain,
background, external cast shadow, text, UI, weapons, smoke or explosions were
baked into the sprites. Camera views show both the roofs and building sides; the
foundation stays contained within each transparent image.

## Exact prompt set

Each call used the shared prompt followed by the corresponding subject prompt.

### Shared prompt

> Use case: stylized-concept. Asset type: one production-ready transparent 2D isometric cinematic building sprite for the Stellarion space strategy game. Input image 1 is the actual shop subject reference: preserve the building's recognizable silhouette, architecture, materials and signature colored lights. Input image 2 is STYLE ONLY: match its detailed painterly hard-surface science-fiction rendering, crisp metal edges, coherent industrial forms and compact foundation, but DO NOT copy its weapon or turret. Scene/backdrop: genuinely transparent alpha, no painted background, no floor, no scenery, no stars, no shadows outside the base, no checkerboard artwork. Composition: elevated three-quarter isometric view looking down about 30 degrees, whole single building centered fully visible with 10 percent clear transparent margin. Structure on one compact terrain-free engineered foundation; no sprawling ground tile. Brushed weathered metal, restrained readable highlights and small status lights. No text, numbers, logos, UI, frames, people, vehicles, weapon barrels, weapon beams, explosions, smoke, fire or motion trails. Clean intact single-frame base to animate at runtime.

### Metal mine

> Subject: METAL MINE from image 1. Condense the recognizable facility into ONE compact mining complex: stepped gunmetal processing halls with sharply sloped metallic roofs and bright cyan vertical roof strips, a short elevated conveyor/gantry bridge connecting a small secondary hopper to the larger main extraction hall, warm amber window bands, stout angular machinery, visible pipes and vents. Keep the source's elegant dark industrial architecture and cyan/amber lighting. The gantry belongs to the same foundation. Do not render the surrounding canyon, mountains, whole mining city or long external roads.

### Crystal mine

> Subject: CRYSTAL MINE from image 1. A single tall industrial extraction rig: pale weathered gray/tan central tapering column, large dark round mechanical extraction drum near its top, two short supported cross-arms with visible taut stabilizing braces/cables, a small red identification light, compact lower processing hut. Incorporate a FEW luminous blue crystal chunks held inside a compact engineered collection hopper at the building's feet as the resource signature, NOT a rock landscape or cave. Preserve the source's distinct vertical tower and cross-braced machinery. Keep all cable anchors and short cross-arms on the same compact base. No gigantic cliff-to-cliff overhead cables, no terrain.

### Deuterium synthesizer

> Subject: DEUTERIUM SYNTHESIZER from image 1. ONE heavy cylindrical processing tower with a tapered armored upper crown, chunky pale gray metallic cladding, a dark annular midsection with round ports, strong vertical buttresses down its lower body, bright teal/cyan vertical processing tubes glowing between the supports, small pipes and intake conduits tucked around its compact round or polygonal industrial foundation. Preserve the source's formidable tower silhouette and aqua processing light. Remove all ocean, water, horizon, clouds, islands, boats and distant duplicate towers. No smoke or steam; only restrained small status lights.

## Built-in output provenance

The selected outputs were returned under the built-in generated-images directory
`01a0b376-4afd-72f1-89fe-fddd95fbf7a3`; originals were retained there.

- metal mine: `exec-48565d38-f846-4de1-9d23-28e389e38df9.png`.
- crystal mine: `exec-1d4f2d2d-249a-405e-9154-6b50de473035.png`.
- deuterium synthesizer: `exec-7f703d24-475b-4e95-b273-e34ea8072ff8.png`.

Related sprite conventions and runtime-alpha handling are recorded in
[cinematic-art.md](cinematic-art.md).
