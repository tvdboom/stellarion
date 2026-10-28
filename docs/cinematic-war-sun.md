# War Sun combat artwork

Replaced on 2026-10-28 with the built-in `image_gen` tool. The final source is
`assets/images/cinematic/cinematic war sun.png`, copied unchanged from
`C:/Users/Mavs/.codex/generated_images/01a182d6-e549-73e1-b7d6-60ad23b18410/exec-7479800e-89ee-42e5-89c2-1d6487096236.png`.
The 1536×1024 PNG retains its generated transparency. The normal asset pipeline
creates the runtime texture; no manual alpha editing or CLI fallback was used.

The canonical identity reference is `assets/images/ships/war sun.png`. Its three
blue forward emitters are weapons, with a central armored spine and a thin
structural hoop around the aft hub. The old combat art reversed that interpretation
and added an elongated cannon on the other end.

The new sprite faces right along a horizontal axis. One stable texture is used
through every firing phase, with charge, muzzle and beam light supplied by the
renderer. This prevents the ring, spokes and housing from changing shape between
generated animation frames. The obsolete `firing war sun.png` source was removed.
Muzzle and rear-engine attachment coordinates are registered in
`src/core/combat/cinematic_sprites.rs`; wreck fragments sample the full cutout.

## Generation prompt

Reference: the shop image. Initial output:
`C:/Users/Mavs/.codex/generated_images/01a182d6-e549-73e1-b7d6-60ad23b18410/exec-ed2fcc2d-309f-4210-915f-eb97873b3ee1.png`.

```text
Use case: stylized-concept. Asset type: production transparent RTS spacecraft sprite for Stellarion. Generate ONE new high quality War Sun, using the supplied SHOP IMAGE as the canonical design reference, preserving its recognizable silhouette and construction. The shop reference shows a long dark gunmetal central reactor/cannon spine, a large thin circular outer structural hoop around its aft hub, straight radial spokes and segmented wedge-like inner armor panels, cyan reactor accents, and a cluster of THREE blue-white forward emitter mouths firing the main weapon. Those three blue emitters are the FRONT WEAPON, not rear engines. Faithfully reinterpret this design as a clean detailed prerendered game sprite. Do not make a generic battleship, space station, sphere, or oversized naval gun. Rotate the camera/object so the ship's longitudinal axis is exactly HORIZONTAL: rear on the LEFT, the triple forward cannon emitter cluster on the RIGHT. Elevated three-quarter isometric view with visible top armor and a little of the right-facing emitter apertures; no dramatic perspective, no nose tilted up/down. The outer hoop is one mechanically coherent complete circle in 3D, projected as a clean ellipse, with straight evenly spaced supports joining the central hub. The ring sits behind the forward emitter assembly; the central spine passes through its center perpendicular to the ring plane. Keep the hoop thin and skeletal like the shop image, not a thick tire. Detailed worn charcoal and silver armor with restrained blue luminous cores, small panel seams, crisp readable silhouette, soft upper-left illumination. Improve the shop reference's clarity while retaining its identity and proportions. One complete centered spacecraft, wide canvas aspect 3:2, all geometry inside x0.08..0.90 y0.10..0.90. Aim the middle of the front triple-emitter cluster at approximately x0.84,y0.50. Small dim rear thruster apertures may be visible at the far LEFT end, but no long exhaust. All three front blue ports are resting, with restrained internal luminosity only. No beam, no muzzle flash, no smoke, no sparks, no surrounding ships, no stars, no planet, no ground shadow, no text, no labels, no border. Genuine transparent alpha background, including open gaps in the hoop and between its spokes. Not black, not a checkerboard. Output only the finished single transparent sprite.
```

## Final correction prompt

Reference 1: the initial generated sprite (edit target). Reference 2: the shop image.

```text
Use case: precise-object-edit/background-extraction. Image1 is the EDIT TARGET: preserve this exact War Sun's level horizontal orientation, ring geometry, armor, long spine, proportions, location, camera, and detailed rendering. Image2 is the shop identity reference only. Make exactly two corrections to image1. FIRST: remove ALL gray atmospheric haze, backdrop, vignette, glow cloud, bloom and shadow outside the solid spacecraft. The exterior and open spaces between hoop/spokes must have genuinely clean transparent alpha zero, not a gray semi-transparent atmosphere. Keep only antialiasing on actual metal silhouette edges. Remove the large gray halo around and behind the ship entirely. It must composite cleanly over the game's own starfield. SECOND: the FRONT at RIGHT must have exactly THREE circular blue weapon emitter mouths in a triangular cluster, matching the shop reference's triple emitter identity. Image1 wrongly has four. Use one top emitter and two lower emitters, all located at the same forward end of the central cylindrical weapon housing. No fourth port. Their combined center lies on the horizontal shaft axis at y0.50. All emitters stay within the hull outline and have quiet blue inner lights only, no external bloom, no firing beam or muzzle flash. Do not mistake the front emitters for engines. Do not change the left rear, ring, metal detail, ship scale, or framing. One whole spacecraft, no text, no grid, no border, no backdrop of any color; clean transparent PNG cutout.
```
