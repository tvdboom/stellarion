# Cinematic combat replay

Hover the settings gear beside the sound icon in the battle chooser, select the
**Schematic** or **Cinematic** image tile, then select a battle from the list.
Schematic remains the default. Cinematic uses individual isometric ships and defenses,
with a planet for ground combat and open space for fauna encounters. Space pauses
the entire scene; Left/Right change speed from 0.25× to 64×. **Ctrl+Shift+Left**
restarts and resumes the whole movie, including its sound and camera, from the opening arrival.
The selected speed and volume are preserved. The same top-right
sound and settings icons as the schematic view provide volume and playback speed.
Hover the sound icon to click or drag its volume slider, or click the icon to mute/restore sound.
In cinematic combat, scroll over the battlefield to zoom toward the mouse pointer
between 0.65× and 4×. Hold the left mouse button and drag, or use **WASD**, to move
around the battle, including while paused. Dragging can stretch 20% of the viewport
beyond the battlefield edges, then returns with the normal map's elastic easing.
The return stays active while the movie is paused and also recenters after zooming out;
the banners, settings and exit button stay fixed on screen. Schematic combat retains
its wheel volume control in 10% steps. Cinematic settings omit the
schematic-only Volley fire and Individual units options. Escape returns to battle
selection at any time, as does the schematic-style **Exit combat** button at the
bottom right; the completed outcome banner is also clickable. Attacker and defender
banners align beneath the top-right controls, using the same participant ordering,
names, colors, and fleet-strength accent segments as the schematic replay.
Ship artwork retains its original armor detail under subtle cool underside lighting.
An additive pass through the hull's own texture reflects nearby planetary light and
brief recorded muzzle/impact flashes. Transparent margins and gaps between station
arms remain transparent; gun lighting follows the same articulated surface as its barrel.
Attached engine nozzles emit tapered plasma plumes tinted by the individual
combatant's selected player color. Thrust follows actual travel speed, with shorter,
steadier capital-ship exhaust and longer fighter burns during arrival and withdrawal.
Exhaust curves through recent nozzle positions, and small maneuvering jets respond
to actual changes in direction and braking instead of firing as a repeating decoration.
Surface defenses use restrained colored pedestal armor. Neutral fauna keep their
original artwork.
Schematic unit images have a thin border in the same owner color. Individual cards
use their own player's color; grouped cards split the perimeter between contributing
players in proportion to their unit counts.

The movie consumes `MissionReport` without modifying it or running combat again.
Stable combatant IDs retain each ship through hits, repairs, and casualties. Both
sides' salvos overlap, but target hit order is preserved and a lethal hit waits for
the victim's final recorded launch. Shared planetary shield hits precede hull damage
to protected defenses. Repair trucks heal the recorded targets and amounts; the
report does not identify which particular truck supplied each repair, so surviving
trucks are assigned only for presentation. Repairs can overlap fire at other targets.
Retreats, returning probes, bombing, planet destruction, and draws follow the report.
The Space Dock is larger than a War Sun and follows a fixed-altitude orbital arc
above the planet, smoothly easing to a slower drift as the fleets arrive. It does
not bob or rock in place, and
participates with its recorded weapons, hits, and sound. Other orbital structures
are omitted from the cinematic picture and soundtrack; the saved combat result
remains unchanged. Surface turrets
fire from their raised gun housings toward their recorded ship targets, using the
shared weapon effects at a readable size even in dense defensive formations.

