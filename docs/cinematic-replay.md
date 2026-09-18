# Cinematic combat replay

Hover the settings gear beside the sound icon in the battle chooser, select the
**Schematic** or **Cinematic** image tile, then select a battle from the list.
Schematic remains the default. Cinematic uses individual isometric ships and defenses,
with a planet for ground combat and open space for fauna encounters. Space pauses
the entire scene; Left/Right change speed from 0.25× to 64×. The same top-right
sound and settings icons as the schematic view provide volume and playback speed.
Hover the sound icon for its volume slider, click to mute/restore sound, or scroll
anywhere during combat to change volume in 10% steps. Cinematic settings omit the
schematic-only Volley fire and Individual units options. Escape returns to battle
selection at any time, as does the schematic-style **Exit combat** button at the
bottom right; the completed outcome banner is also clickable. Attacker and defender
banners align beneath the top-right controls, using the same participant ordering,
names, colors, and fleet-strength accent segments as the schematic replay.

The movie consumes `MissionReport` without modifying it or running combat again.
Stable combatant IDs retain each ship through hits, repairs, and casualties. Both
sides' salvos overlap, but target hit order is preserved and a lethal hit waits for
the victim's final recorded launch. Shared planetary shield hits precede hull damage
to protected defenses. Repair trucks heal the recorded targets and amounts; the
report does not identify which particular truck supplied each repair, so surviving
trucks are assigned only for presentation. Repairs can overlap fire at other targets.
Retreats, returning probes, bombing, planet destruction, and draws follow the report.
Noncombat orbital structures have no invented attacks.

Economic and industrial raids show the planet's relevant Metal Mine, Crystal Mine,
Deuterium Synthesizer, Shipyard, Factory, and Missile Silo as individual surface
structures. Each recorded successful bomb causes an explosion and a rising
**−1 level** caption, reduces the displayed level, and leaves the structure standing
until its last level is lost. Misses never remove levels. Surviving War Suns combine
their charging rays at one focus before a shared beam strikes the planet; the
planet breaks apart only when the saved report records its destruction.

Weapon colors, projectile masks, barrel patterns, trajectories, particle trails,
staged wreck explosions, and sound pitch/gain are shared with the schematic renderer.
Charged weapons build up before their recorded launch, preserving the last shot of a destroyed ship.
The planetary shield reuses the map's energy-field artwork with a three-second
pulse, moving filaments and surface sweeps; its strength follows recorded damage.
The scene uses the normal map background with its proportions preserved. Ships
follow independent curved approaches over 4.8 seconds, banking gently while
preserving the perspective of their artwork, then continue maneuvering in combat.
Both views reveal the same victory, draw, or defeat image in a dark central band,
using the same 1.5-second entrance and no instruction caption.

The renderer uses a single clock for trajectories, sprite banking and recoil, engine
glows, navigation lights, shield impacts, explosion atlas frames, particles, stars,
comets, and parallax. It caches actor layouts and indexes shot time ranges rather
than repeatedly walking the entire shot history. Every combatant remains represented;
larger fleets scale down to fit the viewport. Source artwork and generation prompts
are documented in [cinematic-art.md](cinematic-art.md). Runtime textures are generated
by the existing asset pipeline and included by normal native and web packaging.
The six building references and generation prompts are documented in
[cinematic-economic-art.md](cinematic-economic-art.md) and
[cinematic-industrial-art.md](cinematic-industrial-art.md).
The rebuilt War Sun and mode tiles are documented in
[cinematic-refresh-art.md](cinematic-refresh-art.md).

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
