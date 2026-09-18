# Cinematic combat artwork

The cinematic cutouts are original generated interpretations of Stellarion's existing shop
artwork. Each source reference was visually inspected before generation. The built-in
`image_gen.imagegen` tool was used in September 2026, with one separate reference-based
generation request per unit. No API/CLI fallback, third-party game art, or new service was
used. The existing shop art remains unchanged.

Source PNGs live in `assets/images/cinematic/` as `cinematic <shop image key>.png`.
They retain generated transparency and full source detail. The normal asset pipeline
converts only this category at half size, then produces a smooth mip chain in KTX2.
The registry loads these textures with linear filtering and premultiplied alpha for egui.
Cinematic sprites are animated in the renderer using movement, banking, lights, exhaust,
recoil, shield impacts and explosions; the PNGs are clean single-frame bases.

## Ship and defense prompt set

Every row below used its own shop PNG as **Image 1: visual reference**, with the shared
prompt followed by that row's subject description. The probe's first request omitted the
final camera-direction reinforcement paragraph. All generated pixels were inspected;
the renderer applies explicit per-unit orientation/aspect corrections where the
generator did not follow the requested camera exactly.

### Shared prompt

> Use case: stylized-concept. Asset type: one production-ready 2D cinematic combat sprite for the Stellarion space strategy game. Input image 1 is the shop-art visual reference: retain this exact unit's recognizable silhouette, distinctive structure, materials and characteristic lights, and interpret it as a more detailed isometric sprite, not a thumbnail. Three-quarter isometric camera looking down about 30 degrees, front pointing RIGHT and slightly UP; entire single unit centered with 10% clear margin. Detailed realistic painterly sci-fi illustration with crisp readable forms, brushed gunmetal panels, refined hard-surface detail, restrained small cyan/amber lights; cohesive with the reference's style. TRUE TRANSPARENT alpha background, no painted backdrop, no ground, no planet, no stars, no cast floor shadow, no checkerboard artwork. No text, labels, logos, border or UI. No other ships or objects. Do not crop. No baked weapon beams or firing/explosion effects. Short engine nozzle glow allowed, no long exhaust trails. This is a clean cutout sprite that the game will animate with motion/banking, exhaust, shield hits and explosions at runtime. CRITICAL CAMERA DIRECTION: do NOT copy the source photograph's viewing direction. Rotate this vehicle in 3D so its nose/weapon muzzle is at the UPPER RIGHT and its rear/engine nozzles are at the LOWER LEFT. The long hull axis should slope only gently upward left-to-right, not vertical. If reference points lower-left, reverse the camera view.

### Subject prompts