Most armed ships, surface turrets and the Space Dock have eight-stage firing sprites:
charge, muzzle flash, barrel recoil and recovery. The War Sun uses a single registered
shop-derived hull with procedural charge and muzzle light, preserving its complete
ring through firing. Its three forward blue emitters supply the weapon; the rear
nozzles supply propulsion. Ship and station silhouettes stay whole.
Ground weapons aim through continuous joints attached to fixed foundations.
Projectiles and beams use the rendered cannon's frame-specific muzzle attachment.
The ship artwork has a level bow-to-stern axis; banking follows the actual flight
path with bounded banks and horizontal reflection, so ships never roll upside down.
During ordinary salvos, hulls retain their travel heading with a small aiming suggestion
of roughly 3–6 degrees, depending on mass. Nearby recorded salvos contribute to a smooth
average, so alternating high and low targets cannot wrench the whole ship around.
Shots leave the attached muzzle and reach the recorded target even when firing off-axis.
Ground turrets still track targets through their articulated gun assemblies. Lateral
cruise bends have individual phases instead of tilting the whole fleet together.
Turns also change the hull's perspective slightly, with heavier ships banking less.
The same transform keeps guns, engines and reflected light attached; it settles during
firing alignment. Directional skylight and engine reflections move over the armor.
War Suns form a shallow firing fan and advance toward their shared focus during a
planet strike, with the rest of the battle continuing around them. The convergence
point is anchored beside the planet, independent of fleet height; barrel axes line up
with it throughout charging and firing. Completed salvo afterglows retain their impact
origins instead of following the hulls into the planet-strike formation.

Economic and industrial raids show the planet's relevant Metal Mine, Crystal Mine,
Deuterium Synthesizer, Shipyard, Factory, and Missile Silo as individual surface
structures. Only the selected raid's economic or industrial category is shown;
coordinated attacks include every participating commander's selected category.
Permanent level labels are omitted. Each recorded successful bomb causes an explosion
and a rising **−1 level** caption, and leaves the structure standing
until its last level is lost. Misses never remove levels. Surviving War Suns combine
their charging rays at one focus before a shared beam strikes the planet; the
charge and discharge overlap the escorts' and defenses' recorded covering fire.
War Suns finish their own salvos before charging; final planetary collapse waits
for the round's impacts, repairs, and departures. No extra shots or damage are invented.
The impact sends pressure ripples around the curved surface. Failed strikes leave the
planet intact as those ripples fade. Recorded destruction engulfs the globe in
overlapping versions of the regular battle explosion, with staggered explosion
sounds and small drifting shards. At 0.68 seconds into the detonation, opaque blast
cover conceals the swap to the normal destroyed planet (or moon) artwork. Its transparent
margins are cropped so the ruined globe retains exactly the original visible diameter
and stays visible after the explosions clear. The death ray is 4 dB louder than its default
cue, while still following the player's volume setting.

Weapon colors, projectile masks, barrel patterns, trajectories, particle trails,
staged explosion-atlas frames, and sound pitch/gain are shared with the schematic renderer.
The cinematic atlas uses premultiplied alpha. Soft additive light, feathered beams,
continuous curved projectile wakes, directional impact sparks and brief hot flashes
replace hard glow disks. Wrecks leave delayed rotating fragments after their blast.
Energy bolts have crisp tapered cores, railguns leave longer hot streaks, and missiles
leave curved exhaust and continuous smoke. Barrel patterns remain distinct at small
scales. Beam afterglows begin at contact and fade without a dark gap.
Damaged survivors vent from their armor, with vapor retaining its emission position
and momentum. Repairs stop new venting while existing smoke disperses naturally.
Destroyed ships also separate into three darkened, irregular sections of their actual
hull, retaining their final velocity under the blast. Their smoke and glowing seams
travel with the fragments, with heavier wrecks turning more slowly and fading before
the closing hold ends. Ground hits leave bounded scorch marks at their recorded impact
locations with brief dust clouds; shield-only contacts and gas worlds leave no craters.
Charged weapons build up before their recorded launch, preserving the last shot of a destroyed ship.
The planetary shield reuses the map's energy-field artwork with a three-second
pulse, moving filaments and surface sweeps; its strength follows recorded damage.
The scene uses the normal map background with its proportions preserved, restrained
exposure shading and a soft edge vignette. Planet rims have a directional, feathered
halo colored for the world, and surface units cast soft contact shadows. Defense
terraces pack actual gun widths, mix classes and stagger positions to keep large
weapons from overlapping their neighbors. The far terrace recedes slightly in scale.
Fleet staging reserves room for large hulls and arranges smaller escorts in irregular
groups with cached footprint separation. Three
wrapping star layers respond at different depths to camera movement and zoom,
with the map's blue, white, and warm star colors, independent twinkles, and sparse
glowing flares. Their animation follows the replay clock; navigating while paused
still changes their parallax. Ships
open fire around 1.65–2.0 seconds while still occupying distant outer lanes, then keep
closing under fire through an 8.8-second approach. Their ongoing cruise runs at 55%
of the earlier pace. Strategic missile strikes retain their separate 1.4-second entrance.
Their bows and engine trails follow their actual travel direction, including withdrawals.
Capital ships take slow, steady arcs with a longer steering look-ahead; lighter ships
make faster, broader passes. Long encounters ease forward and lateral travel together,
avoiding sideways oscillation after forward motion slows. Arrival velocity is continuous,
and planet avoidance bends the approach smoothly before reaching the clearance boundary.
Recorded penetrating hits push surviving ships into a short, mass-sensitive recovery;
shield-only contacts do not displace hulls. Reactions never begin before impact.
Ships bend around the Space Dock's occupied berth while the station keeps its orbit.
Bombers make approaches toward their recorded surface
targets while both fleets keep their complete hulls outside the planet and shield.
Turrets scale by class, with Ion and Plasma guns larger than inexpensive emplacements.
The relevant buildings occupy a smaller, lower settlement row. Repair trucks travel
between their assigned recorded repair targets along curved routes, steer toward
their direction of travel, and retain their heading when parked. Tight repair schedules
do not teleport or accelerate trucks: any repair that starts during travel still uses
the truck's actual position as its visual source.
Gas worlds use floating building platforms and hovering defense pads.
Both views reveal the same victory, draw, or defeat image in a dark central band,
using the same 1.5-second entrance and no instruction caption.

