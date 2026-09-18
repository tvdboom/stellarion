# Cinematic station art

These eight station cutouts were generated with the built-in ImageGen tool from their existing shop artwork in `assets/images/orbitals/`. Each result is saved as `assets/images/cinematic/cinematic <name>.png`. The source artwork remains unchanged. The transparent PNGs are single base frames; the cinematic renderer supplies motion, navigation lights, firing and recoil at runtime.

## Inspection and integration

Every final PNG was visually checked against its shop reference and inspected as
32-bit RGBA. Grid sampling found both fully transparent and opaque or nearly
opaque pixels in every image. The jump gate's center pixel has alpha zero. The
recycler's apparent soft background color in some image viewers is stored only
in transparent pixels; sampled halo locations have alpha zero.

| Station | Source dimensions | Orientation / details |
| --- | --- | --- |
| Solar satellite | 1247 × 1261 | Four solar panels; long service mast toward lower-left |
| Recycler | 1536 × 1024 | Intake faces lower-right; grapple structure on left |
| Command relay | 1254 × 1254 | Radial antennae; central lit face toward lower-left |
| Trading post | 1254 × 1254 | Upper isometric circular deck; visiting freighters removed |
| Sensor phalanx | 1254 × 1254 | Dish and primary antenna face upper-right |
| Jump gate | 1254 × 1254 | Tilted ring; empty transparent center; rim-only energy |
| Orbital railgun | 1254 × 1254 | Muzzle faces upper-right, about −40° from screen +X; muzzle near UV (0.96, 0.12) |
| Space dock | 1254 × 1254 | Shop's under-isometric view; cyan hub and downward service mast |

Aspect ratios must be preserved when rendering. The railgun has no baked firing
beam; its cyan glow comes from internal coils. Other stations have no weapon
direction requirement. Navigation lights and station motion are runtime effects.

## Exact prompts

Each request used the following common prompt followed by its station-specific suffix, with the named shop PNG supplied as the reference image.

```
Use case: stylized-concept. Asset type: transparent PNG game sprite for Stellarion cinematic space combat. Input image 1 is the exact shop design reference. Create a detailed realistic painterly science-fiction full-body isometric cutout of this SAME station, matching its recognizable silhouette, architecture, materials and colors, with slightly enhanced hard-surface detail and readable small-scale silhouette. Isometric three-quarter view from above, no perspective foreshortening extremes, single centered station with all antenna tips and appendages intact and generous transparent margin. Genuinely transparent alpha background: no stars, nebula, planet, ground, cast backdrop shadow, text, logos, border, other ships, debris, weapon fire or trails. Strong soft neutral key light upper left, restrained colored navigation lights. One object only, no sprite sheet.
```

### solar satellite

Reference: `assets/images/orbitals/solar satellite.png`
Output: `assets/images/cinematic/cinematic solar satellite.png`

```
Retain the metallic bronze-gray circular cylindrical hub, long central communication mast and four broad rectangular black photovoltaic solar panels on radial struts. Panels show fine silver grid lines. Antennae remain slender and industrial; no large glow or halo. View from above at an isometric angle, entire satellite visible.
```

### recycler

Reference: `assets/images/orbitals/recycler.png`
Output: `assets/images/cinematic/cinematic recycler.png`

```
Retain the compact bulky charcoal and pale-gray industrial salvage/recycling station, rectangular layered armored housing, angular roof with slotted vents, dark cavernous recessed intake on its right end and mechanical processing/grapple structures on its left end, tiny warm work lights. Intake/front points upper-right. Remove all surrounding asteroid rocks, debris and vessels; station alone.
```

### command relay

Reference: `assets/images/orbitals/command relay.png`
Output: `assets/images/cinematic/cinematic command relay.png`

```
Retain the small dark graphite central faceted spherical hub and its many long radial narrow antennae, spikes, rods and a tiny cyan-lit central panel. Asymmetric realistic communication relay architecture exactly like reference. Preserve every antenna inside the frame. Satellite alone.
```

### trading post

Reference: `assets/images/orbitals/trading post.png`
Output: `assets/images/cinematic/cinematic trading post.png`

```
Retain the reference's large gray metal circular orbital station with a glowing amber glass central dome, tall central communications/control tower, broad surrounding segmented annular deck, radial gray docking piers, numerous fine antennae and tiny cyan navigation lamps. Remove every visiting and docked freighter and tiny craft, including the obvious cargo ships in foreground and background; retain only permanent integral architectural piers. Entire station fits uncropped, isometric top view.
```

### sensor phalanx

Reference: `assets/images/orbitals/sensor phalanx.png`
Output: `assets/images/cinematic/cinematic sensor phalanx.png`

```
Retain the bronze-gold and dark metal orbital sensor assembly from the reference: main circular parabolic radar dish with concentric ribs, dark central electronics housing, slender long antenna rods and a smaller secondary dish on its appendage. This is a free-floating sensor station alone, no planet or surface mounts. Isometric three-quarter view, dish oriented upper-right, all antennae visible.
```

### jump gate

Reference: `assets/images/orbitals/jump gate.png`
Output: `assets/images/cinematic/cinematic jump gate.png`

```
Create the reference's single circular orbital jump-gate structure as a thick dark metallic segmented ring with bronze armored protrusions and machinery, tilted in isometric perspective. Retain the blue-white electrical identity, but constrain tiny blue energy arcs to the circumference/rim. The entire center opening must be genuinely transparent alpha, empty like a hole through a hoop: NO glowing portal sheet, NO filled energy membrane, NO lightning through the hole. No spaceship. Complete rim uncropped.
```

### orbital railgun

Reference: `assets/images/orbitals/orbital railgun.png`
Output: `assets/images/cinematic/cinematic orbital railgun.png`

```
Retain the exact reference's monumental elongated dark graphite orbital railgun: bulky rear reactor hub with radial rectangular radiator fins and antennae, long ribbed segmented accelerator barrel, luminous cyan internal coils, forked twin prongs at muzzle. For gameplay rotate the three-quarter isometric composition so rear reactor is at lower-left and the MUZZLE clearly points UPPER-RIGHT. Full weapon fits uncropped, no fired beam/projectile, no halo, no planet. Only the fixed internal cyan coils glow.
```

### space dock

Reference: `assets/images/orbitals/space dock.png`
Output: `assets/images/cinematic/cinematic space dock.png`

```
Retain the exact reference's dark metallic modular space dock: central cyan-lit rounded hub, wide horizontal branched framework with many paired rectangular projecting docking arms/platforms, layered gantries, small blue-lit details, and long narrow central underside service mast pointing downward. Isometric from above, entire structure intact, no visiting spacecraft or planet. Industrial charcoal-gray and subtle steel reflections, blue illuminated central port.
```
