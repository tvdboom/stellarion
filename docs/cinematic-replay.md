# Cinematic combat replay

Choose **Schematic** or **Cinematic** above the battle list, then select a battle.
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

Weapon colors, projectile masks, barrel patterns, trajectories, particle trails,
and sound pitch/gain are shared with the schematic renderer. Charged weapons build
up before their recorded launch, preserving the last shot of a destroyed ship.
The planetary shield reuses the map's energy-field artwork with a three-second
pulse, moving filaments and surface sweeps; its strength follows recorded damage.

The renderer uses a single clock for trajectories, sprite banking and recoil, engine
glows, navigation lights, shield impacts, explosion atlas frames, particles, stars,
comets, and parallax. It caches actor layouts and indexes shot time ranges rather
than repeatedly walking the entire shot history. Every combatant remains represented;
larger fleets scale down to fit the viewport. Source artwork and generation prompts
are documented in [cinematic-art.md](cinematic-art.md). Runtime textures are generated
by the existing asset pipeline and included by normal native and web packaging.

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
banner, the shared settings and volume popovers, the War Sun charge/discharge,
and a 640×480 viewport. It uses the generated runtime textures with the same
filtering and alpha settings as the game; run `just assets` first.