The renderer uses a single clock for trajectories, sprite banking and recoil, engine
glows, navigation lights, shield impacts, explosion atlas frames, particles, stars,
comets, and parallax. It caches actor layouts and indexes shot time ranges rather
than repeatedly walking the entire shot history. Every visible ship and ground unit
keeps its own sprite; larger fleets scale down to fit the viewport. Source artwork and generation prompts
are documented in [cinematic-art.md](cinematic-art.md). Runtime textures are generated
by the existing asset pipeline and included by normal native and web packaging.
The six building references and generation prompts are documented in
[cinematic-economic-art.md](cinematic-economic-art.md) and
[cinematic-industrial-art.md](cinematic-industrial-art.md).
The current War Sun is documented in [cinematic-war-sun.md](cinematic-war-sun.md).
The mode tiles and earlier artwork provenance are in
[cinematic-refresh-art.md](cinematic-refresh-art.md).
Twenty detailed 1254-pixel planet sprites provide two stable variants for each of
the ten planet/moon kinds. The camera frames their globe rims consistently, and
planet destruction uses the same high-resolution texture. Planet and gas-platform
prompts are recorded in [cinematic-planets-a.md](cinematic-planets-a.md),
[cinematic-planets-b.md](cinematic-planets-b.md), and
[cinematic-gas-surface-art.md](cinematic-gas-surface-art.md).

Visual pacing was informed by [Stellaris battle screenshots](https://forum.paradoxplaza.com/forum/threads/obligatory-space-battle-screenshot-thread.927576/).
All ship and structure artwork is derived from Stellarion's own shop references.

## Local verification

Run these commands from the isolated checkout:

```text
just assets
cargo test --lib core::combat::cinematic -j6
cargo test --lib core::combat:: -j6
just lint
just check-wasm
just assets-check
```

On Windows with a GPU, this opt-in test renders the real painter and controls to
images without opening a window or connecting to a multiplayer backend:

```text
cargo test --lib render_cinematic_preview -j6 -- --ignored --nocapture
```

The frames are written under ignored `target/cinematic-preview/`. They include
entrance, active combat, shield impact and motion, repair, destruction, the outcome
banner in both renderers, the mode tiles and shared settings/volume popovers, the War Sun charge/discharge,
planet breakup, both building categories before and during bombing, rising level
loss captions, and a 640×480 viewport. It uses the generated runtime textures with the same
filtering and alpha settings as the game; run `just assets` first.
Set `STELLARION_CINEMATIC_MOTION=1` for the same test to also write 360 sequential
`motion-000.png` through `motion-359.png` frames at 30 fps for reviewing the opening
12 seconds of flight and combat.