| Unit / output key | Shop reference | Subject description |
| --- | --- | --- |
| cinematic probe | `assets/images/ships/probe.png` | Small spindly reconnaissance probe: narrow dark gray core, tall sensor mast, thin lateral arms, tiny orange-red tips and one pale blue engine nozzle. |
| cinematic colony ship | `assets/images/ships/colony ship.png` | Rounded saucer-like colony ark with broad flattened hull, softly glowing green glass dome, exposed curved belly ribs and cyan engine ports. |
| cinematic light fighter | `assets/images/ships/light fighter.png` | Agile small silver-gray arrowhead fighter, narrow dark cockpit, twin side wings with orange-red trim, compact rear engines. |
| cinematic heavy fighter | `assets/images/ships/heavy fighter.png` | Stocky gunmetal armored fighter with broad rounded rear body, blocky rectangular nose and cyan cockpit strip, paired cannons, thick side pods. |
| cinematic destroyer | `assets/images/ships/destroyer.png` | Long lean black angular warship with a sharply pointed wedge prow, layered armor shoulders and small teal hull windows. |
| cinematic cruiser | `assets/images/ships/cruiser.png` | Long industrial cruiser with cylindrical segmented gunmetal hull, layered armored prow, skeletal middle truss and large rear propulsion cluster. |
| cinematic bomber | `assets/images/ships/bomber.png` | Heavy compact rounded bomber with bulky dark plated nose, cyan horizontal visor, reddish weathered side panels, exposed rear industrial frames and paired engine pods. |
| cinematic battleship | `assets/images/ships/battleship.png` | Powerful long battleship with a broad burgundy-red wedge bow, warm gold front window stripe, charcoal rear spine, paired large engine pods and dorsal towers. |
| cinematic dreadnought | `assets/images/ships/dreadnought.png` | Enormous long armored dreadnought with rounded gray cylindrical main hull, tall armored bronze prow, red panel accents, skeletal lower hangars, teal dorsal windows and large rear engines. |
| cinematic war sun | `assets/images/ships/war sun.png` | Colossal ringed superweapon: a long dark cylindrical central cannon with bright blue machinery, radial metallic spokes and broken segmented halo ring around the rear, imposing industrial metal. |
| cinematic crawler | `assets/images/defense/crawler.png` | Compact four-legged salvage robot, boxy blue-gray cabin with yellow panels and red sensor eyes, thin jointed hydraulic legs with wide feet. Front to right, elevated isometric view. |
| cinematic repair truck | `assets/images/defense/repair truck.png` | Small futuristic cream/tan six-wheel repair rover, long low chassis, chunky separated wheels, teal lights and a dorsal articulated repair/tool mast. No drones yet; those are animated in game. |
| cinematic rocket launcher | `assets/images/defense/rocket launcher.png` | Stationary chunky twin-box rocket launcher on a thick rotating mechanical pedestal and compact square foundation; olive-gunmetal armor, arrays of round missile apertures. Aim diagonally right/up. |
| cinematic light laser | `assets/images/defense/light laser.png` | Slender pale-gray laser tower on a flared compact circular pedestal, one narrow angled barrel and small rear cooling cylinder. Aim diagonally right/up. |
| cinematic heavy laser | `assets/images/defense/heavy laser.png` | Heavy tan-bronze laser turret, thick domed armored base, exposed cooling struts, very long slim barrel elevated toward right/up. Compact intact foundation. |
| cinematic gauss cannon | `assets/images/defense/gauss cannon.png` | Massive angular charcoal gauss emplacement with enormous rectangular raised cannon, heavy side support cylinders and compact square armored foundation. Aim diagonally right/up. |
| cinematic ion cannon | `assets/images/defense/ion cannon.png` | Large low armored blue-gray dome emplacement with thick barrel, bright blue ion coil highlights and reinforced angular base. Aim diagonally right/up. |
| cinematic plasma turret | `assets/images/defense/plasma turret.png` | Bulky bronze-white plasma emplacement on a compact flat square foundation, tall ribbed rear barrel housing, emerald green vents and short forward plasma nozzle. Aim diagonally right/up. |
| cinematic antiballistic missile | `assets/images/defense/antiballistic missile.png` | One slender silver-gray interceptor missile, pointed nose, compact dark fins, small orange nozzle, forward pointing right/up, whole missile visible. No second missile. |
| cinematic interplanetary missile | `assets/images/defense/interplanetary missile.png` | One heavy long interplanetary missile, armored dark metallic cylindrical body, white-gray pointed nose, red trim and large rear propulsion bell, nose pointing right/up. No launch trail. |

## Final targeted corrections

### probe correction

> Create a new detailed isometric sprite of this exact shop probe. A small unmanned recon drone with its distinctive tall antenna mast, triangular red side panels, thin outstretched sensor arms, dark metal conical cabin and small blue rear engines. View from ABOVE and BEHIND so you see REAR at BOTTOM LEFT and FRONT NOSE at TOP RIGHT, along the same -25-degree diagonal as a spaceship flying toward the upper right of screen. The image must show one entire compact probe including antenna tips with safe clear margins. No lower-left-facing nose. Do not duplicate the reference's camera; explicitly rotate the camera to the opposite side. Detailed realistic painterly sci-fi rendering. True transparent alpha background, isolated subject, no floor or backdrop, no stars, no labels. No muzzle glow or firing. Clean silhouette for animated game sprite.

### antiballistic missile correction

> Edit this supplied sprite. KEEP the LARGE upper-left-to-upper-right silver interceptor missile completely unchanged: its hull, nose, fins, colors, lighting, exact size and orientation. REMOVE the small SECOND missile in the bottom-right corner entirely, leaving transparent pixels there. This must be an isolated single-missile sprite with genuine transparent alpha background. No other changes, no new objects, no ground, text, UI or backdrop.

## Orbital structures

The eight orbital sprites use the separate reference/prompt record in
[cinematic-station-art.md](cinematic-station-art.md).

## Practical rendering notes

- Shop silhouettes and colors carry unit identity; scale is assigned by the combat
  renderer so larger hulls remain visibly larger.
- Every battle actor has an independent sprite and animation state. Sprite files are
  shared by type, not duplicated per unit.
- Alpha must remain intact. A black appearance in some image previews represents the
  transparent background; do not flatten or color-key these images.
- Use the actual PNG dimensions and native nose direction when changing sprite
  transforms. Square drawing quads distort the long-hull artwork.
- Existing starfield, nebula and explosion artwork supplies the scene backdrop/effects.
