# Cinematic combat visual refresh artwork

Generated with the built-in `image_gen` tool on 2026-09-18. No CLI/API fallback, manual image editing, resampling, or alpha modification was used. Each final PNG is an unchanged copy of its generated output. The mode tile border and text are supplied by the game UI.

## War Sun

Final asset: `assets/images/cinematic/cinematic war sun.png`

Original built-in output: `C:/Users/Mavs/.codex/generated_images/01a0b3fa-4a66-7692-9302-97bc5861fc94/exec-1eca0741-22c6-40fe-af05-932fb2a10f9a.png`

References, inspected before generation:

- `assets/images/ships/war sun.png`
- `assets/images/cinematic/cinematic war sun.png (previous version)`
- `User screenshot codex-clipboard-b07b0b30-2479-4b57-abb4-60330ce24754.png`

Final prompt:

```text
Use case: stylized-concept
Asset type: transparent isometric combat ship sprite for the existing Stellarion game.
Primary request: remake the War Sun capital ship so its large ring has precise coherent geometry rather than the malformed reference. Reference 1 is the canonical shop design and material identity. Reference 2 is the current flawed sprite to replace: its bent ring must NOT be copied. Reference 3 shows the bug in use.
Subject: one massive gunmetal War Sun with a long coaxial cannon, a central armored hub, a single perfectly circular structural wheel around that hub, regularly spaced symmetric radial spokes and segmented radial armored panels, small cyan reactor lights. Preserve the shop's recognizable ring-and-cannon identity.
Geometry is the priority: model the outer ring as one rigid complete circular hoop, viewed obliquely so its outline is a mathematically clean continuous ellipse of uniform thickness. Every spoke is straight and connects from the same central hub to evenly spaced points on that circle. No warped hoop, irregular loops, mismatched arcs, twisted spokes or asymmetric ring. The cannon passes through the hub perpendicular to the plane of the wheel, all coaxial.
Camera: stable elevated three-quarter isometric view, no dramatic perspective distortion, entire ship fits within frame with ample transparent margin. The cannon's FRONT muzzle and ship's forward direction point diagonally toward the UPPER RIGHT of the image, roughly 30 degrees above horizontal. The stern is lower left. Mild short blue thruster lights at stern only; no long exhaust. The forward muzzle must read clearly.
Style: detailed realistic pre-rendered RTS spacecraft sprite, worn dark silver metal, crisp readable silhouette, subdued cyan-blue lights, upper-left soft directional light. Match existing ship imagery's material treatment.
Backdrop: true transparent alpha, no starfield, no planet, no ground plane, no fake checkerboard, no opaque black background.
Constraints: exactly one complete centered spacecraft; no other ships, no text, no labels, no UI, no border, no weapon beam, no explosions. Generous unclipped silhouette. Return a transparent PNG.
```

## Schematic mode

Final asset: `assets/images/resources/combat schematic.png`

Original built-in output: `C:/Users/Mavs/.codex/generated_images/01a0b3fa-4a66-7692-9302-97bc5861fc94/exec-d5df497a-ef96-4c37-ab77-c5dc58736653.png`

References, inspected before generation:

- `assets/images/resources/dock bastion.png`
- `assets/images/resources/dock industrial.png`
- `assets/images/resources/mine normal.png`

Final prompt:

```text
Use case: style-transfer
Asset type: Stellarion game combat-view selection image button.
Primary request: create a new SCHEMATIC combat-view mode image matching the supplied existing dock and mine mode button tiles as an exact visual family.
Input images: references 1 and 2 are the existing dock mode button tiles; reference 3 is the existing normal mine mode tile. Match their dark scratched navy-blue metal background, luminous ice-blue/cyan beveled metallic emblem, strong cyan outer rim and subtle bloom, centered framing and square-corner rectangular 3:2 tile proportions.
Subject: a compact tactical combat diagram emblem: three small stylized triangular spacecraft markers at left facing three matching markers at right, arranged in orderly opposed formations; restrained thin tactical grid and two clearly legible connecting laser/trajectory lines between the formations, all integrated as one clean raised blue metal insignia. Clearly communicate a SCHEMATIC battle view with simple symbols rather than realistic rendered hulls.
Composition: one centered emblem occupying about 60% of the tile width and height, generous even dark navy margin. Same material highlights and embossed edge treatment as the existing mode icons. Plain brushed navy metal tile background fills image.
Constraints: 1536x1024 3:2 landscape tile. NO text, letters, numbers, labels, words, interface panels, logos or watermark. No selected-state marks. No extra rectangular border inside the picture; the existing game UI supplies the button's surrounding border. Avoid filmstrip or camera imagery here so it is visually distinct from the cinematic mode.
```

## Cinematic mode

Final asset: `assets/images/resources/combat cinematic.png`

Original built-in output: `C:/Users/Mavs/.codex/generated_images/01a0b3fa-4a66-7692-9302-97bc5861fc94/exec-ba5af5d3-d392-4419-8762-ab67c492dd4b.png`

References, inspected before generation:

- `assets/images/resources/dock bastion.png`
- `assets/images/resources/dock industrial.png`
- `assets/images/resources/mine normal.png`
- `Newly generated combat schematic.png`

Final prompt:

```text
Use case: style-transfer
Asset type: Stellarion game combat-view selection image button.
Primary request: create a new CINEMATIC combat-view mode image matching the supplied existing dock and mine mode button tiles as an exact visual family, and matching the newly generated schematic companion tile.
Input images: references 1 and 2 are the existing dock mode button tiles; reference 3 is the existing normal mine mode tile; reference 4 is the companion schematic mode tile. Match their dark scratched navy-blue metal background, luminous ice-blue/cyan beveled metallic emblem, strong cyan rim and subtle bloom, centered framing and rectangular 3:2 tile proportions.
Subject: a single clean raised metallic-blue movie-frame insignia containing a dynamic three-quarter isometric spaceship hull with a short laser glint and small starburst behind it, unmistakably communicating a CINEMATIC space battle. Use the simple perforated upper and lower edges of a filmstrip around the spacecraft scene as the emblem. The hull should have depth, unlike the flat tactical markers in the schematic companion. Keep it readable at icon size and restrained, not a crowded illustration.
Composition: one centered emblem occupying about 60% of the tile width and height, generous even dark navy margin, balanced comparable visual weight to companion schematic icon. Same bevels, polished edge light and scratched-metal materials as existing mode icons. Plain brushed navy metal tile background fills the image.
Constraints: 1536x1024 3:2 landscape tile. NO text, letters, numbers, labels, words, UI panels, logos or watermark. No selected-state marks. No additional rectangular border at the image edge; the existing game UI supplies the button's surrounding border. No tactical grid or opposing marker formations, to stay distinct from schematic mode.
```

## Visual verification

The regenerated War Sun has a continuous regular outer hoop, consistent radial supports, and a clear long cannon aligned from lower left to upper right, approximately −31° in image coordinates. The source alpha is retained; the whole silhouette fits within transparent margins. Both mode tiles have the same 3:2 composition, navy brushed-metal background, cyan bevels, and restrained glow as the existing resource mode art, with visibly distinct tactical and film imagery and no embedded text.

